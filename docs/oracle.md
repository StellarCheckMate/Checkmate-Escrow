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

Chess.com game IDs are **numeric strings**, typically 7–12 digits, found in the live game URL:

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

For tournament support, the oracle contract also exposes a batch API:
`submit_batch_results`. This lets the oracle submit 10–100 verified match
results in a single atomic transaction.

---

## Chess.com API Rate Limits, Timeouts, and Offline Fallback

The off-chain Chess.com client (see `oracle-service/src/oracle/chess_com_client.rs`) must obey Chess.com’s public API limits:

- **Rate limit:** **30 requests / minute** (≈ 1 request / 2 seconds, globally).
- **Timeout:** **30 seconds max** per HTTP request.

### Rate limiting behavior

The oracle client uses a client-side rate limiter. If a request would exceed the quota, it waits until tokens are available before issuing the HTTP call.

### Error handling rules

If Chess.com returns:
- **404:** treat as `GameNotFound` (invalid game id or unavailable game).
- **non-2xx:** treat as `HttpStatus` and retry using the oracle service’s retry strategy (if any).
- **timeouts / network errors:** treat as transient; retry with exponential backoff.

### Offline fallback strategy

When Chess.com is unreachable or rate-limited:
- **Do not submit** an on-chain result until a verified end-state is fetched.
- Mark the match as **pending verification** and retry later.
- If a verification attempt observes a game payload without a known terminal
  `end.result`, treat it as **GameNotFinished** and retry.

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

---

## has_result vs has_result_admin

(Existing contract documentation continues unchanged.)

---

## Data Structures

(Existing contract documentation continues unchanged.)

---

## Example: Full Match Lifecycle

(Existing contract documentation continues unchanged.)

