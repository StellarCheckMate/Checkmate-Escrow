## Health Check

The oracle exposes a `/health` endpoint to monitor real-time connectivity and dependency health.

**Endpoint:** GET /health

Returns a comprehensive health status report including:
- Overall service status: `healthy`, `degraded`, or `unhealthy`
- Per-dependency checks (Stellar RPC, escrow contract, oracle contract, chess APIs)
- Latency metrics for each dependency
- Synthetic canary check status (planned)
- Service uptime counter

**Full documentation:** See [docs/monitoring-health-checks.md](monitoring-health-checks.md)

**Integration guide:** See [docs/health-check-integration.md](health-check-integration.md)

The health check system runs every 30 seconds and performs real connectivity
verification to each critical dependency. Unlike the old hardcoded response, the
health check now:

1. Verifies Stellar RPC connectivity with actual `getNetwork` calls
2. Tests escrow contract reachability via `getLedgerEntries`
3. Tests oracle contract reachability
4. Checks chess platform API availability (Lichess and Chess.com)
5. Distinguishes transient failures (rate limits, timeouts) from permanent outages

---

## Oracle Contract Role

The escrow contract uses its configured oracle address as the authoritative
permission for submitting results to trigger payouts. The oracle contract is
supplementary: it does not authorise escrow payouts or act as a gatekeeper for
escrow result submission. It provides an audit log and an independent on-chain
record of results that can be queried later — and, as of the m-of-n consensus
feature described below, it can itself require independent agreement from
multiple staked oracles before that record is written, rather than trusting a
single admin-controlled submitter.

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
  admin-gated read interfaces. It additionally supports a genuine multi-oracle
  consensus path — see [m-of-n Oracle Consensus](#m-of-n-oracle-consensus).

---

## m-of-n Oracle Consensus

### Background

Earlier versions of `OracleContract` shipped staking primitives
(`register_oracle_with_stake` / `slash_oracle`) that looked like the
foundation of a decentralized oracle set, but nothing in the contract actually
required multiple oracles to agree. `submit_result` and `submit_batch_results`
are gated purely by `admin.require_auth()`, and their stake check only ever
looks up `OracleRegistration(admin)` — because the admin is the only address
that can call them. In effect, the "oracle" was a single trusted address, and
the staking/slashing machinery was decorative: it could not be triggered by
anything other than a manual admin call.

`submit_oracle_result` (below) is the genuine multi-oracle path. It makes the
existing staking machinery load-bearing: a result-submitting oracle without
adequate stake is rejected, and provable equivocation triggers automatic
slashing. `submit_result` / `submit_batch_results` are unchanged and remain
available — see [Migration path](#migration-path-from-single-oracle).

### Registration and threshold

Each oracle registers independently with its own key and its own stake:

```rust
oracle_client.register_oracle_with_stake(&oracle_address, &stake_amount, &token);
```

Registering appends the address to the contract's oracle set (queryable via
`get_registered_oracle_count`). The admin configures how many *distinct*
registered oracles must submit a matching result before it finalizes:

```rust
oracle_client.set_consensus_threshold(&m); // m ∈ [1, n]
```

The threshold defaults to **1** if never set — the degenerate single-oracle
configuration described in [Migration path](#migration-path-from-single-oracle).
`get_consensus_threshold()` reads the current value.

### Submitting a vote

```rust
oracle_client.submit_oracle_result(&oracle_address, &match_id, &game_id, &platform, &result);
```

Unlike `submit_result`, this is authenticated by the **submitting oracle**,
not the admin (`oracle_address.require_auth()`). Each call:

1. Rejects if the contract is paused, uninitialized, `game_id` is empty, the
   caller never registered stake (`NotRegisteredOracle`), the caller's stake
   has been slashed to zero (`InsufficientStake`), the match is already
   finalized (`AlreadySubmitted`), or the caller's per-oracle rate limit is
   exceeded (`RateLimitExceeded`) — see [Oracle Submission Rate
   Limiting](#oracle-submission-rate-limiting), which applies identically to
   this path.
2. Groups the vote into a *candidate*: the distinct `(game_id, platform,
   result)` tuple it matches. Each candidate tracks the list of oracles that
   voted for it.
3. If a candidate's submitter count reaches the configured threshold, the
   match **finalizes**: the winning candidate is written to the same
   `Result(match_id)` storage that `submit_result` writes, so `get_result` /
   `has_result` behave identically regardless of which path finalized the
   match.
4. If no candidate has reached the threshold yet, the match stays pending —
   query `get_match_votes(match_id)` to see the current tally.

With the default threshold of 1, a single registered oracle's vote finalizes
immediately: this is the m-of-n code path running in its n=1 degenerate form,
distinct from (but behaviourally equivalent to) the legacy admin-gated
`submit_result`.

### Disagreement handling

Disagreement is resolved with an explicit, two-tier policy: **majority wins,
minority is slashed**, with an **admin-resolved deadlock fallback** for splits
that can never resolve on their own.

**1. Majority wins, minority is slashed (the common case).** When a
candidate's vote count reaches the threshold, the match finalizes with that
result. Every oracle that had already voted for a *different* candidate for
the same match is automatically slashed `MINORITY_SLASH_BPS` (10%) of its
remaining stake, transferred to the admin/treasury address, and an
`oracle / minority` event is emitted naming the slashed oracle. The penalty is
deliberately lighter than the equivocation penalty (below) because landing in
the minority can reflect an honest error — a stale platform-API read, a
transient network partition — rather than malice.

**2. Deadlock → disputed → admin resolution (the unresolvable case).** After
every vote that doesn't finalize, the contract checks whether consensus is
still mathematically reachable: it counts the registered, currently-staked
oracles that haven't yet voted on this match (`remaining_eligible_oracles`),
and checks whether *any* existing candidate's vote count plus that remaining
pool could still reach the threshold. If not — for example, a 3-way split
among 3 oracles at threshold 2, where every oracle has already voted for a
different result — the match is flagged `disputed` (`get_match_votes` returns
`disputed: true`, and an `oracle / disputed` event fires) instead of hanging
forever waiting for votes that can never arrive.

A disputed match is resolved by the admin, who acts as tie-breaker of last
resort — consistent with the admin's existing ultimate authority elsewhere in
this contract (`slash_oracle`, `update_admin`, `pause`):

```rust
oracle_client.resolve_disputed_match(&match_id, &game_id, &platform, &result);
```

This finalizes the match with the admin's chosen result and slashes
`MINORITY_SLASH_BPS` from every oracle whose recorded vote disagreed with it —
so attempting to force a deadlock (to trigger a slower, admin-mediated path)
still costs a losing oracle its stake, the same as losing an ordinary
majority vote.

We chose this policy — rather than escalating every disagreement into
`EscrowContract`'s heavier bonded, snapshot-based dispute-voting system (see
[docs/dispute-governance.md](dispute-governance.md)) — because that mechanism
is designed for economic disputes over a *single* asserted result raised by
third-party token holders, not for reconciling *which of several
independently-submitted oracle results* to accept in the first place. Routing
every m-of-n disagreement through it would mean every close vote incurs a
bond, a voting period, and a quorum check, even though the oracle contract
already has the information needed to resolve most disagreements immediately
via majority count. The admin-resolution fallback here is intentionally
narrow: it only fires when the vote is *unresolvable* by counting, not
whenever there's any disagreement at all. A future iteration could route
`resolve_disputed_match` through the escrow dispute-voting system instead of
direct admin authority, trading a slower resolution for removing the admin as
a trust bottleneck in the rare deadlock case — see [Known
limitations](#known-limitations-of-this-design).

### Equivocation

If the *same* oracle submits two different `(game_id, platform, result)`
votes for the *same* `match_id`, the second submission is provable
equivocation: the oracle has signed two mutually-exclusive claims about the
same event. It cannot be an honest mistake in the way a minority vote can — an
honest oracle simply doesn't re-submit a different answer to the same
question. The vote is discarded (not counted toward any candidate) and the
oracle's **entire remaining stake** is slashed (`EQUIVOCATION_SLASH_BPS`,
100%) — effectively ejecting it from further participation, since
`submit_oracle_result` rejects any oracle with `oracle_stake <= 0` with
`InsufficientStake`.

Equivocation slashing is signaled by the `oracle / equivoc` event rather than
an error return. This is a deliberate consequence of Soroban's execution
model: a contract call that returns `Err` reverts **every** storage write
made during that call, including the slash. Returning an error here would
silently undo the punishment it was reporting. Off-chain callers should treat
a submission that emits `oracle / equivoc` as rejected, exactly as if it had
errored, but detect it by watching contract events rather than the call's
return value.

### Byzantine-fault-tolerance bound

Let `n` be the number of registered, staked oracles and `m` be the configured
threshold. Let `f` be the number of oracles that collude (vote identically on
a false result, in an attempt to force it through).

- **Safety** (a colluding minority cannot force a false result): requires
  `f < m`. If `f ≥ m`, the colluding set alone can supply enough matching
  votes to finalize before any honest oracle needs to act at all.
- **Liveness** (the honest oracles can still finalize the true result even if
  every colluding oracle refuses to cooperate): requires `n − f ≥ m`, i.e.
  `f ≤ n − m`.

Combining both: the system tolerates up to

```
f_max = min(m − 1, n − m)
```

colluding oracles while still guaranteeing both that a false result can't be
forced and that a true result can still be finalized. This bound is maximized
by a simple-majority threshold `m = ⌊n/2⌋ + 1`, which gives the familiar
`f_max = ⌊(n − 1) / 2⌋` — the same "more than half honest" bound as classical
BFT quorum systems. Choosing `m` closer to `n` (e.g. requiring near-unanimity)
tightens the safety margin but weakens liveness, since it takes fewer
colluding/offline oracles to prevent the honest majority from ever reaching
the threshold; choosing `m` closer to 1 does the reverse.

This is a **threshold-voting bound backed by economic slashing**, not a full
Byzantine state-machine-replication protocol: there is no leader election,
view-change, or cryptographic message-authentication step beyond Soroban's
own `require_auth`. A colluding set larger than `f_max` is not cryptographically
prevented from finalizing a false result — it is only made expensive after the
fact via the minority/equivocation slashing described above, and only for
oracles that end up provably in the minority or that equivocate. Operators
choosing `m` should size it, and the number of independently-operated oracles
`n`, so that `f_max` exceeds the number of oracle operators they are willing
to assume could plausibly collude.

### Migration path from single-oracle

The original single-admin-oracle deployment continues to work unmodified:

| | Legacy (`submit_result`) | Consensus (`submit_oracle_result`) |
|---|---|---|
| Auth | `admin.require_auth()` | `oracle.require_auth()` (any registered oracle) |
| Finalizes on | First (and only) call | Threshold-many matching votes |
| Default n | 1 (hardcoded — only admin can call it) | 1 (`ConsensusThreshold` defaults to 1) |
| Disagreement | Not possible — one submitter | Majority-wins-with-slash, or admin-resolved deadlock |

To migrate an existing single-oracle deployment to genuine m-of-n:

1. **Register additional independent oracles.** Each new oracle operator
   calls `register_oracle_with_stake` with its own key and its own stake —
   no admin action is required to add an oracle to the set.
2. **Raise the threshold.** Once enough oracles are registered, the admin
   calls `set_consensus_threshold(m)` with `1 < m ≤ n`. Existing pending
   votes recorded before the change are re-evaluated against the new
   threshold on their next submission; already-finalized results are
   unaffected.
3. **Point the off-chain oracle fleet at `submit_oracle_result`.** Each
   independent oracle service instance authenticates as itself instead of a
   shared admin key.
4. **(Optional) retire the legacy path operationally.** `submit_result` and
   `submit_batch_results` remain callable by the admin indefinitely — they
   are not disabled by raising the threshold. This is intentional for
   backward compatibility and emergency admin override, but it also means
   the admin retains a standing unilateral override regardless of the
   configured m-of-n threshold. Deployments that want the full
   Byzantine-fault-tolerance guarantee above to hold in practice, not just on
   the `submit_oracle_result` path, should treat the admin key with the same
   operational care as an `f_max`-sized set of colluding oracles (e.g. behind
   a multisig or hardware-backed signer used only for emergencies), since it
   can unilaterally finalize any match regardless of the registered oracles'
   votes. Fully removing that override — e.g. a one-way "renounce single-
   submitter admin path" switch — is not implemented; see [Known
   limitations](#known-limitations-of-this-design).

To roll back to single-oracle behavior, call `set_consensus_threshold(1)`;
any one registered oracle's vote (or the admin via the legacy path) then
finalizes a match again.

### Known limitations of this design

- The admin retains a standing override via `submit_result` /
  `submit_batch_results` regardless of the configured threshold (see above).
- The deadlock-resolution fallback (`resolve_disputed_match`) is admin-gated
  rather than routed through `EscrowContract`'s dispute-voting system; see the
  rationale in [Disagreement handling](#disagreement-handling).
- `remaining_eligible_oracles` scans the full registered oracle set on every
  non-finalizing vote (see the [gas benchmarks](#consensus-gas-benchmarks)
  below for measured cost at n = 3/5/10/25) — deployments expecting very
  large oracle sets (hundreds+) should budget for this scaling when sizing
  per-transaction resource limits.
- Vote weight is equal per oracle regardless of stake size; a larger stake
  only means a larger slashing penalty in absolute terms, not more voting
  power. Stake-weighted voting was considered out of scope for this change.

### Consensus gas benchmarks

`contracts/oracle/tests/benchmarks.rs` measures `submit_oracle_result` at
n = 3, 5, 10, and 25 registered oracles, run via:

```bash
cargo test -p oracle --test benchmarks -- --nocapture
```

It isolates two costs that scale differently with the registered oracle set
size `n`:

- **Pending vote** — a submission that does not yet reach the threshold, and
  therefore runs the O(n) deadlock-detection scan over the full registered
  set on every call.
- **Finalizing vote** — the submission that crosses the threshold; its extra
  cost comes from walking each losing candidate's submitter list to
  auto-slash the minority, so it scales with how many oracles already voted
  for a different result, not with `n` directly.

A JSON report is written to
`reports/performance/oracle-consensus-benchmark-results.json`. Representative
CPU-instruction costs from a local run (exact numbers vary by SDK/host
version — treat the report as authoritative over any numbers reproduced
here):

| n (registered oracles) | Pending vote (CPU insns) | Finalizing vote (CPU insns) |
|---:|---:|---:|
| 3 | ~390K | ~620K |
| 5 | ~500K | ~730K |
| 10 | ~840K | ~1.00M |
| 25 | ~2.11M | ~1.72M |

The pending-vote cost grows roughly linearly with `n` (the deadlock scan
touches every registered oracle); the finalizing-vote cost grows more slowly,
since it is bounded by the number of minority submitters rather than the full
registry.

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

---

## Swap Function (Token Exchange)

The `swap` function atomically exchanges `token_in` for `token_out` using a stored fixed-point exchange rate. It is intended for multi-token settlement and conversions.

### Function Signature

```rust
pub fn swap(
    env: Env,
    caller: Address,          // Must authorize the swap
    token_in: Address,        // Token to provide (collected from caller)
    token_out: Address,       // Token to receive (dispensed from contract)
    amount_in: i128,          // Amount of token_in to provide (must be > 0)
    min_amount_out: i128,     // Minimum acceptable token_out (slippage floor)
    recipient: Address,       // Address to receive token_out
) -> Result<(), Error>
```

### Execution Flow (Atomic, Single Transaction)

1. **Authentication:** Verify `caller` has signed the transaction via `caller.require_auth()`.
2. **Input Validation:** Ensure `amount_in > 0`.
3. **Rate Lookup:** Find the exchange rate for `(token_in, token_out)` in either direction.
4. **Compute Output:** Calculate `amount_out` using the rate, scaled as `1e7`.
5. **Slippage Check:** Verify `amount_out ≥ min_amount_out`; abort if not.
6. **Collect Input:** Transfer `amount_in` of `token_in` from `caller` to the contract.
7. **Dispense Output:** Transfer `amount_out` of `token_out` from the contract to `recipient`.

If any step fails, the entire transaction is rolled back (no partial settlement).

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `caller` | `Address` | Yes | The account authorizing and funding the swap. Must sign the transaction. |
| `token_in` | `Address` | Yes | The contract address of the input token. |
| `token_out` | `Address` | Yes | The contract address of the output token. |
| `amount_in` | `i128` | Yes | Quantity of `token_in` to provide. Must be > 0. |
| `min_amount_out` | `i128` | Yes | Minimum acceptable `token_out` to receive. Caller's slippage floor. |
| `recipient` | `Address` | Yes | Address to receive `amount_out` of `token_out`. Can be `caller` or any other address. |

### Errors

| Error | Cause | Recovery |
|-------|-------|----------|
| `Unauthorized` | Contract uninitialized, or insufficient admin stake. | Initialize and ensure oracle stakes exist. |
| `InvalidAmount` | `amount_in ≤ 0`. | Provide a positive `amount_in`. |
| `ResultNotFound` | No rate exists for `(token_in, token_out)` in either direction. | Admin must call `set_rate` to establish the rate. |
| `Overflow` | Numeric overflow during rate multiplication or division. | Use smaller `amount_in` or lower `rate`. |
| `SlippageExceeded` | Computed `amount_out < min_amount_out`. | Increase `min_amount_out` tolerance or wait for better rate. |

### Rate Directions

The contract stores rates bidirectionally:

- **Forward:** `set_rate(token_out, token_in, rate)` means "1 unit of `token_in` costs `rate / 1e7` units of `token_out`"
  ```
  amount_out = amount_in * rate / 1e7
  ```

- **Inverse:** `set_rate(token_in, token_out, rate)` means "1 unit of `token_out` costs `rate / 1e7` units of `token_in`"
  ```
  amount_out = amount_in * 1e7 / rate
  ```

### Examples

**Example 1: Simple exchange (1:1 rate)**

```rust
// Set rate: 1 USDC = 1 XLM
oracle_client.set_rate(&usdc_addr, &xlm_addr, &10_000_000);

// Swap 100 USDC for XLM (expecting ~100 XLM)
oracle_client.swap(
    &my_address,    // caller (must sign)
    &usdc_addr,     // token_in
    &xlm_addr,      // token_out
    &100,           // amount_in (100 USDC)
    &95,            // min_amount_out (accept 95–100 XLM, reject if <95)
    &my_address,    // recipient (me)
);
// Result: My balance increases by ~100 XLM, decreases by 100 USDC
```

**Example 2: Slippage protection (swapping to different recipient)**

```rust
// Rate: 1 ETH = 0.5 BTC
oracle_client.set_rate(&btc_addr, &eth_addr, &5_000_000);

// Swap 10 ETH for BTC, send to vault
oracle_client.swap(
    &my_address,      // caller (I pay)
    &eth_addr,        // token_in
    &btc_addr,        // token_out
    &10,              // amount_in (10 ETH)
    &4_500_000,       // min_amount_out (floor at 4.5 BTC; if <, fail)
    &vault_address,   // recipient (vault receives BTC)
);
// Result: I lose 10 ETH, vault gains ~5 BTC
```

**Example 3: Tight slippage (price-sensitive)**

```rust
// Rate: 1 GOLD = 0.01 SILVER
oracle_client.set_rate(&silver_addr, &gold_addr, &100_000);

// Swap 1000 SILVER for GOLD, accept only 9.99–10 GOLD
oracle_client.swap(
    &trader_address,
    &silver_addr,
    &gold_addr,
    &1000,
    &9_990_000,      // min_amount_out = 9.99 GOLD (0.01% slippage tolerance)
    &trader_address,
);
// If computed amount_out < 9.99, transaction fails without settling
```

### Security Properties

**1. Authorization Required**
- The `caller` must cryptographically sign the transaction.
- Without a valid signature, `swap` is rejected before any state changes.

**2. Atomic Settlement**
- Token input is collected **before** token output is dispensed.
- If the contract doesn't have sufficient `token_out` balance, the output transfer fails and the entire transaction is rolled back.
- No partial settlements (both-or-nothing).

**3. Slippage Protection**
- Caller specifies `min_amount_out`, enforced before settlement.
- If the rate has moved unfavorably (smaller payout), the transaction aborts.
- Protects against MEV, flash loans, and delayed execution.

**4. Reentrancy Safety**
- If `token_out` is a malicious contract with a callback hook, the callback fires during the output transfer (step 7).
- By that time, the input transfer is complete (step 6).
- The callback cannot re-enter `swap` and claim additional funds because the contract has already received the caller's `token_in`.
- Thus, no double-extraction or fund drain via reentrancy is possible.

### Admin Pricing Model

The contract uses **fixed exchange rates** set by the admin via `set_rate`:

```rust
pub fn set_rate(
    env: Env,
    token_a: Address,
    token_b: Address,
    rate: i128,       // Fixed-point rate, scaled 1e7
) -> Result<(), Error>
```

**Advantages:**
- ✅ Deterministic and auditable
- ✅ No external oracle dependency
- ✅ Instant (no latency)
- ✅ Admin has full control

**Limitations:**
- ❌ Static (doesn't respond to market changes)
- ❌ No staleness bounds (rate can be arbitrarily old)
- ❌ No deviation checks (admin can set unreasonable rates)

**Future Enhancements (v1.1+):**
- Oracle-fed rates from an external price feed
- Staleness checks (e.g., reject if rate is >1 hour old)
- Deviation bounds (e.g., reject if rate changed >10% since last update)
- Multi-provider fallback and consensus

### Use Cases

1. **Multi-token escrow settlement:** If a match is funded in one token but requires payout in another, use `swap` to convert.
2. **Cross-asset tournaments:** Players deposit in different stablecoins; `swap` unifies payouts to a single token.
3. **Liquidity protocol:** The contract can mediate token exchanges with stored rates.

### Not Recommended For

- **High-frequency trading:** Rates are static; no real-time market response.
- **Large swaps:** Slippage protection is caller-specified (not algorithmic); large swaps may breach tolerance.
- **Volatile pairs:** Fixed rates quickly become stale; consider oracle-fed rates instead.

---

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
| `set_rate` | `(token_a: Address, token_b: Address, rate: i128)` | Admin-only. Stores a fixed-point exchange rate (scaled `1e7`) between `token_a` and `token_b`, used by `swap` for atomic token conversion. |
| `get_rate` | `(token_a: Address, token_b: Address) -> i128` | Public query. Returns the rate previously stored by `set_rate` for the ordered pair, or `Error::ResultNotFound` if none has been set. |
| `swap` | `(caller: Address, token_in: Address, token_out: Address, amount_in: i128, min_amount_out: i128, recipient: Address)` | Atomically exchanges `amount_in` of `token_in` (collected from `caller`) for `token_out` (dispensed to `recipient`) using a stored rate. Requires `caller` authorization and enforces slippage protection via `min_amount_out`. See [Swap Function](#swap-function-token-exchange) for complete details. |

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
