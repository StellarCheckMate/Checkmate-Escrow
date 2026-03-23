## Fix: initialize can be called multiple times, overwriting oracle address
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`initialize` in the escrow contract has no guard against being called twice. A second call silently overwrites the trusted oracle address, allowing an attacker to substitute a malicious oracle.

**Tasks:**
- Check if `DataKey::Oracle` already exists before writing
- Return an error or panic if already initialized
- Add test for double-initialize rejection

---

## Fix: oracle initialize can be called multiple times, overwriting admin
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`OracleContract::initialize` has no guard against re-initialization. Any caller can overwrite the admin address after deployment.

**Tasks:**
- Check if `DataKey::Admin` already exists before writing
- Panic with structured error if already initialized
- Add test for double-initialize rejection

---

## Fix: create_match allows zero stake_amount
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`create_match` accepts `stake_amount = 0`, creating a match with no economic value. Both players can deposit 0 tokens and the oracle will pay out 0, wasting ledger storage.

**Tasks:**
- Add `if stake_amount <= 0 { return Err(Error::InvalidAmount) }` guard
- Add `InvalidAmount` error variant
- Add test for zero-stake rejection

---

## Fix: cancel_match only allows player1 to cancel — player2 has no recourse
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
Only `player1` (the match creator) can cancel a pending match. If player1 abandons the match after player2 has deposited, player2's funds are locked with no way to recover them.

**Tasks:**
- Allow either player to cancel if the match is still `Pending`
- Or allow player2 to cancel after a timeout if player1 has not deposited
- Add tests for player2-initiated cancellation

---

## Fix: submit_result does not validate winner against match players
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle submits a `Winner` enum (`Player1`, `Player2`, `Draw`) but there is no cross-check that the oracle's `match_id` corresponds to the correct game. A compromised oracle could submit a result for the wrong match ID.

**Tasks:**
- Include `game_id` in `submit_result` and verify it matches `m.game_id`
- Return `Error::GameIdMismatch` on mismatch
- Add test for mismatched game ID

---

## Fix: get_escrow_balance uses boolean arithmetic that silently truncates
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`get_escrow_balance` computes `(player1_deposited as i128 + player2_deposited as i128) * stake_amount`. Casting `bool` to `i128` is non-obvious and fragile. If the type ever changes this silently breaks.

**Tasks:**
- Replace with explicit match/if logic
- Add comment explaining the calculation
- Verify test coverage

---

## Fix: deposit does not check match is not already Cancelled or Completed
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`deposit` only checks `m.state != MatchState::Pending` and returns `InvalidState`. However the error message gives no indication of why. More importantly, a race condition could allow a deposit into a match being cancelled simultaneously.

**Tasks:**
- Add explicit state checks with descriptive errors
- Add `Error::MatchCancelled` and `Error::MatchCompleted` variants
- Add tests for deposit into cancelled/completed match

---

## Fix: oracle submit_result has no link back to escrow contract — results are siloed
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 2 hours

**Description:**
The oracle contract stores results independently but the escrow contract's `submit_result` does not read from the oracle contract — it accepts the result directly from the oracle address. This means the oracle contract's stored results are never used by the escrow, making it redundant.

**Tasks:**
- Decide on architecture: either escrow reads from oracle contract, or oracle calls escrow directly
- Implement the chosen integration
- Add integration test covering the full oracle → escrow flow

---

## Fix: MatchCount can overflow u64 with no guard
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`MatchCount` is incremented with `id + 1` and stored as `u64`. While overflow is practically unlikely, there is no checked arithmetic. In Soroban, integer overflow panics in debug but wraps in release.

**Tasks:**
- Use `id.checked_add(1).ok_or(Error::Overflow)?`
- Add `Overflow` error variant
- Add comment documenting the guard

---

## Fix: cancel_match does not require player2 auth when player2 has deposited
**Labels:** `bug`, `security`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
`cancel_match` only requires `player1.require_auth()`. If player2 has already deposited, player1 can cancel and trigger a refund to player2 without player2's consent. While the refund is correct, this could be used to grief player2 mid-game if the match state transitions are not atomic.

**Tasks:**
- Document the intended cancellation authorization model
- If cancellation after player2 deposit should require both players, enforce it
- Add test for cancellation with both deposits present

---

## Fix: Persistent storage entries have no TTL extension — data can expire
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 2 hours

**Description:**
All `Match` and `Result` entries are written to persistent storage with no TTL extension calls. Soroban persistent storage entries expire after their TTL elapses. Expired match records would cause `MatchNotFound` errors for active matches.

**Tasks:**
- Add `env.storage().persistent().extend_ttl(key, threshold, extend_to)` after every persistent write
- Define a `MATCH_TTL_LEDGERS` constant
- Add tests verifying TTL is set

---

## Fix: submit_result in escrow does not emit an event
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
Payouts triggered by `submit_result` are not observable off-chain without polling storage. Off-chain indexers and frontends cannot detect match completions.

**Tasks:**
- Add `env.events().publish` in `submit_result` after payout
- Define event topics: `(Symbol::new("match"), Symbol::new("completed"))`
- Include `match_id` and `winner` in event data
- Add test asserting event is emitted

---

## Fix: create_match does not emit an event
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Match creation is not observable off-chain. Frontends must poll storage to discover new matches.

**Tasks:**
- Add `env.events().publish` in `create_match`
- Include `match_id`, `player1`, `player2`, `stake_amount` in event data
- Add test asserting event is emitted

---

## Fix: deposit does not emit an event
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Player deposits are not observable off-chain. Frontends cannot notify the opponent that funds are ready without polling.

**Tasks:**
- Add `env.events().publish` in `deposit`
- Include `match_id` and `player` in event data
- Add test asserting event is emitted

---

## Fix: cancel_match does not emit an event
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Match cancellations are silent on-chain. Players and frontends cannot detect cancellations without polling.

**Tasks:**
- Add `env.events().publish` in `cancel_match`
- Include `match_id` in event data
- Add test asserting event is emitted

---

## Fix: oracle submit_result does not emit an event
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Oracle result submissions are not observable off-chain. The escrow contract and external listeners cannot react to new results without polling.

**Tasks:**
- Add `env.events().publish` in `OracleContract::submit_result`
- Include `match_id` and `result` in event data
- Add test asserting event is emitted

---

## Fix: no mechanism to update oracle address post-deploy
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle address is set once at `initialize` and cannot be changed. If the oracle service is compromised or needs to be rotated, there is no way to update it without redeploying the entire escrow contract.

**Tasks:**
- Add `update_oracle(new_oracle: Address)` admin function
- Require existing oracle or a separate admin address to authorize
- Add test for oracle rotation

---

## Fix: no admin role in escrow contract — no emergency controls
**Labels:** `bug`, `security`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** High
**Estimated Time:** 2 hours

**Description:**
The escrow contract has no admin address. There is no way to pause the contract, recover stuck funds, or respond to a critical vulnerability without a full redeploy.

**Tasks:**
- Add `admin: Address` parameter to `initialize`
- Store admin in `DataKey::Admin`
- Add `pause()` / `unpause()` admin functions
- Add test for admin-only operations

---

## Fix: create_match allows player1 == player2 (self-match)
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
There is no check that `player1 != player2`. A single address can create a match against itself, deposit twice, and receive the full pot back (minus any fees), wasting ledger resources.

**Tasks:**
- Add `if player1 == player2 { return Err(Error::InvalidPlayers) }` guard
- Add `InvalidPlayers` error variant
- Add test for self-match rejection

---

## Fix: game_id is not validated for uniqueness — same game can be used in multiple matches
**Labels:** `bug`
**Body:**
**Category:** Smart Contract - Bug
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The same `game_id` can be used to create multiple matches. An attacker could create duplicate matches for the same game and collect payouts multiple times if the oracle submits results for each match ID.

**Tasks:**
- Track used `game_id` values in a `DataKey::GameId(String)` set
- Reject `create_match` if `game_id` already exists
- Add test for duplicate game ID rejection

---

## Add Test: deposit by non-player address should return Unauthorized
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
Verify that calling `deposit` with an address that is neither `player1` nor `player2` returns `Error::Unauthorized`.

**Tasks:**
- Call `deposit` with a random third-party address
- Assert `Error::Unauthorized` is returned
- Add to test suite

---

## Add Test: submit_result on Pending match (not yet Active) should return InvalidState
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
Verify that the oracle cannot submit a result for a match that has not yet reached `Active` state (i.e., both players haven't deposited).

**Tasks:**
- Create match, do not deposit
- Call `submit_result`
- Assert `Error::InvalidState` is returned

---

## Add Test: submit_result on already Completed match should return InvalidState
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
Verify that calling `submit_result` twice on the same match panics or returns `InvalidState` on the second call.

**Tasks:**
- Complete a match with `submit_result`
- Call `submit_result` again on the same match
- Assert `Error::InvalidState`

---

## Add Test: cancel_match on Active match should return InvalidState
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
Verify that a match cannot be cancelled once both players have deposited and it is `Active`.

**Tasks:**
- Create match, both players deposit (match becomes Active)
- Call `cancel_match`
- Assert `Error::InvalidState`

---

## Add Test: get_match on non-existent match_id should return MatchNotFound
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Verify that `get_match` returns `Error::MatchNotFound` for an ID that was never created.

**Tasks:**
- Call `get_match(999)`
- Assert `Error::MatchNotFound`

---

## Add Test: is_funded returns false after only one player deposits
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Verify that `is_funded` returns `false` when only one of the two players has deposited.

**Tasks:**
- Create match, only player1 deposits
- Assert `is_funded` returns `false`
- Deposit player2, assert `is_funded` returns `true`

---

## Add Test: get_escrow_balance reflects correct amount at each deposit stage
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Verify `get_escrow_balance` returns `0`, `stake_amount`, and `2 * stake_amount` at the correct stages.

**Tasks:**
- Assert balance is `0` before any deposit
- Assert balance is `stake_amount` after player1 deposits
- Assert balance is `2 * stake_amount` after player2 deposits

---

## Add Test: Draw payout returns exact stake_amount to each player
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
Verify that in a draw, each player receives exactly their original `stake_amount` back and the contract balance returns to zero.

**Tasks:**
- Record player balances before deposit
- Complete match with `Winner::Draw`
- Assert each player's balance equals pre-deposit balance
- Assert contract escrow balance is zero

---

## Add Test: Non-oracle address calling submit_result should return Unauthorized
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
Verify that only the registered oracle address can call `submit_result` on the escrow contract.

**Tasks:**
- Call `submit_result` from a random address (not the oracle)
- Assert auth error or `Error::Unauthorized`

---

## Add Test: oracle get_result on non-existent match_id should return ResultNotFound
**Labels:** `testing`
**Body:**
**Category:** Smart Contract - Testing
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Verify that `OracleContract::get_result` returns `Error::ResultNotFound` for a match ID with no submitted result.

**Tasks:**
- Call `get_result(999)` on a fresh oracle contract
- Assert `Error::ResultNotFound`

---

## Add GitHub Actions CI — Run cargo test and cargo clippy on Every PR
**Labels:** `infrastructure`
**Body:**
**Category:** Infrastructure - CI/CD
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
There is no CI pipeline. Add a GitHub Actions workflow that runs `cargo test` and `cargo clippy -- -D warnings` on every pull request to prevent regressions and enforce lint standards.

**Tasks:**
- Create `.github/workflows/ci.yml`
- Run `cargo test` on PR and push to `main`
- Run `cargo clippy -- -D warnings`
- Cache cargo dependencies for faster runs
- Add status badge to README
