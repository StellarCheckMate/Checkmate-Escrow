# Storage Layout Reference

This document is the authoritative reference for every `DataKey` variant used by the Escrow contract. It maps each key to its value type, storage scope, TTL behaviour, and purpose. It is intended for upgrade engineers, tooling authors, and anyone auditing contract storage safety.

## Storage Scopes

Soroban provides three storage scopes:

| Scope | Eviction | Typical use |
|---|---|---|
| **Instance** | Expires with contract instance; extended on every contract invocation | Small, infrequently changed global config (oracle, admin, flags) |
| **Persistent** | Independent TTL per key; must be explicitly extended by contract code | Per-match data, player indexes, balance snapshots |
| **Temporary** | Automatically evicted after TTL; not recoverable | Reentrancy guards, short-lived flags |

## TTL Constants

All TTL values are expressed in **ledger sequence numbers** (5 s/ledger on Stellar mainnet).

| Constant | Ledgers | Wall-clock (approx.) |
|---|---|---|
| `MATCH_TTL_LEDGERS` | 518 400 | ~30 days |
| `MIN_MATCH_TIMEOUT_LEDGERS` | 17 280 | 1 day |
| `DEFAULT_MATCH_TIMEOUT_LEDGERS` | 518 400 | ~30 days |
| `MAX_MATCH_TIMEOUT_LEDGERS` | 1 555 200 | ~90 days |

> Instance storage TTL is extended to at least `MATCH_TTL_LEDGERS` on every contract invocation by the Soroban runtime's instance-bump logic.

---

## DataKey Variant Reference

> Source: [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs)

### Core Contract Config

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `Oracle` | `Address` | Instance | Bumped on every invocation | Currently configured oracle address. Set by `initialize` and `update_oracle`. |
| `Admin` | `Address` | Instance | Bumped on every invocation | Contract administrator. Set by `initialize` and `transfer_admin`. |
| `PendingAdmin` | `Address` | Instance | Bumped on every invocation | Two-step admin transfer: proposed new admin. Cleared when accepted or rejected. |
| `Paused` | `bool` | Instance | Bumped on every invocation | Global pause flag. When `true`, all state-mutating functions are blocked. |
| `ProtocolConfig` | `ProtocolConfig` | Instance | Bumped on every invocation | Full protocol configuration struct: fee, stake limits, timeout, treasury, vesting, etc. |
| `ContractVersion` | `u32` | Instance | Bumped on every invocation | Monotonically increasing version counter. Used by `migrate_state` to enforce upgrade ordering. |

### Match Data

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `Match(u64)` | `Match` | Persistent | Extended to `MATCH_TTL_LEDGERS` on every write | Full match record for a given match ID (state, players, deposits, winner, stake, etc.). |
| `MatchCount` | `u64` | Instance | Bumped on every invocation | Auto-incrementing match ID counter. Next match ID = `MatchCount + 1`. |
| `GameId(String)` | `()` (unit) | Persistent | Extended to `MATCH_TTL_LEDGERS` on write | Presence key preventing duplicate game IDs. Set on `create_match`; never deleted. |
| `OracleRecord(u64)` | `String` | Persistent | Extended on write | Stores the `game_id` submitted by the oracle via `submit_result_with_oracle_record` for audit. |

### Player Indexes

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `PlayerMatches(Address)` | `Vec<u64>` | Persistent | Extended to `MATCH_TTL_LEDGERS` on write | Append-only list of match IDs for a player. Updated on `create_match`. Never pruned. |
| `ActiveMatch(Address, u64)` | `()` (unit) | Persistent | Extended on write | O(1) lookup for whether a specific match is currently active for a player. Cleared on completion or cancellation. |
| `PlayerActiveMatchCount(Address)` | `u32` | Persistent | Extended on write | Count of currently active matches for a player. Capped at `MAX_ACTIVE_MATCHES_PER_PLAYER`. |
| `PlayerCompletedMatchCount(Address)` | `u32` | Persistent | Extended on write | Running count of completed matches for a player. Used for dispute vote eligibility. |

### Token Allowlist

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `AllowedToken(Address)` | `bool` | Instance | Bumped on every invocation | Presence flag for an allowlisted token address. |
| `AllowedTokenCount` | `u32` | Instance | Bumped on every invocation | Total count of allowlisted tokens. Used to determine whether the allowlist is enforced. |
| `AllowlistEnforced` | `bool` | Instance | Bumped on every invocation | `true` when at least one token has been added via `add_allowed_token`. New matches are restricted to listed tokens only. |
| `AllowedTokens` | `Vec<Address>` | Instance | Bumped on every invocation | Cached list of all allowlisted token addresses (for enumeration). |
| `BlacklistedToken(Address)` | `bool` | Instance | Bumped on every invocation | Presence flag for a token that is explicitly blocked from new matches. |
| `BlacklistedTokens` | `Vec<Address>` | Instance | Bumped on every invocation | Cached list of all blacklisted token addresses. |

### Balance Snapshots

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `Snapshot(u64, u32)` | `BalanceSnapshot` | Persistent | Extended on write | Per-match ring-buffer slot: `(match_id, slot)` where `slot = index % MAX_SNAPSHOTS_PER_MATCH (8)`. Oldest entry is silently overwritten when the buffer fills. |
| `SnapshotCount(u64)` | `u64` | Persistent | Extended on write | Monotonically increasing counter of total snapshots ever taken for a match. Never reset. |
| `PlayerBalanceSnapshot(Address, u64)` | `BalanceAtTimestamp` | Persistent | Extended on write | Per-player ring-buffer slot: `(player, index % MAX_PLAYER_SNAPSHOTS (32))`. Point-in-time aggregate escrow balance across all deposit-eligible, non-terminal matches. |
| `PlayerBalanceSnapshotCount(Address)` | `u64` | Persistent | Extended on write | Monotonically increasing counter of total player balance snapshots. Never reset. |

### Dispute System

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `DisputePeriod` | `u64` | Instance | Bumped on every invocation | Dispute window in ledger blocks after result submission. `0` = immediate payout with no dispute phase. |
| `PendingWinner(u64)` | `Winner` | Persistent | Extended on write | Stores the oracle-submitted winner for a match that is in the `PendingResult` dispute window. Cleared on finalization. |
| `ResultDeadline(u64)` | `u64` | Persistent | Extended on write | Ledger sequence number after which the pending result is automatically finalized without dispute. |
| `Dispute(u64)` | `Dispute` | Persistent | Extended on write | Full dispute record for a match: disputer, bond, state, vote tallies. |
| `DisputeVote(u64, Address)` | `Winner` | Persistent | Extended on write | Vote cast by a specific `voter` address on `match_id`'s dispute. |
| `DisputeVoteWeight(u64, Address)` | `u64` | Persistent | Extended on write | Snapshot of a voter's vote weight at dispute-creation time, used to prevent manipulation during voting. |
| `DisputeCount` | `u64` | Instance | Bumped on every invocation | Global running count of disputes ever opened. |
| `MatchDispute(u64)` | `u64` | Persistent | Extended on write | Maps a `match_id` to its associated `dispute_id`. |
| `DisputeOracle(u64)` | `Address` | Persistent | Extended on write | Oracle address implicated by a dispute result; used for automatic oracle slashing. |
| `DisputeBondBasisPoints` | `u32` | Instance | Bumped on every invocation | Minimum bond as basis points of match stake required to open a dispute. |
| `MinimumHoldDuration` | `u64` | Instance | Bumped on every invocation | Minimum ledger hold duration required for a voter to be eligible. |
| `QuorumBasisPoints` | `u32` | Instance | Bumped on every invocation | Quorum threshold as percentage of dispute snapshot weight (in basis points). |

### Upgrade System

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `PendingUpgradeHash` | `BytesN<32>` | Instance | Bumped on every invocation | WASM hash of the scheduled upgrade. Set by `schedule_upgrade`; cleared by `cancel_upgrade` or `execute_upgrade`. |
| `UpgradeScheduledAt` | `u64` | Instance | Bumped on every invocation | Ledger sequence at which the upgrade was scheduled. Used to enforce `UPGRADE_REVIEW_PERIOD_LEDGERS`. |

### Multi-Oracle Consensus

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `OracleConfirmations(u64)` | `u32` | Persistent | Extended on write | Running count of oracle confirmations received for a match. Payout triggers when this reaches `RequiredOracleConfirmations`. |
| `OracleVote(u64, Address)` | `Winner` | Persistent | Extended on write | Records which `Winner` a specific oracle submitted for a match. Prevents double-voting; detects conflicting votes. |
| `ApprovedOracles` | `Vec<Address>` | Instance | Bumped on every invocation | List of oracle addresses participating in consensus mode. |
| `RequiredOracleConfirmations` | `u32` | Instance | Bumped on every invocation | Threshold of confirmations required before a match result is finalized in consensus mode. |
| `OracleRotation` | `OracleRotationState` | Instance | Bumped on every invocation | Combined temp + pending oracle rotation state used during oracle handoff to prevent gaps. |

### Miscellaneous Config

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `FeeTiers` | `Vec<FeeTier>` | Instance | Bumped on every invocation | Ordered list of fee tier definitions used by `calculate_fee_by_tier`. |
| `ReferralShareBasisPoints` | `u32` | Instance | Bumped on every invocation | Referral payout share in basis points. |
| `StablecoinIssuer(Address)` | `bool` | Instance | Bumped on every invocation | Presence flag for a registered stablecoin issuer address. |
| `StablecoinIssuerCount` | `u32` | Instance | Bumped on every invocation | Total count of registered stablecoin issuers. |
| `PlayerPreferredToken(Address)` | `Address` | Persistent | Extended on write | Player's preferred token, set explicitly by the player for convenience on new match creation. |
| `Stats` | `PlatformStats` | Instance | Bumped on every invocation | Platform-wide aggregated statistics: total matches created, total volume, total payouts. |

### Reentrancy Guards

| DataKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `DepositInProgress(u64)` | `bool` | Temporary | Evicted automatically after short TTL | Reentrancy guard on `deposit` for a given match ID. Set at entry, cleared at exit. If a reentrant call arrives while set, it is rejected immediately. |

---

## OracleConsensusKey (Separate Enum)

> Kept as a separate `#[contracttype]` enum because Soroban caps a single enum at 50 variants, and `DataKey` is already at that limit.

| OracleConsensusKey variant | Value type | Storage scope | TTL behaviour | Description |
|---|---|---|---|---|
| `OracleDeadlock(u64)` | `bool` | Persistent | Extended on write | Marks a match as deadlocked — the required confirmation threshold can never be reached given the approved oracle set. Prevents the match from waiting forever. |

---

## Upgrade Safety Checklist

When adding a new `DataKey` variant or changing an existing value type, ensure:

1. **Migration hook** — add a branch in `migrate_state` if the key needs a one-time data transformation.
2. **TTL extension** — confirm the key's TTL is bumped on every write that expects the data to be long-lived.
3. **Test coverage** — add a corresponding assertion in `contracts/escrow/tests/upgrade_simulation_tests.rs` verifying the key is readable before and after `migrate_state`.
4. **Backward compatibility** — if removing a variant, ensure no existing storage entries reference it or that the migration clears stale entries.

## Cross-References

- Implementation: [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs) — `DataKey` and `OracleConsensusKey` enums
- Contract logic: [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) — all read/write sites
- Architecture overview: [`docs/architecture.md`](architecture.md)
- Upgrade safety guide: [`docs/upgrade-safety.md`](upgrade-safety.md)
- Upgrade simulation tests: [`contracts/escrow/tests/upgrade_simulation_tests.rs`](../contracts/escrow/tests/upgrade_simulation_tests.rs)
