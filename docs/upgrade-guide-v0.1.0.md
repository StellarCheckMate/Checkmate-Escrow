# Upgrade Guide: v0.0.x → v0.1.0

This guide explains how to safely upgrade a deployed Checkmate-Escrow contract
from any **v0.0.x** release to **v0.1.0**.

For a general reference on the upgrade mechanism, the automated test suite, and
the rollback procedure, see [docs/upgrade-safety.md](upgrade-safety.md).

---

## Table of Contents

1. [What changed in v0.1.0](#what-changed-in-v010)
2. [Breaking ABI changes](#breaking-abi-changes)
3. [New storage keys in v0.1.0](#new-storage-keys-in-v010)
4. [Pre-upgrade checklist](#pre-upgrade-checklist)
5. [Step-by-step upgrade procedure](#step-by-step-upgrade-procedure)
6. [Post-upgrade verification](#post-upgrade-verification)
7. [The `InvalidVersion` error](#the-invalidversion-error)
8. [Rollback](#rollback)

---

## What changed in v0.1.0

v0.1.0 is the first **stable contract release**. It introduces several features
that require new storage keys and, in two cases, changes to function signatures
that break backwards-compatibility with v0.0.x off-chain clients.

### New features

| Feature | Description |
|---|---|
| Protocol fee | `ProtocolConfig.protocol_fee_bps` + `fee_recipient`: optional platform fee deducted from winner payouts (draw refunds exempt) |
| Maximum stake cap | `ProtocolConfig.maximum_stake`: optional upper bound on `stake_amount` per match |
| Stablecoin-only mode | `ProtocolConfig.stablecoin_only_mode` + `StablecoinIssuer` key: gate new matches to issuer-verified tokens |
| Token allowlist enforcement | `AllowlistEnforced` flag and `AllowedTokens` list: when at least one token is added via `add_allowed_token`, only listed tokens are accepted |
| Token blacklist | `BlacklistedToken` and `BlacklistedTokens` keys: block specific tokens from future matches |
| Player freeze | `admin_freeze_player` / `admin_unfreeze_player`: per-player block without a global pause |
| Admin stall resolution | `admin_resolve_stalled_match`: recover funds from Active matches stalled > 7 days with no oracle result |
| Batch result submission | `submit_result_batch`: settle multiple matches in a single transaction |
| `get_contract_version` | New view function returning the semver version string |
| `ContractVersion` storage key | Monotonically increasing version integer used to gate `migrate_state` |
| Oracle consensus | `m-of-n` multi-oracle confirmation before payout; `OracleConfirmations` and `OracleVote` keys |
| `DepositInProgress` guard | Reentrancy guard on `deposit` using a temporary storage key |

---

## Breaking ABI changes

The following changes will break any v0.0.x off-chain client, script, or
integration that calls the affected functions.

### 1. `set_protocol_config` — new fields in `ProtocolConfig`

v0.0.x `ProtocolConfig`:
```rust
ProtocolConfig {
    vesting_duration_seconds: u64,
    cancellation_fee_basis_points: u32,
    treasury: Address,
    minimum_stake: i128,
    stablecoin_only_mode: bool,
    maximum_stake: Option<i128>,
    match_timeout_seconds: u64,
}
```

v0.1.0 `ProtocolConfig` (new fields **bolded**):
```rust
ProtocolConfig {
    vesting_duration_seconds: u64,
    cancellation_fee_basis_points: u32,
    treasury: Address,
    minimum_stake: i128,
    stablecoin_only_mode: bool,
    maximum_stake: Option<i128>,
    match_timeout_seconds: u64,
    protocol_fee_bps: u32,         // NEW — basis points fee on winner payout (0 = disabled)
    fee_recipient: Address,        // NEW — recipient of protocol fee transfers
    max_protocol_fee: Option<i128>, // NEW — hard ceiling on the absolute fee amount
}
```

**Migration action:** Update all `set_protocol_config` call sites to include
the three new fields. For deployments that do not want a protocol fee, pass:
```
protocol_fee_bps = 0
fee_recipient = <admin address>
max_protocol_fee = None
```

### 2. `initialize` — unchanged signature, but `ProtocolConfig` defaults differ

The `initialize` function signature itself did not change, but the default
`ProtocolConfig` written during initialization now includes the new fields
described above. No action needed unless you are re-initialising a fresh
contract from scripts.

### 3. `submit_result_batch` — new function (additive, not breaking)

`submit_result_batch` is new in v0.1.0. It is not a replacement for
`submit_result`; both exist simultaneously. No migration action required.

### 4. `admin_freeze_player` error reuse

`admin_freeze_player` returns `Error::ContractPaused` (code `#9`) when a
frozen player attempts to create a match or deposit. This reuses an existing
error code rather than introducing a new one (the XDR enum is at its 50-variant
cap). Off-chain clients that previously interpreted `Error(Contract, #9)` as
"the contract is globally paused" should be updated to also handle the
per-player freeze case. Check `is_player_frozen(player)` to distinguish the
two scenarios.

### 5. `get_oracle_address` renamed from `get_oracle`

In v0.0.x the oracle read function was named `get_oracle`. In v0.1.0 it is
`get_oracle_address`. Update any script or integration that calls `get_oracle`.

---

## New storage keys in v0.1.0

The following `DataKey` variants are new in v0.1.0. They do not exist in
v0.0.x storage. `migrate_state` back-fills their defaults for in-place
upgrades (see [Step-by-step upgrade procedure](#step-by-step-upgrade-procedure)).

| DataKey variant | Storage scope | Default value after migration |
|---|---|---|
| `ContractVersion` | Instance | `1000` (encodes `0.1.0`) |
| `AllowlistEnforced` | Instance | `false` |
| `AllowedTokens` | Instance | `[]` (empty Vec) |
| `AllowedTokenCount` | Instance | `0` |
| `BlacklistedToken(addr)` | Instance | _(not back-filled; absent = not blacklisted)_ |
| `BlacklistedTokens` | Instance | `[]` (empty Vec) |
| `StablecoinIssuer(addr)` | Instance | _(not back-filled; absent = not registered)_ |
| `StablecoinIssuerCount` | Instance | `0` |
| `Stats` | Instance | `PlatformStats { total_matches: 0, total_volume: 0, total_payouts: 0 }` |
| `OracleConfirmations(match_id)` | Persistent | _(not back-filled; read as `0` by default)_ |
| `OracleVote(match_id, oracle)` | Persistent | _(not back-filled; absent = no vote)_ |
| `ApprovedOracles` | Instance | `[]` (empty Vec — single-oracle mode by default) |
| `RequiredOracleConfirmations` | Instance | `1` (single-oracle mode) |
| `OracleRotation` | Instance | _(not back-filled; initialised on first rotation)_ |
| `FeeTiers` | Instance | `[]` (no fee tiers) |
| `ReferralShareBasisPoints` | Instance | `0` |
| `PlayerActiveMatchCount(addr)` | Persistent | _(not back-filled; counted from existing active matches by migration)_ |
| `PlayerCompletedMatchCount(addr)` | Persistent | _(not back-filled; read as `0` by default)_ |
| `DisputeOracle(match_id)` | Persistent | _(not back-filled)_ |
| `DepositInProgress(match_id)` | Temporary | _(not back-filled; set/cleared transiently)_ |
| `PendingAdmin` | Instance | _(not back-filled; absent = no pending transfer)_ |

### Updated `ProtocolConfig` fields

The existing `ProtocolConfig` value stored at the `ProtocolConfig` instance key
is extended in v0.1.0. `migrate_state` reads the existing struct and writes it
back with the new fields set to their zero defaults:

- `protocol_fee_bps = 0`
- `fee_recipient = <admin address>`
- `max_protocol_fee = None`

If you want non-zero defaults after migration, call `set_protocol_config` after
`migrate_state` and before `unpause`.

---

## Pre-upgrade checklist

Complete all of these items before scheduling the upgrade on-chain.

- [ ] Back up current contract state: `./scripts/backup_state.sh mainnet`
- [ ] Build and audit the v0.1.0 WASM: `./scripts/build.sh`
- [ ] Run the upgrade simulation test suite:
      `cargo test -p escrow --test upgrade_simulation_tests`
- [ ] Update all off-chain clients to handle the new `ProtocolConfig` fields
      (see [Breaking ABI changes](#breaking-abi-changes))
- [ ] Update any script or tool that calls `get_oracle` to call
      `get_oracle_address` instead
- [ ] Decide on initial `protocol_fee_bps` and `fee_recipient` values
- [ ] Communicate the upcoming upgrade and 7-day review window to users
- [ ] Publish the new WASM hash publicly (GitHub release, on-chain memo, or
      Discord announcement) so the community can audit during the review period

---

## Step-by-step upgrade procedure

Replace `$CONTRACT_ESCROW`, `$DEPLOYER_KEYPAIR`, and `$ADMIN_KEYPAIR` with
the values from your deployment environment.

```bash
# 1. Build the v0.1.0 WASM
./scripts/build.sh

# 2. Upload the new WASM to the network and capture its hash
WASM_HASH=$(stellar contract upload \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet)
echo "New WASM hash: $WASM_HASH"

# 3. Schedule the upgrade (starts the 7-day review clock)
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- schedule_upgrade --wasm_hash "$WASM_HASH"

# ── Wait 7 days (120,960 ledgers) ──────────────────────────────────────────

# 4. Pause the contract (required before execute_upgrade)
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- pause

# 5. Execute the upgrade (replaces the on-chain WASM)
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- execute_upgrade

# 6. Run state migration for v0.1.0
#    target_version = 1000 (encodes 0.1.0: minor=1 → 1*1000 = 1000)
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- migrate_state --target_version 1000

# 7. (Optional) configure non-default protocol fee
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- set_protocol_config \
    --config '{
        "protocol_fee_bps": 50,
        "fee_recipient": "<TREASURY_ADDRESS>",
        "max_protocol_fee": null,
        ... <other existing fields unchanged>
    }'

# 8. Validate state — confirms all required storage keys are present
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- validate_state

# 9. Confirm the version
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --network mainnet \
    -- get_contract_version
# Expected output: "0.1.0"

# 10. Unpause the contract
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- unpause

echo "✅ Upgrade to v0.1.0 complete."
```

---

## Post-upgrade verification

Run each of the following after unpausing to confirm the contract is healthy.

```bash
# Verify version
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    -- get_contract_version
# Expected: "0.1.0"

# Verify oracle address is unchanged
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    -- get_oracle_address

# Verify protocol config was migrated correctly
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    -- get_protocol_config

# Confirm an existing Active match is still readable
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    -- get_match --match_id <EXISTING_ACTIVE_MATCH_ID>

# Confirm a new match can be created
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    --source <PLAYER1_KEYPAIR> \
    -- create_match \
    --player1 <PLAYER1_ADDRESS> \
    --player2 <PLAYER2_ADDRESS> \
    --stake_amount 100 \
    --token <TOKEN_ADDRESS> \
    --game_id "testupgrade1" \
    --platform Lichess

# Run the automated smoke test
./scripts/smoke_test.sh mainnet
```

---

## The `InvalidVersion` error

`migrate_state` enforces version monotonicity. Calling it incorrectly will
produce `Error(Contract, #49)` (`InvalidVersion`).

| Scenario | Result |
|---|---|
| `target_version` equals the current stored version | `InvalidVersion` |
| `target_version` is lower than the current stored version | `InvalidVersion` |
| `migrate_state` called a second time with the same version | `InvalidVersion` |
| `target_version` is higher than the current stored version by more than one step | Succeeds — migration arms for all intermediate steps run in order |

If you see `InvalidVersion`:

```bash
# Check the current on-chain version
stellar contract invoke --id $CONTRACT_ESCROW --network mainnet \
    -- get_contract_version

# Ensure target_version=1000 is correct for 0.1.0
# The version encoding is: major*1_000_000 + minor*1_000 + patch
#   0.1.0 → 0*1_000_000 + 1*1_000 + 0 = 1000
```

Common cause: `migrate_state` was called before `execute_upgrade` completed,
so the contract is still running v0.0.x code which does not know about version
1000.

---

## Rollback

### If the upgrade has been scheduled but not yet executed

Cancel the upgrade during the review period:

```bash
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- cancel_upgrade

stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $ADMIN_KEYPAIR \
    --network mainnet \
    -- unpause
```

The contract resumes running v0.0.x as if nothing happened.

### If the upgrade has been executed but `migrate_state` has not yet run

The contract is now running v0.1.0 WASM but storage is still in v0.0.x format.
**Do not unpause** until `migrate_state` completes. If you need more time,
keep the contract paused. There is no way to roll back the WASM to v0.0.x
without a second upgrade cycle.

### If `migrate_state` has run and issues are discovered post-upgrade

There is no on-chain rollback path for Soroban WASM. Options:

1. **Hot-fix release**: prepare a v0.1.1 patch, schedule a new upgrade, and
   execute after the review period.
2. **Emergency governance**: if a critical bug is found, the admin can pause
   the contract to prevent further damage while a fix is prepared.
3. **New deployment**: as a last resort, deploy a fresh v0.1.1 contract and
   migrate users. Follow the [Disaster Recovery](disaster-recovery.md) runbook.

Restore a pre-upgrade state backup:

```bash
# Inspect the backup taken before the upgrade
./scripts/restore_state.sh mainnet --dry-run

# Apply (only useful for off-chain indexer state; on-chain state is immutable)
./scripts/restore_state.sh mainnet
```
