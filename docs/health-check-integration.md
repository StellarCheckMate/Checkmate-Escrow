# Health Check Integration Guide

Quick reference for integrating the oracle health check system with your infrastructure.

---

## Quick Start

### 1. Enable Health Checks (Already Built-In)

The oracle service automatically runs health checks every 30 seconds. No configuration required.

### 2. Poll the Health Endpoint

```bash
curl http://oracle-service:8000/health
```

Returns JSON with overall status and per-dependency checks (see `docs/monitoring-health-checks.md`).

### 3. Configure Kubernetes Liveness Probe

Update your deployment manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: oracle-service
spec:
  template:
    spec:
      containers:
      - name: oracle-service
        image: oracle-service:latest
        ports:
        - containerPort: 8000
          name: http
        
        # Liveness probe: kill pod if unhealthy for 90 seconds
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 5
          failureThreshold: 3
        
        # Readiness probe: remove from load balancer if degraded
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 2
```

### 4. Interpret Probe Results

Kubernetes will interpret HTTP status codes from `/health`:

- **HTTP 200**: Healthy or degraded (service is running)
- **HTTP 503**: Unhealthy (critical dependency down)

For fine-grained control, parse the JSON response:

```bash
#!/bin/bash
RESPONSE=$(curl -s http://oracle-service:8000/health)
STATUS=$(echo $RESPONSE | jq -r '.status')

case $STATUS in
  "healthy")
    echo "All systems operational"
    exit 0
    ;;
  "degraded")
    echo "Warning: Non-critical service down (chess API)"
    # Keep serving but alert ops
    exit 0
    ;;
  "unhealthy")
    echo "Critical: Service cannot operate"
    # Fail the probe
    exit 1
    ;;
esac
```

### 5. Set Up Alerting

#### Prometheus + AlertManager

```yaml
# prometheus.yml
global:
  scrape_interval: 30s

scrape_configs:
  - job_name: 'oracle-health'
    static_configs:
      - targets: ['oracle-service:8000']
    metrics_path: '/metrics'
```

Add alert rules (see `docs/monitoring-health-checks.md` for examples):

```yaml
# alert-rules.yml
groups:
  - name: oracle_health
    rules:
      - alert: OracleServiceUnhealthy
        expr: increase(oracle_unhealthy_total[5m]) > 0
        for: 5m
        annotations:
          summary: "Oracle service is unhealthy"
          runbook: "https://wiki.company.com/oracle-runbooks#critical-outage"
```

#### Manual Polling (Simple Bash Script)

```bash
#!/bin/bash
# check_oracle_health.sh

HEALTH_URL="http://oracle-service:8000/health"
TIMEOUT=5

# Fetch health status
RESPONSE=$(curl -s -m $TIMEOUT "$HEALTH_URL" 2>/dev/null)
STATUS=$(echo "$RESPONSE" | jq -r '.status' 2>/dev/null)

if [ -z "$STATUS" ]; then
  echo "CRITICAL: Could not reach oracle health endpoint"
  exit 2
fi

case "$STATUS" in
  "healthy")
    echo "OK: Oracle service healthy"
    exit 0
    ;;
  "degraded")
    echo "WARNING: Oracle service degraded (chess API down)"
    exit 1
    ;;
  "unhealthy")
    echo "CRITICAL: Oracle service unhealthy (critical dependency down)"
    exit 2
    ;;
  *)
    echo "UNKNOWN: Unexpected status: $STATUS"
    exit 3
    ;;
esac
```

Deploy to Nagios/Icinga:

```
# icinga2.conf
object Service "oracle-health" {
  host_name = "oracle-service"
  check_command = "check_http"
  vars.http_address = "oracle-service"
  vars.http_port = "8000"
  vars.http_uri = "/health"
  vars.http_ssl = "false"
}
```

---

## Integration Checklist

- [ ] **Kubernetes deployment** — added liveness + readiness probes
- [ ] **Prometheus scraping** — `/metrics` endpoint configured (when available)
- [ ] **Alert rules** — AlertManager rules created and tested
- [ ] **On-call notification** — PagerDuty / Slack integration configured
- [ ] **Runbook link** — team wiki has link to `docs/monitoring-health-checks.md`
- [ ] **Grafana dashboard** — created dashboard with health metrics
- [ ] **Synthetic tests** — canary match created for end-to-end verification
- [ ] **Log aggregation** — oracle service logs sent to central logging (ELK, Datadog, etc.)
- [ ] **Baseline metrics** — recorded baseline latencies and error rates under normal load

---

## Response Times & SLA

Expected response times for each health check:

| Dependency | P50 (median) | P95 | P99 | SLA Threshold |
|---|---|---|---|---|
| Stellar RPC | 30ms | 80ms | 150ms | < 100ms (p95) |
| Escrow Contract | 80ms | 180ms | 300ms | < 200ms (p95) |
| Oracle Contract | 75ms | 170ms | 280ms | < 200ms (p95) |
| Lichess API | 40ms | 200ms | 500ms | < 500ms (p95) |
| Chess.com API | 50ms | 150ms | 400ms | < 500ms (p95) |

**Alert if:**
- Any dependency latency > SLA threshold for > 10 minutes
- Any dependency unreachable for > 1 minute (critical) or > 5 minutes (non-critical)

---

## Troubleshooting

### Health Endpoint Unresponsive

```bash
# Check if service is running
docker logs oracle-service | tail -50

# Try accessing endpoint directly
curl -v http://oracle-service:8000/health

# Check firewall rules
netstat -tln | grep 8000
```

### All Dependencies Showing "Unknown"

**Cause:** Service just started, health checks haven't run yet.

**Fix:** Wait 30-60 seconds for the health poller to run. Status will update automatically.

### Stellar RPC Showing "Down" But CLI Works

**Cause:** Network connectivity or DNS resolution issue specific to the oracle service.

**Debug:**
```bash
# From oracle pod
kubectl exec -it oracle-service-pod -- bash
curl -I https://soroban-mainnet.stellar.org
nslookup soroban-mainnet.stellar.org
```

### Chess API Rate-Limited During Canary

**Cause:** Legitimate high-load scenario or production API quota exhausted.

**Response:**
1. Check active match volume: `ls /oracle-queue/pending | wc -l`
2. If high volume is expected, request Chess.com API quota increase
3. If not, investigate for runaway batch processes

---

## Metrics Export (Planned)

When metrics export is implemented, the `/metrics` endpoint will provide:

```prometheus
# Service-level
oracle_health_status{instance="oracle-service"} 1.0  # 0=unhealthy, 0.5=degraded, 1=healthy
oracle_service_ready 1  # 1=ready, 0=not ready
oracle_uptime_seconds 3600

# Per-dependency
oracle_health_latency_ms{dependency="stellar_rpc"} 52
oracle_health_status{dependency="stellar_rpc"} 1  # 1=up, 0.25=rate_limited, 0=down
oracle_health_consecutive_failures{dependency="stellar_rpc"} 0

# Last check timestamp
oracle_health_last_check_timestamp{dependency="stellar_rpc"} 1718457130

# Canary
oracle_canary_status 1  # 1=passed, 0.5=pending, 0=failed
oracle_canary_last_passed_timestamp 1718457100
```

Import these into Grafana using the standard Prometheus data source.

---

## Next Steps

1. **Read** [docs/monitoring-health-checks.md](monitoring-health-checks.md) for architecture details
2. **Review** [docs/runbook-pause.md](runbook-pause.md) for incident response procedures
3. **Test** health checks in staging environment
4. **Deploy** alert rules to production AlertManager
5. **Configure** Kubernetes probes in your deployment
6. **Document** your team's on-call escalation path
