# Event Indexer Service — Horizontally Scalable

A high-performance, horizontally-scalable Soroban event indexing service for the Checkmate Escrow contract. Supports **multi-instance clusters** with PostgreSQL backend, distributed leader election, read-replica support, and deterministic LRU caching.

---

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+
- **PostgreSQL 12+** (required; no longer using SQLite)
- Soroban testnet access
- Docker & Docker Compose (optional, for multi-instance local testing)

### Local Single-Instance Setup

```bash
cd services/event-indexer

# 1. Start a local PostgreSQL instance
docker run --name event-pg -e POSTGRES_USER=event_indexer \
  -e POSTGRES_PASSWORD=pass -e POSTGRES_DB=event_indexer \
  -p 5432:5432 -d postgres:15-alpine

# 2. Configure environment
export DATABASE_URL=postgres://event_indexer:pass@localhost:5432/event_indexer
export CONTRACT_ESCROW=<your-56-char-stellar-contract-address>
export STELLAR_RPC_URL=https://soroban-testnet.stellar.org

# 3. Build and run
cargo build --release
cargo run --release
```

The service starts on `http://localhost:8080`.  Check health:

```bash
curl http://localhost:8080/health
# {"db":"ok"}
```

### Multi-Instance Cluster (Docker Compose)

```bash
# From the repository root:
docker-compose up

# This starts:
#   - 1 PostgreSQL instance
#   - 2 event-indexer replicas (ports 8080, 8081)
#   - Both replicas compete for leader lease; one becomes leader and ingests events

# Scale to 4 replicas:
docker-compose up --scale event-indexer-1=2 --scale event-indexer-2=2

# Check which instance is the leader:
docker-compose logs event-indexer-1 event-indexer-2 | grep "Became leader"
```

---

## ✨ Features

### Core

- **PostgreSQL backend** — connection-pooled, ACID, ready for replication
- **Leader election** — distributed mutex via PG advisory locks + row-level lease; only one instance ingests at a time
- **Idempotent ingestion** — `INSERT … ON CONFLICT DO NOTHING` semantics prevent duplicate events on replay or failover
- **Horizontal read scaling** — separate read-replica pool for API queries; add instances to scale read throughput
- **Deterministic LRU cache** — strict insertion-order eviction using `indexmap`; no undefined HashMap iteration

### API

- `/health` — DB connectivity check
- `/events?player_address=X&status=completed&limit=100` — query events with filters
- `/events/:match_id` — all events for a match (cache-first)
- `/match/:match_id` — full match summary
- `/matches?status=active` — list matches by status
- `/stats` — total events + cache size

See [EVENT_INDEXER_API.md](../../docs/EVENT_INDEXER_API.md) for complete documentation.

---

## 📐 Architecture

### Data Flow

```
┌────────────────────────────────────────────────┐
│  Leader-elected Instance (writes events)       │
│  ┌──────────────────────────────────────────┐  │
│  │  Soroban RPC poller (every 5s)          │  │
│  └───────────┬──────────────────────────────┘  │
│              │                                  │
│   ┌──────────▼─────────┐     ┌──────────────┐ │
│   │ LRU Cache (10k)    │ ◄── │ DB Write Pool│ │
│   └────────────────────┘     └──────┬───────┘ │
└──────────────────────────────────────┼─────────┘
                                       │
                            ┌──────────▼─────────┐
                            │  PostgreSQL Primary│
                            └──────────┬─────────┘
                                       │ streaming replication
                                       │
                            ┌──────────▼─────────┐
                            │  PG Read Replica   │
                            │    (optional)      │
                            └────────────────────┘
                                       ▲
┌──────────────────────────────────────┼─────────┐
│  All instances (serve read traffic)  │         │
│  ┌──────────────────────────────────┐│         │
│  │  REST API (Axum)                 ││         │
│  └───────────┬──────────────────────┘│         │
│              │                        │         │
│   ┌──────────▼─────────┐     ┌───────┴──────┐  │
│   │ LRU Cache (10k)    │ ◄── │DB Read Pool  │  │
│   └────────────────────┘     └──────────────┘  │
└──────────────────────────────────────────────────┘
```

### Leader Election

- Uses **PostgreSQL session-level advisory lock** (`pg_try_advisory_lock`) + **row-level lease** in `leader_state` table.
- Lease TTL = 30s; heartbeat every 10s.
- If leader dies, advisory lock auto-released; lease expires → follower takes over within ≤ 30s.
- No split-brain possible: only one session can hold the advisory lock at a time.

### Idempotency

All ingestion uses:

```sql
INSERT INTO events (id, …) VALUES ($1, …)
ON CONFLICT (id) DO NOTHING
```

Event `id` is a UUID derived from ledger + event data.  Re-ingesting the same ledger range (e.g., after a crash) never creates duplicates.

### Cache

- Each instance has an **independent** LRU cache (no distributed cache).
- `indexmap::IndexMap` maintains strict LRU order: front = least-recently-used (evicted first); back = most-recently-used.
- Cache misses go to the DB read pool.

---

## ⚙️ Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | **required** | Primary PostgreSQL DSN<br/>e.g. `postgres://user:pass@host:5432/dbname` |
| `DATABASE_READ_URL` | `$DATABASE_URL` | Read-replica DSN (optional; falls back to primary) |
| `STELLAR_RPC_URL` | testnet | Soroban JSON-RPC endpoint |
| `CONTRACT_ESCROW` | **required** | Escrow contract address (56 chars) |
| `EVENT_INDEXER_INSTANCE_ID` | hostname / UUID | Unique ID for this instance |
| `EVENT_INDEXER_BIND_ADDR` | `127.0.0.1` | API listen address |
| `EVENT_INDEXER_PORT` | `8080` | API port |
| `EVENT_INDEXER_CACHE_SIZE` | `10000` | LRU cache capacity |
| `EVENT_INDEXER_POLL_INTERVAL` | `5` | Polling interval (1–60 seconds) |
| `EVENT_INDEXER_LOG_LEVEL` | `info` | Log level (`error`, `warn`, `info`, `debug`, `trace`) |
| `EVENT_INDEXER_LEADER_TTL_SECS` | `30` | Leader lease validity (seconds) |
| `EVENT_INDEXER_LEADER_HEARTBEAT_SECS` | `10` | Lease renewal interval (must be < TTL) |
| `EVENT_INDEXER_DB_POOL_SIZE` | `5` | Write pool connections |
| `EVENT_INDEXER_DB_READ_POOL_SIZE` | `10` | Read pool connections |

---

## 🧪 Testing

```bash
# Unit tests (cache, config, no DB required)
cargo test

# Integration tests (requires DATABASE_URL)
DATABASE_URL=postgres://user:pass@localhost:5432/test_db cargo test

# Load tests (throughput measurement, no DB required)
cargo test --release -- --nocapture load_test
```

### Test Coverage

- ✅ LRU eviction determinism
- ✅ Multi-instance no-duplicate ingestion simulation
- ✅ Leader-only ingestion correctness
- ✅ Leader failover with overlapping replay
- ✅ Concurrent cache insert/read throughput (≥ 5 000 ops/s target)
- ✅ N-instance read-scaling benchmark

---

## 📊 Performance

### Single-instance baseline

- **Query latency**: < 50ms (< 10ms from cache)
- **Ingestion throughput**: 100+ events/second (RPC-bound)
- **Cache hit rate**: ~80% for typical workloads
- **Throughput**: 1000+ queries/second per instance

### Multi-instance (3 replicas, 1 leader)

- **Read scaling**: ~3× query throughput (queries spread across all instances)
- **Failover**: ≤ 30s worst-case to elect new leader
- **Zero duplicate events** during failover (idempotent `ON CONFLICT DO NOTHING`)

---

## 🔧 Operational Tasks

### Check which instance is the leader

```bash
psql $DATABASE_URL -c "SELECT instance_id, held_until FROM leader_state"
```

### Force a leader re-election

```bash
psql $DATABASE_URL -c "DELETE FROM leader_state"
# Next poll cycle → a new leader is elected
```

### Add a replica

```bash
docker-compose up --scale event-indexer-1=2
# New instance auto-starts, competes for leader lease, serves read traffic immediately
```

### Remove a replica (graceful)

```bash
docker-compose stop event-indexer-2
# If it was leader, lease expires within 30s → another instance takes over
```

### Scale read capacity (add read replicas)

1. Set up PostgreSQL streaming replication (standard PG procedure).
2. Set `DATABASE_READ_URL=postgres://…replica…` on all indexer instances.
3. Restart indexer instances.  All query traffic now routes to the replica.

### Debugging

```bash
# Logs from all replicas
docker-compose logs -f event-indexer-1 event-indexer-2

# Structured JSON logs
EVENT_INDEXER_LOG_LEVEL=debug cargo run 2>&1 | jq
```

---

## 📚 Documentation

- [Scaling Architecture](../../docs/event-indexer-scaling.md) — complete architectural guide, failure modes, runbook
- [API Reference](../../docs/EVENT_INDEXER_API.md) — endpoint specs, request/response schemas
- [Deployment Guide](../../docs/deployment.md) — production deployment checklist

---

## 🚧 Migration from v0.1 (SQLite)

If you have an existing SQLite-based deployment:

1. **Export events from SQLite** → CSV.
2. **Provision PostgreSQL** and run schema init.
3. **Import CSV** into PG `events` table.
4. **Update environment variables** (`DATABASE_URL`, remove `EVENT_INDEXER_DB_PATH`).
5. **Restart the service**.

The new version is backward-compatible at the API level; no client changes are needed.

---

## 🤝 Contributing

1. Create feature branch
2. Add tests for new functionality
3. Ensure `cargo test` and `cargo fmt` pass
4. Submit PR with description

---

## 📄 License

See LICENSE file in repository root.
