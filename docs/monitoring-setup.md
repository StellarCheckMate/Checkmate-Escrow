# Monitoring Setup Guide

This guide explains how to set up the Checkmate-Escrow monitoring stack: Prometheus metrics export, Grafana dashboards, and alerting rules.

---

## Table of Contents

- [Overview](#overview)
- [Metrics Exported from Contract Events](#metrics-exported-from-contract-events)
- [Prometheus Setup](#prometheus-setup)
- [Grafana Dashboard Setup](#grafana-dashboard-setup)
- [Alerting Rules](#alerting-rules)
- [Docker Compose (Local Dev)](#docker-compose-local-dev)
- [Production Deployment](#production-deployment)
- [Troubleshooting](#troubleshooting)

---

## Overview

The Checkmate-Escrow monitoring stack consists of three layers:

```
Oracle Service / Event Indexer
        │
        │  exposes /metrics (Prometheus format)
        ▼
   Prometheus ──────────── evaluates alerting rules
        │                         │
        │  scrapes                │  fires alerts
        ▼                         ▼
    Grafana                  Alertmanager
  (dashboards)             (PagerDuty / Slack / email)
```

Metrics are derived from two sources:

1. **Oracle service** (`oracle-service/src/health.rs`) — exposes match processing counters, oracle API latency, and Stellar RPC health on `GET /metrics` (port 8000).
2. **Event indexer** (`services/event-indexer`) — exposes ingestion counters and API request metrics on `GET /metrics` (port 8080).

---

## Metrics Exported from Contract Events

The oracle service and event indexer export the following Prometheus metrics. Add these to your scrape targets as described in [Prometheus Setup](#prometheus-setup).

### Contract State

| Metric | Type | Description |
|--------|------|-------------|
| `checkmate_contract_paused` | Gauge | `1` when the escrow contract is paused, `0` otherwise |
| `checkmate_matches_total` | Counter | Total matches created since service start |
| `checkmate_matches_pending` | Gauge | Current count of matches in `Pending` state |
| `checkmate_matches_active` | Gauge | Current count of matches in `Active` state |
| `checkmate_matches_created_total` | Counter | Cumulative matches created |
| `checkmate_matches_completed_total` | Counter | Cumulative matches completed (winner paid out) |
| `checkmate_matches_cancelled_total` | Counter | Cumulative matches cancelled or expired |

### Fund Safety

| Metric | Type | Description |
|--------|------|-------------|
| `checkmate_tvl_stroops` | Gauge | Total Value Locked in escrow (stroops; divide by 1e7 for XLM) |
| `checkmate_payouts_total_stroops` | Counter | Cumulative winner payouts (stroops) |
| `checkmate_refunds_total_stroops` | Counter | Cumulative draw/cancel refunds (stroops) |
| `checkmate_largest_active_stake_stroops` | Gauge | Largest single active match stake (stroops) |

### Operations & Errors

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `checkmate_operations_total` | Counter | `operation` | Total contract operations attempted |
| `checkmate_operations_failed_total` | Counter | `operation`, `error_code` | Failed contract operations |

Example `operation` label values: `create_match`, `deposit`, `submit_result`, `cancel_match`, `expire_match`.

### Oracle

| Metric | Type | Description |
|--------|------|-------------|
| `checkmate_oracle_submissions_total` | Counter | Total result submissions to the contract |
| `checkmate_oracle_submission_errors_total` | Counter | Failed result submissions |
| `checkmate_oracle_rotations_total` | Counter | Number of oracle address rotations |
| `checkmate_oracle_api_duration_seconds` | Histogram | Latency of Lichess/Chess.com API calls |
| `checkmate_stellar_rpc_health` | Gauge | `1` if Stellar RPC is reachable, `0` otherwise |

### Exporting Metrics from Contract Events

The event indexer watches Soroban contract events and increments the counters above whenever it ingests an event. The mapping is:

| Contract Event | Metric Updated |
|---------------|----------------|
| `match/created` | `checkmate_matches_created_total++`, `checkmate_matches_pending++` |
| `match/completed` | `checkmate_matches_completed_total++`, `checkmate_matches_active--` |
| `match/cancelled` | `checkmate_matches_cancelled_total++` |
| `match/expired` | `checkmate_matches_cancelled_total++` |
| `admin/paused` | `checkmate_contract_paused = 1` |
| `admin/unpaused` | `checkmate_contract_paused = 0` |
| `admin/oracle_up` | `checkmate_oracle_rotations_total++` |
| `escrow/deposit` | `checkmate_tvl_stroops += stake` |
| `escrow/payout` | `checkmate_tvl_stroops -= payout`, `checkmate_payouts_total_stroops += payout` |
| `escrow/refund` | `checkmate_tvl_stroops -= refund`, `checkmate_refunds_total_stroops += refund` |

> The event indexer derives these metrics from the event stream rather than direct contract queries, so they remain accurate even if the RPC node is temporarily unavailable — metrics are backfilled when the indexer catches up.

---

## Prometheus Setup

### 1. Add scrape targets

Merge the provided config into your `prometheus.yml`:

```yaml
# monitoring/prometheus/prometheus.yml
scrape_configs:
  - job_name: "checkmate-oracle"
    static_configs:
      - targets: ["oracle-service:8000"]
    metrics_path: /metrics
    scrape_interval: 15s

  - job_name: "checkmate-event-indexer"
    static_configs:
      - targets:
          - "event-indexer-1:8080"
          - "event-indexer-2:8080"
    metrics_path: /metrics
    scrape_interval: 15s
```

A ready-to-use config lives at [`monitoring/prometheus/prometheus.yml`](../monitoring/prometheus/prometheus.yml).

### 2. Add alerting rules

Copy the rules file to your Prometheus rules directory:

```bash
cp monitoring/prometheus/alerts.yml /etc/prometheus/rules/checkmate_alerts.yml
```

Add to `prometheus.yml`:

```yaml
rule_files:
  - "rules/checkmate_alerts.yml"
```

Then reload Prometheus:

```bash
curl -X POST http://localhost:9090/-/reload
```

---

## Grafana Dashboard Setup

### Import via UI

1. Open Grafana → **Dashboards** → **Import**.
2. Upload `monitoring/grafana/dashboards/contract-health.json`.
3. Select your Prometheus datasource when prompted.
4. Click **Import**.

### Auto-provision (recommended for production)

1. Copy the provisioning files into your Grafana config directory:

```bash
cp monitoring/grafana/provisioning/datasources/prometheus.yml \
   /etc/grafana/provisioning/datasources/

cp monitoring/grafana/provisioning/dashboards/checkmate.yml \
   /etc/grafana/provisioning/dashboards/

cp monitoring/grafana/dashboards/contract-health.json \
   /var/lib/grafana/dashboards/
```

2. Restart Grafana. The **Checkmate-Escrow — Contract Health** dashboard will appear under the **Checkmate** folder.

### Dashboard Panels

The dashboard ships with the following panels:

| Section | Panel | Description |
|---------|-------|-------------|
| Contract Status | Contract State | Red/green indicator for paused/active |
| Contract Status | Total Value Locked | Current TVL in XLM |
| Contract Status | Total Matches | Cumulative match count |
| Contract Status | Active Matches | Live gauge with colour thresholds |
| Contract Status | Error Rate (5m) | Percentage of failing operations |
| Match Volume | Match Creation Rate | Created / Completed / Cancelled per second |
| Match Volume | Match State Counts | Time series of state counts |
| Fund Safety & TVL | TVL Over Time | Historical XLM locked in escrow |
| Fund Safety & TVL | Payout Volume | XLM paid out and refunded per second |
| Error Rate | Failed Operations by Type | Bar chart, broken down by operation |
| Error Rate | Error Rate % | Time series with 5% threshold line |
| Oracle Health | Oracle Submissions | Submission rate and error rate |
| Oracle Health | Oracle API Latency | p50 / p95 latency from Lichess/Chess.com |

---

## Alerting Rules

All alerts are defined in [`monitoring/prometheus/alerts.yml`](../monitoring/prometheus/alerts.yml). The following alerts fire automatically:

### Critical Alerts

| Alert | Condition | Action |
|-------|-----------|--------|
| `HighContractErrorRate` | Error rate > 5% for 2 min | Investigate contract transactions; consider pausing |
| `ContractPaused` | `checkmate_contract_paused == 1` | Verify admin action; check for emergency |
| `ContractPausedExtended` | Paused > 30 min | Escalate; active matches are stalled |
| `TVLDroppedToZero` | TVL == 0 with active matches | Potential drain — investigate immediately |
| `OracleServiceDown` | Prometheus cannot scrape oracle | Restart oracle service; check infra |

### Warning Alerts

| Alert | Condition | Action |
|-------|-----------|--------|
| `ElevatedContractErrorRate` | Error rate > 2% for 5 min | Monitor; investigate if rising |
| `AbnormallyLargeStake` | Single stake > 10,000 XLM | Verify match is legitimate |
| `MatchCreationSpike` | > 10 matches/sec for 2 min | Check for automated abuse |
| `HighCancellationRate` | > 50% of matches cancelled | Check for client bugs or griefing |
| `OracleAddressRotated` | Oracle address changed | Confirm expected rotation |
| `OracleSubmissionErrorSpike` | Submission errors > 0.1/sec | Check oracle + Stellar RPC |
| `LargeTVLDrop` | TVL drops > 50% in 5 min | Verify large batch of completions |
| `StellarRPCDegraded` | RPC health != 1 for 3 min | Check RPC node; may delay payouts |

### Connecting Alertmanager

Configure Alertmanager to route `severity: critical` alerts to your on-call channel and `severity: warning` alerts to Slack or email. Example Alertmanager routing:

```yaml
route:
  group_by: ['alertname', 'component']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 1h
  receiver: default

  routes:
    - match:
        severity: critical
      receiver: pagerduty-critical
    - match:
        severity: warning
      receiver: slack-warnings

receivers:
  - name: pagerduty-critical
    pagerduty_configs:
      - service_key: "<YOUR_PAGERDUTY_KEY>"

  - name: slack-warnings
    slack_configs:
      - api_url: "<YOUR_SLACK_WEBHOOK>"
        channel: "#checkmate-alerts"
```

---

## Docker Compose (Local Dev)

A full local monitoring stack (Prometheus + Grafana + Alertmanager) can be started alongside the existing services by adding the following to your `docker-compose.yml` or a separate override file:

```yaml
# docker-compose.monitoring.yml
services:
  prometheus:
    image: prom/prometheus:v2.47.0
    restart: unless-stopped
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - ./monitoring/prometheus/alerts.yml:/etc/prometheus/rules/checkmate_alerts.yml:ro
      - prometheus-data:/prometheus
    command:
      - "--config.file=/etc/prometheus/prometheus.yml"
      - "--storage.tsdb.path=/prometheus"
      - "--web.enable-lifecycle"

  grafana:
    image: grafana/grafana:10.2.0
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - ./monitoring/grafana/provisioning:/etc/grafana/provisioning:ro
      - ./monitoring/grafana/dashboards:/var/lib/grafana/dashboards:ro
      - grafana-data:/var/lib/grafana
    environment:
      GF_SECURITY_ADMIN_PASSWORD: ${GRAFANA_ADMIN_PASSWORD:-changeme}
      GF_USERS_ALLOW_SIGN_UP: "false"

  alertmanager:
    image: prom/alertmanager:v0.26.0
    restart: unless-stopped
    ports:
      - "9093:9093"
    volumes:
      - ./monitoring/alertmanager/alertmanager.yml:/etc/alertmanager/alertmanager.yml:ro

volumes:
  prometheus-data:
  grafana-data:
```

Start monitoring alongside the main stack:

```bash
docker compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

Open Grafana at http://localhost:3000 (default credentials: `admin` / `changeme`).

---

## Production Deployment

For production, consider these additional steps:

1. **Persistent storage** — mount named volumes for Prometheus and Grafana data with regular backups.
2. **TLS** — put Grafana behind a reverse proxy (nginx / Caddy) with HTTPS.
3. **Auth** — enable Grafana OAuth (GitHub, Google) rather than admin/password.
4. **Retention** — set `--storage.tsdb.retention.time=30d` for 30-day metric history.
5. **High availability** — use [Thanos](https://thanos.io) or [VictoriaMetrics](https://victoriametrics.com) in front of Prometheus for HA and long-term storage.
6. **Secrets** — store Alertmanager webhook URLs and PagerDuty keys in a secrets manager (e.g. AWS Secrets Manager, Vault) rather than in config files.

---

## Troubleshooting

### Metrics not appearing in Prometheus

- Confirm the oracle service is running and the `/metrics` endpoint responds:
  ```bash
  curl http://localhost:8000/metrics | grep checkmate_
  ```
- Check Prometheus targets at http://localhost:9090/targets — look for `UP` status on `checkmate-oracle`.
- Verify network connectivity between Prometheus and the oracle service (container name resolution in Docker).

### Dashboard shows "No data"

- Confirm the correct Prometheus datasource is selected in the dashboard variables.
- Check that the time range includes periods when the oracle was running.
- Use Explore mode in Grafana to run `checkmate_matches_total` directly to verify data exists.

### Alerts not firing

- Visit http://localhost:9090/alerts to see rule evaluation state.
- Use `promtool check rules monitoring/prometheus/alerts.yml` to validate rule syntax.
- Ensure the rule file is listed under `rule_files:` in `prometheus.yml` and Prometheus was reloaded.

### High cardinality warnings

- The `operation` and `error_code` label combinations should stay under ~20 unique values total. If you add custom operations, keep labels bounded to avoid cardinality explosion.
