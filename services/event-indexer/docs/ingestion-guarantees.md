# Event Ingestion Guarantees

## Overview

The event indexer provides **exactly-once semantics** for ingested events, with automatic deduplication and recovery from transient failures.

## ID Scheme

**Format:** SHA-256(ledger || txn_hash || event_index_in_txn)

- **Deterministic:** Same event (identified by ledger, transaction, and position) always produces identical ID.
- **Collision-free:** Unique across all ledgers, transactions, and event positions within a transaction.
- **Immutable:** Never changes even if event data is modified elsewhere.
- **Compact:** 64-character hex string (32 bytes), efficient for indexing.

Example:
```
Event: ledger=100, txn_hash="GBNBH2...", event_index=0
ID:    "a7f3d4e5c6b9a8f1e2d3c4b5a6f7e8d9a0b1c2d3e4f5a6b7c8d9e0f1a2b"
```

Re-ingesting the same event produces the identical ID every time.

## Polling Boundary

**Strategy:** Exclusive-start with durably-stored state.

1. After successfully ingesting ledger N, store `last_polled_ledger = N` in the database.
2. On the next poll, request events starting from `last_polled_ledger + 1`.
3. No event from ledger N is fetched a second time.
4. Storage is durable across restarts and leader failovers.

**Durability:** The `ingestion_state` table persists polling progress:
```sql
SELECT contract_id, last_polled_ledger, last_polled_at FROM ingestion_state;
```

## Idempotency Guarantee

**The core guarantee:** Inserting the same event multiple times produces exactly one row.

**Mechanism:** `INSERT ... ON CONFLICT (id) DO NOTHING`

Since event IDs are deterministic:
- Every re-fetch of the same event produces the same ID.
- The database conflict detection catches the duplicate.
- Only the first insertion succeeds; subsequent inserts are silent no-ops.

**Safe operations:**
- ✅ Leader failover: new leader can safely re-poll the last K ledgers.
- ✅ Crash recovery: restart resumes from `last_polled_ledger`, may re-ingest overlap.
- ✅ RPC lag: if RPC reports stale ledger, re-fetching is safe.

## Reorg Handling

**Definition:** A reorg occurs when an already-polled ledger is modified or invalidated.

In Stellar/Soroban, ledgers are finalized immediately, but operational failures can occur:
- RPC instance crashes and loses state, reporting older ledgers.
- Network partition: briefly serving stale data.
- Testnet reset (development environments).

**Recovery strategy:** Append-only markers with audit trail.

When reorg is detected:
1. Mark affected events with `reorg_invalidated_at = NOW()`.
2. Log the reorg event to `reorg_events` table with reason.
3. Reset `last_polled_ledger` to a safe checkpoint (e.g., 10 ledgers back).
4. Resume normal polling from the safe point.

**Queries filter out invalidated events:**
```sql
SELECT * FROM events WHERE reorg_invalidated_at IS NULL;
```

**Audit trail:**
```sql
SELECT * FROM reorg_events ORDER BY created_at DESC;
```

## Failure Modes & Mitigations

| Failure | Symptom | Root Cause | Mitigation |
|---------|---------|-----------|-----------|
| Duplicate events | Event appears twice with different IDs | Random ID + inclusive boundary | Deterministic hash + exclusive start |
| Lost events | Ledger range is skipped | Ledger backtrack on restart | Durably stored `last_polled_ledger` |
| Exponential growth | DB grows by re-ingesting same ledger | Undetected boundary error | Exclusive-start + idempotency check |
| Stale state post-reorg | Invalid events served to API | Reorg unmarked | Append-only markers, query filter |

## Testing & Verification

### Unit: Deterministic ID generation
```bash
cargo test deterministic_id_
```

### Integration: Repeated-poll idempotency
```bash
cargo test idempotency_
```

### Full DB: Repeated polling produces no duplicates
Requires `DATABASE_URL` environment variable.
```bash
DATABASE_URL=postgres://... cargo test --features pg_integration repeated_poll_same_ledger
```

### Dry-run: Identify existing duplicates
Before migration, scan for duplicates:
```bash
cargo run --bin identify-duplicates
```

## Operational Tasks

### Detect unresolved duplicates (should be zero)
```sql
SELECT ledger_sequence, txn_hash, COUNT(*) as dups, array_agg(id) as ids
FROM events
WHERE reorg_invalidated_at IS NULL
GROUP BY ledger_sequence, txn_hash
HAVING COUNT(*) > 1
ORDER BY dups DESC;
```

### Check ingestion progress
```sql
SELECT contract_id, last_polled_ledger, last_polled_at, last_error, last_error_at
FROM ingestion_state;
```

### View recent reorgs
```sql
SELECT * FROM reorg_events ORDER BY created_at DESC LIMIT 20;
```

### Manually reset ingestion state (for recovery)
```sql
UPDATE ingestion_state
SET last_polled_ledger = 100, last_error = NULL
WHERE contract_id = 'YOUR_CONTRACT_ID';
-- Next poll will resume from ledger 101
```

### Estimate deduplication ratio (before migration)
```sql
SELECT
  COUNT(DISTINCT (ledger_sequence, txn_hash, event_type, match_id)) as unique_logical_events,
  COUNT(*) as physical_rows,
  ROUND(100.0 * COUNT(*) / COUNT(DISTINCT (ledger_sequence, txn_hash, event_type, match_id)), 1) as inflation_pct
FROM events
WHERE reorg_invalidated_at IS NULL;
```

## Future Enhancements

- [ ] Ledger finality proofs from Soroban (when available).
- [ ] Cross-contract event correlation and linking.
- [ ] Event replay with time-travel queries.
- [ ] Merkle tree verification of event sequence.
- [ ] Automated reorg recovery tuning based on RPC latency.
