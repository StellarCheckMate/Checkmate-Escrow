# Oracle Service Monitoring & Health Checks

This document describes the monitoring architecture, health check system, and alerting strategy for the oracle service.

---

## Table of Contents

- [Health Check Architecture](#health-check-architecture)
- [Health Check Response Schema](#health-check-response-schema)
- [Dependency Checks](#dependency-checks)
- [Integration Points](#integration-points)
- [Alerting & On-Call](#alerting--on-call)
- [Runbook: Responding to Health Alerts](#runbook-responding-to-health-alerts)
- [Synthetic Canary Checks](#synthetic-canary-checks)
- [Metrics & Observability](#metrics--observability)

---

## Health Check Architecture

The oracle service runs three concurrent subsystems:

### 1. HTTP Health Server (Port 8000)

Exposes two endpoints:

- **GET `/health`** — returns comprehensive health status with per-dependency checks
- **GET `/metrics`** — Prometheus-compatible metrics (planned)

### 2. Health Check Poller (Every 30 seconds)

A background task that:
- Probes Stellar RPC connectivity
- Verifies escrow and oracle contract reachability
- Tests chess platform API availability (Lichess, Chess.com)
- Stores results with timestamps and latency data
- Distinguishes transient failures (rate limits, timeouts) from persistent outages

### 3. Pipeline Poller (Every `ORACLE_POLL_INTERVAL_SECS`)

The existing match verification poller. Not affected by health check state, but health status informs operational decisions (e.g., pause service if unhealthy).

---

## Health Check Response Schema

All responses are JSON. The health endpoint returns:

```json
{
  "status": "healthy|degraded|unhealthy",
  "service_ready": true,
  "network": "testnet",
  "contract_escrow": "CCJMDYMB4O3...",
  "contract_oracle": "CCDAKAQSZ6Y...",
  "oracle_address": "GDZST3XVCDTUJ...",
  
  "checks": {
    "stellar_rpc": {
      "status": "up|degraded|down|rate_limited|unknown",
      "latency_ms": 52,
      "last_checked_at": "2024-06-15T14:32:10Z",
      "consecutive_failures": 0,
      "details": null
    },
    "escrow_contract": {
      "status": "up",
      "latency_ms": 120,
      "last_checked_at": "2024-06-15T14:32:10Z",
      "consecutive_failures": 0,
      "details": null
    },
    "oracle_contract": {
      "status": "up",
      "latency_ms": 98,
      "last_checked_at": "2024-06-15T14:32:10Z",
      "consecutive_failures": 0,
      "details": null
    },
    "lichess_api": {
      "status": "up",
      "latency_ms": 75,
      "last_checked_at": "2024-06-15T14:32:10Z",
      "consecutive_failures": 0,
      "details": null
    },
    "chess_com_api": {
      "status": "up",
      "latency_ms": 85,
      "last_checked_at": "2024-06-15T14:32:10Z",
      "consecutive_failures": 0,
      "details": null
    }
  },

  "canary_status": "pending|passed|failed",
  "canary_details": null,

  "last_full_check_at": "2024-06-15T14:32:10Z",
  "uptime_seconds": 3600
}
```

### Status Meanings

#### Overall `status` field

- **`healthy`**: All critical dependencies (RPC, escrow contract, oracle contract) are up.
  Service is ready to process matches and submit results.
- **`degraded`**: Critical dependencies are up, but one or more non-critical services
  (Lichess or Chess.com) are down or rate-limited. Matches for that platform are
  blocked; other platforms continue normally.
- **`unhealthy`**: One or more critical dependencies (RPC, escrow contract, or oracle
  contract) is down or unreachable. Service cannot submit results and should pause.

#### Per-dependency `status` field

- **`up`**: Probe succeeded, dependency is operational.
- **`degraded`**: Probe succeeded but observed higher-than-normal latency or warnings.
- **`down`**: Probe failed; dependency unreachable or unresponsive.
- **`rate_limited`**: API returned HTTP 429 or equivalent. Will retry after backoff.
- **`unknown`**: Not yet checked (startup only).

#### `service_ready` flag

Set to `true` only when:
- All critical checks (`stellar_rpc`, `escrow_contract`, `oracle_contract`) are `up`
- None of them are `unknown` (service has completed at least one full health check)

#### `canary_status` field

(Planned) Tracks the result of the synthetic end-to-end canary check:
- **`pending`**: Waiting for the next canary run.
- **`passed`**: Canary successfully executed a fetch→verify→submit pipeline.
- **`failed`**: Canary detected an issue (usually indicates the same problem
  would occur for real matches).

---

## Dependency Checks

### Stellar RPC

**Probe:** Call `getNetwork` RPC method.

**Success criteria:** HTTP 200, valid JSON response.

**Failure modes:**
- **Connection refused** → `down` (firewall, RPC offline)
- **HTTP 5xx** → `down` (server error)
- **Timeout (5s)** → `down` (slow/unresponsive)
- **Invalid JSON** → `down` (malformed response)

**Impact:** If down, oracle cannot submit results. Critical.

### Escrow Contract

**Probe:** Call `getLedgerEntries` with escrow contract address.

**Success criteria:** HTTP 200, contract data returned or "not found" (but no RPC error).

**Failure modes:**
- **RPC error** → `down` (cascades from RPC check)
- **Contract address invalid (invalid strkey)** → `down` (configuration error)
- **Contract paused** → `degraded` (detected via method call failure, if implemented)

**Impact:** If down, oracle cannot submit results. Critical.

### Oracle Contract

**Probe:** Call `getLedgerEntries` with oracle contract address.

**Success criteria:** HTTP 200, contract data returned.

**Failure modes:**
- Same as escrow contract.

**Impact:** If down, oracle cannot record results for audit. Critical.

### Lichess API

**Probe:** Call `GET /api/account` (public endpoint).

**Success criteria:** HTTP 200, valid JSON response.

**Failure modes:**
- **HTTP 429** → `rate_limited` (respect backoff header)
- **HTTP 5xx** → `down` (Lichess is down)
- **Timeout (5s)** → `down` (API overloaded)
- **Connection refused** → `down` (firewall / DNS)

**Impact:** If down, Lichess match verification blocks. Degraded service. Non-critical.

**Note:** This probe does *not* require authentication and does not consume rate-limit quota
(unlike game lookups). It is safe to run every 30 seconds without risk of being rate-limited.

### Chess.com API

**Probe:** Call `GET /pub/player/profiles` (public endpoint).

**Success criteria:** HTTP 200, valid JSON response.

**Failure modes:** Same as Lichess API.

**Impact:** Same as Lichess API.

---

## Integration Points

### Kubernetes Liveness Probe

Configure your orchestration platform to periodically poll the `/health` endpoint:

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8000
  initialDelaySeconds: 10
  periodSeconds: 30
  timeoutSeconds: 5
  failureThreshold: 3
```

**Interpretation:**
- If `status == "unhealthy"` for 3 consecutive checks (90 seconds), kill the pod.
- If `status == "degraded"`, keep the pod alive but alert on-call.

### Prometheus Metrics Scraping

(Planned) Export Prometheus metrics via `/metrics`:

```prometheus
oracle_health_check_latency_ms{dependency="stellar_rpc"} 52
oracle_health_check_status{dependency="stellar_rpc"} 1  # 1=up, 0=down
oracle_health_overall_status 1                          # 1=healthy, 0.5=degraded, 0=unhealthy
oracle_health_service_ready 1
oracle_health_consecutive_failures{dependency="stellar_rpc"} 0
```

### Alert Rules (Prometheus AlertManager)

```yaml
groups:
  - name: oracle_health
    interval: 30s
    rules:
      - alert: OracleServiceUnhealthy
        expr: oracle_health_overall_status == 0
        for: 5m
        annotations:
          summary: "Oracle service unhealthy (critical dependency down)"
          runbook_url: "https://wiki.company.com/oracle-runbooks#critical-outage"

      - alert: OracleServiceDegraded
        expr: oracle_health_overall_status == 0.5
        for: 10m
        annotations:
          summary: "Oracle service degraded (chess API unavailable)"
          runbook_url: "https://wiki.company.com/oracle-runbooks#degraded-service"

      - alert: OracleLichessRateLimited
        expr: oracle_health_check_status{dependency="lichess_api"} == 0.25
        for: 5m
        annotations:
          summary: "Lichess API rate-limited for 5+ minutes"
          runbook_url: "https://wiki.company.com/oracle-runbooks#rate-limit-response"

      - alert: OracleHighLatency
        expr: oracle_health_check_latency_ms{dependency="stellar_rpc"} > 1000
        for: 10m
        annotations:
          summary: "Stellar RPC latency over 1 second for 10+ minutes"
```

---

## Alerting & On-Call

### Alert Severity Levels

| Severity | Condition | Response SLA | Action |
|---|---|---|---|
| **Critical** | `status == "unhealthy"` for 5+ minutes | Page on-call immediately | Investigate, pause service if needed (see runbook) |
| **Warning** | `status == "degraded"` for 15+ minutes | Alert but don't page | Monitor closely, investigate chess API issue |
| **Info** | `status == "degraded"` for < 15 minutes | Logged but not alerted | Natural fluctuation; monitor |
| **Informational** | High latency (>500ms) for single dependency | Logged to dashboard | Informational only unless sustained |

### On-Call Escalation

1. **Tier 1 (Automated):** Health status → Prometheus → AlertManager → Slack/PagerDuty
2. **Tier 2 (5 minutes):** If critical alert not acknowledged, escalate to on-call engineer
3. **Tier 3 (15 minutes):** If critical alert not resolved, escalate to service owner

---

## Runbook: Responding to Health Alerts

### Alert: OracleServiceUnhealthy

**Symptoms:**
- Health endpoint returns `status: "unhealthy"`
- Critical dependency is `down` (RPC, escrow contract, or oracle contract)

**Investigation (< 2 minutes):**

1. **Check the health endpoint:**
   ```bash
   curl https://oracle-service.prod/health | jq .
   ```

2. **Identify which dependency is down:**
   ```bash
   curl https://oracle-service.prod/health | jq '.checks[] | select(.status == "down")'
   ```

3. **Check Stellar network status:**
   - Visit https://stellar-horizon.io/status
   - Check if `soroban-mainnet.stellar.org` (or your network) is up

4. **If RPC is down:**
   - Check firewall rules (port 443 outbound to Stellar RPC domain)
   - Check DNS resolution: `nslookup soroban-mainnet.stellar.org`
   - Try manual curl: `curl -I https://soroban-mainnet.stellar.org`

5. **If contract is down:**
   - Verify contract address is correct (config mismatch?)
   - Check if contract was deleted or migrated on-chain
   - Query contract directly: `stellar contract info --id $CONTRACT_ID`

**Immediate action (< 5 minutes):**

If the issue is external (Stellar network down, firewall blocked), pause the oracle service
to prevent pending matches from aging unnecessarily:

```bash
# Pause escrow contract
stellar contract invoke --id $CONTRACT_ESCROW --source $ADMIN_KEY -- pause

# Pause oracle contract
stellar contract invoke --id $CONTRACT_ORACLE --source $ADMIN_KEY -- pause
```

See [runbook-pause.md](runbook-pause.md) for full pause procedure.

**Recovery (< 30 minutes):**

Once the dependency is restored:

1. Verify health endpoint returns `status: "healthy"` for 3+ consecutive checks
2. Unpause both contracts (see [runbook-pause.md](runbook-pause.md))
3. Run a synthetic end-to-end test (create a test match, wait for result)
4. Document the incident (root cause, timeline, fix)

### Alert: OracleServiceDegraded

**Symptoms:**
- Health endpoint returns `status: "degraded"`
- Lichess or Chess.com API is `down` or `rate_limited`
- Stellar RPC and contracts are `up`

**Investigation (< 5 minutes):**

1. Check which chess API is down:
   ```bash
   curl https://oracle-service.prod/health | jq '.checks | {lichess: .lichess_api, chess_com: .chess_com_api}'
   ```

2. Check the platform's public status page:
   - Lichess: https://lichess.org/status
   - Chess.com: https://www.chess.com (status in top right)

3. If rate-limited:
   - Check current rate-limit usage (logged in oracle service logs)
   - Verify no runaway processes are hammering the API

**Immediate action:**

Matches for that platform will be blocked automatically (oracle cannot verify results).
This is expected and safe. No pause needed.

**Recovery:**

1. Wait for the platform to recover (usually < 1 hour)
2. Once recovered, health check will automatically transition to `healthy`
3. Pending matches for that platform will auto-retry

### Alert: OracleLichessRateLimited

**Symptoms:**
- Lichess API health check returns `status: "rate_limited"`
- Alert has been firing for 5+ minutes

**Investigation:**

1. Check rate-limit headers from Lichess:
   ```bash
   curl -I https://lichess.org/api/account
   # Look for: X-Rate-Limit-Limit, X-Rate-Limit-Remaining, Retry-After
   ```

2. Check if our match verification queue has backed up:
   ```bash
   ls -la /path/to/oracle-queue/pending
   wc -l /path/to/oracle-queue/pending/*
   ```

3. If thousands of pending matches, the queue may have naturally
   overrun the rate limit. This is expected under high load.

**Response:**

- If temporary (< 15 minutes): No action needed. Oracle will backoff and retry.
- If sustained: Contact Lichess support to request higher rate-limit tier.

---

## Synthetic Canary Checks

(Planned) Periodic end-to-end verification that the oracle can actually process matches.

### Canary Design

Every 5 minutes, the oracle will:

1. **Create a test match** on-chain with a known outcome (e.g., `player1` always wins)
2. **Fetch the result** from the chess platform (or use a cached test result)
3. **Submit the result** to the escrow contract
4. **Verify the result** was recorded on-chain
5. **Mark the match as completed** and check the payout was triggered

If any step fails, the canary fails and a warning is emitted.

### Canary Failure Response

If `canary_status == "failed"`:

1. **Do NOT automatically pause** — this may be a transient issue
2. **Alert on-call** with high priority (implies real matches would also fail)
3. **Investigate:** Are real match submissions failing too? Check the dead-letter queue
4. **If systemic:** Pause and investigate further (see runbook-pause.md)

---

## Metrics & Observability

### Health Check Metrics to Track

- **Latency per dependency** (histogram)
  - SLA: Stellar RPC < 100ms (p95), contracts < 200ms (p95)
  - SLA: Chess APIs < 500ms (p95)

- **Consecutive failures** (gauge)
  - Alert if > 1 for any dependency (indicates flakiness)

- **Time to first check** (after service start)
  - Alert if > 30 seconds (indicates initialization issue)

- **Time since last full health check** (gauge)
  - Alert if > 60 seconds (indicates health poller stuck)

- **Overall service readiness** (boolean gauge)
  - High-priority alert if false for > 5 minutes

### Logging

Health checks are logged at `debug` level by default:

```
DEBUG oracle_service::health: starting full health check
DEBUG oracle_service::health: health check completed in 245ms
DEBUG oracle_service::health: Stellar RPC latency: 52ms
```

Failures are logged at `warn` level:

```
WARN oracle_service::health: Stellar RPC health check failed: connection refused
WARN oracle_service::health: Lichess API health check timed out
```

Enable verbose logging in production with:

```bash
RUST_LOG=oracle_service::health=debug ./oracle-service
```

### Dashboard Recommendations

Create a Grafana dashboard with:

1. **Status heatmap** (per dependency, per hour)
   - Red = down, yellow = degraded, green = up

2. **Latency time series** (per dependency)
   - Line graph with SLA thresholds marked

3. **Uptime counter** (total seconds since last restart)

4. **Canary status** (pass/fail, time since last pass)

5. **Alert status** (active alerts, firing duration)

6. **Match processing rate** (from pipeline poller)
   - Correlation: matches processed should drop when health is degraded

---

## Testing the Health Check System

### Manual Connectivity Test

```bash
# Test RPC connectivity
curl -X POST https://soroban-mainnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getNetwork","params":{}}'

# Test contract reachability
stellar contract invoke --id $CONTRACT_ESCROW -- get_admin

# Test Lichess API
curl -I https://lichess.org/api/account

# Test Chess.com API
curl -I https://api.chess.com/pub/player/profiles
```

### Simulating Failures

**Block Stellar RPC (firewall rule):**
```bash
sudo iptables -A OUTPUT -d soroban-mainnet.stellar.org -j DROP
# Run health check — should show RPC down
# Restore:
sudo iptables -D OUTPUT -d soroban-mainnet.stellar.org -j DROP
```

**Block Chess API (local hosts file):**
```bash
echo "127.0.0.1 api.chess.com" >> /etc/hosts
# Run health check — should show Chess.com down
# Restore: remove the line from /etc/hosts
```

**Simulate rate limiting (mock server):**
```bash
# Start a mock server that returns 429
# Point LICHESS_API_BASE to it
# Run health check — should show rate_limited
```

---

## Future Enhancements

1. **Metrics export** (`/metrics` endpoint with Prometheus format)
2. **Synthetic canary** (periodic end-to-end test match)
3. **Configurable check intervals** (per-dependency)
4. **Health check history** (store last N checks in-memory)
5. **Custom webhook notifications** (POST to on-call system)
6. **Contract method health checks** (call contract methods instead of just reading storage)
