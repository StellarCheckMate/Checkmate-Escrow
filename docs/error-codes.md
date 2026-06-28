# Smart Contract Error Codes Reference

**Last updated:** 2026-06-22 · **Verified against commit:** `81a3793`

This document is the exhaustive, user-facing reference for every error a
caller can receive from the two on-chain Soroban contracts in this repo:

- [`EscrowContract`](../contracts/escrow/src/lib.rs) — [`contracts/escrow/src/errors.rs`](../contracts/escrow/src/errors.rs) (20 variants)
- [`OracleContract`](../contracts/oracle/src/lib.rs) — [`contracts/oracle/src/errors.rs`](../contracts/oracle/src/errors.rs) (10 variants)

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
| 5 | `InvalidState` | [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`cancel_match`](../contracts/escrow/src/lib.rs), [`expire_match`](../contracts/escrow/src/lib.rs) | The match isn't in the lifecycle state the function requires (e.g. depositing into a `Completed` match, submitting a result for a non-`Active` match). | Call `get_match` and check the `state` field before retrying the action. | Calling `submit_result` on a match already `Completed` → `#5`. |
| 7 | `AlreadyInitialized` | [`initialize`](../contracts/escrow/src/lib.rs) | `initialize` was called a second time. | No action needed — the contract is already configured. Use `get_admin`/`get_oracle` to confirm current config instead of re-initializing. | Re-running a deploy script that calls `initialize` unconditionally → `#7` on the second run. |
| 9 | `ContractPaused` | [`create_match`](../contracts/escrow/src/lib.rs), [`deposit`](../contracts/escrow/src/lib.rs), [`submit_result`](../contracts/escrow/src/lib.rs), [`submit_result_with_oracle_record`](../contracts/escrow/src/lib.rs) | Admin called `pause`; these functions are blocked while paused. | Wait for the admin to call `unpause`; poll `is_paused` to know when it's safe to retry. | `create_match` during an incident-response pause → `#9` until `unpause` is called. |
| 10 | `InvalidAmount` | [`create_match`](../contracts/escrow/src/lib.rs) | `stake_amount <= 0`. | Resubmit with a positive `stake_amount`. | `create_match` with `stake_amount = 0` → `#10`. |
| 13 | `DuplicateGameId` | [`create_match`](../contracts/escrow/src/lib.rs) | `game_id` was already used by a previous match (each game maps to exactly one escrow match, to prevent oracle replay across matches). | Use a fresh, unique `game_id`, or look up the existing match instead of creating a new one. | Two players try to escrow the same Lichess game URL twice → second `create_match` gets `#13`. |
| 14 | `MatchNotExpired` | [`expire_match`](../contracts/escrow/src/lib.rs) | `expire_match` was called before `current_ledger - created_ledger >= timeout`. | Wait until the configured timeout elapses. Check `get_match_timeout` and the match's `created_ledger` (via `get_match`) to compute the earliest valid ledger. | Calling `expire_match` one day into a 30-day default timeout → `#14`. |
| 15 | `InvalidGameId` | [`create_match`](../contracts/escrow/src/lib.rs) | `game_id` is empty or longer than 64 bytes. | Pass a valid Lichess (8-char alphanumeric) or Chess.com (numeric) game ID under the 64-byte limit. | `create_match` with `game_id = ""` → `#15`. |
| 16 | `InvalidPlayers` | [`create_match`](../contracts/escrow/src/lib.rs) | `player1 == player2`, or `player2` is the escrow contract's own address. | Supply two distinct, real player addresses. | `create_match` where both players are the same wallet → `#16`. |
| 17 | `TokenNotAllowed` | [`create_match`](../contracts/escrow/src/lib.rs) | The token allowlist is active (at least one token was ever added) and the supplied token isn't on it. | Admin must call `add_allowed_token` for that token, or the caller should pick an already-allowed one via `get_allowed_tokens`. | `create_match` with an unlisted custom token after the admin enabled allowlisting → `#17`. |
| 18 | `InvalidAddress` | [`initialize`](../contracts/escrow/src/lib.rs), [`update_oracle`](../contracts/escrow/src/lib.rs) | The `oracle`/`new_oracle` address equals the escrow contract's own address. | Supply a distinct external account or contract address. | `initialize` called with `oracle = <ESCROW_CONTRACT_ID itself>` → `#18`. |
| 19 | `MatchAlreadyActive` | [`cancel_match`](../contracts/escrow/src/lib.rs) | `cancel_match` was called on a match that's already `Active` (both players deposited) — voluntary cancellation is pre-activation only. | Let the match proceed to `submit_result`, or wait for `expire_match` eligibility if it stalls. Active matches cannot be cancelled by players. | A player tries to back out after both stakes are in → `#19`. |
| 20 | `InvalidTimeout` | [`set_match_timeout`](../contracts/escrow/src/lib.rs) | `timeout` is outside `[17,280, 1,555,200]` ledgers (1–90 days @ ~6 sec/ledger, or ~1 min – ~104 days wall-clock). | Pass a timeout within the 1–90 day ledger range. Use `MIN_MATCH_TIMEOUT_LEDGERS = 17,280` (1 day) and `MAX_MATCH_TIMEOUT_LEDGERS = 1,555,200` (90 days) as bounds. | `set_match_timeout` with `timeout = 100` (≈10 minutes) → `#20`. |
| 21 | `SnapshotNotFound` | [`submit_result`](../contracts/escrow/src/lib.rs) (ledger snapshot verification) | An internal ledger snapshot required to verify the oracle's result proof is not available — typically when the result is submitted too far in the past (TTL expired) or ledger data was purged. | Resubmit the result sooner after the game finishes. Ensure oracle service processes results within a few hours of completion, not days later. | Oracle attempts to verify a result 1+ months after the game ended → `#21` (ledger snapshot purged). |

### Fatal errors

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 6 | `AlreadyExists` | [`create_match`](../contracts/escrow/src/lib.rs) | A `Match` already exists at the storage slot for the *next* sequential match ID before `create_match` assigns it. Under normal operation `MatchCount` is the sole source of the next ID, so this should never trigger. | Not client-recoverable. Indicates storage/state corruption or a bug in ID assignment — requires admin/dev investigation; in the worst case, a contract migration. | Would only be observed after manual storage tampering or a contract bug — not reachable via the public API in current code. |
| 8 | `Overflow` | [`add_allowed_token`](../contracts/escrow/src/lib.rs) (token counter), [`create_match`](../contracts/escrow/src/lib.rs) (match counter), [`submit_result`](../contracts/escrow/src/lib.rs) (`stake_amount * 2`) | An arithmetic guard (`checked_add`/`checked_mul`) tripped: a counter hit `u32`/`u64::MAX`, or `stake_amount` is large enough that doubling it overflows `i128`. | Counter overflow isn't realistically recoverable (would require billions of matches/tokens) short of a contract upgrade. Pot overflow is **fatal for that one match only** — it must be guarded against at `create_match` time by capping `stake_amount` well under `i128::MAX / 2`; once such a match exists, `submit_result` will always revert, so the only path forward is `cancel_match`/`expire_match` to return the deposits. | A match created with `stake_amount` near `i128::MAX / 2` will permanently fail `submit_result` with `#8` — recover player funds via `expire_match` instead. |

### Reserved / currently unused

| Code | Name | Status |
|------|------|--------|
| 11 | `MatchCancelled` | Defined in `errors.rs` but not returned by any function in the current `lib.rs`. Reserved for a future explicit "this match is cancelled" check (today, a cancelled match falls through to `InvalidState` instead). |
| 12 | `MatchCompleted` | Same as above — reserved for a future explicit "this match is already completed" check; today this case also surfaces as `InvalidState`. |

---

## Oracle Contract (`contracts/oracle/src/errors.rs`)

All ten variants are recoverable — none represent an invariant violation or
unrecoverable state.

| Code | Name | Thrown By | Cause | Recovery | Example |
|------|------|-----------|-------|----------|---------|
| 1 | `Unauthorized` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`has_result_admin`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs), [`update_admin`](../contracts/oracle/src/lib.rs), [`pause`](../contracts/oracle/src/lib.rs), [`unpause`](../contracts/oracle/src/lib.rs), [`set_oracle_rate_limits`](../contracts/oracle/src/lib.rs) | Caller isn't the configured admin, or the contract hasn't been `initialize`d (admin key absent). | Re-sign with the correct admin keypair, or call `initialize` first. Use `is_initialized` to distinguish the two cases. | `submit_result` signed by a non-admin oracle service key → `#1`. |
| 2 | `AlreadySubmitted` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs) | A result for `match_id` is already stored — results are immutable once recorded (integrity guard). | Check `has_result`/`get_result` before submitting. If a genuine correction is needed, admin must `delete_result` first, then resubmit. | The oracle service retries a submission after a network timeout, not realizing the first attempt actually landed → `#2` on the retry (safe — no duplicate result is written). |
| 3 | `ResultNotFound` | [`get_result`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs) | No result exists for `match_id` — never submitted, wrong ID, or the persistent entry's TTL expired and was purged. | Confirm `match_id`, check `has_result` to see if it was ever submitted, or submit the result if it's genuinely missing. | `get_result --match_id 7` before the oracle has reported anything for match 7 → `#3`. |
| 4 | `AlreadyInitialized` | [`initialize`](../contracts/oracle/src/lib.rs) | `initialize` was called a second time. | No action needed — the contract is already configured. | Re-running a deploy script unconditionally → `#4` on the second run. |
| 5 | `ContractPaused` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs), [`delete_result`](../contracts/oracle/src/lib.rs) | Admin called `pause`. | Wait for `unpause`; poll a paused-status check before retrying. | Result submission attempted during an incident-response pause → `#5`. |
| 6 | `InvalidGameId` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs) | `game_id` is empty in the submission (or in any batch entry). | Resubmit with the real platform game ID populated. | A batch entry built from a malformed scrape with `game_id = ""` → `#6`. |
| 7 | `BatchTooLarge` | [`submit_batch_results`](../contracts/oracle/src/lib.rs) | `entries.len() > 100` (`MAX_BATCH_SIZE`). | Split the batch into chunks of ≤100 entries. | Submitting 250 tournament results in one call → `#7`. |
| 8 | `BatchDuplicateEntry` | [`submit_batch_results`](../contracts/oracle/src/lib.rs) | Two entries in the same batch share a `match_id`. | De-duplicate entries client-side — each `match_id` may appear once per batch. | A batch builder accidentally includes the same `match_id` twice after a join bug → `#8`. |
| 9 | `RateLimitExceeded` | [`submit_result`](../contracts/oracle/src/lib.rs), [`submit_batch_results`](../contracts/oracle/src/lib.rs) (via `check_oracle_rate_limit`) | The submission(s) would exceed the oracle's configured hourly or daily sliding-window limit (see `set_oracle_rate_limits`). | Check `get_oracle_rate_limit_status` for remaining quota and window reset timing; wait for the window to roll over, or have the admin raise the limit. | An oracle service burst-submits 150 results in one hour against the default 100/hour limit → `#9` once the limit is hit, with an `oracle / alert` event already emitted at 80% usage. |
| 10 | `InvalidRateLimit` | [`set_oracle_rate_limits`](../contracts/oracle/src/lib.rs) | `hourly_limit > daily_limit` when both are non-zero. | Pass consistent limits (`hourly_limit <= daily_limit`), or pass `0` for either to fall back to the contract default. | `set_oracle_rate_limits(oracle, 500, 100)` → `#10`. |

---

## Troubleshooting quick-lookup table

Use this when you only know the *symptom*, not the code.

| Symptom | Likely error(s) | First thing to check |
|---------|------------------|------------------------|
| "Transaction failed, can't tell why" | Any | Decode the numeric code from the tx result (`Error(Contract, #N)`), then look it up above. |
| Deposit/submit/cancel rejected right after deploy | `Unauthorized` (Escrow #4 / Oracle #1) | Did you call `initialize` on this contract yet? `is_initialized`. |
| Player can't deposit | `MatchNotFound` (#1), `InvalidState` (#5), `AlreadyFunded` (#2), `Unauthorized` (#4) | `get_match` — confirm the ID exists, state is `Pending`, and you haven't already deposited. |
| Oracle can't submit a result | `ContractPaused` (#9 / #5), `MatchNotFound` (#1), `NotFunded` (#3), `Unauthorized` (#4 / #1), `RateLimitExceeded` (#9 oracle) | `is_paused`, `is_funded`, `get_oracle_rate_limit_status`. |
| `create_match` rejected | `InvalidAmount` (#10), `InvalidGameId` (#15), `DuplicateGameId` (#13), `InvalidPlayers` (#16), `TokenNotAllowed` (#17), `ContractPaused` (#9) | Validate `stake_amount > 0`, `game_id` format/uniqueness, distinct players, and `get_allowed_tokens` if allowlisting is on. |
| Can't cancel a match | `MatchAlreadyActive` (#19), `InvalidState` (#5), `Unauthorized` (#4) | `get_match` — cancellation only works on `Pending` matches you're a player in. |
| `expire_match` rejected | `MatchNotExpired` (#14), `InvalidState` (#5), `MatchNotFound` (#1) | Compare `get_match_timeout` against the match's `created_ledger`. |
| Oracle batch submission rejected | `BatchTooLarge` (#7), `BatchDuplicateEntry` (#8), `InvalidGameId` (#6), `AlreadySubmitted` (#2) | Validate the batch client-side before sending: size ≤100, unique `match_id`s, non-empty `game_id`s. |
| Admin config call rejected | `Unauthorized` (#4 / #1), `InvalidTimeout` (Escrow #20), `InvalidRateLimit` (Oracle #10), `InvalidAddress` (Escrow #18) | Confirm you're signing with the current admin key and that the new value is within the documented bounds. |
| A match seems permanently stuck on `submit_result` | `Overflow` (Escrow #8, fatal) | Check `stake_amount` isn't absurdly large; recover funds via `cancel_match`/`expire_match` instead of retrying `submit_result`. |

---

## Coverage

This document covers all variants present in source as of commit `81a3793`:

- Escrow (`contracts/escrow/src/errors.rs`): 22/22 variants documented (19 recoverable, 2 fatal, 1 reserved/unused).
- Oracle (`contracts/oracle/src/errors.rs`): 10/10 variants documented (10 recoverable).

If `cargo build` or a code review surfaces a new variant in either
`errors.rs`, add a row here in the same PR — this file is expected to stay in
lockstep with the source enums.
