# Checkmate-Escrow — Issue Tracker (Batch 3)

> Legend: 🆕 New (unassigned)

---

## 🆕 #105 — Add Test: expire_match refunds player1 when only player1 deposited
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that `expire_match` correctly refunds only player1 when player1 deposited but player2 never did, and the timeout has elapsed.

**Tasks:**
- Create match, player1 deposits, advance ledger past timeout
- Call `expire_match`
- Assert player1 balance is restored to initial amount
- Assert player2 balance is unchanged

---

## 🆕 #106 — Add Test: expire_match fails before timeout elapses
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test confirms that `expire_match` returns `Error::MatchNotExpired` when called before the configured timeout has elapsed.

**Tasks:**
- Create match, player1 deposits
- Call `try_expire_match` without advancing the ledger
- Assert `Err(Ok(Error::MatchNotExpired))`

---

## 🆕 #107 — Add Test: expire_match on Active match returns InvalidState
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
`expire_match` should only work on `Pending` matches. No test verifies it returns `Error::InvalidState` when called on an `Active` match.

**Tasks:**
- Create match, both players deposit (match becomes Active)
- Advance ledger past timeout
- Call `try_expire_match`
- Assert `Err(Ok(Error::InvalidState))`

---

## 🆕 #108 — Add Test: expire_match emits expired event
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`expire_match` emits a `("match", "expired")` event but no test verifies this event is emitted with the correct `match_id`.

**Tasks:**
- Create match, advance ledger past timeout
- Call `expire_match`
- Assert event with topics `("match", "expired")` and data `match_id` is present

---

## 🆕 #109 — Fix: update_oracle does not validate new oracle is not the contract itself
**Status:** Open — unassigned
**Labels:** `bug`, `security`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
`update_oracle` sets the new oracle address without checking it is not the escrow contract's own address. Setting the oracle to the contract itself would allow anyone to satisfy `oracle.require_auth()` trivially.

**Tasks:**
- Add `if new_oracle == env.current_contract_address() { return Err(Error::InvalidAddress) }` guard
- Add test asserting self-address is rejected

---

## 🆕 #110 — Add Test: update_oracle rejects non-admin caller
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** High
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `update_oracle` can only be called by the admin. A non-admin caller should be rejected.

**Tasks:**t

- Call `try_update_oracle` from a non-admin address
- Assert auth failure

---

## 🆕 #111 — Add Test: update_oracle emits oracle_up event with old and new addresses
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`update_oracle` emits an `("admin", "oracle_up")` event with the old and new oracle addresses, but no test verifies the event data is correct.

**Tasks:**
- Call `update_oracle` with a new oracle address
- Assert event topics are `("admin", "oracle_up")`
- Assert event data contains `(old_oracle, new_oracle)`

---

## 🆕 #112 — Add Test: deposit after cancel_match returns InvalidState
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `deposit` on a `Cancelled` match returns `Error::InvalidState`. The guard exists but is untested for this specific state.

**Tasks:**
- Create match, player1 cancels it
- Call `try_deposit` for player2
- Assert `Err(Ok(Error::InvalidState))`

---

## 🆕 #113 — Add Test: get_match on non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_match` returns `Error::MatchNotFound` for an ID that was never created.

**Tasks:**
- Call `try_get_match(9999)` on a fresh contract
- Assert `Err(Ok(Error::MatchNotFound))`

---

## 🆕 #114 — Add Test: is_funded returns false when only player1 deposited
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly checks that `is_funded` returns `false` after only player1 deposits. The existing test only checks the final `true` state.

**Tasks:**
- Create match, player1 deposits
- Assert `is_funded` returns `false`
- Player2 deposits
- Assert `is_funded` returns `true`

---

## 🆕 #115 — Add Test: cancel_match by player2 refunds both players
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that player2 can cancel a pending match and that both players receive their deposits back when both have deposited.

**Tasks:**
- Create match, both players deposit
- Wait — match is now Active, so create a separate scenario: player1 deposits, player2 cancels
- Assert player1 is refunded their stake
- Assert player2 (who hadn't deposited) has unchanged balance

---

## 🆕 #116 — Fix: pause() does not emit a paused event
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`unpause()` emits an `("admin", "unpaused")` event, but `pause()` emits nothing. Off-chain monitors cannot detect when the contract is paused without polling `is_paused`.

**Tasks:**
- Add `env.events().publish(("admin", "paused"), ())` inside `pause()`
- Add test asserting the event is emitted when `pause()` is called

---

## 🆕 #117 — Fix: cancel_match does not extend TTL after state update
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`cancel_match` sets the match state to `Cancelled` and writes it back to persistent storage, but the TTL extension call is present. Verify the TTL is correctly extended and add a test confirming it.

**Tasks:**
- Add test reading TTL after `cancel_match`
- Assert TTL equals `MATCH_TTL_LEDGERS`

---

## 🆕 #118 — Add Test: submit_result on non-existent match_id returns MatchNotFound
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `submit_result` returns `Error::MatchNotFound` when called with an unknown `match_id`.

**Tasks:**
- Call `try_submit_result(9999, Winner::Player1)` on a fresh contract
- Assert `Err(Ok(Error::MatchNotFound))`

---

## 🆕 #119 — Add Test: submit_result on Pending match returns NotFunded
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `submit_result` returns `Error::NotFunded` when called on a match where neither player has deposited.

**Tasks:**
- Create match (state = Pending, no deposits)
- Call `try_submit_result`
- Assert `Err(Ok(Error::NotFunded))`

---

## 🆕 #120 — Add Test: get_escrow_balance returns stake_amount after only player1 deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_escrow_balance` returns exactly `stake_amount` (not `2 * stake_amount`) after only one player has deposited.

**Tasks:**
- Create match with `stake_amount = 100`
- Player1 deposits
- Assert `get_escrow_balance` returns `100`

---

## 🆕 #121 — Add Test: get_escrow_balance returns 0 after cancel_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_escrow_balance` returns `0` after a match is cancelled and deposits are refunded.

**Tasks:**
- Create match, player1 deposits, player1 cancels
- Assert `get_escrow_balance` returns `0`

---

## 🆕 #122 — Fix: oracle contract initialize does not emit an event
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`OracleContract::initialize` sets the admin silently with no on-chain event. Off-chain monitors cannot detect when the oracle is deployed and initialized.

**Tasks:**
- Add `env.events().publish(("oracle", "initialized"), admin)` inside `initialize`
- Add test asserting the event is emitted

---

## 🆕 #123 — Fix: escrow initialize does not emit an event
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`EscrowContract::initialize` sets oracle and admin silently. Off-chain monitors cannot detect contract initialization.

**Tasks:**
- Add `env.events().publish(("escrow", "initialized"), (oracle, admin))` inside `initialize`
- Add test asserting the event is emitted

---

## 🆕 #124 — Add Test: oracle unpause emits no event (verify behaviour is intentional)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`OracleContract::unpause` does not emit an event (unlike the escrow contract's `unpause`). A test should document whether this is intentional or an oversight.

**Tasks:**
- Call `unpause()` on oracle contract
- Assert no events are emitted (or add an event if the omission is unintentional)

---

## 🆕 #125 — Fix: no way to read MatchTimeout value from escrow contract
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`DataKey::MatchTimeout` is used internally by `expire_match` but there is no public getter. Frontends cannot display the configured timeout to users.

**Tasks:**
- Add `pub fn get_match_timeout(env: Env) -> u32` returning `DataKey::MatchTimeout` or `DEFAULT_MATCH_TIMEOUT_LEDGERS`
- Add test asserting it returns the default value before any admin sets it

---

## 🆕 #126 — Fix: no admin function to set MatchTimeout
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`DataKey::MatchTimeout` is read by `expire_match` but there is no admin function to set it. The timeout is permanently fixed at `DEFAULT_MATCH_TIMEOUT_LEDGERS` with no way to adjust it post-deployment.

**Tasks:**
- Add `pub fn set_match_timeout(env: Env, ledgers: u32) -> Result<(), Error>` requiring admin auth
- Store value under `DataKey::MatchTimeout`
- Add test asserting `expire_match` respects the updated timeout

---

## 🆕 #127 — Add Test: two concurrent matches do not share escrow balances
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that `get_escrow_balance` for match A is unaffected by deposits into match B.

**Tasks:**
- Create match A and match B with the same players
- Deposit into match A only
- Assert `get_escrow_balance(A)` equals `stake_amount`
- Assert `get_escrow_balance(B)` equals `0`

---

## 🆕 #128 — Add Test: cancel_match on Cancelled match returns InvalidState
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `cancel_match` on an already-cancelled match returns an error rather than silently succeeding.

**Tasks:**
- Create match, cancel it
- Call `try_cancel_match` again
- Assert `Err(Ok(Error::MatchAlreadyActive))` or `Err(Ok(Error::InvalidState))`

---

## 🆕 #129 — Fix: oracle submit_result does not validate game_id is non-empty
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`OracleContract::submit_result` accepts any `String` as `game_id` including an empty string. An empty `game_id` makes the result entry meaningless and harder to audit.

**Tasks:**
- Add `if game_id.len() == 0 { return Err(Error::InvalidGameId) }` guard
- Add `InvalidGameId` error variant to oracle `errors.rs`
- Add test asserting empty `game_id` is rejected

---

## 🆕 #130 — Fix: escrow create_match does not validate game_id is non-empty
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
`create_match` accepts an empty `game_id` string. A match with no game ID cannot be linked to a real chess game and the oracle cannot meaningfully verify it.

**Tasks:**
- Add `if game_id.len() == 0 { return Err(Error::InvalidGameId) }` guard
- Add `InvalidGameId` error variant to escrow `errors.rs`
- Add test asserting empty `game_id` is rejected

---

## 🆕 #131 — Add Test: oracle has_result returns false for match_id 0 on fresh contract
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly calls `has_result(0)` on a fresh oracle contract before any submission to confirm the default is `false`.

**Tasks:**
- Initialize oracle contract
- Assert `has_result(0)` returns `false`

---

## 🆕 #132 — Add Test: oracle get_result returns correct game_id
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_submit_and_get_result` only asserts `entry.result`. No test verifies that `entry.game_id` matches the value passed to `submit_result`.

**Tasks:**
- Submit result with `game_id = "chess_game_42"`
- Call `get_result`
- Assert `entry.game_id == "chess_game_42"`

---

## 🆕 #133 — Fix: no way to delete a result from oracle storage
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
Once a result is submitted to the oracle, it cannot be removed even if it was submitted in error. An admin-only `delete_result` function would allow correcting mistakes before the escrow contract reads the result.

**Tasks:**
- Add `pub fn delete_result(env: Env, match_id: u64) -> Result<(), Error>` requiring admin auth
- Remove `DataKey::Result(match_id)` from persistent storage
- Add test asserting `has_result` returns `false` after deletion

---

## 🆕 #134 — Fix: escrow contract has no way to transfer admin role
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The escrow admin address is set at `initialize` and cannot be changed. If the admin key is lost or compromised, there is no recovery path.

**Tasks:**
- Add `pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error>` requiring current admin auth
- Update `DataKey::Admin`
- Emit `("admin", "transferred")` event
- Add test for successful transfer and that old admin is rejected afterward

---

## 🆕 #135 — Fix: oracle contract has no way to transfer admin role
**Status:** Open — unassigned
**Labels:** `enhancement`, `security`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
The oracle admin address is set at `initialize` and cannot be changed. If the oracle service key is rotated, there is no way to update the admin without redeploying.

**Tasks:**
- Add `pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error>` requiring current admin auth
- Update `DataKey::Admin`
- Emit `("admin", "transferred")` event
- Add test for successful transfer

---

## 🆕 #136 — Add Test: deposit by unauthorized address returns Unauthorized
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that a third-party address (not player1 or player2) calling `deposit` returns `Error::Unauthorized`.

**Tasks:**
- Create match for player1 vs player2
- Call `try_deposit` with a random third address
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #137 — Add Test: cancel_match by unauthorized address returns Unauthorized
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that a third-party address calling `cancel_match` returns `Error::Unauthorized`.

**Tasks:**
- Create match for player1 vs player2
- Call `try_cancel_match` with a random third address
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #138 — Add Test: submit_result by non-oracle address returns Unauthorized
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** High
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `submit_result` from an address that is not the registered oracle returns `Error::Unauthorized`.

**Tasks:**
- Create and fund a match
- Call `try_submit_result` from a non-oracle address (without mocking oracle auth)
- Assert auth failure

---

## 🆕 #139 — Fix: get_escrow_balance does not extend TTL on read
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`get_escrow_balance` reads from persistent storage but does not extend the TTL. Repeated reads near expiry could cause the match entry to expire between reads.

**Tasks:**
- Add `extend_ttl` call inside `get_escrow_balance` after the read
- Add test verifying TTL is extended after calling `get_escrow_balance`

---

## 🆕 #140 — Fix: is_funded does not extend TTL on read
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`is_funded` reads from persistent storage without extending TTL, same issue as `get_escrow_balance`.

**Tasks:**
- Add `extend_ttl` call inside `is_funded` after the read
- Add test verifying TTL is extended after calling `is_funded`

---

## 🆕 #141 — Fix: get_match does not extend TTL on read
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`get_match` reads from persistent storage without extending TTL. Frequently-read matches near expiry could expire between reads.

**Tasks:**
- Add `extend_ttl` call inside `get_match` after the read
- Add test verifying TTL is extended after calling `get_match`

---

## 🆕 #142 — Add Test: player1 double deposit returns AlreadyFunded
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that player1 calling `deposit` twice on the same match returns `Error::AlreadyFunded`.

**Tasks:**
- Create match, player1 deposits
- Call `try_deposit` for player1 again
- Assert `Err(Ok(Error::AlreadyFunded))`

---

## 🆕 #143 — Add Test: player2 double deposit returns AlreadyFunded
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that player2 calling `deposit` twice returns `Error::AlreadyFunded`.

**Tasks:**
- Create match, both players deposit
- Call `try_deposit` for player2 again
- Assert `Err(Ok(Error::AlreadyFunded))`

---

## 🆕 #144 — Add Test: create_match with zero stake returns InvalidAmount
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `create_match` with `stake_amount = 0` returns `Error::InvalidAmount`.

**Tasks:**
- Call `try_create_match` with `stake_amount = 0`
- Assert `Err(Ok(Error::InvalidAmount))`

---

## 🆕 #145 — Add Test: create_match with negative stake returns InvalidAmount
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `create_match` with a negative `stake_amount` returns `Error::InvalidAmount`.

**Tasks:**
- Call `try_create_match` with `stake_amount = -100`
- Assert `Err(Ok(Error::InvalidAmount))`

---

## 🆕 #146 — Fix: no documentation on minimum viable stake amount
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The contract enforces `stake_amount > 0` but there is no documentation on what the practical minimum stake is given token decimal precision (e.g., 1 stroop for XLM).

**Tasks:**
- Add inline doc comment to `create_match` explaining the minimum stake constraint
- Update `docs/oracle.md` or `README.md` with stake amount guidance

---

## 🆕 #147 — Add Test: paused contract rejects submit_result
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `submit_result` is blocked when the escrow contract is paused.

**Tasks:**
- Create and fund a match
- Admin calls `pause()`
- Call `try_submit_result`
- Assert `Err(Ok(Error::ContractPaused))`

---

## 🆕 #148 — Add Test: paused contract rejects create_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
Verify that `create_match` is blocked when the contract is paused. (Mirrors the existing test but ensures it is present in this batch for completeness.)

**Tasks:**
- Admin calls `pause()`
- Call `try_create_match`
- Assert `Err(Ok(Error::ContractPaused))`

---

## 🆕 #149 — Fix: no view function to check if escrow contract is initialized
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
There is no public function to check whether the escrow contract has been initialized. Deployment scripts and frontends must attempt a call and catch a panic to detect uninitialized state.

**Tasks:**
- Add `pub fn is_initialized(env: Env) -> bool` returning `env.storage().instance().has(&DataKey::Oracle)`
- Add test asserting `false` before `initialize` and `true` after

---

## 🆕 #150 — Fix: no view function to check if oracle contract is initialized
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same gap as #149 but for the oracle contract.

**Tasks:**
- Add `pub fn is_initialized(env: Env) -> bool` returning `env.storage().instance().has(&DataKey::Admin)`
- Add test asserting `false` before `initialize` and `true` after

---

## 🆕 #151 — Fix: oracle submit_result does not validate match_id is reasonable
**Status:** Open — unassigned
**Labels:** `bug`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The oracle contract accepts any `u64` as `match_id` with no upper bound check. Submitting results for arbitrarily large IDs wastes storage and could confuse integrators.

**Tasks:**
- Document that `match_id` should correspond to a valid escrow match
- Consider adding a cross-contract call to verify the match exists before storing the result

---

## 🆕 #152 — Add Test: oracle pause blocks submit_result and unpause restores it
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
Verify the full pause/unpause cycle on the oracle contract: pause blocks `submit_result`, unpause restores it.

**Tasks:**
- Submit result for match 0 (succeeds)
- Pause oracle
- Try submit result for match 1 — assert `Error::ContractPaused`
- Unpause oracle
- Submit result for match 1 — assert success

---

## 🆕 #153 — Add Test: oracle admin cannot submit result for already-submitted match_id
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
Verify that submitting a result for the same `match_id` twice returns `Error::AlreadySubmitted` using `try_submit_result` (not `#[should_panic]`).

**Tasks:**
- Submit result for match 0
- Call `try_submit_result` for match 0 again
- Assert `Err(Ok(Error::AlreadySubmitted))`

---

## 🆕 #154 — Fix: no integration test for oracle → escrow result flow
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** High
**Estimated Time:** 1 hour

**Description:**
No test exercises the full flow where the oracle contract stores a result and the escrow contract reads it to trigger a payout. The two contracts are tested in isolation only.

**Tasks:**
- Deploy both oracle and escrow contracts in the same test environment
- Oracle admin submits result for match 0
- Escrow oracle address calls `submit_result` on escrow
- Assert payout is executed and match is `Completed`

---

## 🆕 #155 — Fix: Match struct does not store the winner after payout
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
After `submit_result` completes, the `Match` struct state is set to `Completed` but the winner is not stored. Querying `get_match` after payout gives no information about who won.

**Tasks:**
- Add `winner: Option<Winner>` field to `Match` struct
- Set it in `submit_result` before writing back to storage
- Add test asserting `get_match` returns the correct winner after payout

---

## 🆕 #156 — Fix: Match struct does not store completed_ledger
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
The `Match` struct records `created_ledger` but not the ledger at which the match was completed. This makes it impossible to calculate match duration on-chain or off-chain.

**Tasks:**
- Add `completed_ledger: Option<u32>` field to `Match` struct
- Set it in `submit_result` via `env.ledger().sequence()`
- Add test asserting `completed_ledger` is set after payout

---

## 🆕 #157 — Add Test: create_match with Platform::ChessDotCom stores correct platform
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `Platform::ChessDotCom` is stored and returned correctly by `get_match`. Existing tests only use `Platform::Lichess`.

**Tasks:**
- Create match with `Platform::ChessDotCom`
- Call `get_match`
- Assert `m.platform == Platform::ChessDotCom`

---

## 🆕 #158 — Add Test: create_match emits created event with correct data
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that the `("match", "created")` event emitted by `create_match` contains the correct `match_id`, `player1`, `player2`, and `stake_amount`.

**Tasks:**
- Create a match
- Read `env.events().all()`
- Assert event topics are `("match", "created")`
- Assert event data contains correct `(id, player1, player2, stake_amount)`

---

## 🆕 #159 — Add Test: cancel_match emits cancelled event with correct match_id
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that the `("match", "cancelled")` event contains the correct `match_id`.

**Tasks:**
- Create and cancel a match
- Assert event topics are `("match", "cancelled")`
- Assert event data is the correct `match_id`

---

## 🆕 #160 — Add Test: submit_result emits completed event with correct winner
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that the `("match", "completed")` event contains the correct `match_id` and `winner` value.

**Tasks:**
- Create, fund, and complete a match with `Winner::Player1`
- Assert event topics are `("match", "completed")`
- Assert event data is `(match_id, Winner::Player1)`

---

## 🆕 #161 — Add Test: update_oracle changes the oracle used for subsequent submit_result calls
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** High
**Estimated Time:** 30 minutes

**Description:**
No test verifies that after `update_oracle`, the new oracle address is accepted for `submit_result` and the old oracle address is rejected.

**Tasks:**
- Initialize with oracle_old
- Call `update_oracle` with oracle_new
- Create and fund a match
- Call `submit_result` from oracle_new — assert success
- Call `submit_result` from oracle_old — assert auth failure

---

## 🆕 #162 — Fix: no documentation for error codes in escrow contract
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`errors.rs` defines error codes as integers (e.g., `MatchNotFound = 1`) but there is no documentation explaining what each code means or when it is returned.

**Tasks:**
- Add doc comments to each error variant in `errors.rs`
- Reference the error codes in the relevant function doc comments

---

## 🆕 #163 — Fix: no documentation for error codes in oracle contract
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same gap as #162 for the oracle contract's `errors.rs`.

**Tasks:**
- Add doc comments to each error variant in oracle `errors.rs`

---

## 🆕 #164 — Fix: Cargo.toml has no description or repository fields
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The workspace and contract `Cargo.toml` files are missing `description`, `repository`, and `license` fields. These are required for publishing and improve discoverability.

**Tasks:**
- Add `description`, `repository = "https://github.com/..."`, and `license = "MIT"` to both contract `Cargo.toml` files

---

## 🆕 #165 — Fix: CI workflow does not run clippy
**Status:** Open — unassigned
**Labels:** `ci`, `enhancement`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
The `.github/workflows/ci.yml` runs `cargo test` but does not run `cargo clippy`. Lint warnings can accumulate silently and hide real issues.

**Tasks:**
- Add a `clippy` step to `ci.yml`: `cargo clippy -- -D warnings`
- Fix any existing clippy warnings

---

## 🆕 #166 — Fix: CI workflow does not run cargo fmt check
**Status:** Open — unassigned
**Labels:** `ci`, `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The CI pipeline does not enforce code formatting. Contributors can merge unformatted code without any automated check.

**Tasks:**
- Add `cargo fmt --check` step to `ci.yml`
- Ensure all existing code passes `cargo fmt`

---

## 🆕 #167 — Fix: scripts/build.sh has no error handling
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`scripts/build.sh` does not use `set -e` or check exit codes. A failed build step will not stop the script, silently producing incomplete artifacts.

**Tasks:**
- Add `set -euo pipefail` at the top of `build.sh`
- Add `set -euo pipefail` to `test.sh` as well

---

## 🆕 #168 — Fix: scripts/test.sh has no error handling
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same issue as #167 for `test.sh`.

**Tasks:**
- Add `set -euo pipefail` to `test.sh`

---

## 🆕 #169 — Add Test: expire_match refunds both players when both deposited
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that `expire_match` refunds both players when both have deposited but the match is still `Pending` (edge case: both deposited but state not yet `Active` due to a hypothetical bug).

**Tasks:**
- Manually set up a match with both `player1_deposited` and `player2_deposited` true but state still `Pending`
- Advance ledger past timeout
- Call `expire_match`
- Assert both players are refunded

---

## 🆕 #170 — Fix: no test for initialize with oracle == contract address
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** High
**Estimated Time:** 15 minutes

**Description:**
`initialize` rejects `oracle == env.current_contract_address()` with `Error::InvalidAddress`, but no test verifies this guard.

**Tasks:**
- Call `try_initialize` with `oracle = contract_id`
- Assert `Err(Ok(Error::InvalidAddress))`

---

## 🆕 #171 — Fix: no test for update_oracle with new_oracle == contract address
**Status:** Open — unassigned
**Labels:** `testing`, `security`
**Priority:** High
**Estimated Time:** 15 minutes

**Description:**
`update_oracle` should reject `new_oracle == env.current_contract_address()` (see #109), but even before that fix, a test should document the expected behaviour.

**Tasks:**
- Call `try_update_oracle` with `new_oracle = contract_id`
- Assert `Err(Ok(Error::InvalidAddress))`

---

## 🆕 #172 — Add Test: get_oracle returns correct address after initialize
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_oracle` returns the exact oracle address passed to `initialize`.

**Tasks:**
- Initialize with a known oracle address
- Call `get_oracle`
- Assert returned address equals the oracle address

---

## 🆕 #173 — Add Test: get_oracle returns updated address after update_oracle
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_oracle` reflects the new oracle address after `update_oracle` is called.

**Tasks:**
- Initialize, call `update_oracle` with a new address
- Call `get_oracle`
- Assert returned address equals the new oracle address

---

## 🆕 #174 — Fix: no test for pause() called twice in a row
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `pause()` twice does not cause an error or unexpected state change.

**Tasks:**
- Call `pause()` twice
- Assert contract is still paused and no error is returned

---

## 🆕 #175 — Fix: no test for unpause() called on already-unpaused contract
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `unpause()` on an already-unpaused contract does not cause an error.

**Tasks:**
- Call `unpause()` without calling `pause()` first
- Assert no error is returned and contract remains unpaused

---

## 🆕 #176 — Fix: Match struct fields are all public — no encapsulation
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
All fields of the `Match` struct are `pub`. While this is common in Soroban contracts, it means any consumer can read internal state flags like `player1_deposited` directly. Consider documenting which fields are stable API vs internal.

**Tasks:**
- Add doc comments to each field in `Match` struct indicating whether it is stable public API or internal state
- Update `docs/architecture.md` with the stable API surface

---

## 🆕 #177 — Fix: no architecture documentation for escrow state machine
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
There is no diagram or written description of the `MatchState` state machine transitions (`Pending` → `Active` → `Completed`, `Pending` → `Cancelled`). New contributors must read the code to understand valid transitions.

**Tasks:**
- Add a state machine diagram (ASCII or Mermaid) to `docs/architecture.md`
- Document which functions trigger each transition and under what conditions

---

## 🆕 #178 — Fix: docs/oracle.md does not describe the ResultEntry struct
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`docs/oracle.md` describes the oracle flow at a high level but does not document the `ResultEntry` struct fields (`game_id`, `result`) or the `MatchResult` enum variants.

**Tasks:**
- Add a section to `docs/oracle.md` documenting `ResultEntry` and `MatchResult`
- Include example values

---

## 🆕 #179 — Fix: README Quick Start section references scripts that may not exist
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
The README references `./scripts/deploy_testnet.sh` but this file does not exist in the repository. Running the Quick Start guide will fail.

**Tasks:**
- Create `scripts/deploy_testnet.sh` with a basic deployment script
- Or update the README to reflect the actual available scripts

---

## 🆕 #180 — Fix: .env.example references CHESSDOTCOM_API_KEY but Chess.com oracle is not implemented
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`.env.example` includes `CHESSDOTCOM_API_KEY` but the Chess.com oracle integration is listed as a v1.1 roadmap item and is not yet implemented. This misleads contributors.

**Tasks:**
- Add a comment in `.env.example` noting that `CHESSDOTCOM_API_KEY` is for a future release
- Or remove it until the feature is implemented

---

## 🆕 #181 — Fix: no CONTRIBUTING.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The README references `CONTRIBUTING.md` but the file does not exist in the repository. Contributors following the README will hit a 404.

**Tasks:**
- Create `CONTRIBUTING.md` with setup instructions, coding standards, and PR guidelines
- Link it correctly from the README

---

## 🆕 #182 — Fix: no CODE_OF_CONDUCT.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
The README references `CODE_OF_CONDUCT.md` but the file does not exist.

**Tasks:**
- Create `CODE_OF_CONDUCT.md` (Contributor Covenant is a standard choice)

---

## 🆕 #183 — Fix: no demo/demo-script.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The README references `demo/demo-script.md` as a step-by-step guide but the file does not exist.

**Tasks:**
- Create `demo/demo-script.md` with a walkthrough of creating a match, depositing, and triggering a payout on testnet

---

## 🆕 #184 — Fix: no docs/security.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`, `security`
**Priority:** High
**Estimated Time:** 2 hours

**Description:**
The README references `docs/security.md` (Threat Model & Security) but the file does not exist.

**Tasks:**
- Create `docs/security.md` covering: oracle trust assumptions, admin key risks, re-initialization protection, pause mechanism, and known limitations

---

## 🆕 #185 — Fix: no docs/roadmap.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
The README references `docs/roadmap.md` but the file does not exist.

**Tasks:**
- Create `docs/roadmap.md` with the v1.0–v4.0 roadmap items from the README expanded with more detail

---

## 🆕 #186 — Fix: no docs/wave-guide.md file exists
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The README references `docs/wave-guide.md` for the Drips Wave contributor program but the file does not exist.

**Tasks:**
- Create `docs/wave-guide.md` explaining how to claim wave-ready issues, point values, and submission process

---

## 🆕 #187 — Add Test: match state is Pending immediately after create_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test explicitly asserts that a newly created match has `MatchState::Pending`. The existing `test_create_match` checks `m.state == MatchState::Pending` but this should be a dedicated, clearly named test.

**Tasks:**
- Create a match
- Assert `m.state == MatchState::Pending`
- Assert `m.player1_deposited == false`
- Assert `m.player2_deposited == false`

---

## 🆕 #188 — Add Test: match state is Active after both deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No dedicated test asserts that `m.state == MatchState::Active` after both players deposit.

**Tasks:**
- Create match, both players deposit
- Call `get_match`
- Assert `m.state == MatchState::Active`

---

## 🆕 #189 — Add Test: match state is Completed after submit_result
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No dedicated test asserts that `m.state == MatchState::Completed` after `submit_result`.

**Tasks:**
- Create, fund, and complete a match
- Call `get_match`
- Assert `m.state == MatchState::Completed`

---

## 🆕 #190 — Add Test: match state is Cancelled after cancel_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No dedicated test asserts that `m.state == MatchState::Cancelled` after `cancel_match`.

**Tasks:**
- Create match, cancel it
- Call `get_match`
- Assert `m.state == MatchState::Cancelled`

---

## 🆕 #191 — Fix: no test for MatchCount after multiple creates and cancels
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `MatchCount` continues to increment correctly even after some matches are cancelled. Cancelled matches should not reset or affect the counter.

**Tasks:**
- Create 3 matches, cancel match 1
- Create 2 more matches
- Assert IDs are 0, 1, 2, 3, 4 (no gaps or resets)

---

## 🆕 #192 — Fix: no test for large stake_amount near i128 max
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies behaviour when `stake_amount` is very large (near `i128::MAX / 2`). The `pot = stake_amount * 2` calculation could overflow for extreme values.

**Tasks:**
- Add a check in `submit_result` that `stake_amount * 2` does not overflow using `checked_mul`
- Add `Error::Overflow` return for this case
- Add test with a large stake amount

---

## 🆕 #193 — Fix: no test for MatchCount at u64::MAX boundary
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 30 minutes

**Description:**
`create_match` uses `checked_add(1)` to guard against overflow, but no test verifies this guard fires correctly when `MatchCount` is at `u64::MAX`.

**Tasks:**
- Manually set `MatchCount` to `u64::MAX` in storage
- Call `try_create_match`
- Assert `Err(Ok(Error::Overflow))`

---

## 🆕 #194 — Fix: no test for deposit when contract has insufficient token balance
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies what happens when a player calls `deposit` but does not have enough token balance. The token `transfer` call will panic, but the error is not caught gracefully.

**Tasks:**
- Create match with `stake_amount = 1000`
- Mint only `500` tokens to player1
- Call `try_deposit` for player1
- Assert the call fails (token transfer panic)

---

## 🆕 #195 — Fix: no test for submit_result when contract token balance is zero
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies what happens if the contract's token balance is somehow zero when `submit_result` tries to pay out. This is a defensive test for state inconsistency.

**Tasks:**
- Create and fund a match
- Drain the contract's token balance externally (if possible in test env)
- Call `try_submit_result`
- Assert the call fails gracefully

---

## 🆕 #196 — Fix: environments.toml is not documented
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`environments.toml` defines network configurations but there are no comments explaining each field or how to add a custom network.

**Tasks:**
- Add inline comments to `environments.toml` explaining each field
- Add a section to `docs/deployment.md` referencing the file

---

## 🆕 #197 — Fix: docs/deployment.md does not cover mainnet deployment
**Status:** Open — unassigned
**Labels:** `documentation`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
`docs/deployment.md` covers testnet deployment but does not include mainnet deployment steps, security considerations, or key management guidance.

**Tasks:**
- Add a mainnet deployment section to `docs/deployment.md`
- Include key management best practices and pre-deployment checklist

---

## 🆕 #198 — Fix: no test for oracle get_result TTL extension on read
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`test_get_result_extends_ttl` exists in the oracle tests but verifies TTL equals `MATCH_TTL_LEDGERS` immediately after submit. A separate test should verify TTL is re-extended on a subsequent `get_result` call after some ledgers have passed.

**Tasks:**
- Submit result, advance ledger by 1000
- Call `get_result`
- Assert TTL is reset to `MATCH_TTL_LEDGERS` (not `MATCH_TTL_LEDGERS - 1000`)

---

## 🆕 #199 — Fix: no test for escrow match TTL extension on get_match read
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
If `get_match` is updated to extend TTL on read (see #141), a test should verify the TTL is reset after some ledgers have passed.

**Tasks:**
- Create match, advance ledger by 1000
- Call `get_match`
- Assert TTL is reset to `MATCH_TTL_LEDGERS`

---

## 🆕 #200 — Fix: no test for escrow instance storage TTL
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Instance storage entries (`Oracle`, `Admin`, `MatchCount`, `Paused`) have no TTL extension calls. If the contract is inactive for a long period, instance storage could expire.

**Tasks:**
- Investigate whether instance storage requires TTL extension in Soroban
- Add `extend_ttl` calls for instance storage if required
- Add test verifying instance storage TTL is maintained

---

## 🆕 #201 — Fix: no test for oracle instance storage TTL
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
Same gap as #200 for the oracle contract's instance storage (`Admin`, `Paused`).

**Tasks:**
- Investigate instance storage TTL requirements for oracle contract
- Add `extend_ttl` calls if required
- Add test

---

## 🆕 #202 — Fix: Winner enum and MatchResult enum are duplicated across contracts
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Medium
**Estimated Time:** 1 hour

**Description:**
The escrow contract defines `Winner` (Player1, Player2, Draw) and the oracle contract defines `MatchResult` (Player1Wins, Player2Wins, Draw). These represent the same concept with different naming, causing confusion and duplication.

**Tasks:**
- Decide on a canonical enum name and variant names
- Consider a shared types crate or document the intentional separation
- Update all references and tests

---

## 🆕 #203 — Fix: no test for Platform enum serialization round-trip
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `Platform::Lichess` and `Platform::ChessDotCom` survive a storage write/read round-trip correctly.

**Tasks:**
- Create match with each platform variant
- Read back with `get_match`
- Assert platform matches the input

---

## 🆕 #204 — Fix: no test for Winner enum serialization round-trip in events
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `Winner::Draw` is correctly serialized and deserialized from event data.

**Tasks:**
- Complete a match with `Winner::Draw`
- Read the `("match", "completed")` event
- Deserialize and assert `winner == Winner::Draw`

---

## 🆕 #205 — Fix: no test for MatchResult::Draw in oracle event
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `MatchResult::Draw` is correctly emitted in the oracle `("oracle", "result")` event.

**Tasks:**
- Submit result with `MatchResult::Draw`
- Read the event
- Assert event data contains `MatchResult::Draw`

---

## 🆕 #206 — Fix: no test for create_match when contract is not initialized
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies what happens when `create_match` is called on an uninitialized contract (before `initialize`). The `Paused` check reads instance storage with `unwrap_or(false)`, so it would not panic, but `MatchCount` would also default to 0 silently.

**Tasks:**
- Register contract without calling `initialize`
- Call `try_create_match`
- Document and assert the expected behaviour (panic or error)

---

## 🆕 #207 — Fix: no test for submit_result when contract is not initialized
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies what happens when `submit_result` is called on an uninitialized escrow contract. The oracle address lookup returns `None`, which maps to `Error::Unauthorized`.

**Tasks:**
- Register contract without calling `initialize`
- Call `try_submit_result`
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #208 — Fix: no test for oracle submit_result when contract is not initialized
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
No test verifies what happens when `OracleContract::submit_result` is called before `initialize`. The admin lookup returns `None`, mapping to `Error::Unauthorized`.

**Tasks:**
- Register oracle contract without calling `initialize`
- Call `try_submit_result`
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #209 — Fix: no test for pause when contract is not initialized
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that calling `pause()` on an uninitialized contract returns `Error::Unauthorized` rather than panicking.

**Tasks:**
- Register contract without calling `initialize`
- Call `try_pause()`
- Assert `Err(Ok(Error::Unauthorized))`

---

## 🆕 #210 — Fix: no fuzz or property-based tests for escrow contract
**Status:** Open — unassigned
**Labels:** `testing`, `enhancement`
**Priority:** Medium
**Estimated Time:** 2 hours

**Description:**
All tests use fixed inputs. Property-based or fuzz testing with random `stake_amount`, `match_id`, and player addresses would catch edge cases that hand-written tests miss.

**Tasks:**
- Evaluate `proptest` or `quickcheck` compatibility with Soroban test environment
- Add at least one property test for `create_match` with random valid inputs
- Document findings if fuzz testing is not feasible in the Soroban test harness

---

## 🆕 #211 — Fix: no benchmark tests for escrow contract
**Status:** Open — unassigned
**Labels:** `enhancement`
**Priority:** Low
**Estimated Time:** 2 hours

**Description:**
There are no benchmarks measuring CPU instruction counts or memory usage for key contract functions. Soroban charges fees based on resource usage, so benchmarks help optimize costs.

**Tasks:**
- Add benchmark tests using `env.budget()` to measure instruction counts for `create_match`, `deposit`, and `submit_result`
- Document baseline resource usage in `docs/deployment.md`

---

## 🆕 #212 — Fix: no test for concurrent deposits from player1 and player2 in same ledger
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
No test verifies that two deposits submitted in the same ledger (simulated sequentially in tests) both succeed and the match transitions to `Active` correctly.

**Tasks:**
- Create match
- Call `deposit` for player1 and player2 back-to-back without ledger advancement
- Assert match is `Active` and `is_funded` returns `true`

---

## 🆕 #213 — Fix: no test for cancel_match when no deposits have been made
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `cancel_match` succeeds and does not attempt any token transfers when neither player has deposited.

**Tasks:**
- Create match, immediately cancel it (no deposits)
- Assert match state is `Cancelled`
- Assert no token transfer events are emitted

---

## 🆕 #214 — Fix: no test for expire_match when no deposits have been made
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `expire_match` succeeds without attempting any token transfers when neither player has deposited.

**Tasks:**
- Create match, advance ledger past timeout, call `expire_match`
- Assert match state is `Cancelled`
- Assert no token transfer events are emitted

---

## 🆕 #215 — Fix: no test for get_match after expire_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_match` returns `MatchState::Cancelled` after `expire_match` is called.

**Tasks:**
- Create match, advance ledger, call `expire_match`
- Call `get_match`
- Assert `m.state == MatchState::Cancelled`

---

## 🆕 #216 — Fix: no test for is_funded after cancel_match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `is_funded` returns `false` after a match is cancelled (even if both players had deposited before cancellation).

**Tasks:**
- Create match, player1 deposits, cancel match
- Assert `is_funded` returns `false`

---

## 🆕 #217 — Fix: no test for is_funded after submit_result
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `is_funded` returns `true` (or documents expected behaviour) after a match is `Completed`. The `is_funded` function checks deposit flags, not state, so it would return `true` even after payout.

**Tasks:**
- Complete a match
- Call `is_funded`
- Assert and document the expected return value

---

## 🆕 #218 — Fix: no test for get_escrow_balance on Completed match
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
`get_escrow_balance` returns `0` for `Completed` and `Cancelled` matches. No test verifies the `Completed` case specifically.

**Tasks:**
- Create, fund, and complete a match
- Assert `get_escrow_balance` returns `0`

---

## 🆕 #219 — Fix: no test for get_escrow_balance on Cancelled match with no deposits
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
No test verifies that `get_escrow_balance` returns `0` for a `Cancelled` match where no deposits were made.

**Tasks:**
- Create match, cancel immediately
- Assert `get_escrow_balance` returns `0`

---

## 🆕 #220 — Fix: no test for oracle has_result after delete_result (if #133 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
If `delete_result` is implemented (see #133), a test should verify that `has_result` returns `false` after deletion and `get_result` returns `Error::ResultNotFound`.

**Tasks:**
- Submit result, delete it
- Assert `has_result` returns `false`
- Assert `try_get_result` returns `Err(Ok(Error::ResultNotFound))`

---

## 🆕 #221 — Fix: no test for oracle admin transfer (if #135 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If `transfer_admin` is implemented on the oracle contract (see #135), tests should verify the old admin is rejected and the new admin is accepted.

**Tasks:**
- Transfer admin to new_admin
- Call `submit_result` from old_admin — assert auth failure
- Call `submit_result` from new_admin — assert success

---

## 🆕 #222 — Fix: no test for escrow admin transfer (if #134 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If `transfer_admin` is implemented on the escrow contract (see #134), tests should verify the old admin is rejected and the new admin is accepted.

**Tasks:**
- Transfer admin to new_admin
- Call `pause()` from old_admin — assert auth failure
- Call `pause()` from new_admin — assert success

---

## 🆕 #223 — Fix: no test for player index (if #91 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If `get_player_matches` is implemented (see #91), tests should verify the index is populated correctly and returns all match IDs for a player.

**Tasks:**
- Create 3 matches for player1 (as player1 or player2)
- Call `get_player_matches(player1)`
- Assert all 3 match IDs are returned

---

## 🆕 #224 — Fix: no test for active match index (if #95 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If `get_active_matches` is implemented (see #95), tests should verify the index is updated correctly through the match lifecycle.

**Tasks:**
- Create 3 matches
- Cancel match 1, complete match 2
- Call `get_active_matches`
- Assert only match 0 and match 2 are returned (or only match 0 if completed matches are removed)

---

## 🆕 #225 — Fix: no test for token allowlist (if #90 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If a token allowlist is implemented (see #90), tests should verify that disallowed tokens are rejected and allowed tokens are accepted.

**Tasks:**
- Try `create_match` with a non-allowlisted token — assert `Error::InvalidToken`
- Add token to allowlist
- Try `create_match` again — assert success

---

## 🆕 #226 — Fix: no test for self-match guard (if #87 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
If the self-match guard is implemented (see #87), a test should verify that `create_match` with `player1 == player2` returns `Error::InvalidPlayers`.

**Tasks:**
- Call `try_create_match` with `player1 == player2`
- Assert `Err(Ok(Error::InvalidPlayers))`

---

## 🆕 #227 — Fix: no test for duplicate game_id guard (if #88 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 15 minutes

**Description:**
If the duplicate `game_id` guard is implemented (see #88), a test should verify that creating two matches with the same `game_id` returns `Error::AlreadyExists` on the second call.

**Tasks:**
- Create match with `game_id = "game1"`
- Call `try_create_match` with `game_id = "game1"` again
- Assert `Err(Ok(Error::AlreadyExists))`

---

## 🆕 #228 — Fix: no test for two-step admin transfer (if #86 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If two-step admin transfer is implemented (see #86), tests should verify that the transfer is not complete until the new admin calls `accept_admin`.

**Tasks:**
- Call `propose_admin(new_admin)`
- Assert old admin is still active (can call `pause`)
- Call `accept_admin` from new_admin
- Assert new admin is now active and old admin is rejected

---

## 🆕 #229 — Fix: no test for is_paused view function (if #83 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Low
**Estimated Time:** 15 minutes

**Description:**
If `is_paused` is implemented (see #83), tests should verify it returns `false` initially, `true` after `pause()`, and `false` after `unpause()`.

**Tasks:**
- Assert `is_paused()` returns `false` after initialize
- Call `pause()`, assert `is_paused()` returns `true`
- Call `unpause()`, assert `is_paused()` returns `false`

---

## 🆕 #230 — Fix: no test for set_match_timeout admin function (if #126 is implemented)
**Status:** Open — unassigned
**Labels:** `testing`
**Priority:** Medium
**Estimated Time:** 30 minutes

**Description:**
If `set_match_timeout` is implemented (see #126), tests should verify that `expire_match` respects the updated timeout value.

**Tasks:**
- Set timeout to 100 ledgers
- Create match, advance ledger by 100
- Call `expire_match` — assert success
- Create another match, advance ledger by 99
- Call `try_expire_match` — assert `Error::MatchNotExpired`

---
