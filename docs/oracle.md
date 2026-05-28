# Oracle Integration Guide

This document describes the current oracle architecture used by Checkmate
Escrow. It explains the two distinct on-chain oracle components and how they
work together with the off-chain oracle service.

The current design has:
- an `EscrowContract` that stores a trusted `oracle` address and authorises
  result submissions for on-chain payout,
- an `OracleContract` that stores an independent, auditable copy of verified
  match results.

The escrow contract uses its configured oracle address as the authoritative
permission for submitting results to trigger payouts. The oracle contract is
supplementary: it does not authorise escrow payouts or act as a gatekeeper for
escrow result submission. It provides an audit log and an independent on-chain
record of results that can be queried later.

The off-chain oracle service today is the trusted operator that:
1. verifies the platform result for `game_id` using an external chess API,
2. calls `EscrowContract::submit_result(match_id, winner)` from the escrow-side
   oracle address,
3. records the same result in `OracleContract` for auditing and optional
   verification.

The two contracts are separate:
- `EscrowContract` enforces match state, funding, and oracle address
  authentication.
- `OracleContract` enforces admin-only result storage and exposes public or
  admin-gated read interfaces.

---

## game_id Format

The `game_id` field is a platform-specific string that uniquely identifies a
chess game. It is supplied when creating a match and must be passed to the
oracle when submitting a result. The oracle uses it to look up the game outcome
via the platform's public API.

### Lichess

Lichess game IDs are **8-character alphanumeric strings** (case-sensitive,
lowercase letters and digits).

They appear in the game URL:

```
https://lichess.org/abcd1234
                    ^^^^^^^^
                    game_id = "abcd1234"
```

Example API call the oracle makes:

```
GET https://lichess.org/game/export/abcd1234
```

Valid example: `"abcd1234"`  
Invalid examples: `"ABCD1234"` (uppercase), `"abcd123"` (7 chars), `""` (empty)

### Chess.com

Chess.com game IDs are **numeric strings**, typically 7–12 digits, found in
the live game URL:

```
https://www.chess.com/game/live/123456789
                                ^^^^^^^^^
                                game_id = "123456789"
```

Example API call the oracle makes:

```
GET https://api.chess.com/pub/game/123456789
```

Valid example: `"123456789"`  
Invalid examples: `"abc"` (non-numeric), `""` (empty)

---

## Validation Rules

| Rule | Details |
|------|---------|
| Max length | 64 bytes (`MAX_GAME_ID_LEN`). Enforced on-chain — `create_match` returns `Error::InvalidGameId` if exceeded. |
| Uniqueness | Each `game_id` can only be used once. A duplicate returns `Error::DuplicateGameId`. |
| Format | Not validated on-chain. Passing a malformed ID will cause the oracle to fail result lookup off-chain. |
| Platform match | The `platform` field must match the source of the `game_id`. Mismatches are not caught on-chain but will cause oracle verification to fail. |

---

## Submitting a Result

Once a game is finished, the off-chain oracle service verifies the result via
an external chess platform API and then submits the verified outcome to the
escrow contract from the configured oracle address.

```rust
// Winner::Player1 | Winner::Player2 | Winner::Draw
escrow_client.submit_result(&match_id, &winner);
```

That escrow submission is the authoritative payout trigger. The escrow contract
trusts only its configured oracle address when authorising `submit_result`.

Separately, the oracle service records the same result in the on-chain
`OracleContract` for auditability and later verification.

```rust
oracle_client.submit_result(&match_id, &game_id, &MatchResult::Player1Wins);
```

This means the current oracle contract is supplementary: it stores an
independent result entry and exposes read APIs, but it is not the primary
authority used by the escrow contract to execute payouts.

---

## Result Deletion Policy (`delete_result`)

The oracle contract exposes a `delete_result` function that allows the admin to remove a previously submitted result from persistent storage:

```rust
oracle_client.delete_result(&match_id); // → Result<(), Error>
```

### Why it exists

On-chain persistent storage has a finite TTL (~30 days). In normal operation results expire naturally. `delete_result` exists for two narrow operational cases:

1. **Erroneous submission** — the oracle submitted a result for the wrong `match_id` (e.g., due to a bug or misconfiguration) before the escrow payout was triggered. Deletion allows the correct result to be re-submitted.
2. **Storage reclamation** — proactively freeing storage rent for results that are no longer needed (e.g., after a dispute is fully resolved off-chain).

### Trust assumptions and risks

`delete_result` is an admin-only operation and is **blocked while the contract is paused**. Despite these guards, deletion carries meaningful trust implications:

| Risk | Detail |
|------|--------|
| Audit trail removal | Deleting a result removes the on-chain record of that outcome. Anyone relying solely on `get_result` for historical verification will see `ResultNotFound` after deletion. |
| Re-submission after deletion | Once deleted, a new result can be submitted for the same `match_id`. A compromised or malicious admin could use this to alter the apparent outcome of a match. |
| Payout already executed | If the escrow payout has already been triggered by `submit_result`, deleting the oracle record does not reverse the payout. The escrow contract state is independent. |

### Expected operational use

- **Do not** use `delete_result` as routine cleanup. Results should be left to expire naturally via TTL.
- **Do** use it only to correct a demonstrably erroneous submission, and only before the corresponding escrow `submit_result` has been called.
- Any deletion should be logged off-chain (e.g., via the admin's operational runbook) since the on-chain event record will no longer contain the original submission after deletion.
- In production, admin keys should be held in a multi-sig wallet so that deletion requires multiple approvals, reducing the risk of unilateral misuse.

---

## has_result vs has_result_admin

The oracle contract exposes two ways to check whether a result has been
submitted for a given `match_id`.

### `has_result` — public, unauthenticated

```rust
oracle_client.has_result(&match_id); // → bool
```

This is a read-only probe that returns `true` once a result has been stored.
It requires **no authentication** and can be called by anyone.

This is intentional: the function exposes only the *existence* of a result,
not its content. For the majority of public tournament contexts this is
acceptable — knowing that *a* result exists leaks no information about *who*
won.

### `has_result_admin` — admin-gated

```rust
oracle_client.has_result_admin(&match_id); // → Result<bool, Error>
```

For private tournaments where even the existence of a result must remain
confidential until an official announcement, use this variant instead. It
requires the stored admin to authorise the call, preventing third-party
probing.

Returns `Error::Unauthorized` if the caller is not the current admin.

---

## Data Structures

### ResultEntry

The `ResultEntry` struct stores a verified match result in the oracle contract's
persistent storage. Each entry is keyed by `match_id`.

```rust
pub struct ResultEntry {
    pub game_id: String,
    pub result: Winner,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `game_id` | `String` | Platform-specific game identifier (e.g., `"abcd1234"` for Lichess, `"123456789"` for Chess.com) |
| `result` | `Winner` | The verified outcome of the match |

Example values:

```rust
ResultEntry {
    game_id: String::from_str(&env, "abcd1234"),
    result: Winner::Player1,
}

ResultEntry {
    game_id: String::from_str(&env, "987654321"),
    result: Winner::Draw,
}
```

### Winner

The `Winner` enum represents the possible outcomes of a chess match. It mirrors
the escrow contract's `Winner` enum for consistency.

```rust
pub enum Winner {
    Player1,
    Player2,
    Draw,
}
```

| Variant | Description |
|---------|-------------|
| `Player1` | Player 1 won the match |
| `Player2` | Player 2 won the match |
| `Draw` | The match ended in a draw (stakes are refunded) |

---

## Example: Full Match Lifecycle

```
1. player1 calls create_match(
       player1, player2,
       stake_amount = 100_000_000,
       token = USDC_ADDRESS,
       game_id = "abcd1234",       // Lichess game ID
       platform = Platform::Lichess
   )

2. player1 calls deposit(match_id, player1)
3. player2 calls deposit(match_id, player2)
   → match state transitions to Active

4. Game is played on Lichess.

5. Oracle fetches result from https://lichess.org/game/export/abcd1234
   → player1 wins

6. Oracle calls escrow.submit_result(match_id, Winner::Player1, oracle_address)
   → player1 receives 2 × stake_amount
```
