# Deployment Sequence

## Network Configuration

Network environments are defined in [`environments.toml`](../environments.toml) at the project root. Each named section maps to a `--network` value used by the Stellar/Soroban CLI.

Available networks: `testnet`, `mainnet`, `futurenet`, `standalone`.

To target a specific network, pass `--network <name>` to any `stellar contract` command. To add a custom network, append a new `[section]` with `rpc_url` and `network_passphrase` fields — see the comments in `environments.toml` for details.

---


This document describes the required deployment order and initialization steps
for the Checkmate Escrow smart contracts.

---

## Why Order Matters

Both the `OracleContract` and `EscrowContract` expose an `initialize` function
that must be called exactly once after deployment. Prior to the fix for
[#216], these functions had no deployer guard, meaning any observer of the
deployment transaction could front-run the call and initialize the contract
with a malicious admin or oracle address.

The fix requires the deployer address to be passed explicitly and to authorize
the `initialize` call via `deployer.require_auth()`. This means only the
account that deployed the contract can initialize it.

---

## Deployment Steps

### 1. Deploy OracleContract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
  --source <DEPLOYER_KEYPAIR>
# → outputs ORACLE_CONTRACT_ID
```

### 2. Initialize OracleContract

The `deployer` argument must be the same account used to deploy the contract.

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <DEPLOYER_KEYPAIR> \
  -- initialize \
  --admin <ORACLE_ADMIN_ADDRESS> \
  --deployer <DEPLOYER_ADDRESS>
```

### 3. Deploy EscrowContract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source <DEPLOYER_KEYPAIR>
# → outputs ESCROW_CONTRACT_ID
```

### 4. Initialize EscrowContract

The `oracle` argument must be the `ORACLE_CONTRACT_ID` from step 1.
The `deployer` argument must be the same account used to deploy the contract.

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <DEPLOYER_KEYPAIR> \
  -- initialize \
  --oracle $ORACLE_CONTRACT_ID \
  --admin <ESCROW_ADMIN_ADDRESS> \
  --deployer <DEPLOYER_ADDRESS>
```

### 5. Configure Token Allowlist (Optional but Recommended for Production)

By default the allowlist is **not enforced** — any token address is accepted in `create_match`. The allowlist activates automatically the moment the first token is added via `add_allowed_token`. Once active, `create_match` rejects any token not on the list with `InvalidToken`.

Add each token you want to permit (e.g. XLM native asset contract, USDC):

```bash
# Allow XLM (native asset contract address)
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  -- add_allowed_token \
  --token <XLM_CONTRACT_ADDRESS>

# Allow USDC (or any other token)
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  -- add_allowed_token \
  --token <USDC_CONTRACT_ADDRESS>
```

> **Note:** After the first `add_allowed_token` call, allowlist enforcement becomes active. If the last allowed token is removed, enforcement is disabled again and `create_match` accepts any token.

### 6. Configure Match Timeout (Optional)

By default, matches expire after ~30 days (518,400 ledgers at 5s/ledger). You can configure a different timeout per environment using `set_match_timeout`. The timeout must be between 1 and 90 days (17,280 to 1,555,200 ledgers).

**Recommended values:**
- Testnet: 1 day (17,280 ledgers) for faster testing
- Mainnet: 30 days (518,400 ledgers) for production stability

```bash
# Set timeout to 14 days (244,800 ledgers)
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  -- set_match_timeout \
  --timeout 244_800
```

To verify the current timeout:

```bash
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_match_timeout
```

---

## Upgrade Rollback

If `scripts/deploy.sh --upgrade` fails partway through — for example the new
Wasm uploads successfully but the contract's state migration panics before
the upgrade is finalized — the escrow contract can be left pointing at a
partially migrated state. `scripts/deploy.sh --rollback` cancels the pending
upgrade so the contract keeps serving its previous, known-good code and
state instead of staying stuck mid-migration.

### Prerequisites

- The escrow contract must expose `cancel_upgrade` (all contracts deployed
  from this repo's `contracts/escrow` do). Rollback is not possible against
  a contract build that predates this entry point.
- `CONTRACT_ESCROW` must be set to the contract ID of the escrow instance
  the failed upgrade targeted.
- The deployer keypair used for `--rollback` must be authorized to call
  `cancel_upgrade` on that contract (the same admin/deployer credentials
  used for the original deploy/upgrade).
- Rollback only cancels the *pending* upgrade recorded on-chain. It does not
  undo any off-chain state (e.g. an already-updated `.env` with a new
  contract ID) — update your own records back to the pre-upgrade values
  separately.

### Usage

```bash
CONTRACT_ESCROW=<escrow-contract-id> \
DEPLOYER_KEYPAIR=deployer \
ORACLE_ADMIN=<oracle-admin-address> \
ESCROW_ADMIN=<escrow-admin-address> \
./scripts/deploy.sh testnet --rollback
```

You'll be asked to type `rollback` to confirm before `cancel_upgrade` is
invoked. `--upgrade` and `--rollback` cannot be combined in a single
invocation. See `scripts/tests/test_deploy_rollback.sh` for a stubbed
smoke test of this path that doesn't require network access.

## Mainnet Deployment Checklist

Before launching on mainnet, verify each item below. These checks are intended to reduce operational risk and confirm that the deployment is configured for production use.

- [ ] Key management is locked down. Store deployer and admin keys in hardware-backed wallets or a secure multisig setup, and remove any temporary single-signature keys once the deployment is complete. This reduces the risk of losing access to the contracts or exposing a critical key.
- [ ] Admin control has been transferred to a multisig. Confirm that the escrow and oracle admin roles are controlled by a multisig account rather than a single operator key. This prevents a single compromised key from changing critical contract parameters.
- [ ] Oracle addresses have been verified. Double-check the oracle contract ID and any admin or authorized addresses used during initialization. This ensures results are routed to the intended oracle and avoids misconfiguration at launch.
- [ ] The token allowlist has been reviewed. Confirm that the approved token set and contract IDs match the production plan. This prevents unintended assets from being accepted in matches.
- [ ] Contract audit confirmation is recorded. Make sure the deployed contracts have passed a recent security review or audit and that any outstanding issues are understood and accepted. This lowers the chance of launching with an unresolved vulnerability.
- [ ] Monitoring and alerting are in place. Configure alerts for deployment status, admin changes, oracle submissions, pause events, and unusual match activity. This gives operators early visibility into incidents or unexpected behavior.

## Security Notes

- Steps 2 and 4 must be executed **in the same transaction or immediately after
  deployment** to eliminate the front-run window. Use a deployment script that
  batches deploy + initialize atomically where possible.
- The `deployer` address passed to `initialize` must match the account signing
  the transaction. Any mismatch will cause `require_auth` to fail.
- Once initialized, `initialize` cannot be called again (guarded by an
  `AlreadyInitialized` check).

---

## Verifying Initialization

### Automated Verification

After initialization, use the automated verification script to confirm the deployment is functioning correctly:

```bash
./scripts/verify-deployment.sh <network> <escrow_contract_id> <oracle_contract_id>
```

**Example:**
```bash
./scripts/verify-deployment.sh testnet CBQS4IYHZS5Z7LCLTTQ7RIFTDBZTUCNBCBF7STJEDSTEVEYK2QY5OXS \
  CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4
```

**What it checks:**
- ✅ Escrow contract is deployed and responds to queries
- ✅ Escrow admin address is set (not null)
- ✅ Escrow oracle address is set and points to the oracle contract
- ✅ Escrow match timeout is configured
- ✅ Escrow can list pending and active matches
- ✅ Oracle contract is deployed and responds to queries
- ✅ Oracle admin address is set (not null)

**Exit codes:**
- `0` — All checks passed; deployment is ready for use
- `1` — One or more checks failed; see error output and troubleshooting section below

The script will exit with status 1 if any check fails, making it suitable for automated CI/CD pipelines.

### Manual Verification

For granular verification or debugging, manually inspect contract state:

```bash
# Escrow: read admin
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_admin

# Escrow: read oracle address
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_oracle

# Oracle: read admin
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- get_admin

# Oracle: verify a result can be submitted (requires oracle admin auth)
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> \
  --source <ORACLE_ADMIN_KEYPAIR> \
  -- has_result_admin --match_id 0
```

---

## Canary Deployment

The `--canary` flag enables a validation gate between contract deployment and
full production traffic cut-over. In canary mode the script:

1. Deploys and initialises the contracts as normal.
2. Runs `scripts/smoke_test.sh` against the freshly deployed contracts.
3. If the smoke tests **pass**, deployment is considered successful and the
   script prints the contract addresses for traffic promotion.
4. If the smoke tests **fail**, the script exits non-zero and prints the
   contract IDs so you can inspect or roll back before any traffic is routed to
   the new deployment.

### Usage

```bash
# Deploy to testnet with canary validation
./scripts/deploy.sh testnet --canary

# Upgrade existing contracts on mainnet with canary validation
./scripts/deploy.sh mainnet --upgrade --canary
```

### What the smoke test covers

`scripts/smoke_test.sh` exercises the full match lifecycle end-to-end: it
creates a match, deposits funds for both players, submits a result, and verifies
the payout was issued. If any step fails the test exits non-zero, triggering the
canary abort path.

### When to use canary mode

| Scenario | Recommended? |
|---|---|
| First mainnet deployment | ✅ Yes — catch misconfigurations before routing users |
| Mainnet contract upgrade | ✅ Yes — verify new WASM behaves identically under live conditions |
| Testnet development iteration | Optional — slower but safer |
| Hotfix to a non-contract service | Not needed |

### Environment variables

No extra variables are required. The canary step re-uses the `CONTRACT_ESCROW`
and `CONTRACT_ORACLE` values the deploy step just produced, so both are always
set correctly.

---

## Smoke Testing: End-to-End Verification

After deployment, use the smoke test script to verify the entire match lifecycle works correctly on testnet before moving to production.

### Prerequisites

The smoke test requires:
- Two funded testnet accounts (players)
- One funded account with oracle admin privileges
- A token contract address (native XLM or other testnet token)
- `stellar` CLI and `jq` installed

### Setup

1. **Create or import keypairs** for testing:
   ```bash
   # Create test keypairs (or use existing ones)
   stellar keys generate alice
   stellar keys generate bob
   stellar keys generate oracle_admin
   
   # Fund them via Friendbot (testnet only)
   ALICE=$(stellar keys address alice)
   BOB=$(stellar keys address bob)
   ORACLE=$(stellar keys address oracle_admin)
   
   curl "https://friendbot.stellar.org?addr=$ALICE"
   curl "https://friendbot.stellar.org?addr=$BOB"
   curl "https://friendbot.stellar.org?addr=$ORACLE"
   ```

2. **Get the native XLM token address** on your network:
   ```bash
   # On testnet, retrieve the XLM contract ID
   stellar contract asset info --network testnet --asset native XLM
   # Output: ContractId: C...
   ```

3. **Set environment variables** in `.env`:
   ```bash
   export PLAYER1_KEYPAIR=alice
   export PLAYER2_KEYPAIR=bob
   export ORACLE_ADMIN_KEYPAIR=oracle_admin
   export TEST_TOKEN=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4  # XLM on testnet
   ```

### Running the Smoke Test

```bash
./scripts/smoke_test.sh testnet
```

**What it tests:**
1. ✅ Creates a match with both players and a token
2. ✅ Verifies match is in `Pending` state
3. ✅ Player 1 deposits their stake
4. ✅ Player 2 deposits their stake
5. ✅ Verifies match transitions to `Active` state
6. ✅ Verifies escrow balance = stake × 2
7. ✅ Oracle submits result (Player 1 wins)
8. ✅ Verifies escrow balance drops to 0 (payout complete)
9. ✅ Verifies match transitions to `Completed` state

**Example output:**
```
🔍 Smoke Test: Full Match Lifecycle

Validating environment...
   ✅ Required env vars present

   Player 1: GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4
   Player 2: GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBSC4
   Oracle Admin: GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCSC4

🎮 Match Configuration:
   Network: testnet
   Escrow: CEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEQZQ
   Oracle: CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF2XVA
   Stake: 100
   Game ID: smoke_test_1627584000

📋 Step 1: Create Match
   ✅ create_match (ID: 0)

📋 Step 2: Verify Match State (Pending)
   ✅ match state is Pending

📋 Step 3: Player 1 Deposits
   ✅ player 1 deposit

📋 Step 4: Player 2 Deposits
   ✅ player 2 deposit

📋 Step 5: Verify Match State (Active)
   ✅ match state is Active

📋 Step 6: Verify Escrow Balance
   ✅ escrow balance is 200 (correct)

📋 Step 7: Submit Result (Player 1 Wins)
   ✅ submit_result (Player1)

📋 Step 8: Verify Payout (Escrow Balance = 0)
   ✅ escrow balance is 0 (payout complete)

📋 Step 9: Verify Match State (Completed)
   ✅ match state is Completed

✅ Smoke Test Passed!

📊 Summary:
   Network:         testnet
   Match ID:        0
   Game ID:         smoke_test_1627584000
   Stake:           100
   Final State:     Completed
   Winner:          Player 1
   Escrow Balance:  0 (all funds disbursed)

🎉 Full lifecycle verified: create → deposit → result → payout
```

### Exit Codes

- `0` — All tests passed; deployment is verified
- `1` — One or more tests failed; check output to identify which step failed

### Troubleshooting Smoke Test Failures

**"Cannot access keypair: alice"**
- Ensure the keypair exists: `stellar keys list`
- Create it if missing: `stellar keys generate alice`
- Verify it has XLM funds: `stellar account info alice --network testnet`

**"Escrow balance is not 200"**
- Verify players have sufficient token balance
- Check that both deposits succeeded in the output
- Ensure the token contract ID is correct

**"match state is not Active"**
- Verify both player deposits completed successfully
- Check that the match exists: `stellar contract invoke --id $CONTRACT_ESCROW --network testnet -- get_match --match_id 0`

**"submit_result failed"**
- Verify oracle admin keypair has funds for fees
- Check that the oracle contract ID is correct
- Ensure oracle admin has been registered with the oracle contract

**"Escrow balance is not 0 after payout"**
- Oracle result submission may not have been processed yet
- Verify result was submitted: `stellar contract invoke --id $CONTRACT_ORACLE --network testnet -- has_result --match_id 0`
- Check match state to ensure it's `Completed`

---

## Resource Usage Baselines

Soroban charges fees based on CPU instruction count and memory bytes. The
table below shows baseline measurements captured via `env.cost_estimate().budget()`
in the test suite (SDK v22, native host — no Wasm overhead included).

| Operation       | CPU Instructions | Memory Bytes |
|-----------------|-----------------|--------------|
| `create_match`  | ~103,736        | ~18,954      |
| `deposit` (p1)  | ~242,178        | ~38,457      |
| `deposit` (p2)  | ~243,232        | ~39,134      |
| `submit_result` | ~253,053        | ~40,766      |

> **Note:** These figures reflect host-level metering only. Real on-chain costs
> will be higher once Wasm execution, VM instantiation, XDR round-trips, and
> ledger entry reads/writes are included. Use `stellar contract invoke --fee`
> on testnet for production fee estimates.

To re-run the benchmarks locally:

```bash
cargo test bench -- --nocapture
```

---

## Troubleshooting

If a deployment or initialization call fails, decode the numeric error code
from the transaction result (`Error(Contract, #N)`) using the
**[Error Codes Reference](error-codes.md)** — it documents every error
variant for both contracts, including the symptom-based quick-lookup table
for issues like "can't initialize," "deposit rejected," or "oracle can't
submit a result."

### `CONTRACT_ESCROW` (or `CONTRACT_ORACLE`) not set

**Symptom:** Scripts fail with `stellar: error: --id: empty string` or a shell
error like `Missing required argument`.

**Cause:** The environment variable was never exported, or `.env` was not
sourced before running the script.

**Fix:**
```bash
cp .env.example .env
# fill in CONTRACT_ESCROW and CONTRACT_ORACLE, then:
source .env
# or, inline for a single command:
CONTRACT_ESCROW=C... stellar contract invoke --id $CONTRACT_ESCROW -- get_admin
```

---

### Insufficient funds / fee bump required

**Symptom:** Transaction submission returns `tx_insufficient_balance` or
`op_underfunded`.

**Cause:** The source account on testnet has run out of XLM, or on mainnet the
account has insufficient XLM to cover the base reserve plus fees.

**Fix (testnet):**
```bash
# Fund the deployer account via Friendbot
curl "https://friendbot.stellar.org?addr=<DEPLOYER_ADDRESS>"
```

**Fix (mainnet):** Send additional XLM to the deployer account to cover the
base reserve (0.5 XLM per account + 0.5 XLM per ledger entry) plus estimated
transaction fees.

---

### WASM upload failure (`HostError: WasmInvalid` or file not found)

**Symptom:** `stellar contract deploy` exits with `WasmInvalid`, a file-not-found
error, or a size-limit error.

**Causes and fixes:**

| Cause | Fix |
|-------|-----|
| Contract was never built | Run `./scripts/build.sh` first |
| Wrong target path | Verify `target/wasm32-unknown-unknown/release/*.wasm` exists |
| WASM exceeds 64 KB limit | Rebuild with `--release` (debug builds are much larger) |
| Corrupted build artifact | Run `cargo clean && ./scripts/build.sh` |

---

### `AlreadyInitialized` error on `initialize`

**Symptom:** `Error(Contract, #1)` when calling `initialize`.

**Cause:** The contract was already initialized (e.g., the script was run
twice, or the contract ID belongs to a previously deployed instance).

**Fix:** You cannot re-initialize an existing contract. Either:
- use the existing deployment and skip the `initialize` step, or
- deploy a fresh contract and initialize the new instance.

---

### `require_auth` failure / deployer mismatch

**Symptom:** Transaction fails with `Error(Auth, InvalidAction)` or
`Error(Contract, #N)` during `initialize`.

**Cause:** The `--deployer` argument does not match the `--source` keypair that
signed the deployment transaction.

**Fix:** Ensure the `<DEPLOYER_ADDRESS>` passed to `--deployer` is the public
key corresponding to `<DEPLOYER_KEYPAIR>`:
```bash
stellar keys address <DEPLOYER_KEYPAIR>   # prints the address; use this as --deployer
```

---

### Oracle address rejected after escrow initialization

**Symptom:** `submit_result` returns `UnauthorizedOracle` immediately after
deployment.

**Cause:** The `--oracle` argument in step 4 was set to a wallet address
instead of the `ORACLE_CONTRACT_ID` from step 1, or the two IDs were swapped.

**Fix:** Re-deploy (or, if the contract is still fresh and no funds have been
deposited, re-initialize after a fresh deploy) ensuring `--oracle` is set to
the `ORACLE_CONTRACT_ID`:
```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <DEPLOYER_KEYPAIR> \
  -- initialize \
  --oracle $ORACLE_CONTRACT_ID \        # ← must be the oracle CONTRACT id
  --admin <ESCROW_ADMIN_ADDRESS> \
  --deployer <DEPLOYER_ADDRESS>
```

---

### Network / RPC connectivity issues

**Symptom:** CLI hangs or returns `connection refused`, `timeout`, or
`service unavailable`.

**Cause:** The RPC URL in `.env` or `environments.toml` is incorrect, the
testnet RPC is temporarily overloaded, or a local standalone node is not
running.

**Fix:**
- Verify `STELLAR_RPC_URL` in `.env` matches the target network.
- For testnet, the public endpoint is `https://soroban-testnet.stellar.org`.
- For standalone, ensure `docker compose up` (or equivalent) is running before
  deploying.
- Check the [Stellar Status page](https://status.stellar.org) for known outages.

---

## Post-Deploy Verification

After deploying or rotating the oracle service, run the dedicated oracle smoke
test to confirm that:

1. The oracle is **reachable** (health endpoint returns HTTP 200).
2. The oracle's **configured contract address** matches `CONTRACT_ESCROW`.
3. The **on-chain oracle address** stored in the escrow contract matches the
   oracle's signing key.

### Running the smoke test

```bash
# Ensure .env is populated (CONTRACT_ESCROW, ORACLE_URL, STELLAR_NETWORK).
cp .env.example .env   # if not already done
# … fill in the values …

./scripts/smoke_test_oracle.sh
```

You can also pass arguments directly to override environment variables:

```bash
./scripts/smoke_test_oracle.sh <CONTRACT_ESCROW> <ORACLE_URL> <NETWORK>
# Example:
./scripts/smoke_test_oracle.sh CABC...XYZ http://oracle.example.com:8080 testnet
```

### Environment variables

| Variable          | Description                                              | Default               |
|-------------------|----------------------------------------------------------|-----------------------|
| `CONTRACT_ESCROW` | Escrow contract ID (`C…`)                                | **required**          |
| `ORACLE_URL`      | Oracle service base URL                                  | `http://localhost:8080` |
| `STELLAR_NETWORK` | Network name from `environments.toml`                    | `testnet`             |

### Expected output (all passing)

```
══════════════════════════════════════════════════════
  Checkmate-Escrow — Oracle Smoke Test
══════════════════════════════════════════════════════
  Contract : CABC...XYZ
  Oracle   : http://oracle.example.com:8080
  Network  : testnet

▶ Pre-flight checks
  ✅ PASS  curl is available
  ✅ PASS  jq is available
  ✅ PASS  stellar is available

▶ Check 1 — Oracle health endpoint
  ✅ PASS  GET http://oracle.example.com:8080/health → HTTP 200

▶ Check 2 — Oracle reports correct escrow contract address
  ✅ PASS  Oracle contract_id matches CONTRACT_ESCROW (CABC...XYZ)

▶ Check 3 — Escrow contract's on-chain oracle address
  ✅ PASS  On-chain oracle address matches oracle signing key (G…)

══════════════════════════════════════════════════════
  ✅ PASS — All oracle smoke tests passed.
══════════════════════════════════════════════════════
```

A non-zero exit code (`❌ FAIL`) is printed for each failing check with an
actionable error message. The script exits with code `1` if any check fails,
making it safe to use in CI or deployment pipelines:

```bash
./scripts/smoke_test_oracle.sh || { echo "Oracle smoke test failed — aborting deploy."; exit 1; }
```

### Check descriptions

| Check | What it verifies | Common failure cause |
|-------|-----------------|----------------------|
| **1 – Health endpoint** | `GET /health` returns HTTP 200 | Oracle service not running or wrong `ORACLE_URL` |
| **2 – Contract address** | Oracle's `contract_id` field == `CONTRACT_ESCROW` | Oracle misconfigured with a stale contract address |
| **3 – On-chain oracle address** | `get_oracle_address` on escrow == oracle's signing key | Oracle rotated without calling `update_oracle` on-chain (or vice-versa) |

### Troubleshooting

**Check 1 fails — HTTP 000 or connection refused**

The oracle service is not reachable. Verify it is running and that `ORACLE_URL`
is correct:

```bash
docker ps | grep oracle          # Docker deployments
systemctl status oracle-service  # systemd deployments
curl -v $ORACLE_URL/health       # manual connectivity check
```

**Check 2 fails — contract address mismatch**

The oracle is pointed at a different contract than `CONTRACT_ESCROW`. Update
the oracle's `CONTRACT_ESCROW` / `ESCROW_CONTRACT_ID` environment variable and
restart the service.

**Check 3 fails — on-chain oracle address mismatch**

Either:
- The oracle was rotated on-chain (via `update_oracle`) but the service was
  not updated to use the new key, **or**
- The service was reconfigured with a new signing key but `update_oracle` was
  not called on the escrow contract.

To rotate the oracle on-chain:

```bash
stellar contract invoke \
  --id $CONTRACT_ESCROW \
  --source <ADMIN_KEYPAIR> \
  --network $STELLAR_NETWORK \
  -- update_oracle \
  --new_oracle <NEW_ORACLE_ADDRESS>
```

Then re-run `./scripts/smoke_test_oracle.sh` to confirm.
