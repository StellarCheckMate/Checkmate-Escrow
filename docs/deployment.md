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
