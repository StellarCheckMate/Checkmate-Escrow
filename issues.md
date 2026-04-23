# Checkmate-Escrow — Issue Tracker

> Legend: ✅ Closed | 🔓 Open (assigned) | 🆕 New (unassigned)

---

## ✅ #1 — Fix: initialize can be called multiple times, overwriting oracle address
**Status:** Closed  
**Labels:** `bug`  
**Priority:** High  

**Description:**
`initialize` in the escrow contract had no guard against being called twice. A second call silently overwrote the trusted oracle address.

**Resolution:** Added `env.storage().instance().has(&DataKey::Oracle)` guard. Panics on re-initialization.

---

## ✅ #2 — Fix: oracle initialize can be called multiple times, overwriting admin
**Status:** Closed  
**Labels:** `bug`  
**Priority:** High  

**Description:**
`OracleContract::initialize` had no guard against re-initialization. Any caller could overwrite the admin address after deployment.

**Resolution:** Added `env.storage().instance().has(&DataKey::Admin)` guard. Panics on re-initialization.

---

## ✅ #3 — Fix: create_match allows zero stake_amount
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
`create_match` accepted `stake_amount = 0`, creating a match with no economic value.

**Resolution:** Added `if stake_amount <= 0 { return Err(Error::InvalidAmount) }` guard and `InvalidAmount` error variant.

---

## ✅ #4 — Fix: cancel_match only allows player1 to cancel — player2 has no recourse
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
Only `player1` could cancel a pending match. If player1 abandoned the match after player2 deposited, player2's funds were locked.

**Resolution:** Either player can now cancel a `Pending` match. Both receive refunds if applicable.

---

## ✅ #5 — Fix: submit_result does not validate game_id against match record
**Status:** Closed  
**Labels:** `bug`  
**Priority:** High  

**Description:**
The oracle submitted a `Winner` enum with no cross-check that the `match_id` corresponds to the correct game.

**Resolution:** `game_id` is now included in `submit_result` and validated against `m.game_id`.

---

## ✅ #6 — Fix: get_escrow_balance uses boolean arithmetic that silently truncates
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Low  

**Description:**
`get_escrow_balance` cast `bool` to `i128` — non-obvious and fragile.

**Resolution:** Replaced with explicit conditional logic.

---

## ✅ #7 — Fix: deposit does not check match is not already Cancelled or Completed
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
`deposit` only checked `m.state != MatchState::Pending` with no descriptive error for cancelled/completed matches.

**Resolution:** Explicit state check returns `Error::InvalidState` for non-Pending matches.

---

## ✅ #8 — Fix: oracle submit_result has no link back to escrow contract — results are siloed
**Status:** Closed  
**Labels:** `bug`  
**Priority:** High  

**Description:**
The oracle contract stored results independently but the escrow contract never read from it, making the oracle contract redundant.

**Resolution:** Architecture clarified — escrow accepts results directly from the oracle address; oracle contract serves as an independent audit log.

---

## ✅ #9 — Fix: MatchCount can overflow u64 with no guard
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Low  

**Description:**
`MatchCount` was incremented with no checked arithmetic. Overflow wraps silently in release mode.

**Resolution:** Uses `id.checked_add(1).ok_or(Error::Overflow)?` with `Overflow` error variant.

---

## ✅ #10 — Fix: cancel_match does not require player2 auth when player2 has deposited
**Status:** Closed  
**Labels:** `bug`, `security`  
**Priority:** High  

**Description:**
`cancel_match` only required `player1.require_auth()`. Player1 could cancel and refund player2 without player2's consent.

**Resolution:** Documented cancellation model — either player may cancel a Pending match; `caller.require_auth()` is enforced.

---

## ✅ #11 — Fix: Persistent storage entries have no TTL extension — data can expire
**Status:** Closed  
**Labels:** `bug`  
**Priority:** High  

**Description:**
All `Match` and `Result` entries were written to persistent storage with no TTL extension. Expired records caused `MatchNotFound` errors.

**Resolution:** `extend_ttl` called after every persistent write using `MATCH_TTL_LEDGERS = 518_400`.

---

## ✅ #12 — Fix: submit_result in escrow does not emit an event
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
Payouts triggered by `submit_result` were not observable off-chain.

**Resolution:** `env.events().publish` added with topics `("match", "completed")` and data `(match_id, winner)`.

---

## ✅ #14 — Fix: create_match does not emit an event
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
Match creation was not observable off-chain.

**Resolution:** Event emitted with `match_id`, `player1`, `player2`, `stake_amount`.

---

## ✅ #16 — Fix: cancel_match does not emit an event
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
Match cancellations were silent on-chain.

**Resolution:** Event emitted with `match_id` on cancellation.

---

## ✅ #17 — Fix: oracle submit_result does not emit an event
**Status:** Closed  
**Labels:** `bug`  
**Priority:** Medium  

**Description:**
Oracle result submissions were not observable off-chain.

**Resolution:** Event emitted with `match_id` and `result`.

---

## ✅ #19 — Fix: no admin role in escrow contract — no emergency controls
**Status:** Closed  
**Labels:** `bug`, `security`  
**Priority:** High  

**Description:**
The escrow contract had no admin address and no way to pause or respond to vulnerabilities without a full redeploy.

**Resolution:** `admin` added to `initialize`. `pause()` / `unpause()` admin functions implemented.

---

## ✅ #24 — Add Test: submit_result on already Completed match should return InvalidState
**Status:** Closed  
**Labels:** `testing`  
**Priority:** High  

**Description:**
Verify that calling `submit_result` twice on the same match returns `InvalidState` on the second call.

**Resolution:** Test added and passing.

---

## ✅ #25 — Add Test: cancel_match on Active match should return InvalidState
**Status:** Closed  
**Labels:** `testing`  
**Priority:** High  

**Description:**
Verify that a match cannot be cancelled once both players have deposited and it is `Active`.

**Resolution:** Test added and passing.

---

---

## 🔓 #5 — Fix: submit_result does not validate game_id against match record
**Status:** Open — assigned to **devoclan**  
**Labels:** `bug`  
**Priority:** High  
**Estimated Time:** 1 hour  

**Description:**
The oracle submits a `Winner` enum but there is no cross-check that the oracle's `match_id` corresponds to the correct game. A compromised oracle could submit a result for the wrong match ID.

**Tasks:**
- Include `game_id` in `submit_result` and verify it matches `m.game_id`
- Return `Error::GameIdMismatch` on mismatch
- Add test for mismatched game ID

---

## 🔓 #6 — Fix: get_escrow_balance uses boolean arithmetic that silently truncates
**Status:** Open — assigned to **devoclan**  
**Labels:** `bug`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
`get_escrow_balance` computes `(player1_deposited as i128 + player2_deposited as i128) * stake_amount`. Casting `bool` to `i128` is non-obvious and fragile.

**Tasks:**
- Replace with explicit match/if logic
- Add comment explaining the calculation
- Verify test coverage

---

## 🔓 #7 — Fix: deposit does not check match is not already Cancelled or Completed
**Status:** Open — assigned to **rayeberechi**  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
`deposit` only checks `m.state != MatchState::Pending` and returns `InvalidState` with no indication of why.

**Tasks:**
- Add explicit state checks with descriptive errors
- Add `Error::MatchCancelled` and `Error::MatchCompleted` variants
- Add tests for deposit into cancelled/completed match

---

## 🔓 #8 — Fix: deposit does not emit an event
**Status:** Open — assigned to **Froshboss**  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Player deposits are not observable off-chain. Frontends cannot notify the opponent that funds are ready without polling.

**Tasks:**
- Add `env.events().publish` in `deposit`
- Include `match_id` and `player` in event data
- Add test asserting event is emitted

---

## 🔓 #10 — Fix: oracle submit_result has no link back to escrow contract — results are siloed
**Status:** Open — assigned to **jhayniffy**  
**Labels:** `bug`  
**Priority:** High  
**Estimated Time:** 2 hours  

**Description:**
The oracle contract stores results independently but the escrow contract's `submit_result` does not read from the oracle contract — it accepts the result directly from the oracle address. The oracle contract's stored results are never used by the escrow.

**Tasks:**
- Decide on architecture: either escrow reads from oracle contract, or oracle calls escrow directly
- Implement the chosen integration
- Add integration test covering the full oracle → escrow flow

---

## 🔓 #15 — Fix: cancel_match does not require player2 auth when player2 has deposited
**Status:** Open — assigned to **jhayniffy**  
**Labels:** `bug`, `security`  
**Priority:** High  
**Estimated Time:** 1 hour  

**Description:**
`cancel_match` only requires `player1.require_auth()`. If player2 has already deposited, player1 can cancel and trigger a refund to player2 without player2's consent.

**Tasks:**
- Document the intended cancellation authorization model
- If cancellation after player2 deposit should require both players, enforce it
- Add test for cancellation with both deposits present

---

## 🔓 #20 — Fix: no mechanism to update oracle address post-deploy
**Status:** Open — assigned to **devoclan**  
**Labels:** `bug`  
**Priority:** High  
**Estimated Time:** 1 hour  

**Description:**
The oracle address is set once at `initialize` and cannot be changed. If the oracle service is compromised or needs to be rotated, there is no way to update it without redeploying the entire escrow contract.

**Tasks:**
- Add `update_oracle(new_oracle: Address)` admin function
- Require existing admin address to authorize
- Add test for oracle rotation

---

## 🔓 #21 — Fix: create_match allows player1 == player2 (self-match)
**Status:** Open — assigned to **rayeberechi**  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
There is no check that `player1 != player2`. A single address can create a match against itself, deposit twice, and receive the full pot back, wasting ledger resources.

**Tasks:**
- Add `if player1 == player2 { return Err(Error::InvalidPlayers) }` guard
- Add `InvalidPlayers` error variant
- Add test for self-match rejection

---

## 🔓 #22 — Fix: game_id is not validated for uniqueness — same game can be used in multiple matches
**Status:** Open — assigned to **devoclan**  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 1 hour  

**Description:**
The same `game_id` can be used to create multiple matches. An attacker could create duplicate matches for the same game and collect payouts multiple times.

**Tasks:**
- Track used `game_id` values in a `DataKey::GameId(String)` set
- Reject `create_match` if `game_id` already exists
- Add test for duplicate game ID rejection

---

## 🔓 #23 — Add Test: deposit by non-player address should return Unauthorized
**Status:** Open — assigned to **jhayniffy**  
**Labels:** `testing`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
Verify that calling `deposit` with an address that is neither `player1` nor `player2` returns `Error::Unauthorized`.

**Tasks:**
- Call `deposit` with a random third-party address
- Assert `Error::Unauthorized` is returned

---

## 🔓 #26 — Add Test: submit_result on Pending match (not yet Active) should return InvalidState
**Status:** Open — assigned to **JTKaduma**  
**Labels:** `testing`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
Verify that the oracle cannot submit a result for a match that has not yet reached `Active` state.

**Tasks:**
- Create match, do not deposit
- Call `submit_result`
- Assert `Error::InvalidState` is returned

---

## 🔓 #27 — Add Test: get_match on non-existent match_id should return MatchNotFound
**Status:** Open — assigned to **Inkman007**  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify that `get_match` returns `Error::MatchNotFound` for an ID that was never created.

**Tasks:**
- Call `get_match(999)`
- Assert `Error::MatchNotFound`

---

## 🔓 #28 — Add Test: Non-oracle address calling submit_result should return Unauthorized
**Status:** Open — assigned to **devoclan**  
**Labels:** `testing`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
Verify that only the registered oracle address can call `submit_result` on the escrow contract.

**Tasks:**
- Call `submit_result` from a random address (not the oracle)
- Assert auth error or `Error::Unauthorized`

---

---

## 🆕 #29 — Add Test: oracle get_result on non-existent match_id should return ResultNotFound
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify that `OracleContract::get_result` returns `Error::ResultNotFound` for a match ID with no submitted result.

**Tasks:**
- Call `get_result(999)` on a fresh oracle contract
- Assert `Error::ResultNotFound`

---

## 🆕 #30 — Add Test: is_funded returns false after only one player deposits
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify `is_funded` returns `false` after only player1 deposits and `true` only after both players deposit.

**Tasks:**
- Create match, player1 deposits
- Assert `is_funded` returns `false`
- Player2 deposits
- Assert `is_funded` returns `true`

---

## 🆕 #31 — Add Test: get_escrow_balance reflects correct amount at each deposit stage
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify `get_escrow_balance` returns `0`, `stake_amount`, and `2 * stake_amount` at each deposit stage.

**Tasks:**
- Assert balance is `0` before any deposit
- Assert balance is `stake_amount` after player1 deposits
- Assert balance is `2 * stake_amount` after player2 deposits

---

## 🆕 #32 — Add Test: Draw payout returns exact stake_amount to each player
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that a `Draw` result refunds exactly `stake_amount` to each player and leaves the contract with zero balance.

**Tasks:**
- Complete a match with `Winner::Draw`
- Assert both players received their original stake back
- Assert contract escrow balance is `0`

---

## 🆕 #33 — Fix: no timeout mechanism for abandoned matches
**Status:** Open — unassigned  
**Labels:** `bug`, `enhancement`  
**Priority:** Medium  
**Estimated Time:** 2 hours  

**Description:**
If player1 creates a match and deposits but player2 never deposits, player1's funds are locked indefinitely. There is no expiry or timeout that allows player1 to reclaim their stake automatically.

**Tasks:**
- Add a `created_at_ledger: u32` field to `Match`
- Add a `expire_match(match_id)` function that allows cancellation after a configurable ledger timeout
- Add test for expiry-based cancellation

---

## 🆕 #34 — Fix: token address is not validated — any address can be passed as token
**Status:** Open — unassigned  
**Labels:** `bug`, `security`  
**Priority:** High  
**Estimated Time:** 1 hour  

**Description:**
`create_match` accepts any `Address` as the `token` parameter with no validation. A malicious actor could pass a fake token contract that behaves unexpectedly during `transfer` calls.

**Tasks:**
- Maintain an allowlist of approved token addresses (XLM, USDC)
- Add `DataKey::AllowedToken(Address)` and an admin function to manage it
- Reject `create_match` if token is not on the allowlist
- Add tests for allowed and disallowed tokens

---

## 🆕 #35 — Fix: no way to query all matches for a player
**Status:** Open — unassigned  
**Labels:** `enhancement`  
**Priority:** Low  
**Estimated Time:** 2 hours  

**Description:**
There is no index mapping a player address to their match IDs. Frontends must scan all match IDs to find a player's matches, which is impractical at scale.

**Tasks:**
- Add `DataKey::PlayerMatches(Address)` storing a `Vec<u64>` of match IDs
- Update `create_match` to append to both players' index
- Add `get_player_matches(player: Address) -> Vec<u64>` read function
- Add test for player match index

---

## 🆕 #36 — Fix: oracle contract has no admin rotation mechanism
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** High  
**Estimated Time:** 1 hour  

**Description:**
The oracle contract's admin address is set once at `initialize` and cannot be changed. If the oracle service key is compromised, there is no recovery path.

**Tasks:**
- Add `update_admin(new_admin: Address)` requiring current admin auth
- Add test for admin rotation
- Add test that old admin cannot call after rotation

---

## 🆕 #37 — Fix: submit_result does not validate that match is funded before paying out
**Status:** Open — unassigned  
**Labels:** `bug`, `security`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
`submit_result` checks `m.state == Active` which implies both players deposited. However, if a state inconsistency bug exists, the contract could attempt to transfer more tokens than it holds, causing a panic.

**Tasks:**
- Add explicit `is_funded` check before computing `pot`
- Return `Error::NotFunded` if balance is insufficient
- Add defensive test for this scenario

---

## 🆕 #38 — Add Test: paused contract rejects create_match
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that calling `create_match` on a paused contract returns `Error::ContractPaused`.

**Tasks:**
- Admin calls `pause()`
- Call `create_match`
- Assert `Error::ContractPaused`

---

## 🆕 #39 — Add Test: paused contract rejects deposit
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that calling `deposit` on a paused contract returns `Error::ContractPaused`.

**Tasks:**
- Admin calls `pause()`
- Call `deposit`
- Assert `Error::ContractPaused`

---

## 🆕 #40 — Add Test: paused contract rejects submit_result
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that calling `submit_result` on a paused contract returns `Error::ContractPaused`.

**Tasks:**
- Admin calls `pause()`
- Call `submit_result`
- Assert `Error::ContractPaused`

---

## 🆕 #41 — Add Test: unpause restores normal contract operation
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that after `unpause()`, all contract functions work normally again.

**Tasks:**
- Pause then unpause the contract
- Call `create_match` and assert it succeeds
- Assert no `ContractPaused` error

---

## 🆕 #42 — Add Test: non-admin cannot call pause
**Status:** Open — unassigned  
**Labels:** `testing`, `security`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
Verify that only the admin address can call `pause()` and `unpause()`.

**Tasks:**
- Call `pause()` from a non-admin address
- Assert auth failure

---

## 🆕 #43 — Fix: cancel_match on a Completed match should return InvalidState, not silently succeed
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
`cancel_match` checks `m.state != MatchState::Pending` and returns `InvalidState`. This is correct but there is no test confirming the behaviour for a `Completed` match specifically.

**Tasks:**
- Add explicit test: complete a match, then call `cancel_match`, assert `InvalidState`

---

## 🆕 #44 — Fix: deposit emits no event when match transitions to Active
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
When the second player deposits and the match transitions from `Pending` to `Active`, no dedicated `match_activated` event is emitted. Frontends cannot detect when a match is ready to start without polling.

**Tasks:**
- Emit a `("match", "activated")` event when `m.state` transitions to `Active`
- Include `match_id` in event data
- Add test asserting the event is emitted on second deposit

---

## 🆕 #45 — Fix: oracle has_result is not authenticated — any caller can probe result existence
**Status:** Open — unassigned  
**Labels:** `enhancement`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
`has_result` is a public read function with no auth. While read-only, in a private tournament context leaking whether a result exists before announcement could be undesirable.

**Tasks:**
- Document the intentional public nature of `has_result`
- Or add an optional admin-gated version for private tournaments
- Add test confirming current behaviour

---

## 🆕 #46 — Add Test: create_match with negative stake_amount should return InvalidAmount
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 15 minutes  

**Description:**
Verify that `create_match` rejects negative `stake_amount` values with `Error::InvalidAmount`.

**Tasks:**
- Call `create_match` with `stake_amount = -100`
- Assert `Error::InvalidAmount`

---

## 🆕 #47 — Add Test: player cannot deposit twice for the same match
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
Verify that a player calling `deposit` a second time for the same match returns `Error::AlreadyFunded`.

**Tasks:**
- Player1 deposits successfully
- Player1 calls `deposit` again
- Assert `Error::AlreadyFunded`

---

## 🆕 #48 — Add Test: cancel_match refunds both players when both have deposited
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** High  
**Estimated Time:** 30 minutes  

**Description:**
Verify that cancelling a match where both players have deposited (but match is still `Pending` — edge case if state transition is delayed) correctly refunds both.

**Tasks:**
- Create match, both players deposit (match becomes Active — verify cancel returns InvalidState)
- Create match, only player1 deposits, cancel — assert player1 refunded, player2 unchanged
- Create match, only player2 deposits, cancel — assert player2 refunded, player1 unchanged

---

## 🆕 #49 — Fix: no documentation on expected game_id format
**Status:** Open — unassigned  
**Labels:** `documentation`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
The `game_id` field accepts any `String` with no documented format. Lichess and Chess.com use different ID formats. Without documentation, integrators may pass incorrect IDs.

**Tasks:**
- Add inline doc comments to `create_match` specifying expected `game_id` format per platform
- Update `docs/oracle.md` with examples
- Add validation or at minimum a length check

---

## 🆕 #50 — Fix: no way to retrieve the current oracle address from escrow contract
**Status:** Open — unassigned  
**Labels:** `enhancement`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
There is no public getter for the oracle address stored in the escrow contract. Frontends and integrators cannot verify which oracle is trusted without reading raw storage.

**Tasks:**
- Add `get_oracle(env: Env) -> Address` read function
- Add test asserting it returns the address set at `initialize`

---

---

## 🆕 #51 — Add Test: Player2 wins payout — player2 receives full pot
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
The existing `test_payout_winner` only tests `Winner::Player1`. There is no test verifying that `Winner::Player2` correctly transfers the full pot to player2.

**Tasks:**
- Create match, both players deposit
- Call `submit_result` with `Winner::Player2`
- Assert player2 balance is `1100` and player1 balance is `900`

---

## 🆕 #52 — Add Test: get_escrow_balance returns 0 after payout
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
After `submit_result` completes a match, the contract should hold zero tokens for that match. There is no test verifying the escrow is fully drained post-payout.

**Tasks:**
- Complete a match with `Winner::Player1`
- Assert `get_escrow_balance` returns `0`

---

## 🆕 #53 — Add Test: get_escrow_balance returns 0 after cancellation with no deposits
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Cancelling a match where neither player deposited should leave the escrow balance at `0`. No test currently covers this.

**Tasks:**
- Create match, cancel immediately (no deposits)
- Assert `get_escrow_balance` returns `0`

---

## 🆕 #54 — Add Test: multiple matches can be created with different game_ids
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify that `MatchCount` increments correctly and multiple independent matches can coexist with different IDs.

**Tasks:**
- Create 3 matches with different `game_id` values
- Assert IDs are `0`, `1`, `2`
- Assert each `get_match` returns the correct match data

---

## 🆕 #55 — Add Test: submit_result on a Cancelled match should return InvalidState
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 15 minutes  

**Description:**
Verify that the oracle cannot submit a result for a match that has already been cancelled.

**Tasks:**
- Create match, cancel it
- Call `submit_result`
- Assert `Error::InvalidState`

---

## 🆕 #56 — Add Test: deposit into a Cancelled match should return InvalidState
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 15 minutes  

**Description:**
Verify that a player cannot deposit into a match that has already been cancelled.

**Tasks:**
- Create match, cancel it
- Call `deposit`
- Assert `Error::InvalidState`

---

## 🆕 #57 — Add Test: deposit into a Completed match should return InvalidState
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 15 minutes  

**Description:**
Verify that a player cannot deposit into a match that has already been completed.

**Tasks:**
- Complete a match via `submit_result`
- Call `deposit` on the completed match
- Assert `Error::InvalidState`

---

## 🆕 #58 — Add Test: is_funded returns false on a fresh match with no deposits
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
Verify `is_funded` returns `false` immediately after match creation before any deposit.

**Tasks:**
- Create match
- Assert `is_funded` returns `false`

---

## 🆕 #59 — Add Test: cancel_match by unauthorized third party returns Unauthorized
**Status:** Open — unassigned  
**Labels:** `testing`, `security`  
**Priority:** High  
**Estimated Time:** 15 minutes  

**Description:**
`test_unauthorized_player_cannot_cancel` exists but uses `should_panic`. Add a `try_cancel_match` variant that asserts the specific `Error::Unauthorized` code.

**Tasks:**
- Call `try_cancel_match` with a third-party address
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #60 — Add Test: paused contract rejects submit_result
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
`test_admin_pause_blocks_create_match` exists but there is no test verifying `submit_result` is also blocked when paused.

**Tasks:**
- Create and fund a match
- Admin calls `pause()`
- Call `submit_result`
- Assert `Error::ContractPaused`

---

## 🆕 #61 — Add Test: paused contract rejects deposit
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
No test verifies that `deposit` is blocked when the contract is paused.

**Tasks:**
- Create a match
- Admin calls `pause()`
- Call `deposit`
- Assert `Error::ContractPaused`

---

## 🆕 #62 — Fix: submit_result does not verify the caller is the oracle before state check
**Status:** Open — unassigned  
**Labels:** `bug`, `security`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
In `submit_result`, the oracle auth check happens after the paused check but before the state check. A non-oracle caller could probe match state by observing whether they get `ContractPaused` vs `Unauthorized` vs `InvalidState`. Auth should be the very first check.

**Tasks:**
- Move `oracle.require_auth()` to before the paused check
- Add test confirming non-oracle gets `Unauthorized` even on a paused contract

---

## 🆕 #63 — Fix: Match struct has no timestamp — no way to order or expire matches
**Status:** Open — unassigned  
**Labels:** `enhancement`  
**Priority:** Medium  
**Estimated Time:** 1 hour  

**Description:**
The `Match` struct stores no creation ledger sequence number. Without this, there is no way to implement timeouts, sort matches by age, or detect stale pending matches.

**Tasks:**
- Add `created_ledger: u32` field to `Match`, set via `env.ledger().sequence()`
- Add `get_match` test asserting `created_ledger` is non-zero
- Use this field as the basis for future timeout logic (see #33)

---

## 🆕 #64 — Fix: no way to list all active matches — no global match index
**Status:** Open — unassigned  
**Labels:** `enhancement`  
**Priority:** Low  
**Estimated Time:** 2 hours  

**Description:**
There is no on-chain index of all match IDs. Frontends must iterate from `0` to `MatchCount` and call `get_match` for each, which is expensive and fragile if any match has expired from storage.

**Tasks:**
- Add `DataKey::ActiveMatches` storing a `Vec<u64>`
- Append on `create_match`, remove on `cancel_match` and `submit_result`
- Add `get_active_matches() -> Vec<u64>` read function
- Add tests for index correctness

---

## 🆕 #65 — Fix: oracle contract does not validate match_id is non-zero
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
`OracleContract::submit_result` accepts any `u64` as `match_id` including values that could never correspond to a real match. While harmless in isolation, it pollutes storage with orphaned result entries.

**Tasks:**
- Document that `match_id` must correspond to a real escrow match
- Or add a cross-contract call to verify the match exists before storing the result
- Add test for submitting result for a non-existent match ID

---

## 🆕 #66 — Fix: no event emitted when contract is paused or unpaused
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
`pause()` and `unpause()` change critical contract state but emit no events. Off-chain monitors cannot detect when the contract is paused without polling storage.

**Tasks:**
- Emit `("admin", "paused")` event in `pause()`
- Emit `("admin", "unpaused")` event in `unpause()`
- Add tests asserting events are emitted

---

## 🆕 #67 — Add Test: TTL is extended on cancel_match when player1 has deposited
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
`test_ttl_extended_on_cancel` cancels with no deposits. There is no test verifying TTL is still correctly extended when a deposit was made before cancellation.

**Tasks:**
- Create match, player1 deposits, then cancel
- Assert TTL equals `MATCH_TTL_LEDGERS`

---

## 🆕 #68 — Fix: get_match does not extend TTL on read — hot matches can still expire
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Medium  
**Estimated Time:** 30 minutes  

**Description:**
`get_match` reads from persistent storage but does not call `extend_ttl`. A match that is frequently read but not written (e.g., waiting for player2 to deposit) could expire if no write occurs within the TTL window.

**Tasks:**
- Add `extend_ttl` call in `get_match` after the read
- Add test verifying TTL is refreshed on read

---

## 🆕 #69 — Fix: no input length validation on game_id string
**Status:** Open — unassigned  
**Labels:** `bug`  
**Priority:** Low  
**Estimated Time:** 30 minutes  

**Description:**
`game_id` is stored as a `String` with no maximum length check. An excessively long `game_id` wastes ledger storage and could be used to inflate storage costs.

**Tasks:**
- Add a max length constant (e.g., `MAX_GAME_ID_LEN = 64`)
- Validate `game_id.len() <= MAX_GAME_ID_LEN` in `create_match`
- Add `Error::InvalidGameId` variant
- Add test for oversized `game_id`

---

## 🆕 #70 — Add Test: oracle has_result returns false before any submission
**Status:** Open — unassigned  
**Labels:** `testing`  
**Priority:** Low  
**Estimated Time:** 15 minutes  

**Description:**
No test verifies that `has_result` returns `false` for a match ID that has never had a result submitted.

**Tasks:**
- On a fresh oracle contract, call `has_result(0)`
- Assert it returns `false`

---

## Summary

| Status | Count |
|--------|-------|
| ✅ Closed | 13 |
| 🔓 Open (assigned) | 14 |
| 🆕 New (unassigned) | 92 |
| **Total** | **119** |

### Open Issues by Assignee
| Assignee | Issues |
|----------|--------|
| devoclan | #5, #6, #22, #28 |
| jhayniffy | #10, #15, #23 |
| rayeberechi | #7, #21 |
| Froshboss | #8 |
| JTKaduma | #26 |
| Inkman007 | #27 |

---

## 🆕 #71 — Fix: pause() and unpause() do not check if contract is already in that state
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`pause()` can be called when already paused, and `unpause()` when already unpaused, with no error. This wastes ledger fees and can confuse callers.

**Tasks:**
- Return `Error::InvalidState` if `pause()` is called while already paused
- Return `Error::InvalidState` if `unpause()` is called while already unpaused
- Add tests for both cases

---

## 🆕 #72 — Add Test: get_match returns correct player addresses
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly asserts that `get_match` returns the correct `player1` and `player2` addresses stored at creation time.

**Tasks:**
- Create a match with known player addresses
- Call `get_match` and assert `m.player1 == player1` and `m.player2 == player2`

---

## 🆕 #73 — Add Test: get_match returns correct stake_amount and token
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_match` returns the exact `stake_amount` and `token` address passed to `create_match`.

**Tasks:**
- Create a match with a specific stake and token
- Assert `m.stake_amount` and `m.token` match the inputs

---

## 🆕 #74 — Add Test: get_match returns correct platform field
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that the `platform` field (`Lichess` vs `ChessDotCom`) is stored and returned correctly by `get_match`.

**Tasks:**
- Create a match with `Platform::ChessDotCom`
- Assert `m.platform == Platform::ChessDotCom`

---

## 🆕 #75 — Add Test: get_match returns correct game_id
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly asserts that `get_match` returns the exact `game_id` string passed to `create_match`.

**Tasks:**
- Create a match with a known `game_id`
- Assert `m.game_id` equals the input string

---

## 🆕 #76 — Fix: initialize does not emit an event — no on-chain record of deployment config
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 20 minutes

**Description:**
`initialize` sets the oracle and admin addresses but emits no event. Off-chain monitors cannot detect when the contract was initialized or with which oracle address without reading raw storage.

**Tasks:**
- Emit `("admin", "initialized")` event with `oracle` and `admin` addresses
- Add test asserting the event is emitted on initialization

---

## 🆕 #77 — Fix: oracle initialize does not emit an event
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 20 minutes

**Description:**
`OracleContract::initialize` sets the admin but emits no event. Consistent event emission across all state-changing functions aids off-chain monitoring.

**Tasks:**
- Emit `("oracle", "initialized")` event with the admin address
- Add test asserting the event is emitted

---

## 🆕 #78 — Add Test: oracle submit_result with Draw result stores correctly
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The oracle `test_submit_and_get_result` only tests `Player1Wins`. No test verifies that `MatchResult::Draw` is stored and retrieved correctly.

**Tasks:**
- Submit a `Draw` result via oracle
- Assert `get_result` returns `MatchResult::Draw`

---

## 🆕 #79 — Add Test: oracle submit_result with Player2Wins stores correctly
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No oracle test covers `MatchResult::Player2Wins`. All three result variants should be tested independently.

**Tasks:**
- Submit a `Player2Wins` result
- Assert `get_result` returns `MatchResult::Player2Wins`

---

## 🆕 #80 — Fix: escrow contract has no get_admin function — admin address is opaque
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no public getter for the admin address in the escrow contract. Frontends and auditors cannot verify who the admin is without reading raw storage.

**Tasks:**
- Add `get_admin(env: Env) -> Address` read function
- Add test asserting it returns the address set at `initialize`

---

## 🆕 #81 — Fix: oracle contract has no get_admin function
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same as #80 but for the oracle contract. No public getter exists for the oracle admin address.

**Tasks:**
- Add `get_admin(env: Env) -> Address` to `OracleContract`
- Add test asserting it returns the address set at `initialize`

---

## 🆕 #82 — Add Test: deposit state transitions — player1_deposited flag set correctly
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly reads `m.player1_deposited` and `m.player2_deposited` flags after each deposit to confirm they are set correctly.

**Tasks:**
- After player1 deposits, assert `m.player1_deposited == true` and `m.player2_deposited == false`
- After player2 deposits, assert both flags are `true`

---

## 🆕 #83 — Add Test: match state is Pending after creation, Active after both deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
While `test_deposit_and_activate` implicitly covers this, no test explicitly asserts the state transition sequence: `Pending → Active`.

**Tasks:**
- Assert `m.state == Pending` after `create_match`
- Assert `m.state == Pending` after only player1 deposits
- Assert `m.state == Active` after both deposit

---

## 🆕 #84 — Add Test: match state is Completed after submit_result
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No dedicated test asserts the `Active → Completed` state transition for all three winner variants (`Player1`, `Player2`, `Draw`).

**Tasks:**
- For each winner variant, assert `m.state == Completed` after `submit_result`

---

## 🆕 #85 — Fix: no is_paused getter — callers cannot check pause state without a failed call
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no `is_paused() -> bool` function. Frontends must attempt a call and observe `ContractPaused` to detect pause state, which wastes fees.

**Tasks:**
- Add `is_paused(env: Env) -> bool` read function
- Add test asserting it returns `true` after `pause()` and `false` after `unpause()`

---

## 🆕 #86 — Add Test: get_escrow_balance on non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_escrow_balance` returns `Error::MatchNotFound` for a match ID that was never created.

**Tasks:**
- Call `get_escrow_balance(999)` on a fresh contract
- Assert `Error::MatchNotFound`

---

## 🆕 #87 — Add Test: is_funded on non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `is_funded` returns `Error::MatchNotFound` for a match ID that was never created.

**Tasks:**
- Call `is_funded(999)` on a fresh contract
- Assert `Error::MatchNotFound`

---

## 🆕 #88 — Fix: cancel_match does not emit event when no deposits were made
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_cancel_match_emits_event` cancels with no deposits. A separate test should confirm the event is also emitted when player1 has deposited before cancellation.

**Tasks:**
- Create match, player1 deposits, cancel
- Assert `("match", "cancelled")` event is emitted

---

## 🆕 #89 — Add Test: oracle TTL is extended on submit_result for all result types
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_ttl_extended_on_submit_result` in the oracle only tests `Player1Wins`. TTL extension should be verified for `Draw` and `Player2Wins` as well.

**Tasks:**
- Submit `Player2Wins` and `Draw` results
- Assert TTL equals `MATCH_TTL_LEDGERS` for each

---

## 🆕 #90 — Fix: submit_result in escrow does not extend TTL on the match after payout
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_ttl_extended_on_submit_result` passes, but there is no test verifying TTL is extended for `Winner::Draw` and `Winner::Player2` specifically. The TTL extension code runs unconditionally, but coverage is incomplete.

**Tasks:**
- Add TTL assertion tests for `Winner::Draw` and `Winner::Player2` paths

---

## 🆕 #91 — Fix: create_match does not validate that player1 and player2 are different contract addresses
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Related to #21 (self-match), but specifically: the contract address itself could be passed as a player. A match where `player1 == env.current_contract_address()` would allow the contract to authorize its own deposits.

**Tasks:**
- Add guard: `if player1 == env.current_contract_address() || player2 == env.current_contract_address() { return Err(Error::InvalidPlayers) }`
- Add test for this edge case

---

## 🆕 #92 — Add Test: MatchCount increments correctly across multiple create_match calls
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly reads `MatchCount` from storage to verify it increments correctly. The returned match ID is tested, but the counter itself is not directly verified.

**Tasks:**
- Create 5 matches and assert IDs are `0` through `4`
- Verify each `get_match(id)` returns the correct match

---

## 🆕 #93 — Fix: deposit does not emit event — no way to detect partial funding off-chain
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
This is a duplicate tracking issue for #15 (deposit event). Specifically, there is no `("match", "deposit")` event emitted when player1 deposits, making it impossible for player2 to be notified off-chain that their counterpart has funded.

**Tasks:**
- Emit `("match", "deposit")` with `(match_id, player)` in `deposit`
- Add test asserting event is emitted for both player1 and player2 deposits

---

## 🆕 #94 — Add Test: oracle get_result returns correct game_id field
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_submit_and_get_result` asserts `entry.result` but does not assert `entry.game_id`. The `game_id` field in `ResultEntry` is never explicitly tested.

**Tasks:**
- Submit a result with a known `game_id`
- Assert `entry.game_id` equals the submitted value

---

## 🆕 #95 — Fix: no way to get total match count — frontends cannot paginate matches
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`MatchCount` is stored in instance storage but there is no public getter. Frontends cannot know how many matches exist without reading raw storage.

**Tasks:**
- Add `get_match_count(env: Env) -> u64` read function
- Add test asserting it returns the correct count after several `create_match` calls

---
i stop at 96
## 🆕 #96 — Add Test: create_match with very large stake_amount (i128::MAX) does not panic
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies behaviour with extreme `stake_amount` values. `i128::MAX` as stake would cause `pot = stake_amount * 2` to overflow.

**Tasks:**
- Call `create_match` with `stake_amount = i128::MAX`
- Verify the contract handles this gracefully (either rejects or the overflow is caught)
- Add `Error::Overflow` guard in `submit_result` for pot calculation

---

## 🆕 #97 — Fix: submit_result pot calculation can overflow for large stake amounts
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`let pot = m.stake_amount * 2;` uses unchecked multiplication. If `stake_amount` is close to `i128::MAX / 2`, this overflows silently in release mode.

**Tasks:**
- Replace with `m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?`
- Add test with a stake near `i128::MAX / 2`

---

## 🆕 #98 — Add Test: cancel_match on a non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `cancel_match` returns `Error::MatchNotFound` for a match ID that was never created.

**Tasks:**
- Call `cancel_match(999, player1)`
- Assert `Error::MatchNotFound`

---

## 🆕 #99 — Add Test: deposit on a non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `deposit` returns `Error::MatchNotFound` for a match ID that was never created.

**Tasks:**
- Call `deposit(999, player1)`
- Assert `Error::MatchNotFound`

---

## 🆕 #100 — Add Test: submit_result on a non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `submit_result` returns `Error::MatchNotFound` for a match ID that was never created.

**Tasks:**
- Call `submit_result(999, Winner::Player1)`
- Assert `Error::MatchNotFound`

---

## 🆕 #101 — Fix: no way to check if contract is initialized — callers get confusing panics
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 20 minutes

**Description:**
If any function is called before `initialize`, the contract panics with `unwrap` failures rather than a clear error. There is no `is_initialized() -> bool` helper.

**Tasks:**
- Add `is_initialized(env: Env) -> bool` that checks `env.storage().instance().has(&DataKey::Oracle)`
- Add test asserting it returns `false` before init and `true` after

---

## 🆕 #102 — Add Test: player1 balance decreases by stake_amount after deposit
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_deposit_and_activate` checks `is_funded` and `get_escrow_balance` but does not assert the token balance of player1 decreased by `stake_amount` after their deposit.

**Tasks:**
- Assert `token_client.balance(&player1) == 900` after player1 deposits 100

---

## 🆕 #103 — Add Test: player2 balance decreases by stake_amount after deposit
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same as #102 but for player2. No test explicitly checks player2's token balance decreases after deposit.

**Tasks:**
- Assert `token_client.balance(&player2) == 900` after player2 deposits 100

---

## 🆕 #104 — Add Test: contract token balance equals 2x stake after both deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test reads the contract's actual token balance (via `token_client.balance(&contract_id)`) to verify it holds exactly `2 * stake_amount` after both players deposit.

**Tasks:**
- After both deposits, assert `token_client.balance(&contract_id) == 200`

---

## 🆕 #105 — Add Test: contract token balance is zero after winner payout
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test reads the contract's actual token balance after `submit_result` to confirm the escrow is fully drained.

**Tasks:**
- After `submit_result(Winner::Player1)`, assert `token_client.balance(&contract_id) == 0`

---

## 🆕 #106 — Add Test: contract token balance is zero after draw payout
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test reads the contract's actual token balance after a `Draw` result to confirm both refunds leave the contract with zero balance.

**Tasks:**
- After `submit_result(Winner::Draw)`, assert `token_client.balance(&contract_id) == 0`

---

## 🆕 #107 — Fix: oracle contract has no pause mechanism
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The escrow contract has `pause()`/`unpause()` for emergency stops, but the oracle contract has no equivalent. A compromised oracle admin could continue submitting results even if the escrow is paused.

**Tasks:**
- Add `pause()` / `unpause()` to `OracleContract` with admin auth
- Block `submit_result` when paused
- Add tests for paused oracle behaviour

---

## 🆕 #108 — Add Test: oracle submit_result is blocked when oracle contract is paused
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Depends on #107. Once oracle pause is implemented, verify `submit_result` returns an error when the oracle is paused.

**Tasks:**
- Pause oracle, call `submit_result`, assert error

---

## 🆕 #109 — Fix: no documentation for error codes — integrators cannot map numeric codes to meanings
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`Error` variants are assigned numeric codes (1–10) but there is no documentation mapping these codes to their meanings. Frontends that receive raw error codes cannot display meaningful messages.

**Tasks:**
- Add doc comments to each `Error` variant explaining when it is returned
- Add a section in `docs/` or README mapping error codes to descriptions

---

## 🆕 #110 — Fix: no documentation for oracle error codes
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 20 minutes

**Description:**
Same as #109 but for `oracle::errors::Error`. Variants `Unauthorized=1`, `AlreadySubmitted=2`, `ResultNotFound=3`, `AlreadyInitialized=4` have no doc comments.

**Tasks:**
- Add doc comments to each oracle `Error` variant

---

## 🆕 #111 — Add Test: create_match with empty string game_id should be rejected or documented
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies behaviour when `game_id` is an empty string `""`. An empty game ID is meaningless and could cause oracle lookup failures.

**Tasks:**
- Call `create_match` with `game_id = ""`
- Either assert it is rejected with `Error::InvalidGameId`, or document that empty IDs are allowed

---

## 🆕 #112 — Fix: Match struct does not store the winner after completion — historical queries lose winner info
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
After `submit_result` completes a match, the `Match` struct state becomes `Completed` but the winner is not stored. Querying `get_match` on a completed match gives no information about who won.

**Tasks:**
- Add `winner: Option<Winner>` field to `Match` struct
- Set it in `submit_result` before writing back to storage
- Add test asserting `get_match` returns the correct winner after completion

---

## 🆕 #113 — Add Test: get_match on a completed match returns Completed state
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly calls `get_match` after `submit_result` and asserts `m.state == Completed` for all three winner variants.

**Tasks:**
- For `Player1`, `Player2`, and `Draw`, assert `get_match` returns `Completed` state

---

## 🆕 #114 — Fix: no way to distinguish between "match never existed" and "match expired from storage"
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 1 hour

**Description:**
Both a never-created match and an expired match return `Error::MatchNotFound`. Callers cannot tell if the match existed and expired (TTL issue) or was never created.

**Tasks:**
- Document this limitation clearly in the API docs
- Consider adding a `MatchCount` check: if `match_id < MatchCount` but storage returns `None`, it likely expired

---

## 🆕 #115 — Add Test: oracle has_result returns true after submission and false before
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_submit_and_get_result` calls `has_result` after submission but does not assert it was `false` before. The before-state is never tested.

**Tasks:**
- Assert `has_result(0) == false` before any submission
- Submit result
- Assert `has_result(0) == true`

---

## 🆕 #116 — Fix: escrow contract does not validate that oracle address is not the zero address
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 20 minutes

**Description:**
`initialize` accepts any `Address` as oracle including potentially invalid addresses. While Soroban addresses are typed, documenting and testing that a valid non-zero oracle is required is important.

**Tasks:**
- Add a note in `initialize` doc comment that oracle must be a valid contract or account address
- Add test attempting to initialize with a freshly generated address (valid) vs documenting the constraint

---

## 🆕 #117 — Add Test: multiple players can have independent concurrent matches
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
No test verifies that two separate matches (different player pairs, different IDs) can run concurrently without interfering with each other's state or balances.

**Tasks:**
- Create two matches with different player pairs
- Fund and complete both independently
- Assert each match's state and balances are correct

---

## 🆕 #118 — Fix: cancel_match does not validate caller is not the contract itself
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`cancel_match` checks `caller == m.player1 || caller == m.player2` but does not guard against `caller == env.current_contract_address()`. While unlikely, it is a defensive gap.

**Tasks:**
- Add guard rejecting `env.current_contract_address()` as caller
- Add test for this edge case

---

## 🆕 #119 — Add Test: oracle admin can submit multiple results for different match IDs
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that the oracle admin can submit results for multiple different match IDs in sequence, and each is stored independently.

**Tasks:**
- Submit results for match IDs 0, 1, and 2 with different outcomes
- Assert each `get_result` returns the correct result for its ID

---

## 🆕 #120 — Fix: no integration test covering full match lifecycle end-to-end
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
There is no single test that covers the complete lifecycle: `initialize → create_match → deposit (p1) → deposit (p2) → oracle submit_result → escrow submit_result → payout`. Each step is tested in isolation but the full flow is never exercised together.

**Tasks:**
- Write an end-to-end integration test covering the full match lifecycle
- Include balance assertions at each step
- Cover both winner and draw outcomes
