# Oracle Troubleshooting Guide

This guide covers the most common failure scenarios operators encounter when
running the Checkmate-Escrow oracle service and provides concrete remediation
steps for each.

For general oracle architecture and configuration, see
[docs/oracle.md](oracle.md).

---

## Table of Contents

1. [Dead-letter backlog accumulation](#dead-letter-backlog-accumulation)
2. [RPC connectivity failures](#rpc-connectivity-failures)
3. [Lichess API errors](#lichess-api-errors)
4. [Chess.com API errors](#chesscom-api-errors)
5. [Rate limit errors](#rate-limit-errors)
6. [Stale or unresolvable game IDs](#stale-or-unresolvable-game-ids)
7. [Oracle not submitting results](#oracle-not-submitting-results)
8. [Replay commands reference](#replay-commands-reference)
9. [Health check quick-reference](#health-check-quick-reference)

---

## Dead-letter backlog accumulation

**Symptom:** The oracle service logs messages like
`[dead_letter] entry promoted after N retries` or
`dead_letter queue depth: N`. The Prometheus metric
`oracle_dead_letter_queue_depth` is non-zero and growing.

**Cause:** One or more match-result submissions have failed repeatedly and
exceeded the retry budget. Entries in the dead-letter queue (DLQ) are no
longer automatically retried.

**Immediate diagnosis:**

```bash
# Check current DLQ depth via the health endpoint
curl -s http://localhost:8080/health | jq '.checks.dead_letter'

# Or inspect the Prometheus metric directly
curl -s http://localhost:9000/metrics | grep oracle_dead_letter
```

**Causes and fixes:**

| Root cause | Indicator | Fix |
|---|---|---|
| Persistent chess API outage | All DLQ entries share the same platform | Wait for the platform to recover, then replay (see [Replay commands](#replay-commands-reference)) |
| Wrong oracle keypair on-chain | DLQ entries all fail with `Unauthorized` | See [Oracle not submitting results](#oracle-not-submitting-results) |
| Match already settled | DLQ entries fail with `OracleAlreadyConfirmed` or `InvalidState` | Safe to discard — the match was settled by another path |
| Invalid game ID | DLQ entries fail with `InvalidGameId` | Verify the game IDs with the chess platform API and correct the source data |
| Soroban RPC unreachable | DLQ entries fail with RPC errors | See [RPC connectivity failures](#rpc-connectivity-failures) |

**Replaying dead-letter entries:**

```bash
# Replay all DLQ entries (oracle-service binary must be running)
curl -X POST http://localhost:8080/api/dead_letter/replay

# Replay a specific entry by match ID
curl -X POST http://localhost:8080/api/dead_letter/replay \
  -H "Content-Type: application/json" \
  -d '{"match_id": 42}'
```

**Discarding stale entries:**

If entries correspond to already-settled matches, discard them to reduce noise:

```bash
# Discard a specific DLQ entry
curl -X DELETE http://localhost:8080/api/dead_letter/42
```

**Alerting:** Consider setting a Prometheus alert when
`oracle_dead_letter_queue_depth > 10` persists for more than 5 minutes. See
[docs/monitoring-setup.md](monitoring-setup.md) for alert rule examples.

---

## RPC connectivity failures

**Symptom:** Oracle service logs `rpc error`, `connection refused`,
`request timeout`, or `UnknownError(RpcTransport)`. No results are submitted
on-chain.

**Cause:** The oracle service cannot reach the Soroban RPC endpoint configured
in `STELLAR_RPC_URL`.

**Diagnosis:**

```bash
# Test RPC connectivity directly
curl -s -X POST "$STELLAR_RPC_URL" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getNetwork","params":{}}' \
  | jq '.result.networkPassphrase'

# Check oracle health endpoint
curl -s http://localhost:8080/health | jq '.checks.stellar_rpc'
```

**Fixes:**

1. **Verify the RPC URL** in your `.env`:
   ```env
   STELLAR_RPC_URL=https://soroban-testnet.stellar.org
   ```
   Common mistakes: trailing slash, wrong network (testnet vs mainnet), or a
   stale custom node URL.

2. **Check the public RPC node status:**
   - Testnet: [https://status.stellar.org](https://status.stellar.org)
   - If the public node is down, switch to a backup:
     ```env
     # Horizon-based fallback (less feature-complete, but good for basic ops)
     STELLAR_RPC_URL=https://horizon-testnet.stellar.org
     ```

3. **Firewall / egress:** Ensure outbound HTTPS (port 443) is open from the
   oracle host to the RPC endpoint.

4. **Private node connectivity:**
   ```bash
   # From the oracle host
   curl -I https://soroban-testnet.stellar.org
   traceroute soroban-testnet.stellar.org
   ```

5. **Retry behaviour:** The oracle service uses exponential backoff with jitter
   for RPC failures. Short outages (< 2 minutes) typically self-heal without
   intervention. DLQ accumulation indicates a longer-lived issue.

---

## Lichess API errors

**Symptom:** Oracle service logs `lichess: 401 Unauthorized`,
`lichess: 429 Too Many Requests`, `lichess: 404 Not Found`, or
`GameNotFinished` for Lichess-platform games.

### 401 Unauthorized

**Cause:** `LICHESS_API_TOKEN` is missing, expired, or revoked.

**Fix:**
1. Visit [https://lichess.org/account/oauth/token](https://lichess.org/account/oauth/token)
   and generate a new token with the `game:read` scope.
2. Update `.env`:
   ```env
   LICHESS_API_TOKEN=<your-lichess-api-token>
   ```
3. Restart the oracle service.

### 404 Not Found

**Cause:** The `game_id` stored on-chain does not correspond to a real Lichess
game, or the game was deleted.

**Fix:**
1. Verify the game exists:
   ```bash
   curl -s "https://lichess.org/game/export/<GAME_ID>?pgnInJson=true" \
     -H "Authorization: Bearer $LICHESS_API_TOKEN" | jq '.status'
   ```
2. If the game does not exist, the match cannot be auto-resolved by the oracle.
   Escalate to admin dispute resolution — see
   [docs/dispute-governance.md](dispute-governance.md).

### 429 Too Many Requests

**Cause:** Lichess rate limit exceeded. The public API allows approximately
20 requests per second per IP.

**Fix:**
- The oracle service applies automatic backoff. For sustained overload, reduce
  `ORACLE_POLL_INTERVAL_SECS` in `.env` or distribute load across multiple
  oracle instances.
- Lichess provides a dedicated API token quota. For high-volume deployments,
  apply for a [Lichess patron token](https://lichess.org/patron) with higher
  limits.

### GameNotFinished

**Cause:** The Lichess API returned a game payload but the game has not yet
concluded (status is not `mate`, `resign`, `draw`, etc.).

**Fix:** This is expected. The oracle will retry on the next poll cycle. No
action needed unless the game has ended but the result has not propagated:
- Wait up to 5 minutes for Lichess to update the game status.
- For games suspected of being abandoned, check the match timeout and consider
  calling `expire_match` once the timeout elapses.

---

## Chess.com API errors

**Symptom:** Oracle service logs `chessdotcom: 401`, `chessdotcom: 403`,
`chessdotcom: 404`, `chessdotcom: 429`, or `chessdotcom: 503`.

### 401 / 403 Unauthorized or Forbidden

**Cause:** `CHESSDOTCOM_API_KEY` is missing, revoked, or the account has been
suspended.

**Fix:**
1. Verify the key in your [Chess.com developer portal](https://www.chess.com/club/chess-com-developer-community).
2. Update `.env`:
   ```env
   CHESSDOTCOM_API_KEY=your-key-here
   ```
3. Restart the oracle service.

### 404 Not Found

**Cause:** The `game_id` does not match any Chess.com game, or the game is
private / deleted.

**Fix:**
1. Verify the game URL directly:
   ```bash
   curl -s "https://api.chess.com/pub/game/<GAME_ID>" | jq '.end_time'
   ```
2. If the game does not exist, the oracle cannot resolve the match
   automatically. Use admin dispute resolution.

### 429 Too Many Requests

**Cause:** Chess.com enforces a rate limit of ~1 request/second for
unauthenticated clients and a higher quota for API key holders.

**Fix:**
- Increase `ORACLE_POLL_INTERVAL_SECS` to reduce call frequency.
- The oracle service's built-in rate limiter should prevent sustained 429s.
  If they persist, check for duplicate oracle instances or runaway retry loops.

### 503 Service Unavailable

**Cause:** Chess.com is temporarily unavailable.

**Fix:**
- Check [https://www.chess.com](https://www.chess.com) for ongoing incidents.
- The oracle service will back off automatically. No intervention needed for
  outages under 30 minutes.

---

## Rate limit errors

**Symptom:** `submit_result` or `submit_result_batch` on-chain calls fail with
`Error(Contract, #9)` (maps to `ContractPaused` in the error enum — reused for
the oracle rate limiter).

**Cause:** The oracle address has exhausted its on-chain submission quota on
the `OracleContract` (default: 100/hour, 1,000/day).

**Diagnosis:**

```bash
stellar contract invoke --id $CONTRACT_ORACLE \
  -- get_oracle_rate_limit_status \
  --oracle <ORACLE_ADDRESS>
```

**Fix:**

- **Wait for the rolling window to reset** (up to 1 hour for hourly, 24 hours
  for daily limits).
- **Raise the limits** if the default quota is too low for your deployment
  (requires oracle admin):
  ```bash
  stellar contract invoke --id $CONTRACT_ORACLE \
    --source <ORACLE_ADMIN_KEYPAIR> \
    -- set_oracle_rate_limits \
    --oracle <ORACLE_ADDRESS> \
    --hourly_limit 500 \
    --daily_limit 5000
  ```
- **Batch submissions** — prefer `submit_result_batch` over individual
  `submit_result` calls to maximise throughput within the quota.

---

## Stale or unresolvable game IDs

**Symptom:** The oracle service repeatedly attempts to fetch a game result but
never succeeds. The match remains `Active` indefinitely. Oracle logs show
`GameNotFinished`, `404 Not Found`, or `InvalidGameId`.

**Cause:** The `game_id` recorded on-chain is incorrect, the game was never
started, or the game was played on a different platform than the `platform`
field indicates.

**Diagnosis:**

```bash
# Retrieve the game_id from the on-chain match record
stellar contract invoke --id $CONTRACT_ESCROW \
  -- get_match --match_id <MATCH_ID>
```

Cross-check the `game_id` and `platform` fields:
- **Lichess game ID format:** 8 alphanumeric characters (e.g. `aBcD1234`)
- **Chess.com game ID format:** numeric (e.g. `12345678901`)

If the platform is wrong (e.g., a Lichess ID registered as Chess.com):

1. The oracle will never resolve this match automatically.
2. Players can call `cancel_match` (before the match becomes `Active`) or
   wait for `expire_match` to recover funds.
3. For `Active` matches where both players have deposited, use admin stall
   resolution after the stall window elapses
   (`ADMIN_STALL_WINDOW_SECONDS`, default 7 days):
   ```bash
   stellar contract invoke --id $CONTRACT_ESCROW \
     --source <ADMIN_KEYPAIR> \
     -- admin_resolve_stalled_match \
     --match_id <MATCH_ID>
   ```

---

## Oracle not submitting results

**Symptom:** `submit_result` is rejected with `Error(Contract, #4)`
(`Unauthorized`). The oracle service signs the transaction correctly but the
on-chain call still fails.

**Cause:** The oracle address stored in the escrow contract does not match the
keypair the oracle service is using.

**Diagnosis:**

```bash
# Read the on-chain oracle address
stellar contract invoke --id $CONTRACT_ESCROW -- get_oracle_address

# Compare with the oracle service's configured address
cat .env | grep ORACLE_SECRET_KEY
# Derive the public key from the secret key:
stellar keys show <KEYPAIR_NAME>
```

**Fix:**

- **Update the oracle service keypair** to match the on-chain address (no
  on-chain transaction required, just update `.env` and restart), **or**
- **Rotate the on-chain oracle address** (requires escrow admin keypair):
  ```bash
  stellar contract invoke --id $CONTRACT_ESCROW \
    --source <ESCROW_ADMIN_KEYPAIR> \
    -- update_oracle \
    --new_oracle <CORRECT_ORACLE_ADDRESS>
  ```

After rotating, verify the change:

```bash
stellar contract invoke --id $CONTRACT_ESCROW -- get_oracle_address
```

---

## Replay commands reference

The oracle service exposes a `/api/dead_letter` REST API for inspecting and
replaying failed submissions. All endpoints require the service to be running.

| Command | Description |
|---|---|
| `GET /api/dead_letter` | List all DLQ entries with error details |
| `POST /api/dead_letter/replay` | Replay all DLQ entries immediately |
| `POST /api/dead_letter/replay` + `{"match_id": N}` | Replay a single entry |
| `DELETE /api/dead_letter/<match_id>` | Discard a DLQ entry permanently |

**Replay all:**

```bash
curl -X POST http://localhost:8080/api/dead_letter/replay
```

**Replay one match:**

```bash
curl -X POST http://localhost:8080/api/dead_letter/replay \
  -H "Content-Type: application/json" \
  -d '{"match_id": 42}'
```

**List DLQ entries:**

```bash
curl -s http://localhost:8080/api/dead_letter | jq '.'
```

**Force replay from the CLI** (alternative — stops and restarts the service
with the replay flag):

```bash
ORACLE_REPLAY_DEAD_LETTER=true ./oracle-service
```

See also the `scripts/smoke_test_oracle.sh` script for an automated end-to-end
smoke test that includes DLQ drain verification.

---

## Health check quick-reference

The oracle service exposes a `/health` endpoint that surfaces the state of all
critical dependencies in a single call.

```bash
curl -s http://localhost:8080/health | jq '.'
```

Typical healthy response:

```json
{
  "status": "healthy",
  "checks": {
    "stellar_rpc":        { "status": "ok",      "latency_ms": 45 },
    "escrow_contract":    { "status": "ok",      "latency_ms": 62 },
    "oracle_contract":    { "status": "ok",      "latency_ms": 58 },
    "lichess_api":        { "status": "ok",      "latency_ms": 120 },
    "chess_com_api":      { "status": "ok",      "latency_ms": 95 },
    "dead_letter":        { "status": "ok",      "depth": 0 }
  },
  "uptime_seconds": 86423
}
```

A `degraded` status means one or more non-critical checks are failing but
the service is still processing results. An `unhealthy` status means a
critical dependency (RPC or escrow contract) is unreachable.

For full health check documentation, see
[docs/monitoring-health-checks.md](monitoring-health-checks.md).
