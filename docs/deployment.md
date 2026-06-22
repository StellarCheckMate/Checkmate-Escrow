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

## Balance Snapshot Storage Overhead

`EscrowContract` records a `BalanceSnapshot` (match id, lifecycle reason,
ledger sequence, token address + symbol, stake amount, escrow balance, and
both players' deposit flags) at four lifecycle points: match creation, each
deposit, completion, and cancellation (including timeout-driven expiry).

Snapshots are stored per-match in a fixed-size ring buffer
(`MAX_SNAPSHOTS_PER_MATCH = 8` in `contracts/escrow/src/lib.rs`) under
`DataKey::Snapshot(match_id, slot)`, with `DataKey::SnapshotCount(match_id)`
tracking the total ever recorded. Once a match's snapshot count exceeds the
buffer capacity, the oldest entry is overwritten — this caps growth so a
single match can never accumulate unbounded snapshot history regardless of
how many lifecycle transitions it goes through.

**Estimated overhead per match**, based on the `BalanceSnapshot` field
layout (two `Address`/`String` fields plus four numeric/bool fields, each XDR
ledger-entry holding one snapshot):

| Quantity                                   | Approx. size      |
|---------------------------------------------|-------------------|
| One `BalanceSnapshot` entry                  | ~200–250 bytes    |
| `SnapshotCount` entry (once per match)       | ~40 bytes         |
| Typical lifecycle (created + 2 deposits + completed/cancelled = 4 entries) | ~1.0 KB |
| Worst case at ring-buffer cap (8 entries)    | ~2.0 KB           |

These are conservative engineering estimates, not measured fee figures —
confirm exact ledger-entry sizes with `stellar contract invoke --fee` on
testnet before relying on them for capacity planning. All snapshot entries
share the same TTL behavior as their parent match (`MATCH_TTL_LEDGERS`,
refreshed on every write), so they expire alongside it.
