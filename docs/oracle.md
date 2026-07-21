## Health Check

The oracle exposes a /health endpoint to monitor connectivity and uptime.

**Endpoint:** GET /health

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

## Game ID Formats

| Platform   | Format                        | Example         | Validation Rule                                      |
|------------|-------------------------------|-----------------|------------------------------------------------------|
| Lichess    | 8-character alphanumeric      | `abcd1234`      | Exactly 8 chars; lowercase letters and digits only   |
| Chess.com  | Numeric string (7–12 digits)  | `123456789`     | Digits only; no letters or special characters        |

All game IDs are subject to a maximum length of **64 bytes** (`MAX_GAME_ID_LEN`). Submissions exceeding this limit are rejected on-chain with `Error::InvalidGameId` before any off-chain lookup is attempted.

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

## Chess.com Integration

This section is the primary reference for oracle contributors working with the Chess.com platform. It covers everything needed to fetch a game result from Chess.com and feed it into the on-chain `submit_result` flow.

### Environment Variable

The off-chain oracle service reads the Chess.com API key from:

```env
CHESSDOTCOM_API_KEY=your-key-here
```

Set this in your `.env` file (copy from `.env.example`). The key is sent as a request header on every Chess.com API call:

```
X-Chess-Com-API-Key: your-key-here
```

> **Note:** The Chess.com public API does not require authentication for game lookups today, but the `CHESSDOTCOM_API_KEY` header is included for forward-compatibility and to receive higher rate-limit tiers if Chess.com introduces them. Contrast this with Lichess, which uses `LICHESS_API_TOKEN` sent as a `Bearer` token in the `Authorization` header.

### Game ID Format

Chess.com game IDs are **numeric strings** (digits only), typically 7–12 digits, found in the game URL:

```
https://www.chess.com/game/live/123456789
                                ^^^^^^^^^
                                game_id = "123456789"
```

The oracle client validates that the ID is non-empty and contains only ASCII digits. Any other character (letters, hyphens, etc.) causes `ChessComError::InvalidGameId` before any network call is made.

Valid example: `"123456789"`  
Invalid examples: `"abc"` (non-numeric), `""` (empty), `"123-456"` (hyphen)

### API Endpoint

The oracle fetches game results from the Chess.com public API:

```
GET https://api.chess.com/pub/game/{game_id}
```

No query parameters are required. The `game_id` path segment must be a valid numeric game ID as described above.

### Example Request / Response

**Request:**

```http
GET https://api.chess.com/pub/game/123456789
X-Chess-Com-API-Key: your-key-here
```

**Successful response (white wins):**

```json
{
  "end": {
    "result": "white"
  }
}
```

**Draw response:**

```json
{
  "end": {
    "result": "draw"
  }
}
```

**Game still in progress (no terminal result yet):**

```json
{
  "end": null
}
```

The oracle only reads the `end.result` field. All other fields in the response are ignored. If `end` is absent or `end.result` is `null`, the oracle treats the game as unfinished and will not submit a result on-chain.

### Response Parsing and Result Mapping

The `end.result` string is mapped to the on-chain `Winner` type as follows:

| `end.result` value | On-chain `Winner`  | Notes                                      |
|--------------------|--------------------|--------------------------------------------|
| `"white"`          | `Winner::Player1`  | Player 1 is always white in the match      |
| `"black"`          | `Winner::Player2`  | Player 2 is always black in the match      |
| `"draw"`           | `Winner::Draw`     | Stakes are refunded to both players        |
| anything else      | Error              | `ChessComError::InvalidResponse` — not submitted |
| absent / `null`    | Error              | `ChessComError::InvalidResponse` — game not finished |

The mapping is implemented in `oracle-service/src/oracle/chess_com_client.rs` in the `fetch_result` method.

### Error Handling

| Condition                               | Error variant                  | Oracle behaviour                                      |
|-----------------------------------------|--------------------------------|-------------------------------------------------------|
| Empty or non-numeric game ID            | `InvalidGameId`                | Rejected before any HTTP call; not retried            |
| HTTP 404                                | `GameNotFound`                 | Game ID invalid or game unavailable; not retried      |
| HTTP non-2xx (other than 404)           | `HttpStatus { status }`        | Transient; retried with exponential backoff            |
| Request timeout (> 30 s)               | `Timeout`                      | Transient; retried with exponential backoff            |
| Network error (connection refused etc.) | `Http(reqwest::Error)`         | Transient; retried with exponential backoff            |
| `end.result` absent, null, or unknown   | `InvalidResponse`              | Game not finished or unrecognised result; retried later |

The oracle will **never** submit a result on-chain until a verified terminal `end.result` of `"white"`, `"black"`, or `"draw"` is received.

### Rate Limiting and Authentication Differences from Lichess

| Aspect                    | Chess.com                                          | Lichess                                               |
|---------------------------|----------------------------------------------------|-------------------------------------------------------|
| Authentication            | `X-Chess-Com-API-Key` header (optional today)      | `Authorization: Bearer <LICHESS_API_TOKEN>` (required) |
| Rate limit                | 30 req/min (≈ 1 req / 2 s), enforced client-side  | No documented hard limit; same 2 s spacing applied    |
| Client-side spacing       | Shared token bucket with configurable burst/rate   | Shared token bucket with configurable burst/rate     |
| Per-request timeout       | 30 seconds                                         | 30 seconds                                            |
| Response format           | JSON; result in `end.result`                       | JSON; result in top-level `winner` field              |
| Draw representation       | `"draw"` in `end.result`                           | `winner` field absent from JSON object                |
| Game ID format            | Numeric string, 7–12 digits                        | Exactly 8 alphanumeric characters                     |
| API base URL              | `https://api.chess.com`                            | `https://lichess.org`                                 |
| Export path               | `/pub/game/{game_id}`                              | `/game/export/{game_id}`                              |

Key difference to highlight for contributors: Lichess signals a **draw** by omitting the `winner` key entirely, while Chess.com signals a draw with the explicit value `"draw"` in `end.result`. Make sure any result-parsing code handles both conventions correctly.

### Oracle rate-limiter and failover design

The oracle service now uses a shared, clone-safe token bucket for each provider instead of a single global spacing gate. The effective knobs are:

- `capacity`: how many burst requests may be dispatched immediately before the sustained rate starts to throttle
- `refill_rate`: sustained token generation rate in requests/second
- `max_concurrent`: per-provider in-flight request ceiling, distinct from the token-bucket ceiling

Both values are configured in the provider client config (`ChessComClientConfig` / `LichessClientConfig`) and are shared across all concurrent verification tasks for that provider.

The provider registry enforces precedence and failover rules:

1. The first provider in the registry order is the primary source.
2. If the primary returns `ProviderError::Unavailable` or `ProviderError::RateLimited`, the registry tries the next provider immediately.
3. If the provider returns a terminal logical error such as `GameNotFound` or `InvalidGameId`, the registry stops and returns that error without consulting the secondary provider.
4. If every provider is unavailable or rate-limited, the registry returns `ProviderError::AllProvidersFailed` with the per-provider error list preserved in precedence order.

This preserves the distinction the automated oracle pipeline needs:

- `ProviderError::RateLimited` means the provider is healthy but backed off; retry later with the recommended `retry_after` delay.
- `ProviderError::Unavailable` means the provider is down or returning a transient server-side failure; fail over to the next provider immediately.
- `ProviderError::ConcurrencyLimitReached` means the per-provider request queue is saturated; back off or retry later rather than flagging a logical failure.

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

## Oracle Submission Rate Limiting

To prevent spam or denial-of-service against the on-chain oracle log, the
`OracleContract` enforces per-oracle submission limits on `submit_result` and
`submit_batch_results`:

| Limit | Default | Notes |
|-------|---------|-------|
| Hourly | 100 submissions | Rolling 1-hour window |
| Daily | 1,000 submissions | Rolling 24-hour window |

A `submit_batch_results` call counts its full entry count against both limits
in a single check — e.g. a 40-entry batch consumes 40 units of quota. The
check runs before any storage writes, so a rejected call (whole batch or
single result) never partially succeeds and never consumes quota.

### Sliding window algorithm

Limits are tracked with a sliding-window counter rather than a naive fixed
window, so a burst spanning a window boundary can't double the effective
limit. Each window (hourly, daily) stores:

- `window_start` — the timestamp (`env.ledger().timestamp()`) the current
  window began,
- `current_count` — submissions recorded since `window_start`,
- `previous_count` — submissions recorded in the window immediately before.

The estimated count for rate-limit purposes is:

```
estimate = current_count + previous_count * (window_size - elapsed_in_current) / window_size
```

This weights the previous window's count by how much of it still falls inside
the trailing lookback period, giving an accurate approximation of a true
sliding window without storing a timestamp per submission.

### Admin configuration

The admin can override the default limits per oracle address:

```rust
oracle_client.set_oracle_rate_limits(&oracle_address, &hourly_limit, &daily_limit);
```

- Passing `0` for either field resets that field to the contract default
  (100/1000).
- `hourly_limit` must not exceed `daily_limit` (when both are non-zero), or
  the call returns `Error::InvalidRateLimit`.
- Emits an `oracle / ratelim` event with `(oracle, hourly_limit, daily_limit)`.

### Querying rate limit status

There is no HTTP layer on-chain, so instead of rate-limit response headers,
callers query current usage directly:

```rust
let status = oracle_client.get_oracle_rate_limit_status(&oracle_address);
// status.hourly_used / .hourly_limit / .hourly_remaining
// status.daily_used  / .daily_limit  / .daily_remaining

let limits = oracle_client.get_oracle_rate_limits(&oracle_address);
// limits.hourly_limit / .daily_limit
```

### Suspicious pattern alerts

Once an oracle's usage reaches **80%** of either its hourly or daily limit,
the contract emits an `oracle / alert` event with
`(oracle, window_label, used, limit)`, where `window_label` is `"hourly"` or
`"daily"`. Off-chain monitoring can subscribe to this event to page an admin
before the oracle is actually throttled.

### Errors

- `Error::RateLimitExceeded` (9) — the submission(s) would exceed the
  oracle's hourly or daily limit.
- `Error::InvalidRateLimit` (10) — `set_oracle_rate_limits` was called with
  `hourly_limit > daily_limit`.

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

Both functions answer "has a result been submitted for this `match_id`?", but differ in access control:

| Function | Auth Required | Use Case |
|---|---|---|
| `has_result(match_id) -> bool` | None (public) | General-purpose polling by indexers, frontends, or the escrow contract's integrators. |
| `has_result_admin(match_id) -> Result<bool, Error>` | Admin (`require_auth`) | Private-tournament contexts where even the *existence* of a submitted result should not be observable by third parties before an official announcement. Returns `Error::Unauthorized` if the contract is uninitialized or the caller is not the admin. |

Both read the same underlying `DataKey::Result(match_id)` storage entry; `has_result_admin` adds an authorization gate in front of the identical lookup.

---

## Data Structures

The oracle contract's public `#[contracttype]` types, defined in `contracts/oracle/src/types.rs`:

### `ResultEntry`

Returned by `get_result(match_id)`.

| Field | Type | Description |
|---|---|---|
| `game_id` | `String` | External game ID from the chess platform. |
| `platform` | `Platform` | `Lichess` or `ChessDotCom`. |
| `result` | `Winner` | `Player1`, `Player2`, or `Draw`. |
| `submitted_ledger` | `u32` | Ledger sequence at submission time. |
| `submitter` | `Address` | Admin address that submitted the result. |

### `BatchResultEntry`

One entry within the `entries` vector passed to `submit_batch_results`.

| Field | Type | Description |
|---|---|---|
| `match_id` | `u64` | Match this entry's result applies to. |
| `game_id` | `String` | External game ID from the chess platform. |
| `platform` | `Platform` | `Lichess` or `ChessDotCom`. |
| `result` | `Winner` | `Player1`, `Player2`, or `Draw`. |

### `OracleRegistration`

Stored per oracle address via `register_oracle_with_stake`; read and mutated by `slash_oracle`.

| Field | Type | Description |
|---|---|---|
| `oracle_address` | `Address` | The registered oracle's address. |
| `oracle_stake` | `i128` | Token stake currently backing this oracle; reduced by `slash_oracle`. |
| `token` | `Address` | Token contract the stake was posted in. |

### `RateLimitConfig`

Returned by `get_oracle_rate_limits`; set by `set_oracle_rate_limits`.

| Field | Type | Description |
|---|---|---|
| `hourly_limit` | `u32` | Max submissions accepted per rolling hour. Defaults to 100 if never set. |
| `daily_limit` | `u32` | Max submissions accepted per rolling day. Defaults to 1,000 if never set. |

### `RateLimitStatus`

Returned by `get_oracle_rate_limit_status`. The on-chain analogue of HTTP rate-limit response headers.

| Field | Type | Description |
|---|---|---|
| `hourly_used` | `u32` | Estimated submissions counted in the current hourly sliding window. |
| `hourly_limit` | `u32` | Configured (or default) hourly limit. |
| `hourly_remaining` | `u32` | `hourly_limit - hourly_used`, saturating at 0. |
| `daily_used` | `u32` | Estimated submissions counted in the current daily sliding window. |
| `daily_limit` | `u32` | Configured (or default) daily limit. |
| `daily_remaining` | `u32` | `daily_limit - daily_used`, saturating at 0. |

## Contract Function Reference

The complete public function surface of `OracleContract` (`contracts/oracle/src/lib.rs`). Functions above with dedicated sections are cross-referenced rather than re-described.

| Function | Signature | Description |
|---|---|---|
| `initialize` | `(admin: Address)` | One-time setup; stores the admin address. Panics-free — returns `Error::AlreadyInitialized` on a second call. |
| `is_initialized` | `() -> bool` | Returns whether `initialize` has been called. |
| `register_oracle_with_stake` | `(oracle_address: Address, stake_amount: i128, token: Address)` | Transfers `stake_amount` of `token` from `oracle_address` into the contract as a slashable bond, recorded in `OracleRegistration`. |
| `slash_oracle` | `(oracle_address: Address, slash_amount: i128)` | Admin-only. Deducts `slash_amount` from a registered oracle's stake and transfers it to the admin. |
| `submit_result` | `(match_id: u64, game_id: String, platform: Platform, result: Winner)` | See [Submitting a Result](#submitting-a-result). Admin-authorized, rate-limited, one result per `match_id`. |
| `submit_batch_results` | `(entries: Vec<BatchResultEntry>)` | All-or-nothing submission of up to `MAX_BATCH_SIZE` (100) results in one call; each entry counts toward the submitting admin's rate limit. |
| `get_result` | `(match_id: u64) -> ResultEntry` | Returns the stored result, or `Error::ResultNotFound`. Extends the entry's TTL on read. |
| `has_result` | `(match_id: u64) -> bool` | Public existence check. See [has_result vs has_result_admin](#has_result-vs-has_result_admin). |
| `has_result_admin` | `(match_id: u64) -> Result<bool, Error>` | Admin-gated existence check. See [has_result vs has_result_admin](#has_result-vs-has_result_admin). |
| `delete_result` | `(match_id: u64)` | Admin-only, irreversible. See [Result Deletion Policy](#result-deletion-policy-delete_result). |
| `get_admin` | `() -> Address` | Returns the stored admin address, or `Error::Unauthorized` if uninitialized. |
| `update_admin` | `(new_admin: Address)` | Rotates the admin address. Requires current admin auth. |
| `pause` | `()` | Admin-only. Blocks `submit_result` and `submit_batch_results` while paused (`delete_result` is also blocked; see [Security: Oracle Contract Pause](security.md#oracle-contract-pause)). |
| `unpause` | `()` | Admin-only. Reverses `pause`. |
| `set_oracle_rate_limits` | `(oracle: Address, hourly_limit: u32, daily_limit: u32)` | Admin-only. Overrides the default hourly/daily submission caps for one oracle address. Pass `0` for either field to reset that field to the contract default. |
| `get_oracle_rate_limits` | `(oracle: Address) -> RateLimitConfig` | Returns the effective (override or default) rate-limit configuration for `oracle`. |
| `get_oracle_rate_limit_status` | `(oracle: Address) -> RateLimitStatus` | Returns `oracle`'s current sliding-window usage and remaining quota. |
| `set_rate` | `(token_a: Address, token_b: Address, rate: i128)` | Admin-only. Stores a fixed-point exchange rate (scaled `1e7`) between `token_a` and `token_b`, used by `swap` and by the escrow contract's multi-token conversion-rate validation (see [Roadmap v1.0.1](roadmap.md#v101--multi-token-conversion-rate-hardening-complete)). |
| `get_rate` | `(token_a: Address, token_b: Address) -> i128` | Returns the rate previously stored by `set_rate` for the ordered pair, or `Error::ResultNotFound` if none has been set. |
| `swap` | `(token_in: Address, token_out: Address, amount_in: i128, recipient: Address)` | Converts `amount_in` of `token_in` to `token_out` using whichever direction of a stored rate exists, and transfers the result to `recipient` from the contract's own balance. Does not pull `token_in` from the caller — callers must fund the contract with `token_in` separately. |

> **Note:** `set_rate`, `get_rate`, and `swap` predate formal documentation and currently carry no `# Errors` doc comments in source (unlike the rest of the contract's public functions). They are included here for conformance-checker coverage; treat their error behavior (both return `Error::ResultNotFound`/`Error::InvalidRateLimit`/`Error::Unauthorized` in the cases shown in `contracts/oracle/src/lib.rs:710-789`) as subject to change without the same stability guarantee as the rest of this reference.

---

## Example: Full Match Lifecycle

(Existing contract documentation continues unchanged.)


---

## Troubleshooting

### Rate limit exceeded (`RateLimitExceeded`)

**Symptom:** `submit_result` or `submit_batch_results` returns
`Error(Contract, #9)`.

**Cause:** The oracle has exhausted its hourly (100) or daily (1,000)
submission quota on the `OracleContract`.

**Fix:**
- Wait until the rolling window resets (up to 1 hour for hourly, 24 hours for
  daily).
- Query current usage before retrying:
  ```bash
  stellar contract invoke --id $CONTRACT_ORACLE \
    -- get_oracle_rate_limit_status --oracle <ORACLE_ADDRESS>
  ```
- If the default limits are too low for your workload, the admin can raise
  them:
  ```bash
  stellar contract invoke --id $CONTRACT_ORACLE \
    --source <ORACLE_ADMIN_KEYPAIR> \
    -- set_oracle_rate_limits \
    --oracle <ORACLE_ADDRESS> \
    --hourly_limit 500 \
    --daily_limit 5000
  ```

---

### API key invalid / authentication failure

**Symptom:** The off-chain oracle service logs `401 Unauthorized` or
`403 Forbidden` when calling the chess platform API.

**Cause:** `LICHESS_API_TOKEN` or `CHESSDOTCOM_API_KEY` in `.env` is missing,
expired, or incorrect.

**Fix:**
1. Re-generate or copy the correct key from your Lichess/Chess.com developer
   account.
2. Update `.env`:
   ```env
   LICHESS_API_TOKEN=lip_xxxxxxxxxxxx
   CHESSDOTCOM_API_KEY=your-key-here
   ```
3. Restart the oracle service. No on-chain changes are required.

---

### Game not finished yet (`GameNotFinished`)

**Symptom:** The oracle service logs `GameNotFinished` and does not submit a
result; the match stays `Active` on-chain.

**Cause:** The chess platform API returned a game payload without a terminal
`end.result` field — the game is still in progress.

**Fix:** This is expected behaviour. The oracle will retry automatically. No
manual intervention is needed unless the game has genuinely ended but the
platform API is lagging. In that case:
- Wait a few minutes and allow the retry backoff to resolve it.
- If the platform API continues to show the game as in progress after it has
  clearly ended, contact the platform's support or wait for the result to
  propagate (usually < 5 minutes).

---

### Network timeout / chess platform unreachable

**Symptom:** Oracle service logs `timeout`, `connection refused`, or
`HttpStatus` errors; no result is submitted on-chain.

**Cause:** The chess platform API is temporarily unreachable, or the 30-second
HTTP timeout was exceeded.

**Fix:**
- The oracle will not submit a result until a verified end-state is confirmed.
  Retry is automatic with exponential backoff.
- Check the platform's status page ([lichess.org/status](https://lichess.org/status)
  or [chess.com](https://www.chess.com)) for ongoing incidents.
- Verify outbound connectivity from the oracle host:
  ```bash
  curl -I https://lichess.org/game/export/abcd1234
  curl -I https://api.chess.com/pub/game/123456789
  ```
- If the oracle host is behind a firewall, ensure outbound HTTPS (port 443) is
  open to the chess platform domains.

---

### Oracle not submitting results (wrong oracle address configured)

**Symptom:** `submit_result` returns `UnauthorizedOracle`; the transaction is
signed by the oracle keypair but still rejected.

**Cause:** The escrow contract's stored oracle address does not match the
keypair the oracle service is using.

**Fix:** Check which address the escrow contract has on record:
```bash
stellar contract invoke --id $CONTRACT_ESCROW -- get_oracle
```
Compare this to the oracle service's configured keypair address. If they
differ, either:
- Update the oracle service's keypair to match the on-chain address, or
- Rotate the on-chain oracle address (requires escrow admin):
  ```bash
  stellar contract invoke --id $CONTRACT_ESCROW \
    --source <ESCROW_ADMIN_KEYPAIR> \
    -- update_oracle \
    --new_oracle <CORRECT_ORACLE_ADDRESS>
  ```
