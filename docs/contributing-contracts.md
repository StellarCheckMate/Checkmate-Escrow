# Contributing Contract Changes

This guide explains Soroban-specific patterns and conventions for contributors working on smart contract code in `contracts/escrow` and `contracts/oracle`.

## Why this guide exists

`CONTRIBUTING.md` covers general workflow and testing, but Soroban contract development also requires an understanding of:

- authorization patterns like `require_auth`
- Soroban storage tiers and state layout
- TTL behavior across ledgers
- safe contract initialization and upgrade boundaries
- event-driven indexing and off-chain expectations

Use this document whenever you are changing contract logic, storage layout, or any on-chain behavior.

## Authorization and `require_auth`

Soroban contracts must explicitly authorize every privileged action.

### Recommended pattern

- Require auth at the beginning of every entrypoint that changes state.
- Use a small helper for common checks, especially when multiple callers are allowed.
- Do not rely on `env.invoker()` alone without `env.require_auth(...)`.

Example:

```rust
fn require_player(env: &Env, player: &Address) {
    env.require_auth(player);
}

pub fn deposit(env: Env, match_id: u64) {
    let player = current_caller(&env);
    require_player(&env, &player);
    // ... state changes follow
}
```

### Common contract roles

- `player1` and `player2` — require auth from the caller when updating match state or depositing funds.
- `oracle` — require auth from the stored oracle address for result submission.
- `admin` or `owner` — require auth only for configurational or upgrade-style operations.

### Best practices

- Prefer explicit role checks over generic `if caller == ...` guards.
- Keep authorization logic centralized, so auth rules are easy to review.
- Do not assume a transaction signer is the same as the caller unless the function is explicitly authorized.

## Storage tiers and state layout

Soroban storage is not free. Store only what needs to exist on-chain and keep the layout predictable.

### Persistent vs temporary data

- Persistent storage is state that survives across ledgers and may expire if not accessed or extended.
- Temporary transaction-local storage may be useful for intermediate computation, but do not use it to store long-lived state.

### Contract storage recommendations

- Store only the canonical contract state on-chain: matches, active indices, oracle configuration, and minimal metadata.
- Avoid large or unbounded collections inside persistent storage unless you also use TTL or limits.
- Prefer lightweight keys and structured enums for storage keys.

Example storage layout:

- `DataKey::Match(match_id)` — single match record
- `DataKey::PlayerMatches(player)` — list of match IDs for a player
- `DataKey::ActiveMatches` — list of currently pending/active match IDs
- `DataKey::OracleAddress` — authorized oracle account
- `DataKey::ContractConfig` — contract-level settings and limits

### Avoiding expensive on-chain history

- Do not store every event or historical state on-chain if you can emit an event and let off-chain indexers build history.
- Use contract events for audit, query, and monitoring.
- Keep on-chain storage focused on current state and near-term state transitions.

## TTL management and ledger-aware state

Soroban storage records are not permanent forever. Each persistent write has a TTL and must be managed deliberately.

### What to keep in mind

- Persistent storage entries expire after a ledger-defined TTL if they are not written or read again.
- Indexes and long-lived records must be refreshed or rewritten when the contract updates them.
- Use ledger sequence and explicit fields for expiry-aware state.

### Practical guidance

- Write `created_ledger` and optional `completed_ledger` into match records so off-chain consumers can reason about expiration.
- When updating an index or match entry, write the same key again to refresh its TTL.
- For active match indices, refresh TTL on every state transition to prevent accidental expiration.

### Recommended TTL model for Checkmate-Escrow

- Treat `Match` records as persistent but bounded by a retention window.
- Treat `PlayerMatches` and `ActiveMatches` as indexes that are refreshed as state changes.
- Avoid assuming `get_player_matches` or `get_active_matches` will return every historical match forever.

### Failure modes to watch for

- `PlayerMatches` returns an empty list after a long period of inactivity if the key expires.
- An index entry may temporarily be stale if state changes and the index write is processed separately.
- Do not depend on expired storage as the only source of truth for match history.

## Contract initialization and upgrade safety

A contract should defend against double initialization and misconfiguration.

### Initialization patterns

- Use a dedicated `initialize` function guarded by `require_auth` and a presence check.
- Store a boolean or version marker after initialization.
- Panic or return an error if `initialize` is called a second time.

Example:

```rust
pub fn initialize(env: Env, oracle: Address, admin: Address) {
    if contract_initialized(&env) {
        panic!("Contract already initialized");
    }
    env.require_auth(&admin);
    env.storage().set(&DataKey::OracleAddress, &oracle);
    env.storage().set(&DataKey::Admin, &admin);
    env.storage().set(&DataKey::Initialized, &true);
}
```

### Upgrade-aware considerations

- Do not encode ephemeral or environment-specific values in storage keys if you expect the contract to be upgraded.
- Keep storage key variants stable and additive, not reshaped.
- Prefer new keys for new state instead of changing the meaning of existing keys.

## Events, observability, and off-chain expectations

Emitting events is the preferred way for external systems to observe contract activity.

### Why events matter

- They provide an immutable audit trail of important state transitions.
- Off-chain indexers can reconstruct history without reading every on-chain record.
- Events are easier to filter than raw storage scans.

### Event recommendations

- Emit events for match creation, deposit, cancellation, result submission, and payout.
- Include essential identifiers like `match_id`, `player1`, `player2`, `token`, and `platform`.
- Keep event payloads small but complete enough for off-chain indexing.

## Testing Soroban contract behavior

### Prefer typed `try_` tests

- Use `try_` entrypoints to assert exact error variants.
- Avoid `#[should_panic]` for normal contract errors, because it only checks for a panic and not the specific failure mode.

### Test coverage guidance

- Cover happy paths for every public entrypoint.
- Cover authorization failures and invalid-role access.
- Cover TTL-related edge cases where storage may expire or indexes may become stale.
- Test state transitions across `Pending`, `Active`, `Completed`, and `Cancelled`.

### Examples of strong contract tests

- `test_deposit_by_non_player_returns_unauthorized`
- `test_submit_result_by_non_oracle_fails`
- `test_match_record_expires_when_inactive` (for TTL behavior)
- `test_active_index_refreshes_on_state_change`

## Review checklist for contract contributions

Before opening a PR, make sure your change includes:

- [ ] explicit authorization checks for every state-changing entrypoint
- [ ] consistent use of storage keys and state layout
- [ ] TTL refresh logic for persistent records that should survive
- [ ] events for every important state transition
- [ ] typed `try_` error tests instead of `#[should_panic]`
- [ ] clear documentation in `docs/` or code comments for new storage patterns
- [ ] a short note in PR description describing any contract storage or TTL assumptions

## Where to update this guide

If you add a new Soroban contract pattern or change the storage model for `contracts/escrow` or `contracts/oracle`, update this guide.
