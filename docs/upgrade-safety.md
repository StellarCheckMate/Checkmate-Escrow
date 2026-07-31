# Contract Upgrade Safety Guide

This document explains how to safely upgrade the Checkmate-Escrow smart
contract, what the automated tests cover, and what to check manually before
and after each upgrade.

---

## Table of Contents

1. [Overview](#overview)
2. [Upgrade Lifecycle](#upgrade-lifecycle)
3. [Automated Upgrade Tests](#automated-upgrade-tests)
4. [Running Upgrade Tests Locally](#running-upgrade-tests-locally)
5. [Upgrade Compatibility Checklist](#upgrade-compatibility-checklist)
6. [How to Perform an Upgrade](#how-to-perform-an-upgrade)
7. [Rollback Procedure](#rollback-procedure)
8. [Versioning Convention](#versioning-convention)
9. [CI Integration](#ci-integration)
10. [Common Pitfalls](#common-pitfalls)

---

## Overview

Soroban contracts are upgraded by uploading a new WASM blob and calling
`execute_upgrade`. The contract enforces a **7-day review period** between
scheduling and executing an upgrade, during which anyone can audit the new
WASM. A separate `migrate_state` function handles any storage-format changes
between versions.

Automated upgrade simulation tests guard against:

- **Breaking storage keys** — a rename or removal of a `DataKey` variant
  silently makes old data unreadable.
- **Breaking function signatures** — changing argument types or return types
  breaks existing integrations without a compile-time error in the contract
  itself.
- **Fee regression** — a bug in fee calculation causes winners to receive the
  wrong payout after an upgrade.
- **Data corruption** — migration code that overwrites existing values
  instead of back-filling absent ones.
- **Version downgrade** — a `migrate_state` call that lowers the version
  counter, making it impossible to detect which migrations have run.

---

## Upgrade Lifecycle

```
schedule_upgrade(wasm_hash)
        │
        │  7-day review period (UPGRADE_REVIEW_PERIOD_LEDGERS = 120,960)
        ▼
pause() the contract                ← admin must pause before executing
        │
        ▼
execute_upgrade()                   ← uploads new WASM, contract is live
        │
        ▼
migrate_state(target_version)       ← runs data migrations, updates version
        │
        ▼
validate_state()                    ← confirms instance store is healthy
        │
        ▼
unpause()                           ← re-opens the contract to users
```

---

## Automated Upgrade Tests

The test file `contracts/escrow/tests/upgrade_simulation_tests.rs` contains
the following categories of tests:

### State Preservation

| Test | What it verifies |
|---|---|
| `test_admin_preserved_across_migration` | Admin address unchanged after `migrate_state` |
| `test_oracle_preserved_across_migration` | Oracle address unchanged after `migrate_state` |
| `test_pause_state_preserved_across_migration` | Paused flag unchanged |
| `test_match_timeout_preserved_across_migration` | Custom timeout unchanged |
| `test_protocol_config_preserved_across_migration` | Vesting, fees, treasury unchanged |

### Old Match Accessibility

| Test | What it verifies |
|---|---|
| `test_pending_match_readable_after_migration` | Pending match state and fields intact |
| `test_active_match_readable_after_migration` | Active match state and deposit flags intact |
| `test_escrow_balance_preserved_after_migration` | `get_escrow_balance` returns same value |
| `test_is_funded_flag_preserved_after_migration` | `is_funded` stays `true` for funded matches |

### Oracle Results After Migration

| Test | What it verifies |
|---|---|
| `test_oracle_can_submit_result_after_migration` | Oracle can finalize an active match post-migration |
| `test_draw_payout_correct_after_migration` | Draw refunds both players the correct amount |

### Function-Signature Compatibility

| Test | What it verifies |
|---|---|
| `test_create_match_api_compatible_after_migration` | `create_match` accepts same args |
| `test_deposit_api_compatible_after_migration` | `deposit` works on post-migration matches |
| `test_cancel_match_api_compatible_after_migration` | `cancel_match` still transitions to Cancelled |
| `test_get_player_matches_api_compatible_after_migration` | `get_player_matches` returns consistent data |

### Fee Correctness

| Test | What it verifies |
|---|---|
| `test_fee_tiers_preserved_and_applied_after_migration` | Fee tiers set before migration produce the correct treasury transfer and winner payout after migration |

### Storage-Key Stability

| Test | What it verifies |
|---|---|
| `test_validate_state_passes_before_and_after_migration` | `validate_state` finds all required keys |
| `test_version_storage_key_increments_correctly` | `ContractVersion` key is readable and updates |

### Upgrade Guard Enforcement

| Test | What it verifies |
|---|---|
| `test_execute_upgrade_rejected_when_not_paused` | `InvalidPauseState` if not paused |
| `test_execute_upgrade_rejected_before_review_period` | `UpgradeReviewPeriodNotElapsed` if too early |
| `test_execute_upgrade_rejected_with_no_scheduled_upgrade` | `UpgradeNotScheduled` if none pending |

### Rollback Safety

| Test | What it verifies |
|---|---|
| `test_cancel_upgrade_allows_reschedule` | `cancel_upgrade` → `schedule_upgrade` succeeds |
| `test_contract_operational_after_cancelled_upgrade` | Matches work after a cancelled upgrade |

### Version Monotonicity

| Test | What it verifies |
|---|---|
| `test_migrate_state_rejects_same_version` | `InvalidVersion` for same version |
| `test_migrate_state_rejects_downgrade` | `InvalidVersion` for lower version |
| `test_migrate_state_double_migration_rejected` | Re-running a migration is blocked |

### Concurrent-Match Safety

| Test | What it verifies |
|---|---|
| `test_multiple_matches_survive_migration` | Pending + Active matches all intact post-migration |
| `test_new_match_completes_normally_after_migration` | New matches created after migration complete correctly |

---

## Running Upgrade Tests Locally

```bash
# Run only the upgrade simulation tests
cargo test -p escrow --test upgrade_simulation_tests

# Run with output for debugging
cargo test -p escrow --test upgrade_simulation_tests -- --nocapture

# Run a single test
cargo test -p escrow --test upgrade_simulation_tests test_fee_tiers_preserved -- --nocapture
```

The tests use only in-memory Soroban environments (`Env::default()`); no
network connection or deployed contract is needed.

---

## Upgrade Compatibility Checklist

Before merging any contract change, verify all items below. The automated
tests cover most of these; items marked ✋ require manual review.

### Storage Keys

- [ ] No existing `DataKey` variant was renamed or removed.
- [ ] If a new `DataKey` variant was added, `validate_state` was updated to
      check for it when required.
- [ ] ✋ Any new persistent key has a sensible default that `migrate_state`
      back-fills for contracts that pre-date the key.

### Function Signatures

- [ ] No existing public function had argument types changed.
- [ ] No existing public function had its return type changed.
- [ ] ✋ Any new public function is additive (not a replacement of an existing
      one without a deprecation period).

### Versioning

- [ ] `CONTRACT_VERSION` in `src/lib.rs` was incremented.
- [ ] A new arm in `migrate_state` handles the version step from the previous
      version to the new one.
- [ ] The `match` in `migrate_state` includes a `_ => {}` or explicit catch-all
      that does not panic on unknown future versions.

### Fee Tiers

- [ ] Fee calculation logic is unchanged, or any change is intentional and
      the fee test was updated to reflect the new expected amounts.

### Events

- [ ] All events listed in the README Events Reference table still use the
      same topic strings.

### ✋ Manual Pre-Upgrade Steps (production)

1. Take a backup: `./scripts/backup_state.sh mainnet`
2. Review the new WASM diff with a second engineer.
3. Publish the WASM hash publicly (GitHub release or on-chain memo) for the
   7-day review period.
4. Announce the upcoming upgrade to users.

---

## How to Perform an Upgrade

```bash
# 1. Build the new WASM
./scripts/build.sh

# 2. Upload the WASM and get its hash
WASM_HASH=$(stellar contract upload \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet)
echo "WASM hash: $WASM_HASH"

# 3. Schedule the upgrade (starts the 7-day review clock)
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- schedule_upgrade --wasm_hash "$WASM_HASH"

# 4. Wait for the review period to elapse (7 days = 120,960 ledgers)

# 5. Pause the contract
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- pause

# 6. Execute the upgrade
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- execute_upgrade

# 7. Run state migration
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- migrate_state --target_version <new_version_number>

# 8. Validate state
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- validate_state

# 9. Unpause the contract
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- unpause

echo "✅ Upgrade complete."
```

---

## Rollback Procedure

If the upgrade is scheduled but has NOT yet been executed, cancel it:

```bash
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- cancel_upgrade
stellar contract invoke \
    --id $CONTRACT_ESCROW \
    --source $DEPLOYER_KEYPAIR \
    --network mainnet \
    -- unpause
```

If the upgrade HAS been executed and is broken, there is no on-chain rollback
— Soroban does not support downgrading WASM. Options:

1. Upload a hot-fix WASM and schedule a new upgrade (requires another 7-day
   review period unless the contract is already broken in a way that warrants
   emergency governance).
2. Deploy a new contract and migrate users — follow the
   [Disaster Recovery](disaster-recovery.md) runbook.

---

## Versioning Convention

The contract version is encoded as `major * 1_000_000 + minor * 1_000 + patch`:

| Version string | Encoded integer |
|---|---|
| 0.1.0 | 1,000 |
| 0.1.1 | 1,001 |
| 0.2.0 | 2,000 |
| 1.0.0 | 1,000,000 |

The current version is defined in `src/lib.rs`:

```rust
pub const CONTRACT_VERSION: u32 = 1_000; // 0.1.0
```

Increment the appropriate component and update the constant before opening a
PR that changes on-chain behavior or storage layout.

---

## CI Integration

The upgrade simulation tests are part of the standard CI pipeline defined in
`.github/workflows/ci.yml`. They run on every push and pull request to
`main`/`master` as part of the `test` job:

```yaml
- name: Run upgrade simulation tests
  run: cargo test -p escrow --test upgrade_simulation_tests
```

This ensures no merge to `main` can introduce a breaking upgrade path without
first failing CI.

---

## Common Pitfalls

**Renaming a DataKey variant**
Rust enum variants are not stable identifiers in XDR encoding; renaming
`DataKey::Foo` to `DataKey::Bar` writes data under a different storage key,
making all existing `Foo` data invisible. Never rename a variant — add a new
one and write a migration.

**Removing a DataKey variant**
Same effect as renaming. Keep removed variants as dead code or use a
`#[deprecated]` annotation until all data under that key has been migrated.

**Changing a contracttype field**
Adding a field to a `#[contracttype]` struct changes its XDR encoding.
Existing stored values will fail to deserialize. Always add fields as
`Option<T>` and back-fill them in `migrate_state`.

**Skipping validate_state after migration**
`validate_state` is the only automated check that the migration completed
cleanly. Always call it before unpausing the contract.

**Forgetting to increment CONTRACT_VERSION**
Without a version bump, `migrate_state` cannot distinguish a fresh contract
from one that ran the migration. Always bump the version with every
storage-layout change.
