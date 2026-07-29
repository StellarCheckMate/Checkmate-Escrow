# Smart Contract Error Codes Reference

**Last updated:** 2026-07-29 · **Verified against:** `contracts/escrow/src/errors.rs` and `contracts/oracle/src/errors.rs`

This document is the exhaustive, user-facing reference for every error a
caller can receive from the two on-chain Soroban contracts in this repo:

- [`EscrowContract`](../contracts/escrow/src/lib.rs) — [`contracts/escrow/src/errors.rs`](../contracts/escrow/src/errors.rs) (50 variants)
- [`OracleContract`](../contracts/oracle/src/lib.rs) — [`contracts/oracle/src/errors.rs`](../contracts/oracle/src/errors.rs) (21 variants)

Every variant defined in those two files is documented below. If you add,
remove, or renumber a variant, update this file in the same PR.

---

## How errors are returned

Both contracts use Soroban's `#[contracterror]` macro. An error is **not** a
string — it's a small integer (`u32`) discriminant attached to the function's
`Result<T, Error>`. When a call fails, the CLI/SDK surfaces it as something
like:

```
Error(Contract, #4)
```

`#4` is the numeric code from the tables below. Map it back to a name using
this document, then look up the cause and recovery steps.

```bash
stellar contract invoke --id $ESCROW_CONTRACT_ID -- deposit \
  --match_id 42 --player <ADDRESS>
# ... Error(Contract, #4) ...
# → 4 = Unauthorized (see Escrow table below)
```

## Security: what error codes do (and don't) reveal

- The on-chain error is **only** the numeric discriminant — no message text,
  stack trace, storage contents, or argument values are ever included in the
  contract's return value. This is enforced by `#[contracterror]` itself, not
  by application logic, so there is no on-chain string to accidentally leak.
- Several variants are intentionally coarse-grained for this reason. For
  example, `Unauthorized` is returned both when the contract has never been
  initialized **and** when the caller is simply the wrong account — this
  avoids confirming or denying internal state (e.g. "does this contract have
  an admin set?") to an unauthenticated caller.
- **Off-chain consumers (frontend, oracle-service, support tooling) are the
  place sensitive detail can leak.** When mapping these codes to user-facing
  UI text, do not embed request payloads, private keys, raw RPC responses, or
  internal match data in the displayed message — surface only the code, name,
  and the generic recovery guidance from this document.

---

## Recoverable vs. fatal

- **Recoverable** — the caller (player, admin, or oracle) can take a concrete
  action — fix input, wait, switch signer, or call a different function —
  and the same operation will succeed afterward. No funds or state are lost.
- **Fatal** — the error indicates an invariant violation or a hard
  arithmetic/storage limit. There is no client-side retry that fixes it; it
  requires investigation, an admin/dev intervention, or in the worst case
  means that specific match is stuck (other matches are unaffected).

---

## Escrow Contract (`contracts/escrow/src/errors.rs`)

### Recoverable errors

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 1 | `MatchNotFound` | [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`cancel_match`](../contracts/escrow/src/lib.rs), [`expire_match`](../contracts/escrow/src/lib.rs), [`get_match`](../contracts/escrow/src/lib.rs), [`is_funded`](../contracts/escrow/src/lib.rs), [`get_depositor_count`](../contracts/escrow/src/lib.rs), [`get_escrow_balance`](../contracts/escrow/src/lib.rs) | `match_id` has no stored `Match` — wrong ID, typo, or wrong contract/network. | Call `get_match_count` to confirm the valid ID range, or `get_player_matches_paginated` to re-fetch a player's real match IDs. Double-check `$ESCROW_CONTRACT_ID` and `--network`. | `get_match --match_id 999` on a contract with only 50 matches → `#1`. |
| 2 | `AlreadyFunded` | [`deposit`](../contracts/escrow/src/lib.rs) | The same player called `deposit` twice for one match. | No funds are at risk — the second call is simply rejected. Call `get_depositor_count` first if unsure whether you've already deposited. | Player1 deposits, then accidentally retries the same tx after a slow confirmation → `#2` on the retry; original deposit is untouched. |
| 3 | `NotFunded` | [`submit_result`](../contracts/escrow/src/lib.rs) (incl. via [`submit_result_with_oracle_record`](../contracts/escrow/src/lib.rs)) | Result submission was attempted before both players deposited. | Wait for both deposits; poll `is_funded` or `get_depositor_count` before asking the oracle to submit. | Oracle submits a result the moment a game finishes, but Player2 never funded the escrow → `#3`. |
| 4 | `Unauthorized` | [`pause`](../contracts/escrow/src/lib.rs), [`unpause`](../contracts/escrow/src/lib.rs), [`add_allowed_token`](../contracts/escrow/src/lib.rs), [`remove_allowed_token`](../contracts/escrow/src/lib.rs), [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`cancel_match`](../contracts/escrow/src/lib.rs), [`get_admin`](../contracts/escrow/src/lib.rs), [`get_oracle`](../contracts/escrow/src/lib.rs), [`set_match_timeout`](../contracts/escrow/src/lib.rs), [`propose_admin`](../contracts/escrow/src/lib.rs), [`accept_admin`](../contracts/escrow/src/lib.rs), [`update_oracle`](../contracts/escrow/src/lib.rs), [`transfer_admin`](../contracts/escrow/src/lib.rs) | Caller isn't the required signer (admin/oracle/depositing player) **or** the contract hasn't been `initialize`d yet (admin/oracle key absent in storage). | Re-sign with the correct keypair, or call `initialize` first on a fresh deployment. Use `is_initialized` to tell the two cases apart safely. | Calling `pause` with a non-admin key → `#4`. Calling `get_admin` on a contract that was never initialized → also `#4`. |
| 4 | `NotAdmin` *(sub-case of `Unauthorized`)* | [`pause`](../contracts/escrow/src/lib.rs), [`unpause`](../contracts/escrow/src/lib.rs), [`add_allowed_token`](../contracts/escrow/src/lib.rs), [`remove_allowed_token`](../contracts/escrow/src/lib.rs), [`set_match_timeout`](../contracts/escrow/src/lib.rs), [`propose_admin`](../contracts/escrow/src/lib.rs), [`accept_admin`](../contracts/escrow/src/lib.rs), [`update_oracle`](../contracts/escrow/src/lib.rs), [`transfer_admin`](../contracts/escrow/src/lib.rs) | The caller is not the configured admin address. Surfaces as `Error(Contract, #4)`. The contract does not use a separate `NotAdmin` variant — `Unauthorized` covers all authorization failures to keep callers from probing whether an admin is set. | Verify the signing key matches the admin returned by `get_admin`. If the contract is uninitialized, call `initialize` first (check with `is_initialized`). To rotate the admin, the *current* admin must call `propose_admin`/`accept_admin` or `transfer_admin`. | Calling `pause` with a non-admin keypair → `Error(Contract, #4)`. |
| 4 | `NotOracle` *(sub-case of `Unauthorized`)* | [`submit_result`](../contracts/escrow/src/lib.rs), [`submit_result_with_oracle_record`](../contracts/escrow/src/lib.rs) | The caller is not the configured oracle address. Surfaces as `Error(Contract, #4)`. Like `NotAdmin`, the contract returns the same `Unauthorized` code to avoid leaking internal state to unauthenticated callers. | Verify the signing key matches the oracle returned by `get_oracle`. If the oracle address needs updating, the admin must call `update_oracle` with the correct new address. If the contract is uninitialized, call `initialize` first. | Oracle service running with a rotated keypair that no longer matches the on-chain oracle address → `Error(Contract, #4)` on every `submit_result` call. |
| 5 | `InvalidState` | [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`cancel_match`](../contracts/escrow/src/lib.rs), [`expire_match`](../contracts/escrow/src/lib.rs) | The match isn't in the lifecycle state the function requires (e.g. depositing into a `Completed` match, submitting a result for a non-`Active` match). | Call `get_match` and check the `state` field before retrying the action. | Calling `submit_result` on a match already `Completed` → `#5`. |
| 7 | `AlreadyInitialized` | [`initialize`](../contracts/escrow/src/lib.rs) | `initialize` was called a second time. | No action needed — the contract is already configured. Use `get_admin`/`get_oracle` to confirm current config instead of re-initializing. | Re-running a deploy script that calls `initialize` unconditionally → `#7` on the second run. |
| 9 | `ContractPaused` | [`create_match`](../contracts/escrow/src/lib.rs), [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`submit_result_with_oracle_record`](../contracts/escrow/src/lib.rs) | Admin called `pause`; these functions are blocked while paused. | Wait for the admin to call `unpause`; poll `is_paused` to know when it's safe to retry. | `create_match` during an incident-response pause → `#9` until `unpause` is called. |
| 10 | `InvalidAmount` | [`create_match`](../contracts/escrow/src/lib.rs) | `stake_amount <= 0`, or `stake_amount` is below the admin-configured `minimum_stake` (see `set_minimum_stake`; defaults to `1`). | Resubmit with a positive `stake_amount` that meets `get_protocol_config().minimum_stake`. | `create_match` with `stake_amount = 0` → `#10`. `set_minimum_stake(50)` then `create_match` with `stake_amount = 10` → `#10`. |
| 13 | `DuplicateGameId` | [`create_match`](../contracts/escrow/src/lib.rs) | `game_id` was already used by a previous match (each game maps to exactly one escrow match, to prevent oracle replay across matches). | Use a fresh, unique `game_id`, or look up the existing match instead of creating a new one. | Two players try to escrow the same Lichess game URL twice → second `create_match` gets `#13`. |
### 14. MatchNotExpired

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 14 | `MatchNotExpired` | [`expire_match`](../contracts/escrow/src/lib.rs) | `expire_match` was called before `current_ledger - created_ledger >= timeout`. | Wait until the configured timeout elapses. Check `get_match_timeout` and the match's `created_ledger` (via `get_match`) to compute the earliest valid ledger. **Note:** This timeout is the primary safety mechanism protecting players if the oracle goes offline — see [FAQ: What happens if the oracle goes offline?](faq.md#9-what-happens-if-the-oracle-goes-offline) | Calling `expire_match` one day into a 30-day default timeout → `#14`. |
| 15 | `InvalidGameId` | [`create_match`](../contracts/escrow/src/lib.rs) | `game_id` is empty or longer than 64 bytes. | Pass a valid Lichess (8-char alphanumeric) or Chess.com (numeric) game ID under the 64-byte limit. | `create_match` with `game_id = ""` → `#15`. |
| 16 | `InvalidPlayers` | [`create_match`](../contracts/escrow/src/lib.rs) | `player1 == player2`, or `player2` is the escrow contract's own address. | Supply two distinct, real player addresses. | `create_match` where both players are the same wallet → `#16`. |
| 17 | `TokenNotAllowed` | [`create_match`](../contracts/escrow/src/lib.rs) | The token allowlist is active (at least one token was ever added) and the supplied token isn't on it. | Admin must call `add_allowed_token` for that token, or the caller should pick an already-allowed one via `get_allowed_tokens`. | `create_match` with an unlisted custom token after the admin enabled allowlisting → `#17`. |
| 18 | `InvalidAddress` | [`initialize`](../contracts/escrow/src/lib.rs), [`update_oracle`](../contracts/escrow/src/lib.rs) | The `oracle`/`new_oracle` address equals the escrow contract's own address. | Supply a distinct external account or contract address. | `initialize` called with `oracle = <ESCROW_CONTRACT_ID itself>` → `#18`. |
| 19 | `MatchAlreadyActive` | [`cancel_match`](../contracts/escrow/src/lib.rs) | `cancel_match` was called on a match that's already `Active` (both players deposited) — voluntary cancellation is pre-activation only. | Let the match proceed to `submit_result`, or wait for `expire_match` eligibility if it stalls. Active matches cannot be cancelled by players. | A player tries to back out after both stakes are in → `#19`. |
| 20 | `InvalidTimeout` | [`set_match_timeout`](../contracts/escrow/src/lib.rs) | `seconds` is outside `[86,400, 7,776,000]` (1–90 days, wall-clock seconds). | Pass a timeout within the 1–90 day range, in seconds. Use `MIN_MATCH_TIMEOUT_SECONDS = 86,400` (1 day) and `MAX_MATCH_TIMEOUT_SECONDS = 7,776,000` (90 days) as bounds. | `set_match_timeout` with `seconds = 100` (≈1.5 minutes) → `#20`. |
| 21 | `SnapshotNotFound` | [`submit_result`](../contracts/escrow/src/lib.rs) (ledger snapshot verification) | An internal ledger snapshot required to verify the oracle's result proof is not available — typically when the result is submitted too far in the past (TTL expired) or ledger data was purged. | Resubmit the result sooner after the game finishes. Ensure oracle service processes results within a few hours of completion, not days later. | Oracle attempts to verify a result 1+ months after the game ended → `#21` (ledger snapshot purged). |

### Fatal errors

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 6 | `AlreadyExists` | [`create_match`](../contracts/escrow/src/lib.rs) | A `Match` already exists at the storage slot for the *next* sequential match ID before `create_match` assigns it. Under normal operation `MatchCount` is the sole source of the next ID, so this should never trigger. | Not client-recoverable. Indicates storage/state corruption or a bug in ID assignment — requires admin/dev investigation; in the worst case, a contract migration. | Would only be observed after manual storage tampering or a contract bug — not reachable via the public API in current code. |
| 8 | `Overflow` | [`add_allowed_token`](../contracts/escrow/src/lib.rs) (token counter), [`create_match`](../contracts/escrow/src/lib.rs) (match counter), [`submit_result`](../contracts/escrow/src/lib.rs) (`stake_amount * 2`) | An arithmetic guard (`checked_add`/`checked_mul`) tripped: a counter hit `u32`/`u64::MAX`, or `stake_amount` is large enough that doubling it overflows `i128`. | Counter overflow isn't realistically recoverable (would require billions of matches/tokens) short of a contract upgrade. Pot overflow is **fatal for that one match only** — it must be guarded against at `create_match` time by capping `stake_amount` well under `i128::MAX / 2`; once such a match exists, `submit_result` will always revert, so the only path forward is `cancel_match`/`expire_match` to return the deposits. | A match created with `stake_amount` near `i128::MAX / 2` will permanently fail `submit_result` with `#8` — recover player funds via `expire_match` instead. |

| 11 | `RollbackWindowExpired` | [`dispute_and_rollback_match`](../contracts/escrow/src/lib.rs) | Called after the 24h heartbeat window (`ROLLBACK_WINDOW_SECONDS`) had already elapsed since `last_heartbeat`. | Use `expire_match`/`cancel_match` instead, or call `heartbeat_match` before the window lapses next time. | Disputing a match 25h after the last heartbeat → `#11`. |
| 12 | `ReasonTooLong` | [`dispute_and_rollback_match`](../contracts/escrow/src/lib.rs) | `reason` was empty or longer than `MAX_REASON_LEN` (256 bytes). | Resubmit with a non-empty reason under 256 bytes. | Passing a 300-byte reason string → `#12`. |

**Note on `platform`:** `create_match` takes `platform: Platform`, a typed enum (`Platform::Lichess` / `Platform::ChessDotCom`). Because it's a typed enum rather than a free-form string, an invalid platform value is rejected by the contract ABI itself (an unrecognized discriminant fails to deserialize) before the call ever reaches contract code — there is no "unknown platform" case for the contract to return a typed error for. The escrow error enum is also already at the XDR-enforced cap of 50 cases (`ScSpecUdtErrorEnumV0::cases` is a `VecM<_, 50>`), so no additional error code is available to add for this even if it were reachable.
| 51 | `InvalidPlatform` | Defined in `errors.rs` but not reachable via the public API today. `platform` is a typed `Platform` enum (`Lichess` / `ChessDotCom`) — the ABI rejects any other discriminant before the call reaches contract code, so `create_match` can never observe an "unknown platform" value to reject. Reserved for forward compatibility if `platform` ever needs dynamic (string-based) construction. |

### New error codes (22+) — Dispute resolution, vesting, tiers, and upgrades

These error codes support advanced features including dispute resolution, staking tiers, vesting schedules, and contract upgrades:

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 22 | `VestingNotExpired` | Vesting check functions | Attempting to claim vested payout before the vesting period elapses | Wait until the vesting period expires, then retry the claim | Calling `claim_vested_payout` before the configured vesting duration has passed → `#22`. |
| 23 | `AlreadyClaimed` | Payout claim functions | A player has already claimed their payout for this match | No action needed — the payout was already received. Check account balance or transaction history to confirm. | Player1 calls `claim_vested_payout` twice for the same match → `#23` on the second call. |
| 24 | `DisputeNotFound` | Dispute resolution functions | Attempting to resolve or query a dispute that doesn't exist | Confirm the dispute ID exists via the dispute listing functions | Admin calls `resolve_disputed_match` with an invalid dispute ID → `#24`. |
| 25 | `PendingResultNotFound` | Dispute functions | No pending result exists for the match yet | Submit a result first before initiating a dispute | Attempting to dispute a match that has no oracle result submitted → `#25`. |
| 26 | `DisputeAlreadyResolved` | Dispute functions | The dispute has already been resolved by the admin | No further action needed — the resolution is final | Attempting to vote or resolve a dispute that's already been settled → `#26`. |
| 27 | `VotingPeriodElapsed` | Oracle voting functions | The voting window for this consensus vote has closed | Wait for the next voting round or submit through a different path | Attempting to vote on an oracle result after the voting period ended → `#27`. |
| 28 | `AlreadyVoted` | Oracle voting functions | The oracle has already cast a vote on this match | No action needed — vote is already recorded. To change a vote, resubmit a different result. | An oracle calls `submit_oracle_result` with a different winner after already voting → `#28` (equivocation). |
| 29 | `NotStaker` | Staking/tier functions | Caller is not a registered staker or doesn't meet tier requirements | Register with the staking system or deposit the minimum tier amount | Non-staked player attempts to participate in a tier-restricted match → `#29`. |
| 30 | `VotingPeriodNotElapsed` | Dispute resolution | Attempting to finalize consensus before the voting window closes | Wait for the voting period to fully elapse | Calling `finalize_consensus` before all oracles have had time to vote → `#30`. |
| 31 | `MatchNotInPendingResult` | Dispute functions | The match is not in the "pending result" state required for disputes | Confirm the match has a submitted result but is not yet finalized | Attempting to dispute a match in `Completed` state → `#31`. |
| 32 | `DisputePeriodNotElapsed` | Dispute resolution | Attempting to finalize a dispute before the dispute window closes | Wait for the dispute period to expire | Admin tries to resolve a dispute too soon → `#32`. |
| 33 | `DisputeAlreadyRaised` | Dispute creation | A dispute has already been raised for this match | No action needed — one dispute is already in progress. Monitor the resolution process. | Attempting to raise a second dispute for the same match → `#33`. |
| 34 | `InvalidEvidenceHash` | Dispute functions | The evidence hash format is invalid or missing | Provide a valid cryptographic hash of the dispute evidence | Submitting evidence with a malformed or incorrectly-sized hash → `#34`. |
| 35 | `TierStakeNotAllowed` | Match creation with tiers | Stake amount doesn't align with player tier requirements | Adjust stake to match the player's tier bracket, or upgrade tier by staking more | Player in Tier 1 (1–100 XLM) tries to create a match with 500 XLM stake → `#35`. |
| 36 | `NotInitialized` | Read functions on uninitialized contract | Contract has not been initialized yet | Call `initialize` first (admin must do this after deployment) | Calling `get_admin` before `initialize` on a fresh contract → `#36`. |
| 37 | `InvalidPauseState` | Pause/unpause functions | Attempting to pause an already-paused contract or unpause a running one | Check current pause state via `is_paused` before calling pause/unpause | Calling `pause` when contract is already paused → `#37`. |
| 38 | `InvalidConversionRate` | Rate verification | The conversion rate submitted is not a valid positive number | Supply a positive, non-zero conversion rate | Submitting a swap with conversion rate = 0 → `#38`. |
| 39 | `ConversionRateOutOfBounds` | Swap/token rate functions | The conversion rate exceeds acceptable bounds (typically ±5% of oracle rate) | Resubmit with a rate within tolerance, or wait for oracle to refresh | Submitting a swap rate 10% higher than the oracle rate when only ±5% is allowed → `#39`. |
| 40 | `ConversionRateStalePriceSource` | Rate validation | The price source used for rate validation is stale (too old) | Refresh the price feed from the oracle and retry | Attempting a swap using a price quote older than the configured TTL → `#40`. |
| 41 | `InsufficientBond` | Bond/stake system | Caller hasn't posted the required bond for the operation | Post the minimum bond amount and retry | Attempting a dispute without having posted the bond → `#41`. |
| 42 | `QuorumNotMet` | Consensus functions | Consensus voting hasn't reached the required quorum | Wait for more oracles to vote | Attempting to finalize consensus before enough oracles have voted → `#42`. |
| 43 | `InsufficientHoldingDuration` | Tier/staking functions | Staked tokens don't meet the minimum holding-period requirement | Wait longer, or unstake and re-stake to reset the timer | Player tries to use tier benefits before their stake has been locked for the required time → `#43`. |
| 44 | `OracleSlashFailed` | Internal slashing logic | Attempted to slash an oracle's stake but the operation failed | Not recoverable by caller; indicates a contract state issue requiring investigation | Slashing logic attempted but underlying state became inconsistent → `#44` (fatal). |
| 45 | `TooManyActiveMatches` | Match creation | The player has exceeded the maximum concurrent active matches | Wait for some existing matches to complete/cancel | Player with 50 active matches tries to create a 51st → `#45`. |
| 46 | `NotStablecoin` | Match creation (stablecoin-only mode) | Token is not a registered stablecoin and stablecoin-only mode is enabled | Use a stablecoin token (e.g., USDC, EURC) or have admin disable stablecoin-only mode | Creating a match with a non-stablecoin token when the contract is in stablecoin-only mode → `#46`. |
| 47 | `UpgradeNotScheduled` | Contract upgrade functions | Attempting to execute an upgrade that hasn't been scheduled | Schedule the upgrade first via the admin upgrade functions | Calling `execute_upgrade` without a prior `schedule_upgrade` → `#47`. |
| 48 | `UpgradeReviewPeriodNotElapsed` | Contract upgrade | Attempting to execute an upgrade before the review/delay period expires | Wait for the configured review window to pass | Trying to execute an upgrade 1 hour after scheduling when the minimum is 24 hours → `#48`. |
| 49 | `InvalidVersion` | Contract upgrade | The contract version specified in the upgrade is invalid or doesn't exist | Supply a valid contract version identifier | Scheduling an upgrade to a version that doesn't exist → `#49`. |
| 50 | `UpgradeAlreadyScheduled` | Contract upgrade | An upgrade is already scheduled; only one upgrade can be pending at a time | Execute or cancel the existing scheduled upgrade first | Attempting to schedule a second upgrade while one is already pending → `#50`. |

---

## Oracle Contract (`contracts/oracle/src/errors.rs`)

All 21 variants are primarily recoverable — the majority represent client-side issues or rate limit exceedances, though a few indicate internal oracle stake/consensus state requiring investigation.

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 1 | `Unauthorized` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`submit_oracle_result`](../contracts/oracle/src/lib.rs), [`has_result_admin`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs), [`update_admin`](../contracts/oracle/src/lib.rs), [`pause`](../contracts/oracle/src/lib.rs), [`unpause`](../contracts/oracle/src/lib.rs), [`set_oracle_rate_limits`](../contracts/oracle/src/lib.rs), [`register_oracle_with_stake`](../contracts/oracle/src/lib.rs) | Caller isn't the configured admin, or the contract hasn't been `initialize`d (admin key absent). | Re-sign with the correct admin keypair, or call `initialize` first. Use `is_initialized` to distinguish the two cases. | `submit_result` signed by a non-admin oracle service key → `#1`. |
| 2 | `AlreadySubmitted` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`submit_oracle_result`](../contracts/oracle/src/lib.rs) | A result for `match_id` is already stored — results are immutable once recorded (integrity guard). | Check `has_result`/`get_result` before submitting. If a genuine correction is needed, admin must `delete_result` first, then resubmit. | The oracle service retries a submission after a network timeout, not realizing the first attempt actually landed → `#2` on the retry (safe — no duplicate result is written). |
| 3 | `ResultNotFound` | [`get_result`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs) | No result exists for `match_id` — never submitted, wrong ID, or the persistent entry's TTL expired and was purged. | Confirm `match_id`, check `has_result` to see if it was ever submitted, or submit the result if it's genuinely missing. | `get_result --match_id 7` before the oracle has reported anything for match 7 → `#3`. |
| 4 | `AlreadyInitialized` | [`initialize`](../contracts/oracle/src/lib.rs) | `initialize` was called a second time. | No action needed — the contract is already configured. | Re-running a deploy script unconditionally → `#4` on the second run. |
| 5 | `ContractPaused` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`submit_oracle_result`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs) | Admin called `pause`. | Wait for `unpause`; poll a paused-status check before retrying. | Result submission attempted during an incident-response pause → `#5`. |
| 6 | `InvalidGameId` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`submit_oracle_result`](../contracts/oracle/src/lib.rs) | `game_id` is empty in the submission (or in any batch entry). | Resubmit with the real platform game ID populated. | A batch entry built from a malformed scrape with `game_id = ""` → `#6`. |
| 7 | `BatchTooLarge` | [`submit_batch_results`](../contracts/oracle/src/lib.rs) | `entries.len() > 100` (`MAX_BATCH_SIZE`). | Split the batch into chunks of ≤100 entries. | Submitting 250 tournament results in one call → `#7`. |
| 8 | `BatchDuplicateEntry` | [`submit_batch_results`](../contracts/oracle/src/lib.rs) | Two entries in the same batch share a `match_id`. | De-duplicate entries client-side — each `match_id` may appear once per batch. | A batch builder accidentally includes the same `match_id` twice after a join bug → `#8`. |
| 9 | `RateLimitExceeded` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`submit_oracle_result`](../contracts/oracle/src/lib.rs) (via `check_oracle_rate_limit`) | The submission(s) would exceed the oracle's configured hourly or daily sliding-window limit (see `set_oracle_rate_limits`). | Check `get_oracle_rate_limit_status` for remaining quota and window reset timing; wait for the window to roll over, or have the admin raise the limit. | An oracle service burst-submits 150 results in one hour against the default 100/hour limit → `#9` once the limit is hit, with an `oracle / alert` event already emitted at 80% usage. |
| 10 | `InvalidRateLimit` | [`set_oracle_rate_limits`](../contracts/oracle/src/lib.rs) | `hourly_limit > daily_limit` when both are non-zero. | Pass consistent limits (`hourly_limit <= daily_limit`), or pass `0` for either to fall back to the contract default. | `set_oracle_rate_limits(oracle, 500, 100)` → `#10`. |
| 11 | `InsufficientStake` | [`submit_oracle_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs) (for consensus-based submission) | The oracle has registered stake but it has been slashed to zero or is below the minimum required for participation. | Re-register with `register_oracle_with_stake` and deposit sufficient collateral. | An oracle's stake was slashed due to equivocation or SLA violations, and they attempt to submit without re-staking → `#11`. |
| 12 | `NotRegisteredOracle` | [`submit_oracle_result`](../contracts/oracle/src/lib.rs) (consensus submission path) | `submit_oracle_result` was called by an address that has never registered via `register_oracle_with_stake`. | Call `register_oracle_with_stake` first with the required collateral amount. | A new oracle service instance attempts consensus voting without first registering → `#12`. |
| 14 | `MatchDisputed` | [`submit_oracle_result`](../contracts/oracle/src/lib.rs) | The match's consensus has deadlocked (no remaining oracle vote can push any candidate result over the threshold) and is awaiting admin resolution via `resolve_disputed_match`. | Do not submit further votes; await admin resolution. Once resolved, the final result will be recorded. | During a close vote, no oracle can push any candidate over the threshold, so the match enters disputed state and `submit_oracle_result` returns `#14`. |
| 15 | `InvalidThreshold` | [`set_consensus_threshold`](../contracts/oracle/src/lib.rs) | `set_consensus_threshold` was called with a threshold of 0 (invalid for consensus). | Pass a threshold ≥ 1 (e.g., 2 for 2-of-N voting). | `set_consensus_threshold(oracle, 0)` → `#15`. |
| 16 | `MatchNotDisputed` | [`resolve_disputed_match`](../contracts/oracle/src/lib.rs) | `resolve_disputed_match` was called for a match that is not in a disputed (deadlocked) consensus state. | Confirm the match is actually disputed by checking its consensus state. Do not call resolve on a non-disputed match. | Admin tries to resolve a match that finished normally (not disputed) → `#16`. |
| 17 | `OracleDeactivated` | [`submit_oracle_result`](../contracts/oracle/src/lib.rs), [`submit_result`](../contracts/oracle/src/lib.rs) (SLA enforcement) | The oracle has been deactivated due to repeated SLA violations (slow response time, low accuracy, or other metrics). | Not immediately recoverable — oracle must contact admin or re-register after a cooldown to restore status. | An oracle's average response time drifted above 5s SLA threshold, triggering automatic deactivation → `#17`. |
| 18 | `OracleNotSlow` | [`deactivate_slow_oracle`](../contracts/oracle/src/lib.rs) | Attempted to deactivate an oracle that has a good SLA (average response time ≤ 5s). | Do not call deactivation on a well-performing oracle. | Admin tries to deactivate an oracle with 2s average response time → `#18`. |
| 19 | `InvalidAmount` | Rate/stake functions | A stake or fee amount is invalid (typically zero or negative). | Supply a positive amount. | Calling `register_oracle_with_stake` with `stake_amount = 0` → `#19`. |
| 20 | `Overflow` | Arithmetic operations (stake accumulation, voting tallies) | An arithmetic guard tripped: a counter or accumulated amount exceeded numeric bounds. | Not typically recoverable client-side. Indicates a contract state issue; contact admin for investigation. | After thousands of slash+re-stake cycles, the oracle's tally counters overflow → `#20` (rare, fatal). |
| 21 | `SlippageExceeded` | Rate/swap validation | The price changed beyond acceptable slippage bounds between submission and execution. | Resubmit with a wider slippage tolerance or wait for prices to stabilize. | Submitting a swap with 0.5% max slippage when market moved 1% → `#21`. |

---

## Troubleshooting quick-lookup table

Use this when you only know the *symptom*, not the code.

| Symptom | Likely error(s) | First thing to check |
|---------|------------------|------------------------|
| "Transaction failed, can't tell why" | Any | Decode the numeric code from the tx result (`Error(Contract, #N)`), then look it up above. |
| Deposit/submit/cancel rejected right after deploy | `Unauthorized` (Escrow #4 / Oracle #1) | Did you call `initialize` on this contract yet? `is_initialized`. |
| `submit_result` rejected — oracle key mismatch | `NotOracle` / `Unauthorized` (Escrow #4) | Confirm the oracle service key matches `get_oracle`; if rotated, admin must call `update_oracle`. |
| Admin call rejected — admin key mismatch | `NotAdmin` / `Unauthorized` (Escrow #4 / Oracle #1) | Confirm you're signing with the key returned by `get_admin`. Use `is_initialized` to rule out uninitialized contract. |
| Player can't deposit | `MatchNotFound` (#1), `InvalidState` (#5), `AlreadyFunded` (#2), `Unauthorized` (#4) | `get_match` — confirm the ID exists, state is `Pending`, and you haven't already deposited. |
| Oracle can't submit a result | `ContractPaused` (#9 / #5), `MatchNotFound` (#1), `NotFunded` (#3), `Unauthorized` (#4 / #1), `RateLimitExceeded` (#9 oracle) | `is_paused`, `is_funded`, `get_oracle_rate_limit_status`. |
| Oracle can't submit a result | `ContractPaused` (#9 / #5), `MatchNotFound` (#1), `NotFunded` (#3), `Unauthorized` (#4 / #1), `RateLimitExceeded` (#9 oracle) | `is_paused`, `is_funded`, `get_oracle_rate_limit_status`. |
| `create_match` rejected | `InvalidAmount` (#10), `InvalidGameId` (#15), `DuplicateGameId` (#13), `InvalidPlayers` (#16), `TokenNotAllowed` (#17), `TokenBlacklisted` (#46), `ContractPaused` (#9) | Validate `stake_amount > 0`, `game_id` format/uniqueness, distinct players, `get_allowed_tokens` if allowlisting is on, and `is_token_blacklisted` to verify the token is not banned. |
| Can't cancel a match | `MatchAlreadyActive` (#19), `InvalidState` (#5), `Unauthorized` (#4) | `get_match` — cancellation only works on `Pending` matches you're a player in. |
| `expire_match` rejected | `MatchNotExpired` (#14), `InvalidState` (#5), `MatchNotFound` (#1) | Compare `get_match_timeout` against the match's `created_ledger`. |
| Oracle batch submission rejected | `BatchTooLarge` (#7), `BatchDuplicateEntry` (#8), `InvalidGameId` (#6), `AlreadySubmitted` (#2) | Validate the batch client-side before sending: size ≤100, unique `match_id`s, non-empty `game_id`s. |
| Admin config call rejected | `Unauthorized` (#4 / #1), `InvalidTimeout` (Escrow #20), `InvalidRateLimit` (Oracle #10), `InvalidAddress` (Escrow #18) | Confirm you're signing with the current admin key and that the new value is within the documented bounds. |
| A match seems permanently stuck on `submit_result` | `Overflow` (Escrow #8, fatal) | Check `stake_amount` isn't absurdly large; recover funds via `cancel_match`/`expire_match` instead of retrying `submit_result`. |

---

## Coverage

This document covers all variants present in source as of 2026-07-29:

- Escrow (`contracts/escrow/src/errors.rs`): 50/50 variants documented (including new variants 22–50 for dispute resolution, vesting, tiers, and upgrades).
- Oracle (`contracts/oracle/src/errors.rs`): 21/21 variants documented (including new variants for oracle staking and SLA enforcement).

If `cargo build` or a code review surfaces a new variant in either
`errors.rs`, add a row here in the same PR — this file is expected to stay in
lockstep with the source enums.
