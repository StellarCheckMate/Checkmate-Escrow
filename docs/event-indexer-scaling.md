# Event-Indexer Horizontal Scalability

> Architecture guide, failure-mode analysis, and operational runbook.

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [PostgreSQL Backend](#postgresql-backend)
4. [Leader Election](#leader-election)
5. [LRU Cache](#lru-cache)
6. [Read Scaling](#read-scaling)
7. [Failure Modes](#failure-modes)
8. [Operational Runbook](#operational-runbook)
9. [Configuration Reference](#configuration-reference)
10. [Metrics & Observability](#metrics--observability)

---

## Overview

The event-indexer has been redesigned from a single-process, SQLite-backed
service to a **horizontally-scalable cluster**:

| Concern | Before | After |
|---|---|---|
| Storage | SQLite file (single writer) | PostgreSQL connection pool (multi-instance) |
| Ingestion | Unguarded – multiple instances corrupt data | Leader-elected single writer |
| Cache eviction | HashMap iteration order (undefined) | Strict LRU via `IndexMap` |
| Read scaling | One pool, one process | Separate read-replica pool per instance |
| Idempotency | `INSERT OR REPLACE` (overwrites on replay) | `INSERT … ON CONFLICT DO NOTHING` |

---

## Architecture

```
                    ┌─────────────────────────────────────┐
                    │           Load Balancer              │
                    │         (API traffic only)           │
                    └───────┬─────────────┬───────────────┘
                            │             │
               ┌────────────▼───┐   ┌─────▼────────────┐
               │  Indexer #1    │   │  Indexer #2       │
               │  (LEADER)      │   │  (FOLLOWER)       │
               │                │   │                   │
               │  ┌──────────┐  │   │  ┌──────────┐    │
               │  │Poller    │  │   │  │Poller    │    │
               │  │(active)  │  │   │  │(skips)   │    │
               │  └────┬─────┘  │   │  └──────────┘    │
               │       │        │   │                   │
               │  ┌────▼─────┐  │   │  ┌────────────┐  │
               │  │LRU Cache │  │   │  │ LRU Cache  │  │
               │  └────┬─────┘  │   │  └────────────┘  │
               │       │        │   │                   │
               │  ┌────▼─────┐  │   │  ┌────────────┐  │
               │  │Write Pool│  │   │  │ Read Pool  │  │
               │  └──────────┘  │   │  └─────┬──────┘  │
               └───────┬────────┘   └────────┼─────────┘
                       │                     │
              ┌────────▼─────────────────────▼──────────┐
              │            PostgreSQL Primary            │
              └────────────────────┬────────────────────┘
                                   │  streaming replication
                              ┌────▼─────────────┐
                              │   PG Read Replica │
                              │   (optional)      │
                              └───────────────────┘
```

**Key invariants:**

1. Exactly one instance is the *leader* at any time (the leader election module
   enforces mutual exclusion at the DB level).
2. All instances serve read traffic via the REST API regardless of leader status.
3. The DB is the single source of truth; the in-process LRU cache is a
   read-through optimisation, not a write-behind buffer.

---

## PostgreSQL Backend

### Connection pools

Two `deadpool-postgres` pools are created per process:

| Pool | Variable | Default size | Used by |
|---|---|---|---|
| **Write** | `EVENT_INDEXER_DB_POOL_SIZE` | 5 | Leader ingestion path only |
| **Read** | `EVENT_INDEXER_DB_READ_POOL_SIZE` | 10 | All API query handlers |

### Schema

```sql
-- Main events table.
CREATE TABLE events (
    id               TEXT        PRIMARY KEY,   -- UUID, idempotency key
    ledger_sequence  INTEGER     NOT NULL,
    match_id         BIGINT      NOT NULL,
    event_type       TEXT        NOT NULL,
    player1          TEXT,
    player2          TEXT,
    status           TEXT,
    winner           TEXT,
    stake_amount     TEXT,
    token            TEXT,
    game_id          TEXT,
    platform         TEXT,
    timestamp        TIMESTAMPTZ NOT NULL,
    txn_hash         TEXT
);

-- Leader-election table.
CREATE TABLE leader_state (
    instance_id TEXT        PRIMARY KEY,   -- always '__leader__'
    held_until  TIMESTAMPTZ NOT NULL
);
```

### Idempotency

```sql
INSERT INTO events (id, …) VALUES ($1, …)
ON CONFLICT (id) DO NOTHING
```

Re-ingesting a ledger range after a crash or failover is always safe.  The
`id` column (a UUID derived from the event's ledger position and content)
serves as the deduplication key.

---

## Leader Election

### Algorithm

Leader election combines two PostgreSQL primitives for defence in depth:

1. **Session-level advisory lock** (`pg_try_advisory_lock(0x65766474697864)`)
   – non-blocking; automatically released when the connection drops.
2. **Row-level lease** in the `leader_state` table – a heartbeat that must
   be renewed every `HEARTBEAT_SECS`; expires after `TTL_SECS`.

Acquiring the lease requires holding *both* mechanisms.  If a leader process
is killed, its connection drops → advisory lock released; the row lease
expires after at most `TTL_SECS` → a follower can win the next election.

### Timing

```
leader_ttl_secs        = 30   # lease valid for 30 s without renewal
leader_heartbeat_secs  = 10   # leader renews every 10 s
                               # worst-case failover = ttl (30 s)
```

The heartbeat is run in the *poller loop itself* (every `poll_interval_secs`)
as well as in a dedicated background task to prevent starvation when the RPC
call takes longer than expected.

### State machine

```
FOLLOWER ──try_acquire()──► LEADER
    ▲                           │
    │                       heartbeat renews lease
    │                           │
    └──── lease expires ─────────┘
    └──── connection drop ───────┘
```

---

## LRU Cache

The in-process cache uses `indexmap::IndexMap` to maintain strict insertion /
access-time ordering with O(1) eviction:

- **MRU position** = back of the map.
- **LRU position** = front of the map (evicted when capacity is exceeded).
- Re-inserting an existing key moves it to MRU (`shift_remove` + re-insert).

The eviction order is **fully deterministic** and verified by the unit test
`lru_evicts_oldest_inserted_entry` in `cache.rs`.

Each instance maintains an **independent** cache.  This is intentional: it
avoids distributed cache coherency complexity, and the DB (via the read pool)
serves as the consistent backing store for cache misses.

---

## Read Scaling

All API query handlers access PostgreSQL through the **read pool**
(`read_pool`).  To scale read throughput:

1. Provision a PostgreSQL streaming replica.
2. Set `DATABASE_READ_URL=postgres://…` pointing to the replica.
3. Scale the number of indexer replicas (they only need the read pool for API
   traffic; the write pool is used only by the leader's ingestion path).

Horizontally scaling API capacity is therefore as simple as adding more
indexer instances behind the load balancer — no code changes required.

---

## Failure Modes

### Leader process crashes

| Step | What happens |
|---|---|
| 1 | Leader's TCP connection to PG drops |
| 2 | Advisory lock released immediately |
| 3 | `leader_state.held_until` still in the future |
| 4 | Followers' `try_acquire` returns `false` until `held_until < NOW()` |
| 5 | After ≤ `TTL_SECS`, a follower wins the election and resumes ingestion |
| 6 | New leader re-polls from `last_ledger - safety_overlap`; duplicates handled by `ON CONFLICT DO NOTHING` |

**Worst-case data gap:** `TTL_SECS` seconds of ledgers are not indexed
while no leader is active.  These are caught up immediately once a new
leader is elected.

### PostgreSQL primary unavailable

All instances enter a degraded state:
- Poller: retries with exponential backoff (see `event_poller` error handling).
- API: returns `503 {"db": "error"}` from `/health`; query endpoints return
  `500` until the DB recovers.
- In-process cache continues to serve warm data.

### Replica lag

If `DATABASE_READ_URL` points to a lagging replica, `/events` and `/match/:id`
queries may return slightly stale data.  The `/health` endpoint's `db` field
reports replica connectivity.  To mitigate: tune `max_standby_streaming_delay`
on the replica and monitor replica lag via `pg_stat_replication`.

### Split-brain (two leaders)

Theoretically possible if the clock skew between two hosts causes both to
believe the lease has expired simultaneously.  Mitigations:

1. **NTP enforcement** on all hosts.
2. **Advisory lock** as a second mutex — only one PG session can hold it at
   a time regardless of clock state.
3. **`ON CONFLICT DO NOTHING`** in the ingestion path — even if both believe
   they are leaders temporarily, they write the same events and the DB deduplicates.

### Network partition (instance isolated from PG)

The isolated instance loses both the advisory lock renewal and the heartbeat
row update, so it cannot hold the lease.  Once partition heals, it re-contests
normally.

---

## Operational Runbook

### Adding an instance

1. Provision a new host / container.
2. Set all environment variables (same `DATABASE_URL`; unique
   `EVENT_INDEXER_INSTANCE_ID`).
3. Start the binary.  It will immediately begin serving read traffic and
   compete for the leader lease on the next election cycle.
4. Verify via `/health` that the new instance reports `"db": "ok"`.

### Removing an instance (graceful)

1. Drain the instance from the load balancer (stop routing API requests to it).
2. Send `SIGTERM`; the process exits.
3. If it was the leader, the lease expires within `TTL_SECS` and another
   instance takes over.

### Removing an instance (abrupt)

- Same as a crash.  See *Leader process crashes* above.

### Scaling the read layer

```bash
# Scale to 4 replicas in Docker Compose:
docker-compose up --scale event-indexer=4
```

Each replica uses the read pool; no additional configuration is needed.

### Changing PostgreSQL primary (planned failover)

1. Promote the replica to primary.
2. Update `DATABASE_URL` (and optionally `DATABASE_READ_URL`) on all instances.
3. Rolling-restart the instances; each reconnects to the new primary.
4. The write pool will re-establish connections automatically via
   `deadpool-postgres` recycling.

### Changing PostgreSQL primary (unplanned failover)

1. Ensure HA tooling (e.g., Patroni, Repmgr) has promoted the replica.
2. Update `DATABASE_URL` in the environment / secrets store.
3. Restart all indexer instances.

### Checking leader status

```bash
# Which instance currently holds the lease?
psql $DATABASE_URL -c "SELECT instance_id, held_until FROM leader_state"
```

### Manually forcing a leader re-election

```bash
# Delete the lease row; the next instance to poll wins.
psql $DATABASE_URL -c "DELETE FROM leader_state"
```

### Resetting the event store (development only)

```bash
psql $DATABASE_URL -c "TRUNCATE TABLE events; DELETE FROM leader_state"
```

---

## Configuration Reference

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | **required** | Primary PG DSN (`postgres://user:pass@host:5432/db`) |
| `DATABASE_READ_URL` | `$DATABASE_URL` | Read-replica PG DSN (falls back to primary) |
| `EVENT_INDEXER_DB_POOL_SIZE` | `5` | Write pool connections |
| `EVENT_INDEXER_DB_READ_POOL_SIZE` | `10` | Read pool connections per instance |
| `EVENT_INDEXER_INSTANCE_ID` | hostname / UUID | Unique ID for this process |
| `EVENT_INDEXER_LEADER_TTL_SECS` | `30` | Lease validity window (seconds) |
| `EVENT_INDEXER_LEADER_HEARTBEAT_SECS` | `10` | Lease renewal interval (must be < TTL) |
| `STELLAR_RPC_URL` | testnet endpoint | Soroban JSON-RPC endpoint |
| `CONTRACT_ESCROW` | **required** | 56-char Stellar contract address |
| `EVENT_INDEXER_BIND_ADDR` | `127.0.0.1` | API listen address |
| `EVENT_INDEXER_PORT` | `8080` | API listen port |
| `EVENT_INDEXER_CACHE_SIZE` | `10000` | LRU cache capacity (entries) |
| `EVENT_INDEXER_POLL_INTERVAL` | `5` | Polling interval (1–60 seconds) |
| `EVENT_INDEXER_LOG_LEVEL` | `info` | Log level (`error`/`warn`/`info`/`debug`/`trace`) |

---

## Metrics & Observability

The service emits structured JSON logs via `tracing-subscriber`.  Recommended
metrics to track:

| Metric | How to derive | Alert threshold |
|---|---|---|
| `leader_election_wins_total` | count log lines containing `"Became leader"` | n/a |
| `leader_election_losses_total` | count `"Lost leader lease"` | > 3/min |
| `poll_events_latency_ms` | trace span `poll_iteration` duration | p99 > 5000 ms |
| `db_query_latency_ms` | instrument `query_events` call | p99 > 200 ms |
| `cache_hit_rate` | `cache_size / total_events` ratio from `/stats` | < 0.5 |
| `replica_lag_seconds` | `pg_stat_replication.replay_lag` | > 10 s |

### Health check

```bash
curl http://localhost:8080/health
# {"db":"ok"}
```

### Stats

```bash
curl http://localhost:8080/stats
# {"success":true,"data":{"total_events":42000,"cache_size":10000},"error":null}
```
