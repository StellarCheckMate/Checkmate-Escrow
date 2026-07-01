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

After initialization, confirm the stored admin and oracle addresses:

```bash
# Escrow: read admin
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_admin

# Oracle: verify a result can be submitted (requires oracle admin auth)
stellar contract invoke --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  -- has_result_admin --match_id 0
```

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
