# Error Code Reference

This document maps the numeric error codes returned by the Checkmate smart
contracts to their meanings. Frontends and integrators can use this table to
display human-readable messages when a transaction fails.

Soroban surfaces contract errors as a numeric `u32` value in the transaction
result. Match the value against the tables below to identify the cause.

---

## EscrowContract Errors

| Code | Variant | When it is returned |
|------|---------|---------------------|
| 1 | `MatchNotFound` | The requested match ID does not exist in storage. |
| 2 | `AlreadyFunded` | The calling player has already deposited into this match. |
| 3 | `NotFunded` | The match has not been fully funded yet; both players must deposit before this operation is allowed. |
| 4 | `Unauthorized` | The caller is not authorised to perform this operation (e.g. a non-player trying to cancel, or a non-oracle submitting a result). |
| 5 | `InvalidState` | The match is in a state that does not permit the requested operation (e.g. trying to deposit into an already-active match). |
| 6 | `AlreadyExists` | A generic "already exists" guard was triggered. |
| 7 | `AlreadyInitialized` | `initialize` has already been called; it can only be called once. |
| 8 | `Overflow` | An arithmetic operation would overflow. |
| 9 | `ContractPaused` | The contract is paused by the admin; no new matches can be created until it is unpaused. |
| 10 | `InvalidAmount` | The supplied stake amount is zero or otherwise invalid. |
| 11 | `MatchCancelled` | The match has been cancelled; the requested operation cannot be performed on a cancelled match. |
| 12 | `MatchCompleted` | The match has already been completed; the requested operation cannot be performed on a completed match. |
| 13 | `DuplicateGameId` | The `game_id` supplied to `create_match` has already been used by another match. |
| 14 | `MatchNotExpired` | The match has not yet expired; expiry-based operations (e.g. admin cancellation after timeout) are not yet permitted. |
| 15 | `InvalidGameId` | The `game_id` string is invalid (e.g. exceeds the maximum allowed length of 64 bytes). |
| 16 | `ResultNotFound` | No oracle result has been recorded for this match ID. |
| 17 | `InvalidToken` | The token address supplied is not the token this contract was initialised with. |

---

## OracleContract Errors

| Code | Variant | When it is returned |
|------|---------|---------------------|
| 1 | `Unauthorized` | The caller is not the authorised oracle admin. |
| 2 | `AlreadySubmitted` | A result for this match ID has already been submitted; results are immutable once recorded. |
| 3 | `ResultNotFound` | No result has been recorded for the requested match ID. |
| 4 | `AlreadyInitialized` | `initialize` has already been called; it can only be called once. |
| 5 | `MatchNotFound` | The match ID referenced does not exist in the escrow contract. |

---

## Suggested Frontend Messages

```js
const ESCROW_ERRORS = {
  1:  "Match not found.",
  2:  "You have already deposited into this match.",
  3:  "Match is not fully funded yet.",
  4:  "You are not authorised to perform this action.",
  5:  "This action is not allowed in the current match state.",
  6:  "Already exists.",
  7:  "Contract is already initialised.",
  8:  "Arithmetic overflow.",
  9:  "Contract is currently paused.",
  10: "Invalid stake amount.",
  11: "Match has been cancelled.",
  12: "Match has already been completed.",
  13: "This game ID has already been used.",
  14: "Match has not expired yet.",
  15: "Invalid game ID.",
  16: "No result found for this match.",
  17: "Invalid token.",
};

const ORACLE_ERRORS = {
  1: "Unauthorised.",
  2: "Result already submitted for this match.",
  3: "No result found for this match.",
  4: "Contract is already initialised.",
  5: "Match not found.",
};
```
