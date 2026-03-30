# Oracle Integration Guide

This document describes how the off-chain oracle service interacts with the
Checkmate Escrow smart contracts, with a focus on the `game_id` field.

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

Once a game is finished, the oracle calls `submit_result` on the escrow
contract with the `match_id`, `game_id`, and `Winner` enum:

```rust
// Winner::Player1 | Winner::Player2 | Winner::Draw
escrow_client.submit_result(&match_id, &winner, &oracle_address);
```

The oracle also records the result independently via `OracleContract::submit_result`:

```rust
oracle_client.submit_result(&match_id, &game_id, &MatchResult::Player1Wins);
```

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
