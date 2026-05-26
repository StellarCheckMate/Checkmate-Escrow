# Checkmate-Escrow - Contributor Issue Backlog v5

> Legend: Open - unassigned

---

## Core Contract Gaps

## #1 - Fix: escrow `initialize` still panics on re-initialization instead of returning `Error::AlreadyInitialized`
**Status:** Open - unassigned  
**Labels:** `bug`, `api`  
**Priority:** High  

**Description:**  
`EscrowContract::initialize` uses `panic!` even though `Error::AlreadyInitialized` already exists. That makes the API harder to handle from clients and inconsistent with the rest of the contract.

**Tasks:**
- Return `Err(Error::AlreadyInitialized)` instead of panicking
- Update any affected tests to use `try_initialize`

---

## #2 - Fix: oracle `initialize` still panics on re-initialization instead of returning `Error::AlreadyInitialized`
**Status:** Open - unassigned  
**Labels:** `bug`, `api`  
**Priority:** High  

**Description:**  
`OracleContract::initialize` also panics on duplicate initialization. A typed error would make contract integrations safer and more consistent.

**Tasks:**
- Change duplicate init path to return `Err(Error::AlreadyInitialized)`
- Update oracle tests to assert the typed error

---

## #3 - Fix: escrow instance storage TTL is never refreshed
**Status:** Open - unassigned  
**Labels:** `bug`, `storage`  
**Priority:** High  

**Description:**  
The escrow contract writes `Oracle`, `Admin`, `Paused`, and related instance keys, but unlike the oracle contract it never extends instance TTL. Long-lived deployments risk losing core config.

**Tasks:**
- Add an `extend_instance_ttl` helper to escrow
- Call it on every public method
- Add regression tests for instance TTL refresh

---

## #4 - Fix: `PlayerMatches` index entries do not get TTL extension
**Status:** Open - unassigned  
**Labels:** `bug`, `storage`  
**Priority:** Medium  

**Description:**  
`PlayerMatches(Address)` is written to persistent storage, but its TTL is never extended on write or read. Player history indexes can expire before the underlying matches do.

**Tasks:**
- Extend TTL when appending player match IDs
- Extend TTL when reading player match lists
- Add TTL coverage tests

---

## #5 - Fix: `ActiveMatches` index does not get TTL extension
**Status:** Open - unassigned  
**Labels:** `bug`, `storage`  
**Priority:** Medium  

**Description:**  
The active-match index is stored persistently but never has TTL refreshed. A hot deployment could lose its index while matches still exist.

**Tasks:**
- Extend TTL whenever `ActiveMatches` is written or read
- Add tests around index survival after ledger advancement

---

## #6 - Fix: reserved `GameId` entries do not get TTL extension
**Status:** Open - unassigned  
**Labels:** `bug`, `storage`  
**Priority:** Medium  

**Description:**  
The uniqueness guard for `game_id` relies on `DataKey::GameId`, but those reservation entries do not have their TTL extended. Duplicate games could become possible after expiry.

**Tasks:**
- Extend TTL when reserving a game ID
- Decide whether TTL should also be refreshed on reads
- Add a regression test

---

## #7 - Fix: allowed-token entries do not get TTL extension
**Status:** Open - unassigned  
**Labels:** `bug`, `storage`  
**Priority:** Medium  

**Description:**  
`AllowedToken(Address)` is stored persistently but never refreshed. Token allowlist state should not quietly disappear.

**Tasks:**
- Extend TTL when adding allowed tokens
- Extend TTL when checking or listing allowed tokens
- Add tests for ledger advancement

---

## #8 - Fix: `get_active_matches` currently includes pending matches
**Status:** Open - unassigned  
**Labels:** `bug`, `api`  
**Priority:** High  

**Description:**  
Matches are added to `ActiveMatches` at creation time, which means the current getter returns pending matches too. The name suggests fully funded or in-progress matches, so the behavior is misleading.

**Tasks:**
- Decide whether to rename the index or change when matches enter it
- Update implementation and docs to match the chosen meaning
- Add explicit tests

---

## #9 - Feature: add separate `get_pending_matches` and `get_live_matches` queries
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
Frontends need a reliable distinction between matches awaiting deposits and matches already active. Separate read methods would avoid overloading `get_active_matches`.

**Tasks:**
- Add distinct indexes or filtered getters
- Document exact semantics for each list
- Add tests for transitions between lists

---

## #10 - Fix: `cancel_match` does not set `completed_ledger` on terminal transition
**Status:** Open - unassigned  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**  
Completed matches record a terminal ledger, but cancelled matches do not. That makes lifecycle history inconsistent.

**Tasks:**
- Set `completed_ledger = Some(env.ledger().sequence())` on cancel
- Add tests for the stored value

---

## #11 - Fix: `expire_match` does not set `completed_ledger` on terminal transition
**Status:** Open - unassigned  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**  
Expired matches move to a terminal state but do not record when that happened. Historical queries lose useful metadata.

**Tasks:**
- Set `completed_ledger` during expiry
- Add a regression test

---

## #12 - Fix: `create_match` does not reject the escrow contract address as a player
**Status:** Open - unassigned  
**Labels:** `bug`, `security`  
**Priority:** High  

**Description:**  
The contract rejects self as oracle, but not self as `player1` or `player2`. Defensive validation should prevent matches that include the escrow contract itself.

**Tasks:**
- Reject `env.current_contract_address()` for both players
- Reuse `Error::InvalidPlayers` or add a clearer variant
- Add tests for both positions

---

## #13 - Refactor: replace boolean arithmetic in `get_escrow_balance` with explicit logic
**Status:** Open - unassigned  
**Labels:** `bug`, `cleanup`  
**Priority:** Low  

**Description:**  
`m.player1_deposited as i128 + m.player2_deposited as i128` works, but it is non-obvious and brittle. The read path should be clearer.

**Tasks:**
- Replace bool casts with explicit branching
- Keep behavior identical
- Add a short comment if needed

---

## #14 - Feature: clarify or replace `is_funded` semantics after payout
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
`is_funded` stays `true` after payout because deposit flags are never cleared. That is internally consistent, but many callers will read it as "funds are still in escrow."

**Tasks:**
- Decide whether to keep current behavior or add a new helper
- Consider `is_currently_escrowed` or `is_settled`
- Update docs and tests

---

## #15 - Feature: stop using `Winner::Draw` as the default unresolved winner value
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
New matches currently store `winner = Draw`, which makes unresolved and drawn matches indistinguishable unless the caller also inspects `state`.

**Tasks:**
- Replace with `Option<Winner>` or introduce an `Undecided` variant
- Update serialization and tests
- Document migration impact

---

## #16 - Feature: add `get_match_count()` public getter
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Low  

**Description:**  
`MatchCount` exists in storage but there is no public read method. Frontends need it for pagination and analytics.

**Tasks:**
- Add `get_match_count(env: Env) -> u64`
- Add tests after multiple `create_match` calls

---

## #17 - Feature: add paginated player-match queries
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
`get_player_matches` returns the full vector, which will become expensive over time. A paginated API would scale better.

**Tasks:**
- Add `(player, offset, limit)` or cursor-based pagination
- Keep existing getter or deprecate it clearly
- Add boundary tests

---

## #18 - Feature: add paginated active/live match queries
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
Like player match history, active/live queries should support pagination for frontend use.

**Tasks:**
- Add offset/limit or cursor support
- Define ordering guarantees
- Add tests for empty, partial, and full pages

---

## #19 - Feature: add batch `get_matches` read API
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
Frontends currently need one contract read per match ID. A batch getter would reduce RPC overhead.

**Tasks:**
- Add `get_matches(env, ids: Vec<u64>) -> Vec<Match>`
- Decide how missing IDs should be handled
- Add tests for mixed valid and invalid batches

---

## #20 - Feature: add `remove_allowed_token` admin function
**Status:** Open - unassigned  
**Labels:** `enhancement`, `admin`  
**Priority:** Medium  

**Description:**  
There is an admin path to add allowed tokens but no way to remove one. Operators need a clean rollback path for risky or deprecated assets.

**Tasks:**
- Add `remove_allowed_token`
- Decide what happens when the last token is removed
- Add auth and behavior tests

---

## #21 - Feature: add `is_token_allowed` getter
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Low  

**Description:**  
Clients should be able to check token status without probing `create_match`.

**Tasks:**
- Add a public boolean getter
- Add tests for allowed and unknown tokens

---

## #22 - Feature: add a way to list allowed tokens
**Status:** Open - unassigned  
**Labels:** `enhancement`, `api`  
**Priority:** Medium  

**Description:**  
Current storage supports membership checks only. Wallet and frontend flows need discoverability too.

**Tasks:**
- Add an indexed list of allowed tokens
- Keep it in sync with add/remove operations
- Add tests for ordering and deduplication

---

## #23 - Feature: emit a `deposit` event from escrow
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Medium  

**Description:**  
Deposits are currently silent. Off-chain consumers cannot detect partial or full funding without polling state.

**Tasks:**
- Publish a `("match", "deposit")` event
- Include `match_id`, `player`, and optionally new state
- Add event tests

---

## #24 - Feature: emit an event when match timeout is updated
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Low  

**Description:**  
`set_match_timeout` changes important economic behavior but leaves no on-chain event trail.

**Tasks:**
- Emit an admin event with old and new timeout
- Add tests

---

## #25 - Feature: emit events when the token allowlist changes
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Low  

**Description:**  
Token policy changes are operationally important and should be observable off-chain.

**Tasks:**
- Emit events for add and remove
- Include token address and caller context if useful
- Add tests

---

## #26 - Feature: emit events for `propose_admin` and `accept_admin`
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Low  

**Description:**  
The two-step admin transfer flow is good, but it is not externally observable today.

**Tasks:**
- Emit an event when a pending admin is proposed
- Emit an event when the pending admin accepts
- Add event tests

---

## #27 - Feature: emit an event when oracle admin is rotated
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Low  

**Description:**  
`update_admin` in the oracle contract silently changes authority. Indexers and operators need an audit trail.

**Tasks:**
- Emit an event with old and new admin
- Add tests

---

## #28 - Feature: emit an event when the oracle contract is unpaused
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Low  

**Description:**  
`pause` emits an event, but `unpause` does not. Admin state transitions should be symmetric.

**Tasks:**
- Publish an `unpaused` event
- Add tests

---

## #29 - Feature: emit an event when a result is deleted from oracle storage
**Status:** Open - unassigned  
**Labels:** `enhancement`, `events`  
**Priority:** Medium  

**Description:**  
`delete_result` removes audit data without an event. Even if deletion remains allowed, it should be visible.

**Tasks:**
- Emit a deletion event with `match_id`
- Add tests

---

## #30 - Feature: store `platform` in oracle `ResultEntry`
**Status:** Open - unassigned  
**Labels:** `enhancement`, `oracle`  
**Priority:** Medium  

**Description:**  
The oracle stores `game_id` and `result`, but not the platform source. That weakens the audit log for multi-platform support.

**Tasks:**
- Add `platform` to `ResultEntry`
- Update submit and read APIs
- Add tests

---

## #31 - Feature: store submission ledger or timestamp in oracle `ResultEntry`
**Status:** Open - unassigned  
**Labels:** `enhancement`, `oracle`  
**Priority:** Medium  

**Description:**  
There is no persistent record of when the oracle stored a result. Timestamps or ledger numbers would improve traceability.

**Tasks:**
- Add `submitted_ledger` or `submitted_at`
- Populate it on submit
- Add tests

---

## #32 - Feature: store the submitting admin address in oracle `ResultEntry`
**Status:** Open - unassigned  
**Labels:** `enhancement`, `oracle`  
**Priority:** Low  

**Description:**  
If admin rotation happens, the stored result no longer records which admin wrote it. Keeping that metadata improves auditability.

**Tasks:**
- Add a submitter field to `ResultEntry`
- Populate it during `submit_result`
- Add tests

---

## #33 - Fix: oracle docs claim escrow match validation that is not implemented
**Status:** Open - unassigned  
**Labels:** `bug`, `oracle`, `documentation`  
**Priority:** High  

**Description:**  
`oracle::submit_result` documentation says it verifies the match exists in escrow, but the implementation does not perform a cross-contract read.

**Tasks:**
- Either implement real validation or rewrite the docs
- Add tests if validation is added

---

## #34 - Feature: integrate oracle result storage with escrow payout flow
**Status:** Open - unassigned  
**Labels:** `enhancement`, `oracle`, `integration`  
**Priority:** High  

**Description:**  
The oracle contract is currently an audit log, while escrow pays out from a direct oracle-authenticated call. A tighter integration path would reduce ambiguity for integrators.

**Tasks:**
- Choose a single canonical result flow
- Implement contract-to-contract or explicit off-chain orchestration
- Add end-to-end tests

---

## #35 - Feature: decide whether cancelled or expired matches should release `game_id` for rematches
**Status:** Open - unassigned  
**Labels:** `enhancement`, `product`  
**Priority:** Medium  

**Description:**  
Right now every `game_id` becomes permanently reserved. That blocks rematch or restart flows that reuse the same platform game identifier after cancellation or timeout.

**Tasks:**
- Decide on intended uniqueness scope
- Update storage behavior accordingly
- Add tests and docs

---

## Testing Backlog

## #36 - Add Test: escrow duplicate initialize returns `AlreadyInitialized`
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** High  

**Description:**  
Once issue #1 is fixed, the duplicate init path should be covered with `try_initialize`.

**Tasks:**
- Initialize once
- Call `try_initialize` again
- Assert `Error::AlreadyInitialized`

---

## #37 - Add Test: oracle duplicate initialize returns `AlreadyInitialized`
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** High  

**Description:**  
Mirror issue #36 for the oracle contract after issue #2 is implemented.

**Tasks:**
- Initialize oracle once
- Retry via `try_initialize`
- Assert `Error::AlreadyInitialized`

---

## #38 - Add Test: `get_active_matches` excludes pending matches
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
This test should lock down the chosen semantics from issue #8.

**Tasks:**
- Create a match with no deposits
- Read active/live list
- Assert pending match presence or absence based on the new contract rule

---

## #39 - Add Test: `get_pending_matches` returns newly created matches
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
If issue #9 lands, there should be dedicated coverage for pending-only queries.

**Tasks:**
- Create at least two matches
- Assert both appear in pending results before funding

---

## #40 - Add Test: `get_live_matches` returns only fully funded matches
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
The live list should include only matches that reached `Active`.

**Tasks:**
- Create one pending and one funded match
- Assert only the funded match appears

---

## #41 - Add Test: `cancel_match` sets `completed_ledger`
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
Terminal cancellation metadata should be covered explicitly.

**Tasks:**
- Cancel a pending match
- Assert `completed_ledger.is_some()`

---

## #42 - Add Test: `expire_match` sets `completed_ledger`
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
Expiry should record the same terminal metadata as completion or cancellation.

**Tasks:**
- Advance the ledger past timeout
- Expire the match
- Assert `completed_ledger.is_some()`

---

## #43 - Add Test: `create_match` rejects the contract address as `player1`
**Status:** Open - unassigned  
**Labels:** `testing`, `security`  
**Priority:** High  

**Description:**  
This covers the first half of issue #12.

**Tasks:**
- Use the escrow contract address as `player1`
- Assert `Error::InvalidPlayers`

---

## #44 - Add Test: `create_match` rejects the contract address as `player2`
**Status:** Open - unassigned  
**Labels:** `testing`, `security`  
**Priority:** High  

**Description:**  
This covers the second half of issue #12.

**Tasks:**
- Use the escrow contract address as `player2`
- Assert `Error::InvalidPlayers`

---

## #45 - Add Test: `get_match_count` increments correctly
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
The getter from issue #16 should be validated across multiple creations.

**Tasks:**
- Create several matches
- Assert count after each create

---

## #46 - Add Test: `get_player_matches` preserves insertion order
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Frontends often rely on stable ordering. The contract should either guarantee it or document otherwise.

**Tasks:**
- Create multiple matches for the same player
- Assert returned IDs are in expected order

---

## #47 - Add Test: player history index excludes unrelated matches for other players
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
The repo already has coverage that a player's index returns all of their match IDs. A stronger follow-up is verifying the index does not leak unrelated matches from other player pairs.

**Tasks:**
- Create multiple matches for player A and separate matches for player B
- Assert player A only receives their own match IDs
- Assert player B only receives their own match IDs

---

## #48 - Add Test: player-match pagination handles empty and partial pages
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Pagination APIs should have direct boundary coverage.

**Tasks:**
- Query offset 0, middle page, and beyond end
- Assert lengths and IDs

---

## #49 - Add Test: active/live pagination handles empty and partial pages
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Mirror issue #48 for active/live queries.

**Tasks:**
- Create enough matches for multiple pages
- Assert pagination boundaries

---

## #50 - Add Test: active/live index ordering stays stable after cancellation gaps
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
The repo already covers lifecycle updates for the active match index. A useful follow-up is making sure cancellation does not leave the remaining IDs reordered in surprising ways.

**Tasks:**
- Create several matches that enter the same index
- Cancel one in the middle
- Assert remaining IDs preserve documented ordering

---

## #51 - Add Test: active/live index stays correct across concurrent cancellations and completions
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
Basic lifecycle coverage already exists. This follow-up should verify the index remains correct when different terminal transitions happen across multiple matches in the same run.

**Tasks:**
- Create at least three matches
- Cancel one and complete another
- Assert only the still-live match IDs remain

---

## #52 - Add Test: game ID reservation remains enforced after ledger advancement
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
Issue #6 needs a regression test to prove reserved game IDs survive long enough.

**Tasks:**
- Reserve a game ID
- Advance ledgers
- Assert duplicate create still fails

---

## #53 - Add Test: multiple approved tokens can coexist after allowlist enforcement is enabled
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
The repo already covers rejection of disallowed tokens. A better follow-up is asserting that once enforcement is on, more than one approved token can still be used successfully.

**Tasks:**
- Add two distinct allowed tokens
- Create matches using both approved tokens
- Assert both succeed while an unrelated token still fails

---

## #54 - Add Test: removed tokens can no longer be used for new matches
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
This covers issue #20 after removal support is added.

**Tasks:**
- Add then remove a token
- Assert new match creation rejects it

---

## #55 - Add Test: `is_token_allowed` returns false for unknown tokens
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Getter behavior should be explicit for unset values.

**Tasks:**
- Query a random token address
- Assert `false`

---

## #56 - Add Test: adding an allowed token emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
This is the event-side coverage for issue #25.

**Tasks:**
- Add a token
- Assert the event topics and payload

---

## #57 - Add Test: removing an allowed token emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
Removal should be just as observable as addition.

**Tasks:**
- Remove a previously allowed token
- Assert the event topics and payload

---

## #58 - Add Test: `deposit` emits an event for player1
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Medium  

**Description:**  
The first-funding path is the most important off-chain notification for UIs.

**Tasks:**
- Player1 deposits
- Assert a `deposit` event with `match_id` and player address

---

## #59 - Add Test: `deposit` emits an event for player2 and includes final state
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Medium  

**Description:**  
The second deposit is where the match becomes active, so that path should be asserted too.

**Tasks:**
- Player2 deposits second
- Assert event data reflects the transition

---

## #60 - Add Test: `set_match_timeout` emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
This covers the operational change introduced in issue #24.

**Tasks:**
- Set a new timeout
- Assert old/new values in event data

---

## #61 - Add Test: `propose_admin` stores the pending admin and emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
The first step of the two-step transfer should be verified on-chain.

**Tasks:**
- Propose a new admin
- Assert storage and emitted event

---

## #62 - Add Test: `accept_admin` finalizes the transfer and emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
The acceptance step needs its own explicit coverage.

**Tasks:**
- Propose then accept
- Assert new admin and emitted event

---

## #63 - Add Test: current admin retains privileges after `propose_admin` and before `accept_admin`
**Status:** Open - unassigned  
**Labels:** `testing`, `security`  
**Priority:** Medium  

**Description:**  
The repo already covers that acceptance is required before transfer takes effect. A focused follow-up is proving the current admin continues to work normally during the pending state.

**Tasks:**
- Propose a new admin
- Call an admin-only method from the current admin before acceptance
- Assert the action still succeeds

---

## #64 - Add Test: proposing a second pending admin cleanly replaces the first proposal
**Status:** Open - unassigned  
**Labels:** `testing`, `security`  
**Priority:** Medium  

**Description:**  
The two-step transfer flow should have clear behavior when the current admin changes their mind before acceptance. Right now that overwrite behavior is not explicitly tested.

**Tasks:**
- Propose pending admin A
- Propose pending admin B before acceptance
- Assert only pending admin B can finalize the transfer

---

## #65 - Add Test: oracle `update_admin` emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
This covers issue #27.

**Tasks:**
- Rotate the admin
- Assert event payload

---

## #66 - Add Test: oracle `unpause` emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
This covers issue #28.

**Tasks:**
- Pause then unpause
- Assert an `unpaused` event

---

## #67 - Add Test: oracle `delete_result` emits an event
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Medium  

**Description:**  
Deleting audit data should leave a visible event trail.

**Tasks:**
- Submit then delete a result
- Assert a deletion event

---

## #68 - Add Test: `delete_result` leaves `has_result` false
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
This is a basic state assertion for the oracle cleanup path.

**Tasks:**
- Submit a result
- Delete it
- Assert `has_result == false`

---

## #69 - Add Test: deleted oracle results can be resubmitted for the same match ID
**Status:** Open - unassigned  
**Labels:** `testing`, `oracle`  
**Priority:** Medium  

**Description:**  
If deletion is kept, the follow-up semantics should be explicit. Either resubmission should work, or deletion should be removed entirely.

**Tasks:**
- Submit a result
- Delete it
- Attempt a second submit
- Assert the chosen behavior

---

## #70 - Add Test: oracle `ResultEntry` stores the correct platform
**Status:** Open - unassigned  
**Labels:** `testing`, `oracle`  
**Priority:** Low  

**Description:**  
Coverage for issue #30 once that metadata is added.

**Tasks:**
- Submit a result with a known platform
- Assert the stored platform

---

## #71 - Add Test: oracle `ResultEntry` stores submission ledger or timestamp
**Status:** Open - unassigned  
**Labels:** `testing`, `oracle`  
**Priority:** Low  

**Description:**  
Coverage for issue #31 once submission metadata exists.

**Tasks:**
- Submit a result
- Assert `submitted_ledger` or `submitted_at`

---

## #72 - Add Test: oracle empty `game_id` rejection uses `try_submit_result`
**Status:** Open - unassigned  
**Labels:** `testing`, `api`  
**Priority:** Low  

**Description:**  
The invalid `game_id` path should be asserted with a typed error instead of a panic expectation.

**Tasks:**
- Call `try_submit_result` with `""`
- Assert `Error::InvalidGameId`

---

## #73 - Add Test: escrow empty `game_id` rejection uses `try_create_match`
**Status:** Open - unassigned  
**Labels:** `testing`, `api`  
**Priority:** Low  

**Description:**  
Mirror issue #72 for the escrow contract.

**Tasks:**
- Call `try_create_match` with `""`
- Assert `Error::InvalidGameId`

---

## #74 - Add Test: `get_oracle` returns the initialized address
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
There should be direct coverage for the read-only getter.

**Tasks:**
- Initialize contract
- Assert `get_oracle()` equals the configured address

---

## #75 - Add Test: `get_admin` returns the initialized address
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Mirror issue #74 for the admin getter.

**Tasks:**
- Initialize contract
- Assert `get_admin()` equals the configured address

---

## #76 - Add Test: `get_match_timeout` returns the default value before override
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
The default timeout constant is part of observable behavior and should have direct coverage.

**Tasks:**
- Call `get_match_timeout` before any update
- Assert the default value

---

## #77 - Add Test: `set_match_timeout` rejects non-admin caller
**Status:** Open - unassigned  
**Labels:** `testing`, `security`  
**Priority:** Medium  

**Description:**  
Admin-only access for timeout changes should be verified explicitly.

**Tasks:**
- Attempt `set_match_timeout` from a non-admin
- Assert `Unauthorized`

---

## #78 - Add Test: zero timeout is rejected
**Status:** Open - unassigned  
**Labels:** `testing`, `validation`  
**Priority:** Medium  

**Description:**  
If timeout validation is added, zero should be covered directly.

**Tasks:**
- Call `set_match_timeout(0)`
- Assert the chosen error

---

## #79 - Add Test: overly large timeout is rejected
**Status:** Open - unassigned  
**Labels:** `testing`, `validation`  
**Priority:** Low  

**Description:**  
If a max timeout is introduced, it should be regression-tested.

**Tasks:**
- Set an extreme timeout
- Assert the chosen error

---

## #80 - Add Test: lowering the timeout after match creation changes expiry eligibility as expected
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Medium  

**Description:**  
The repo already has direct coverage that a configured timeout is respected. A more interesting edge case is whether changing the timeout after a pending match already exists affects that match immediately.

**Tasks:**
- Create a pending match under the default timeout
- Lower the timeout before the match expires
- Assert expiry behavior matches the documented policy

---

## #81 - Add Test: `expire_match` emits the expected event payload
**Status:** Open - unassigned  
**Labels:** `testing`, `events`  
**Priority:** Low  

**Description:**  
The event exists, but its exact topics and payload should be locked down.

**Tasks:**
- Expire a match
- Assert `("match", "expired")` and `match_id`

---

## #82 - Add Test: cancellation records terminal metadata consistently
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
After issues #10 and #11, all terminal paths should be covered in one place.

**Tasks:**
- Cancel and expire separate matches
- Assert state plus terminal ledger metadata

---

## #83 - Add Test: unresolved matches do not expose a fake winner value
**Status:** Open - unassigned  
**Labels:** `testing`, `api`  
**Priority:** Medium  

**Description:**  
Coverage for issue #15 once outcome semantics are fixed.

**Tasks:**
- Create a new match
- Assert winner is `None` or `Undecided`

---

## #84 - Add Test: post-payout funding helper returns false if a new helper is introduced
**Status:** Open - unassigned  
**Labels:** `testing`, `api`  
**Priority:** Low  

**Description:**  
If issue #14 adds a clearer helper for current escrow status, it should be tested directly.

**Tasks:**
- Fund and complete a match
- Assert the new helper returns `false`

---

## #85 - Add Test: `get_escrow_balance` covers all explicit deposit branches
**Status:** Open - unassigned  
**Labels:** `testing`  
**Priority:** Low  

**Description:**  
Once issue #13 refactors the logic, each branch should have a direct assertion.

**Tasks:**
- Assert balance for 0, 1, and 2 deposits
- Assert 0 again after completion/cancellation

---

## #86 - Add Test: escrow instance TTL is refreshed after `create_match`
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Medium  

**Description:**  
Coverage for issue #3 should include creation paths.

**Tasks:**
- Inspect instance TTL before and after `create_match`
- Assert refresh

---

## #87 - Add Test: escrow instance TTL is refreshed after `deposit`
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Medium  

**Description:**  
Deposit is a hot path and should keep instance config alive too.

**Tasks:**
- Call `deposit`
- Assert instance TTL refresh

---

## #88 - Add Test: escrow instance TTL is refreshed after admin actions
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Medium  

**Description:**  
Pause, unpause, oracle rotation, and admin transfer should all refresh instance TTL.

**Tasks:**
- Exercise at least one admin path
- Assert instance TTL refresh

---

## #89 - Add Test: player-match index TTL refreshes on append and read
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Low  

**Description:**  
Coverage for issue #4 should include both write and getter paths.

**Tasks:**
- Create matches for one player
- Read the index
- Assert TTL extension

---

## #90 - Add Test: active/live index TTL refreshes on append and removal
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Low  

**Description:**  
The active/live list should survive both additions and removals over time.

**Tasks:**
- Create and complete a match
- Assert index TTL changes on both transitions

---

## #91 - Add Test: `remove_allowed_token` handles removing the last listed token cleanly
**Status:** Open - unassigned  
**Labels:** `testing`, `admin`  
**Priority:** Low  

**Description:**  
The last-token removal case often exposes edge-case state bugs around `AllowlistEnabled`.

**Tasks:**
- Add one token
- Remove it
- Assert the final allowlist state

---

## #92 - Add Test: `get_matches` batch getter handles duplicate IDs deterministically
**Status:** Open - unassigned  
**Labels:** `testing`, `api`  
**Priority:** Low  

**Description:**  
If issue #19 adds a batch getter, duplicate IDs should have clear behavior.

**Tasks:**
- Query a batch with repeated IDs
- Assert the documented result shape

---

## #93 - Add Test: `transfer_admin` and two-step admin transfer remain mutually consistent
**Status:** Open - unassigned  
**Labels:** `testing`, `admin`  
**Priority:** Low  

**Description:**  
There are now two admin-transfer paths. Their interaction should be tested explicitly.

**Tasks:**
- Use one path, then the other
- Assert final admin state matches expectations

---

## #94 - Add Test: oracle `delete_result` extends instance TTL
**Status:** Open - unassigned  
**Labels:** `testing`, `storage`  
**Priority:** Low  

**Description:**  
Unlike other oracle methods, `delete_result` currently does not call `extend_instance_ttl`. This should either be fixed and tested or documented.

**Tasks:**
- Measure instance TTL before and after deletion
- Assert refresh once implemented

---

## #95 - Add Test: oracle `delete_result` is blocked when paused if that policy is adopted
**Status:** Open - unassigned  
**Labels:** `testing`, `oracle`  
**Priority:** Low  

**Description:**  
Pause semantics for admin cleanup actions are currently implicit. If the team tightens them, they need tests too.

**Tasks:**
- Pause the oracle
- Attempt `delete_result`
- Assert the chosen behavior

---

## Documentation and Product Surface

## #96 - Fix: README public API section is out of sync with actual contract signatures
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** High  

**Description:**  
The README still shows simplified signatures like `create_match(stake_amount, token, game_id, platform)` and `deposit(match_id)`, which no longer match the code.

**Tasks:**
- Update README API examples
- Include player and admin/oracle details where needed

---

## #97 - Fix: README references nonexistent `verify_result` and `execute_payout` APIs
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** High  

**Description:**  
These API names appear in README even though payout happens inside `submit_result`.

**Tasks:**
- Remove or replace outdated function names
- Explain the current payout flow

---

## #98 - Fix: README feature list should reflect current token allowlist behavior
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Medium  

**Description:**  
The project now supports token addresses plus an allowlist model, but the README still reads like a simpler XLM/USDC-only design.

**Tasks:**
- Update feature and quick-start copy
- Clarify supported token policy

---

## #99 - Fix: README should document `is_funded` vs `get_escrow_balance` semantics
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Medium  

**Description:**  
This is one of the easiest places for integrators to misread the contract today.

**Tasks:**
- Add a note explaining deposit flags vs actual escrowed funds
- Include examples for active and completed matches

---

## #100 - Feature: add README guidance for admin and oracle governance flows
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Medium  

**Description:**  
The repository has pause, oracle rotation, admin transfer, and timeout controls, but the main docs barely surface them.

**Tasks:**
- Document admin-only operations
- Add a short operational section or link to deployment docs

---

## #101 - Feature: add a lifecycle and event reference table to README
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Low  

**Description:**  
Contributors and integrators both benefit from a compact match-state and event summary near the top-level docs.

**Tasks:**
- Add state transitions
- Add current event names and payload notes

---

## #102 - Fix: `docs/architecture.md` still exposes outdated API names
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** High  

**Description:**  
The architecture doc still lists `verify_result` and `execute_payout` as public functions even though they do not exist.

**Tasks:**
- Update function tables
- Make the architecture diagram match the real flow

---

## #103 - Feature: document index behavior, pagination strategy, and TTL caveats in architecture docs
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Medium  

**Description:**  
As the read surface grows, the docs should explain which getters are authoritative and how indexes behave over time.

**Tasks:**
- Document player-match and active/live indexes
- Add TTL notes and pagination guidance

---

## #104 - Fix: `docs/oracle.md` should explain the current oracle audit-log model clearly
**Status:** Open - unassigned  
**Labels:** `documentation`, `oracle`  
**Priority:** Medium  

**Description:**  
The repo has both an oracle contract and an escrow-side oracle address. That architecture needs clearer explanation for contributors.

**Tasks:**
- Document what the oracle contract does today
- Clarify whether it is authoritative or supplementary

---

## #105 - Feature: document oracle result deletion policy and trust assumptions
**Status:** Open - unassigned  
**Labels:** `documentation`, `security`  
**Priority:** Medium  

**Description:**  
Allowing an admin to delete audit entries is a meaningful trust decision that should be documented explicitly.

**Tasks:**
- Explain why deletion exists
- Document risks and expected operational use

---

## #106 - Fix: `docs/security.md` should cover panic-vs-error behavior and migration plans
**Status:** Open - unassigned  
**Labels:** `documentation`, `security`  
**Priority:** Medium  

**Description:**  
Error handling strategy is part of the contract surface, especially for clients using `try_` methods.

**Tasks:**
- Document current panic paths
- Update after issues #1 and #2 land

---

## #107 - Feature: update deployment docs to include token allowlist setup
**Status:** Open - unassigned  
**Labels:** `documentation`, `devops`  
**Priority:** Medium  

**Description:**  
Deploy steps should cover post-deploy admin actions that affect whether `create_match` succeeds in production.

**Tasks:**
- Add allowlist configuration commands
- Clarify when `AllowlistEnabled` turns on

---

## #108 - Feature: add deployment runbook for admin rotation and oracle rotation
**Status:** Open - unassigned  
**Labels:** `documentation`, `operations`  
**Priority:** Medium  

**Description:**  
The contracts now support authority changes, but operators need a safe step-by-step runbook.

**Tasks:**
- Document normal rotation steps
- Add rollback and verification steps

---

## #109 - Feature: add deployment runbook for pause and incident response
**Status:** Open - unassigned  
**Labels:** `documentation`, `security`  
**Priority:** Medium  

**Description:**  
Emergency controls are only useful if operators know how to use them under pressure.

**Tasks:**
- Document pause/unpause procedures
- Include what user-facing effects to expect

---

## #110 - Fix: roadmap docs should be synced with already-implemented contract features
**Status:** Open - unassigned  
**Labels:** `documentation`  
**Priority:** Low  

**Description:**  
The roadmap still frames some features as future work even though parts of them already exist in code.

**Tasks:**
- Review implemented vs planned items
- Update status labels and milestone text

---

## #111 - Fix: `demo/demo-script.md` should match the latest contract API and admin flow
**Status:** Open - unassigned  
**Labels:** `documentation`, `demo`  
**Priority:** Medium  

**Description:**  
The demo should reflect current token, timeout, and governance behavior so contributors do not learn an outdated flow.

**Tasks:**
- Update commands and sequence
- Add expected state/event checks

---

## #112 - Fix: `CONTRIBUTING.md` should prefer typed `try_` tests over `#[should_panic]` for contract errors
**Status:** Open - unassigned  
**Labels:** `documentation`, `testing`  
**Priority:** Medium  

**Description:**  
The codebase already mixes both styles. Contributor guidance should push toward more precise assertions.

**Tasks:**
- Add a testing conventions section
- Include example patterns

---

## #113 - Feature: document issue labels and priority meanings in `CONTRIBUTING.md`
**Status:** Open - unassigned  
**Labels:** `documentation`, `community`  
**Priority:** Low  

**Description:**  
The tracker format uses labels and priorities heavily, but the contribution docs do not define them.

**Tasks:**
- Add label meanings
- Add priority and sizing guidance

---

## #114 - Fix: `CODE_OF_CONDUCT.md` is currently empty
**Status:** Open - unassigned  
**Labels:** `documentation`, `community`  
**Priority:** High  

**Description:**  
The repo links to a code of conduct, but the file has no content. That is a clear contributor-experience gap.

**Tasks:**
- Add a complete policy, such as Contributor Covenant
- Link reporting and enforcement contacts

---

## #115 - Fix: README references a `LICENSE` file that is missing from the repo
**Status:** Open - unassigned  
**Labels:** `documentation`, `legal`  
**Priority:** High  

**Description:**  
Top-level docs claim the project is MIT licensed, but no `LICENSE` file is present in the repository.

**Tasks:**
- Add the intended license file
- Verify README and package metadata agree

---

## Contributor Tooling and Repository Hygiene

## #116 - Feature: add GitHub issue templates for bug, feature, testing, and docs work
**Status:** Open - unassigned  
**Labels:** `community`, `tooling`  
**Priority:** Low  

**Description:**  
Structured issue templates would help external contributors submit consistent reports and proposals.

**Tasks:**
- Add templates under `.github/ISSUE_TEMPLATE`
- Match the label/priority vocabulary used in the tracker

---

## #117 - Feature: add a pull request template focused on contracts, tests, and docs sync
**Status:** Open - unassigned  
**Labels:** `community`, `tooling`  
**Priority:** Low  

**Description:**  
This repo would benefit from a checklist that asks contributors about tests, docs, and event/API changes.

**Tasks:**
- Add `.github/pull_request_template.md`
- Include test and doc-impact prompts

---

## #118 - Feature: add `CODEOWNERS` or equivalent reviewer guidance
**Status:** Open - unassigned  
**Labels:** `community`, `tooling`  
**Priority:** Low  

**Description:**  
With separate escrow, oracle, and docs areas, clearer reviewer routing would speed up outside contributions.

**Tasks:**
- Add `CODEOWNERS` or reviewer docs
- Map contract and docs ownership areas

---

## #119 - Feature: add a label taxonomy document for maintainers and contributors
**Status:** Open - unassigned  
**Labels:** `community`, `documentation`  
**Priority:** Low  

**Description:**  
The project already uses labels like `bug`, `testing`, and `security`; a shared taxonomy would improve consistency.

**Tasks:**
- Define labels such as `good first issue`, `wave-ready`, and `high priority`
- Add the doc to `docs/` or `CONTRIBUTING.md`

---

## #120 - Feature: add a repository health checklist that validates referenced files exist
**Status:** Open - unassigned  
**Labels:** `tooling`, `documentation`  
**Priority:** Medium  

**Description:**  
The missing `LICENSE` and empty `CODE_OF_CONDUCT.md` show that documentation references can drift.

**Tasks:**
- Add a simple checklist or script
- Verify key top-level links and required files

---

## #121 - Feature: add a script or CI step that validates local Markdown links
**Status:** Open - unassigned  
**Labels:** `tooling`, `documentation`  
**Priority:** Medium  

**Description:**  
Broken repo-local links are easy to introduce and hard to notice without automation.

**Tasks:**
- Add a link checker step or script
- Cover `README.md`, `docs/`, and `demo/`

---

## #122 - Feature: add a docs consistency check for nonexistent API names in Markdown
**Status:** Open - unassigned  
**Labels:** `tooling`, `documentation`  
**Priority:** Medium  

**Description:**  
Docs currently mention APIs that do not exist. A lightweight check could prevent that class of drift.

**Tasks:**
- Define a small allowlist or generated API reference
- Fail when docs reference removed function names

---

## #123 - Refactor: split `contracts/escrow/src/tests.rs` into thematic modules
**Status:** Open - unassigned  
**Labels:** `cleanup`, `testing`  
**Priority:** Medium  

**Description:**  
The escrow test file is large and getting harder to navigate. Splitting by lifecycle, admin, events, and TTL would make contributor work easier.

**Tasks:**
- Break tests into smaller modules or files
- Preserve helper reuse and readability

---

## #124 - Refactor: move oracle inline tests out of `contracts/oracle/src/lib.rs`
**Status:** Open - unassigned  
**Labels:** `cleanup`, `testing`  
**Priority:** Medium  

**Description:**  
The oracle contract mixes implementation and a long inline test module in the same file. Separating them would improve maintainability.

**Tasks:**
- Move tests to `src/tests.rs` or submodules
- Keep imports and helpers tidy

---

## #125 - Feature: add shared test helpers plus invariant-style lifecycle tests
**Status:** Open - unassigned  
**Labels:** `testing`, `quality`  
**Priority:** Medium  

**Description:**  
Both contracts repeat setup code, and the suite would benefit from more reusable fixtures plus a few invariant-style checks around fund conservation and terminal states.

**Tasks:**
- Extract common test helpers
- Add invariant-oriented tests for escrow balance conservation and terminal transitions
