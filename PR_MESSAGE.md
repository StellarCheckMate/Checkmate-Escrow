# Event Emissions & Public Getters for Escrow & Oracle Contracts

## Overview
This PR addresses four issues related to event emissions and public getters across the escrow and oracle contracts. Each issue is implemented as a separate, focused commit for clarity and reviewability.

## Changes

### 1. **fix(#341): Add get_match_timeout getter to escrow contract**
- **Issue**: `DataKey::MatchTimeout` was used internally by `expire_match` but had no public getter
- **Changes**:
  - Added `MatchTimeout` variant to `DataKey` enum in `types.rs`
  - Implemented `pub fn get_match_timeout(env: Env) -> u32` that returns the stored value or `MATCH_TTL_LEDGERS` as default
  - Added test `test_get_match_timeout_returns_default()` verifying it returns the default value before any admin sets it

### 2. **fix(#339): Add initialize event to escrow contract**
- **Issue**: `EscrowContract::initialize` set oracle and admin silently with no on-chain event
- **Changes**:
  - Added event emission in `initialize()`: `env.events().publish(("escrow", "initialized"), (oracle, admin))`
  - Added test `test_initialize_emits_event()` asserting the event is emitted with correct oracle and admin addresses

### 3. **fix(#338): Add initialize event to oracle contract**
- **Issue**: `OracleContract::initialize` set admin silently with no on-chain event
- **Changes**:
  - Added event emission in `initialize()`: `env.events().publish(("oracle", "initialized"), admin)`
  - Added test `test_initialize_emits_event()` asserting the event is emitted with correct admin address

### 4. **test(#340): Add test verifying oracle unpause emits no event**
- **Issue**: `OracleContract::unpause` did not emit an event unlike the escrow contract's unpause. A test should document whether this is intentional.
- **Changes**:
  - Added `pub fn unpause(env: Env) -> Result<(), Error>` to oracle contract (admin-only, no event emission)
  - Added test `test_unpause_emits_no_event()` asserting unpause does not emit any events, documenting this as intentional behavior

## Commits
```
28a1d81 test(#340): Add test verifying oracle unpause emits no event
4d2f56f fix(#338): Add initialize event to oracle contract
d8a839b fix(#339): Add initialize event to escrow contract
02854d2 fix(#341): Add get_match_timeout getter to escrow contract
```

## Testing
All changes include corresponding tests:
- `test_get_match_timeout_returns_default()` - Verifies default timeout getter
- `test_initialize_emits_event()` (escrow) - Verifies escrow initialization event
- `test_initialize_emits_event()` (oracle) - Verifies oracle initialization event
- `test_unpause_emits_no_event()` - Verifies oracle unpause intentionally emits no event

## Impact
- **Escrow Contract**: New public getter for match timeout; initialization now emits event
- **Oracle Contract**: Initialization now emits event; new unpause function with documented no-event behavior
- **Backwards Compatibility**: All changes are additive; no breaking changes to existing functionality
