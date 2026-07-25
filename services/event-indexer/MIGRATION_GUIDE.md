# Event Indexer Data-Integrity Fix: Complete Migration Guide

## Problem Statement

The event indexer had a critical data-integrity bug:
- Events were assigned random UUIDs instead of deterministic keys
- Polling used inclusive boundaries (same ledger fetched repeatedly)
- Result: **Every event on the most-recent ledger was re-ingested on every poll cycle**, causing unbounded data duplication

This guide walks through the complete fix and migration process.

---

## Architecture of the Fix

### 1. Deterministic Event IDs
**Old:** `id = UUID::new_v4()` (random)  
**New:** `id = SHA-256(ledger || txn_hash || event_index_in_txn)` (deterministic)

**Benefit:** Same event always produces identical ID, enabling proper deduplication via `ON CONFLICT (id) DO NOTHING`.

### 2. Exclusive-Start Polling
**Old:** Poll from ledger N, set `last_ledger = N`, poll again from ledger N (inclusive, creates overlap)  
**New:** Poll from ledger N, store `last_polled_ledger = N`, poll from ledger N+1 (exclusive, no overlap)

**Benefit:** Eliminates re-fetching of already-processed ledgers.

### 3. Reorg Handling
**Detection:** Ledger backtracking (received ledger ≤ last_polled)  
**Recovery:** Soft-delete events with `reorg_invalidated_at` marker, reset polling point

**Benefit:** Auditable, reversible, safe.

---

## Deployment Timeline

### Phase 1: Foundation (Completed ✅)
**Duration:** ~1 day  
**Risk:** LOW – all changes are additive; no behavior changes

- [x] Added `id_gen` module with deterministic ID computation
- [x] Added schema columns: `event_index_in_txn`, `reorg_invalidated_at`
- [x] Added tables: `ingestion_state`, `reorg_events`
- [x] Added DB helper functions: `get_latest_polled_ledger()`, `update_ingestion_state()`, `mark_events_as_invalid()`
- [x] Added comprehensive tests (idempotency, determinism)
- [x] Updated Cargo.toml with `sha2` dependency
- [x] Created documentation

**Status:** Code is in place but inactive. Old behavior (random IDs, inclusive boundaries) still active.

### Phase 2: Behavior Activation (Completed ✅)
**Duration:** ~1 week (gradual rollout)  
**Risk:** MEDIUM – behavior changes; requires monitoring

**What changed:**
- `parse_event()` now computes deterministic IDs using `id_gen::compute_event_id()`
- `poll_events()` now:
  - Loads `last_polled_ledger` from DB (exclusive-start)
  - Tracks `event_index_in_txn` for each event
  - Detects reorgs (ledger backtrack)
  - Updates `ingestion_state` after successful poll
  - Logs reorg events to `reorg_events` table
- `event_poller()` now reads state from `ingestion_state` table instead of holding in-memory

**Integration tests added:**
- `poll_ingestion_state_tracking()` – verifies state persistence
- `reorg_event_marking()` – verifies reorg handling
- `event_index_in_txn_is_preserved()` – verifies event tracking

**Rollout strategy:**
1. Deploy with both flags OFF (code present, inactive) – verify no regressions
2. Enable on 1 follower instance (low risk, no writes)
3. Monitor 24h for errors in logs
4. Enable on all follower instances
5. Monitor 24h
6. Enable on leader instance
7. Monitor full poll cycle (≥ 24h)

**Monitoring checklist:**
- [ ] No `parse_event` errors in logs
- [ ] `update_ingestion_state` succeeds
- [ ] `get_latest_polled_ledger` returns expected values
- [ ] No ledger backtrack warnings (reorg detection)
- [ ] Event count stable (no unexplained duplicates)

### Phase 3: Data Migration (Next)
**Duration:** ~4 hours (during maintenance window)  
**Risk:** HIGH – touches production data; must be done carefully

#### Step 1: Identify Duplicates (Read-Only)
```bash
cargo run --bin identify-duplicates
```

Output shows:
- How many duplicate event groups exist
- Database inflation percentage
- Recommendation to proceed or skip

**Example output:**
```
Summary:
  Total duplicate event groups: 1,247
  Total duplicate events: 3,892
  Extra rows to remove: 2,645
  Database inflation: 12.3% (2,645 extra rows / 21,500 total)
```

**Decision point:** If inflation is >0%, proceed to Step 2.

#### Step 2: Dry-Run Migration (Safe)
```bash
BACKUP_TAKEN=yes cargo run --bin migrate-ids -- --dry-run
```

Generates SQL preview. Review for correctness.

**Sanity checks:**
```sql
-- Before migration
SELECT COUNT(*) as total FROM events WHERE reorg_invalidated_at IS NULL;
SELECT COUNT(DISTINCT id) as unique_ids FROM events WHERE reorg_invalidated_at IS NULL;

-- Expected: total > unique_ids (duplicates exist)
```

#### Step 3: Execute Migration (Requires Downtime)
```bash
# 1. Notify users of maintenance window
# 2. Stop all poller instances
# 3. Backup database
# 4. Run migration
BACKUP_TAKEN=yes cargo run --bin migrate-ids -- --commit

# 5. Verify results
SELECT COUNT(*) as total FROM events WHERE reorg_invalidated_at IS NULL;
SELECT COUNT(DISTINCT id) as unique_ids FROM events WHERE reorg_invalidated_at IS NULL;
-- Expected: total == unique_ids (no more duplicates)

# 6. Rebuild indices
REINDEX INDEX CONCURRENTLY idx_events_ledger;
REINDEX INDEX CONCURRENTLY idx_events_match_id;
# ... (rebuild all indices)

# 7. Vacuum
VACUUM ANALYZE events;

# 8. Verify database health
SELECT pg_size_pretty(pg_total_relation_size('events'));
```

#### Step 4: Resume Services
```bash
# 1. Start all poller instances (they will resume from last_polled_ledger)
# 2. Monitor for errors
# 3. Verify API queries work
# 4. Communicate completion to users
```

### Phase 4: Cleanup (Final)
**Duration:** ~1 day  
**Risk:** LOW – code removal only

- [ ] Remove feature flags (if used)
- [ ] Remove legacy code paths (if any)
- [ ] Update documentation
- [ ] Add to release notes

---

## Verification Checklist

### After Phase 2 (Behavior Activated)

- [ ] Logs show no `parse_event` errors
- [ ] `ingestion_state` table has rows with increasing `last_polled_ledger`
- [ ] No reorg warnings in logs (unless expected)
- [ ] API queries return correct data
- [ ] Load test: ≥5000 ops/sec on cache
- [ ] Concurrent ingestion: no duplicates observed

**SQL Queries:**
```sql
-- Check state tracking
SELECT contract_id, last_polled_ledger, last_polled_at, last_error
FROM ingestion_state
ORDER BY last_polled_at DESC;

-- Verify no new duplicates (should be 0)
SELECT COUNT(*) as duplicate_groups
FROM (
  SELECT ledger_sequence, txn_hash, COUNT(*) as cnt
  FROM events
  WHERE reorg_invalidated_at IS NULL
  GROUP BY ledger_sequence, txn_hash
  HAVING COUNT(*) > 1
) subq;
```

### After Phase 3 (Migration Complete)

- [ ] Migration tool completed successfully
- [ ] Total event count unchanged
- [ ] Unique ID count equals total event count
- [ ] Database size reduced (duplicates removed)
- [ ] Indices rebuilt and healthy
- [ ] API queries still work
- [ ] Event ordering preserved (ORDER BY ledger_sequence)

**SQL Queries:**
```sql
-- Verify migration success
SELECT 
  COUNT(*) as total_events,
  COUNT(DISTINCT id) as unique_ids,
  COUNT(*) = COUNT(DISTINCT id) as is_unique
FROM events
WHERE reorg_invalidated_at IS NULL;
-- Expected: is_unique = true

-- Check reorg event log
SELECT COUNT(*) as reorg_count FROM reorg_events;

-- Verify no reorg_invalidated events unless intentional
SELECT COUNT(*) as invalidated_count FROM events WHERE reorg_invalidated_at IS NOT NULL;
-- Expected: 0 (unless reorg happened during migration)
```

---

## Rollback Plan

If issues arise during any phase:

### During Phase 2 (Before Migration)
- Disable feature flags (if used)
- Restart poller with old behavior
- Existing data is unaffected (old random IDs remain)
- **Impact:** Temporary re-duplication on next poll, but DB recovers once feature is re-enabled

### During Phase 3 (Migration)
- **Before Step 3:** Simply skip the migration. Old duplicates remain but system continues.
- **During Step 3 (in transaction):** Transaction rolls back automatically on error. DB unchanged.
- **After Step 3 (committed):** Use backup restore:
  ```bash
  # Restore from backup taken before migration
  pg_restore --dbname=checkmate-escrow /path/to/backup.sql
  ```

### After Phase 3 (Post-Rollback)
- Resume Phase 2 with debugging
- Fix root cause
- Re-attempt migration

---

## Monitoring & Observability

### Key Metrics

**Ingestion health:**
```sql
-- Events per poll cycle
SELECT 
  DATE_TRUNC('hour', last_polled_at) as hour,
  COUNT(*) as polls,
  SUM(events_ingested) as total_ingested
FROM ingestion_state
GROUP BY DATE_TRUNC('hour', last_polled_at)
ORDER BY hour DESC;
```

**Reorg frequency:**
```sql
-- Reorg events over time
SELECT 
  DATE_TRUNC('day', created_at) as day,
  COUNT(*) as reorg_count,
  SUM(events_invalidated_count) as events_affected
FROM reorg_events
GROUP BY DATE_TRUNC('day', created_at)
ORDER BY day DESC;
```

**Data quality:**
```sql
-- Duplicate check (should stay at 0)
SELECT COUNT(*) as duplicate_groups
FROM (
  SELECT ledger_sequence, txn_hash, COUNT(*) as cnt
  FROM events
  WHERE reorg_invalidated_at IS NULL
  GROUP BY ledger_sequence, txn_hash
  HAVING COUNT(*) > 1
) subq;
```

### Alerting

Set up alerts for:
1. **Reorg detected:** `reorg_events` table receives new row
2. **Ingestion stalled:** `last_polled_at` hasn't updated in >2 poll intervals
3. **Duplicates appearing:** Above query returns > 0
4. **Error rate spike:** `last_error` field populated

---

## Testing

Run full test suite:

```bash
# Unit tests (id_gen, determinism, idempotency)
cargo test --lib

# Integration tests (requires DATABASE_URL)
DATABASE_URL=postgres://... cargo test --features pg_integration

# Specific test
cargo test repeated_poll_same_ledger -- --nocapture
```

**Critical tests:**
- ✅ `deterministic_id_*` (5 tests) – verify ID scheme correctness
- ✅ `idempotency_*` (3 tests) – verify duplicate handling
- ✅ `poll_ingestion_state_tracking` – verify state persistence
- ✅ `reorg_event_marking` – verify reorg recovery

---

## FAQ

**Q: Will this cause downtime?**  
A: Phase 1-2 have zero downtime. Phase 3 (data migration) requires a brief maintenance window (typically 4 hours for 1M events).

**Q: Can we do the migration live (no downtime)?**  
A: Yes, but more complex. Requires:
- Running migration in background without stopping pollers
- Careful coordination of ID updates and deduplication
- More thorough testing
- Not recommended for first-time migration

**Q: What if the RPC API changes?**  
A: If `txnMeta` (transaction hash) disappears or changes format, deterministic ID computation will fail. Mitigation:
- Monitor RPC API changes
- Add schema version tracking
- Have a plan for ID scheme migration (future work)

**Q: How long does migration take?**  
A: Typically:
- 1M events: ~10 minutes
- 10M events: ~30 minutes
- 100M events: ~2 hours
Scale based on your database size and hardware.

**Q: Can I cancel mid-migration?**  
A: Yes. If migration is running in a transaction, just kill the process. All changes roll back automatically. If already committed, restore from backup.

---

## Post-Migration Maintenance

### Periodic Audits
```bash
# Monthly duplicate check
cargo run --bin identify-duplicates
# Should output: "No duplicates found!"

# Check ingestion health
SELECT * FROM ingestion_state ORDER BY last_polled_at DESC;
```

### Performance Tuning
After migration, consider:
- Analyzing slow queries: `EXPLAIN ANALYZE SELECT ...`
- Adjusting index strategies if new query patterns emerge
- Partitioning events table by ledger_sequence if it grows very large

### Future Enhancements
- Add ledger finality proofs (when Soroban exposes them)
- Implement event replay / time-travel queries
- Add merkle tree verification

---

## Support

If issues arise:
1. Check logs for error messages
2. Run `identify-duplicates` to check DB state
3. Review `ingestion_state` and `reorg_events` tables
4. Run verification queries above
5. Consult documentation in `docs/ingestion-guarantees.md`
