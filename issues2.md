# Checkmate-Escrow — Issue Tracker (Batch 2)

> Legend: 🆕 New (unassigned)

---

## 🆕 #65 — Add Test: player1 balance decreases by stake_amount after deposit
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
There is no test verifying that player1's token balance is reduced by exactly `stake_amount` after calling `deposit`. The existing `test_deposit_and_activate` only checks `is_funded` and `get_escrow_balance`, not the player's actual balance.

**Tasks:**
- Create match, record player1 balance before deposit
- Call `deposit` for player1
- Assert player1 balance decreased by `stake_amount`

---

## 🆕 #66 — Add Test: player2 balance decreases by stake_amount after deposit
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that player2's token balance is reduced by exactly `stake_amount` after calling `deposit`. Mirrors the gap identified for player1 in #65.

**Tasks:**
- Create match, record player2 balance before deposit
- Call `deposit` for player2
- Assert player2 balance decreased by `stake_amount`

---

## 🆕 #67 — Add Test: contract token balance equals 2x stake after both deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
After both players deposit, the contract address should hold exactly `2 * stake_amount` tokens. No test currently reads the contract's token balance directly to confirm this.

**Tasks:**
- Create match with `stake_amount = 100`
- Both players deposit
- Use `TokenClient::balance(&contract_id)` to assert balance is `200`

---

## 🆕 #68 — Fix: no way to retrieve the current oracle address from escrow contract
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no public getter for the oracle address stored in the escrow contract. Frontends and integrators cannot verify which oracle is trusted without reading raw storage.

**Tasks:**
- Add `get_oracle(env: Env) -> Address` read function returning `DataKey::Oracle`
- Add test asserting it returns the address passed to `initialize`

---

## 🆕 #69 — Add Test: get_match returns correct stake_amount and token
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_create_match` only asserts the returned `id` and `state`. No test verifies that `get_match` returns the correct `stake_amount` and `token` address that were passed to `create_match`.

**Tasks:**
- Create match with a specific `stake_amount` and `token`
- Call `get_match`
- Assert `m.stake_amount` and `m.token` match the inputs

---

## 🆕 #70 — Add Test: get_match returns correct player1 and player2 addresses
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test asserts that `get_match` returns the correct `player1` and `player2` addresses after creation.

**Tasks:**
- Create match with known player addresses
- Call `get_match`
- Assert `m.player1` and `m.player2` match the inputs

---

## 🆕 #71 — Add Test: get_match returns correct game_id and platform
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_match` returns the correct `game_id` string and `platform` enum value after creation.

**Tasks:**
- Create match with `game_id = "testgame1"` and `Platform::ChessDotCom`
- Call `get_match`
- Assert `m.game_id` and `m.platform` match the inputs

---

## 🆕 #72 — Add Test: multiple players can have independent concurrent matches
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that two separate pairs of players can run concurrent matches without interfering with each other's state, deposits, or payouts.

**Tasks:**
- Create match A (player1 vs player2) and match B (player3 vs player4)
- Both matches go through full deposit and payout lifecycle independently
- Assert each match's state and balances are correct and isolated

---

## 🆕 #73 — Add Test: paused contract rejects deposit
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that `deposit` is blocked when the contract is paused. `test_admin_pause_blocks_create_match` only covers `create_match`.

**Tasks:**
- Create a match before pausing
- Admin calls `pause()`
- Call `deposit` on the existing match
- Assert `Error::ContractPaused`

---

## 🆕 #74 — Add Test: oracle submit_result on non-existent match_id should return ResultNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`OracleContract::get_result` returns `Error::ResultNotFound` for unknown IDs, but there is no test confirming this behaviour.

**Tasks:**
- Call `get_result(9999)` on a fresh oracle contract
- Assert `Error::ResultNotFound` is returned

---

## 🆕 #75 — Fix: oracle contract missing pause/unpause mechanism
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The escrow contract has `pause()` and `unpause()` admin functions for emergency stops, but the oracle contract has no equivalent. A compromised oracle admin key could continue submitting fraudulent results with no way to halt the oracle independently.

**Tasks:**
- Add `DataKey::Paused` to oracle contract
- Add `pause()` and `unpause()` admin-only functions
- Guard `submit_result` with a paused check returning `Error::ContractPaused`
- Add tests for pause/unpause on oracle

---

## 🆕 #76 — Fix: oracle get_result does not extend storage TTL on read
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`OracleContract::get_result` reads from persistent storage but does not call `extend_ttl`. If a result entry is near expiry and is read without extension, it may expire before the next access, causing `ResultNotFound` errors for valid results.

**Tasks:**
- Add `env.storage().persistent().extend_ttl(&DataKey::Result(match_id), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS)` inside `get_result`
- Add test verifying TTL is extended after a `get_result` call

---

## 🆕 #77 — Fix: cancel_match allows active matches to be cancelled by either player
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`cancel_match` checks `m.state != MatchState::Pending` and returns `InvalidState`, which correctly blocks cancellation of `Active` matches. However, there is no dedicated test confirming this guard works, and the error message gives no indication of why cancellation was rejected.

**Tasks:**
- Add explicit test: fund both players (match becomes Active), call `cancel_match`, assert `Error::InvalidState`
- Consider adding a descriptive error variant `Error::MatchAlreadyActive`

---

## 🆕 #78 — Fix: submit_result in escrow does not verify caller is the registered oracle
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`submit_result` retrieves the oracle address and calls `oracle.require_auth()`, but the auth check is placed after the paused check. A non-oracle caller can observe whether the contract is paused before being rejected, leaking contract state. Auth must be the first check.

**Tasks:**
- Reorder `submit_result` so `oracle.require_auth()` is called before the paused check
- Add test confirming a non-oracle address receives `Unauthorized` even on a paused contract

---

## 🆕 #79 — Add Test: paused contract rejects deposit
**Status:** Open — unassigned
**Labels:** `testing`, `Stellar Wave`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Verify that calling `deposit` on a paused contract returns `Error::ContractPaused`. Currently only `create_match` is tested under pause conditions.

**Tasks:**
- Create a match (before pausing)
- Admin calls `pause()`
- Call `deposit` on the created match
- Assert `Error::ContractPaused` is returned

---

## 🆕 #80 — Fix: no integration test covering full match lifecycle end-to-end
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
There is no single test that exercises the complete match lifecycle: `create_match` → `deposit` (both players) → `submit_result` → verify final balances and match state. Individual unit tests exist but no end-to-end integration test.

**Tasks:**
- Write `test_full_match_lifecycle` covering all steps
- Assert match state transitions: `Pending` → `Active` → `Completed`
- Assert final token balances for winner and loser
- Assert `get_escrow_balance` returns `0` after payout

---

## 🆕 #81 — Fix: escrow contract does not validate that oracle address is not the zero address
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`initialize` accepts any `Address` as the oracle without checking it is a valid non-zero address. Passing a zero/default address would silently set a useless oracle, permanently blocking `submit_result`.

**Tasks:**
- Add a check that the oracle address is not the contract's own address or a known invalid address
- Document the expected oracle address format
- Add test for invalid oracle address rejection

---

## 🆕 #82 — Implement get_oracle View Function on Escrow Contract
**Status:** Open — unassigned
**Labels:** `enhancement`, `Stellar Wave`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no public getter for the oracle address stored in the escrow contract. Frontends and integrators cannot verify which oracle is trusted without reading raw storage.

**Tasks:**
- Add `pub fn get_oracle(env: Env) -> Result<Address, Error>` returning `DataKey::Oracle`
- Return `Error::Unauthorized` if not initialized
- Add test asserting it returns the address set at `initialize`

---

## 🆕 #83 — Implement is_paused View Function
**Status:** Open — unassigned
**Labels:** `enhancement`, `Stellar Wave`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no public getter for the paused state of the escrow contract. Frontends cannot check whether the contract is paused before attempting transactions, leading to confusing `ContractPaused` errors.

**Tasks:**
- Add `pub fn is_paused(env: Env) -> bool` returning `DataKey::Paused`
- Add test asserting `is_paused` returns `false` initially, `true` after `pause()`, and `false` after `unpause()`

---

## 🆕 #84 — Implement update_oracle Admin Function on Escrow Contract
**Status:** Open — unassigned
**Labels:** `enhancement`, `Stellar Wave`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle address is set once at `initialize` and cannot be changed. If the oracle service is compromised or needs to be rotated, there is no way to update it without redeploying the entire escrow contract.

**Tasks:**
- Add `pub fn update_oracle(env: Env, new_oracle: Address) -> Result<(), Error>` requiring admin auth
- Update `DataKey::Oracle` with the new address
- Add test for oracle rotation
- Add test that old oracle address is rejected after rotation

---

## 🆕 #85 — Implement Emergency Pause for Oracle Contract
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`, `Stellar Wave`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle contract has no emergency pause mechanism. If the oracle admin key is compromised, fraudulent results can be submitted indefinitely with no way to halt the oracle independently of the escrow contract.

**Tasks:**
- Add `DataKey::Paused` to oracle `DataKey` enum
- Add `pub fn pause(env: Env) -> Result<(), Error>` and `pub fn unpause(env: Env) -> Result<(), Error>` requiring admin auth
- Guard `submit_result` with paused check
- Add tests for pause/unpause behaviour on oracle

---

## 🆕 #86 — Implement Two-Step Admin Transfer for Both Contracts
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`, `Stellar Wave`
**Priority:** High
**Estimated Time:** 2 hours

**Description:**
Neither the escrow nor oracle contract has a safe admin transfer mechanism. A direct `set_admin(new_admin)` call risks permanently locking admin access if the wrong address is provided. A two-step transfer (propose → accept) prevents accidental lockout.

**Tasks:**
- Add `DataKey::PendingAdmin` to both contracts
- Add `propose_admin(new_admin: Address)` requiring current admin auth
- Add `accept_admin()` requiring new admin auth
- Add tests for successful transfer and rejection of unaccepted proposals

---

## 🆕 #87 — Fix: create_match allows player1 == player2 (self-match)
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
There is no check that `player1 != player2`. A single address can create a match against itself, deposit twice, and receive the full pot back, wasting ledger resources and potentially gaming any future reward systems.

**Tasks:**
- Add `if player1 == player2 { return Err(Error::InvalidPlayers) }` guard in `create_match`
- Add `InvalidPlayers` error variant to `errors.rs`
- Add test asserting self-match creation returns `Error::InvalidPlayers`

---

## 🆕 #88 — Fix: game_id is not validated for uniqueness — same game can be used in multiple matches
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The same `game_id` can be used to create multiple matches. An attacker could create duplicate matches for the same game and collect payouts multiple times if the oracle submits results for the same game ID.

**Tasks:**
- Track used `game_id` values in a `DataKey::GameId(String)` persistent entry
- Reject `create_match` if `game_id` already exists, returning `Error::AlreadyExists`
- Add test for duplicate game ID rejection

---

## 🆕 #89 — Fix: no timeout mechanism for abandoned matches
**Status:** Open — unassigned
**Labels:** `bug`, `enhancement`
**Priority:** Medium
**Estimated Time:** 2 hours

**Description:**
If player1 creates a match and deposits but player2 never deposits, player1's funds are locked indefinitely. There is no expiry or timeout that allows player1 to reclaim their stake automatically.

**Tasks:**
- Add `created_ledger: u32` field to `Match` struct, set via `env.ledger().sequence()`
- Add `expire_match(match_id: u64)` function that allows cancellation after a configurable ledger timeout
- Add `DataKey::MatchTimeout` for the configurable timeout value
- Add test for expiry-based cancellation

---

## 🆕 #90 — Fix: token address is not validated — any address can be passed as token
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
`create_match` accepts any `Address` as the `token` parameter with no validation. A malicious actor could pass a fake token contract that behaves unexpectedly during `transfer` calls, potentially draining the contract.

**Tasks:**
- Add `DataKey::AllowedToken(Address)` and an admin function `add_allowed_token(token: Address)` to manage the allowlist
- Reject `create_match` if token is not on the allowlist, returning `Error::InvalidToken`
- Add `InvalidToken` error variant
- Add tests for allowed and disallowed tokens

---

## 🆕 #91 — Fix: no way to query all matches for a player
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 2 hours

**Description:**
There is no index mapping a player address to their match IDs. Frontends must scan all match IDs to find a player's matches, which is impractical at scale.

**Tasks:**
- Add `DataKey::PlayerMatches(Address)` storing a `Vec<u64>` of match IDs
- Update `create_match` to append the new match ID to both players' index entries
- Add `get_player_matches(env: Env, player: Address) -> Vec<u64>` read function
- Add test asserting the index is populated correctly after match creation

---

## 🆕 #92 — Fix: oracle contract has no admin rotation mechanism
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle contract's admin address is set once at `initialize` and cannot be changed. If the oracle service key is compromised, there is no recovery path short of redeploying the contract.

**Tasks:**
- Add `update_admin(env: Env, new_admin: Address) -> Result<(), Error>` requiring current admin auth
- Update `DataKey::Admin` with the new address
- Add test for admin rotation
- Add test that old admin cannot call `submit_result` after rotation

---

## 🆕 #93 — Fix: submit_result does not validate that match is funded before paying out
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`submit_result` checks `m.state == Active` which implies both players deposited. However, if a state inconsistency bug exists, the contract could attempt to transfer more tokens than it holds, causing a panic rather than a graceful error.

**Tasks:**
- Add explicit `is_funded` check before computing `pot`
- Return `Error::NotFunded` if the match is not fully funded
- Add defensive test for this scenario

---

## 🆕 #94 — Fix: Match struct has no timestamp — no way to order or expire matches
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The `Match` struct stores no creation ledger sequence number. Without this, there is no way to implement timeouts, sort matches by age, or detect stale pending matches.

**Tasks:**
- Add `created_ledger: u32` field to `Match`, populated via `env.ledger().sequence()`
- Add test asserting `created_ledger` is non-zero after `create_match`
- Use this field as the basis for future timeout logic (see #89)

---

## 🆕 #95 — Fix: no way to list all active matches — no global match index
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 2 hours

**Description:**
There is no on-chain index of all match IDs. Frontends must iterate from `0` to `MatchCount` and call `get_match` for each, which is expensive and fragile if any match has expired from storage.

**Tasks:**
- Add `DataKey::ActiveMatches` storing a `Vec<u64>`
- Append on `create_match`, remove on `cancel_match` and `submit_result`
- Add `get_active_matches(env: Env) -> Vec<u64>` read function
- Add test asserting the index is updated correctly through the match lifecycle

---

## 🆕 #96 — Add Test: oracle has_result returns false before submit and true after
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly verifies the `has_result` function returns `false` before a result is submitted and `true` after. The existing `test_submit_and_get_result` only calls `has_result` after submission.

**Tasks:**
- Assert `has_result(0)` returns `false` on a fresh contract
- Submit a result for match `0`
- Assert `has_result(0)` returns `true`

---

## 🆕 #97 — Add Test: oracle submit_result emits correct event data
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`test_submit_result_emits_event` in the oracle tests verifies the event is emitted but only checks `Player1Wins`. No test covers `Player2Wins` or `Draw` result events.

**Tasks:**
- Add test for `MatchResult::Player2Wins` event emission
- Add test for `MatchResult::Draw` event emission
- Assert `match_id` and `result` in event data are correct for each case

---

## 🆕 #98 — Add Test: oracle duplicate submit_result returns AlreadySubmitted error code
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
`test_duplicate_submit_fails` uses `#[should_panic]` without asserting the specific error code. A `try_submit_result` variant should assert `Error::AlreadySubmitted` explicitly.

**Tasks:**
- Call `try_submit_result` twice for the same `match_id`
- Assert the second call returns `Err(Ok(Error::AlreadySubmitted))`

---

## 🆕 #99 — Fix: deposit event not emitted when match transitions to Active state
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
When the second player deposits and the match transitions from `Pending` to `Active`, no `match_activated` event is emitted. Frontends cannot detect when a match is ready to start without polling `get_match`.

**Tasks:**
- Emit a `("match", "activated")` event inside `deposit` when `m.state` transitions to `Active`
- Include `match_id` in event data
- Add test asserting the event is emitted only on the second deposit

---

## 🆕 #100 — Add Test: create_match increments MatchCount correctly across multiple matches
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `MatchCount` increments sequentially and that each new match receives the correct ID. The existing `test_create_match` only creates one match and asserts `id == 0`.

**Tasks:**
- Create 5 matches in sequence
- Assert returned IDs are `0`, `1`, `2`, `3`, `4`
- Assert `get_match(4)` returns the last created match correctly

---

## 🆕 #101 — Add Test: non-admin cannot call unpause
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** High
**Estimated Time:** 15 minutes

**Description:**
`test_admin_pause_blocks_create_match` verifies pause works but there is no test confirming that a non-admin address cannot call `unpause()` to re-enable a paused contract.

**Tasks:**
- Admin calls `pause()`
- Call `unpause()` from a non-admin address
- Assert auth failure (panic or `Error::Unauthorized`)

---

## 🆕 #102 — Add Test: cancel_match on Completed match returns InvalidState
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test explicitly verifies that `cancel_match` returns `Error::InvalidState` when called on a `Completed` match. The guard exists in code but is untested for this specific state.

**Tasks:**
- Create match, both players deposit, call `submit_result` to complete it
- Call `try_cancel_match` on the completed match
- Assert `Err(Ok(Error::InvalidState))`

---

## 🆕 #103 — Add Test: get_escrow_balance returns 0 after draw payout
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_draw_refund` verifies player balances are restored but does not assert that `get_escrow_balance` returns `0` after the draw payout, leaving a gap in escrow drain verification.

**Tasks:**
- Complete a match with `Winner::Draw`
- Assert `get_escrow_balance(match_id)` returns `0`

---

## 🆕 #104 — Add Test: submit_result with Winner::Player2 pays correct balances
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
`test_payout_winner` only tests `Winner::Player1`. There is no test verifying that `Winner::Player2` correctly transfers the full pot to player2 and leaves player1 with their post-deposit balance.

**Tasks:**
- Create match with `stake_amount = 100`, both players start with `1000`
- Both players deposit
- Call `submit_result` with `Winner::Player2`
- Assert player2 balance is `1100` and player1 balance is `900`
