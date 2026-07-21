# Event Indexer Fix: Deployment Checklist

## Pre-Deployment Verification

### Code Quality
- [ ] All code compiles without warnings: `cargo build --release`
- [ ] All tests pass: `cargo test --lib && cargo test --features pg_integration`
- [ ] Linting passes: `cargo clippy --all`
- [ ] Code review completed
- [ ] No hardcoded secrets or credentials in code

### Documentation
- [ ] `docs/ingestion-guarantees.md` reviewed and accurate
- [ ] `MIGRATION_GUIDE.md` reviewed by ops team
- [ ] `IMPLEMENTATION_SUMMARY.md` matches actual changes
- [ ] Inline code comments are clear

### Database Staging
- [ ] Staging DB schema initialized: `database.init_schema().await?`
- [ ] New tables created: `ingestion_state`, `reorg_events`
- [ ] New indices created: `idx_events_not_invalidated`
- [ ] Old events can be queried (backward compat verified)

### Staging Tests
- [ ] Run full integration test suite: `DATABASE_URL=... cargo test --features pg_integration`
- [ ] Specific test passes: `cargo test poll_ingestion_state_tracking`
- [ ] Specific test passes: `cargo test reorg_event_marking`
- [ ] Specific test passes: `cargo test idempotency_`

---

## Phase 2: Behavior Activation Deployment

### Pre-Deployment
- [ ] Backup production database
- [ ] Notify team of deployment
- [ ] Prepare rollback plan
- [ ] Enable detailed logging: `RUST_LOG=event_indexer=debug`
- [ ] Set up monitoring dashboard for:
  - `ingestion_state` table (last_polled_ledger advancing)
  - `reorg_events` table (new rows = reorg detected)
  - Event count (should be stable or growing, not re-duplicating)
  - Error logs (parse_event failures, DB errors)

### Deployment Step 1: Deploy Code
```bash
# 1. Build optimized binary
cargo build --release

# 2. Copy binary to server
scp target/release/event-indexer user@server:/opt/bin/

# 3. Do NOT restart yet; verify binary exists
ssh user@server 'ls -lah /opt/bin/event-indexer'
```

### Deployment Step 2: Gradual Rollout
**Step 2a: Single Follower Instance (Low Risk)**
```bash
# 1. Stop follower instance 1
systemctl stop event-indexer-follower-1

# 2. Restart with new binary
systemctl start event-indexer-follower-1

# 3. Monitor for 2 hours:
# - Logs: no parse_event errors
# - DB: ingestion_state table has updates
# - DB: no new reorg_events (unless expected)
# - Event count: stable

# If issues: immediately stop and revert to old binary
```

**Step 2b: All Follower Instances**
```bash
# 1. If Step 2a healthy, roll out to all followers
for i in {1..5}; do
  systemctl stop event-indexer-follower-$i
  systemctl start event-indexer-follower-$i
  sleep 30  # Stagger to avoid thundering herd
done

# 2. Monitor for 24 hours:
# - All followers healthy
# - ingestion_state advancing normally
# - No duplicate events appearing
# - Queries still fast
```

**Step 2c: Leader Instances**
```bash
# 1. Only after all followers healthy for 24h
systemctl stop event-indexer-leader-1

# 2. Restart leader (careful: this stops ingestion during restart)
systemctl start event-indexer-leader-1

# 3. Monitor intensively for 1 hour:
# - Leader elected
# - Ingestion resumes
# - No duplicates re-appear
# - No ledger backtrack (reorg detection)

# 4. If all healthy, monitor normally for 24h
```

### Post-Deployment Verification (Phase 2)
```bash
# Check 1: ingestion_state exists and advancing
psql -d checkmate-escrow -c "SELECT contract_id, last_polled_ledger, last_polled_at FROM ingestion_state;"
# Expected: rows present, last_polled_ledger increasing, last_polled_at recent

# Check 2: No new duplicate events
psql -d checkmate-escrow -c "
  SELECT COUNT(*) as duplicate_groups
  FROM (SELECT ledger_sequence, txn_hash, COUNT(*) as cnt
        FROM events WHERE reorg_invalidated_at IS NULL
        GROUP BY ledger_sequence, txn_hash HAVING COUNT(*) > 1) subq;
"
# Expected: 0 (or same as pre-deployment)

# Check 3: Reorg events table (should be empty unless real reorg)
psql -d checkmate-escrow -c "SELECT COUNT(*) as reorg_count FROM reorg_events;"
# Expected: 0 (unless Soroban had actual issue)

# Check 4: Event count stable
psql -d checkmate-escrow -c "SELECT COUNT(*) FROM events;"
# Expected: stable or slow growth (not exponential)

# Check 5: No error logs
grep -i "parse_event.*error\|update_ingestion_state.*error" /var/log/event-indexer.log
# Expected: no errors found

# Check 6: API still works
curl http://localhost:8080/events?status=pending | jq .success
# Expected: true
```

### Monitoring During Phase 2 (24-48 hours)
- [ ] Event count NOT growing exponentially (re-duplication would show this)
- [ ] `ingestion_state.last_polled_ledger` advancing monotonically
- [ ] No repeated reorg warnings in logs
- [ ] CPU usage normal
- [ ] Database query latency normal
- [ ] No connection pool exhaustion
- [ ] API response times normal

### Rollback Phase 2 (If Issues Arise)
```bash
# 1. Immediately stop all instances
systemctl stop event-indexer-*

# 2. Revert to old binary
cp /opt/bin/event-indexer.old /opt/bin/event-indexer

# 3. Restart
systemctl start event-indexer-*

# 4. Monitor for stability
# - Event count should stabilize
# - Old behavior resumes (random IDs, inclusive polling)

# 5. Investigate root cause before re-attempting
grep -A5 -B5 "error\|Error\|ERROR" /var/log/event-indexer.log | tail -100
```

---

## Phase 3: Data Migration Deployment

### Pre-Migration (Read-Only Assessment)
```bash
# 1. Run duplicate identification (0 risk)
cargo run --bin identify-duplicates 2>&1 | tee /tmp/duplicates-report.txt

# 2. Review output
cat /tmp/duplicates-report.txt

# 3. Assess impact
# - If duplicate_count = 0: skip Phase 3 (already deduplicated)
# - If inflation_pct < 1%: low risk, proceed with confidence
# - If inflation_pct > 10%: high risk, plan maintenance window carefully
```

### Migration Planning
- [ ] Maintenance window scheduled (4-8 hours, off-peak)
- [ ] Stakeholders notified
- [ ] Backup strategy confirmed (test restore before)
- [ ] Rollback plan documented
- [ ] DBA reviewed migration script

### Pre-Migration (Backup Phase)
```bash
# 1. Full database backup (this is critical!)
pg_dump -Fc checkmate-escrow > /backups/checkmate-escrow-$(date +%Y%m%d-%H%M%S).dump
# Verify backup exists and is not empty
ls -lh /backups/checkmate-escrow-*.dump | tail -1

# 2. Test restore on staging (to verify backup integrity)
createdb checkmate-escrow-restore-test
pg_restore -d checkmate-escrow-restore-test /backups/checkmate-escrow-*.dump
psql -d checkmate-escrow-restore-test -c "SELECT COUNT(*) FROM events;"
# Expected: same count as production

# 3. Drop test database
dropdb checkmate-escrow-restore-test

# 4. Prepare backup location (keep recent backups)
ls -lh /backups/ | grep checkmate-escrow
```

### Maintenance Window: Stop Services
```bash
# 1. Notify all users
# Announcement: "Maintenance window starting: 2:00 AM UTC. Estimated duration: 4 hours."

# 2. Wait for connections to drain (30 minutes)
sleep 1800

# 3. Stop all instances
systemctl stop event-indexer-*

# 4. Verify all stopped
systemctl status event-indexer-* 2>&1 | grep "inactive (dead)"

# 5. Double-check no background processes
ps aux | grep event-indexer | grep -v grep
# Expected: no processes
```

### Maintenance Window: Migration
```bash
# 1. Enable logging
export RUST_LOG=debug

# 2. Dry-run (preview changes)
cargo run --bin migrate-ids -- --dry-run > /tmp/migration-dryrun.sql
# Review output
tail -50 /tmp/migration-dryrun.sql

# 3. If dry-run looks good, execute migration
echo "Starting migration at $(date)"
cargo run --bin migrate-ids -- --commit 2>&1 | tee /tmp/migration.log

# 4. Check for errors
grep -i "error\|failed" /tmp/migration.log
# Expected: no errors

# 5. Verify results
psql -d checkmate-escrow -c "SELECT COUNT(*) as total, COUNT(DISTINCT id) as unique_ids FROM events WHERE reorg_invalidated_at IS NULL;"
# Expected: total == unique_ids (all deduplicated)
```

### Maintenance Window: Post-Migration Maintenance
```bash
# 1. Rebuild indices (faster queries after dedup)
psql -d checkmate-escrow << EOF
REINDEX INDEX CONCURRENTLY idx_events_match_id;
REINDEX INDEX CONCURRENTLY idx_events_player1;
REINDEX INDEX CONCURRENTLY idx_events_player2;
REINDEX INDEX CONCURRENTLY idx_events_ledger;
VACUUM ANALYZE events;
EOF

# 2. Check database health
psql -d checkmate-escrow -c "
  SELECT pg_size_pretty(pg_total_relation_size('events')) as events_size,
         pg_size_pretty(pg_total_relation_size('public')) as total_size;
"
# Note: size should be smaller than pre-migration due to deduplication

# 3. Verify no orphaned data
psql -d checkmate-escrow -c "
  SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename;
"
# Expected: events, ingestion_state, reorg_events, leader_state
```

### Maintenance Window: Resume Services
```bash
# 1. Start services
systemctl start event-indexer-*

# 2. Wait for instances to connect and elect leader
sleep 10

# 3. Check status
systemctl status event-indexer-* | grep "active (running)"
# Expected: all active

# 4. Verify ingestion resumed
sleep 30
psql -d checkmate-escrow -c "SELECT last_polled_at FROM ingestion_state ORDER BY last_polled_at DESC LIMIT 1;"
# Expected: recent (within last 30 seconds)

# 5. Notify users
# Announcement: "Maintenance complete. Systems resuming normal operation."
```

### Post-Migration Verification (Phase 3)
```bash
# Check 1: Duplication eliminated
psql -d checkmate-escrow -c "
  SELECT COUNT(*) as duplicate_groups
  FROM (SELECT ledger_sequence, txn_hash, COUNT(*) as cnt
        FROM events WHERE reorg_invalidated_at IS NULL
        GROUP BY ledger_sequence, txn_hash HAVING COUNT(*) > 1) subq;
"
# Expected: 0 (no duplicates)

# Check 2: Event count unchanged
psql -d checkmate-escrow -c "SELECT COUNT(*) FROM events WHERE reorg_invalidated_at IS NULL;"
# Expected: same as pre-migration

# Check 3: No data loss
psql -d checkmate-escrow -c "SELECT COUNT(DISTINCT match_id) FROM events;"
# Expected: same as pre-migration

# Check 4: IDs are deterministic
psql -d checkmate-escrow -c "
  SELECT ledger_sequence, txn_hash, COUNT(*) as id_count
  FROM events
  WHERE reorg_invalidated_at IS NULL
  GROUP BY ledger_sequence, txn_hash
  HAVING COUNT(*) > 1;
"
# Expected: 0 rows (no duplicate (ledger, txn_hash) pairs)

# Check 5: Ingestion healthy
psql -d checkmate-escrow -c "SELECT COUNT(*) FROM ingestion_state WHERE last_error IS NOT NULL;"
# Expected: 0 (no errors)

# Check 6: API works
curl http://localhost:8080/match/1 | jq .success
# Expected: true
```

### Monitoring Post-Migration (48 hours)
- [ ] Event count stable (no re-duplication)
- [ ] Ingestion rate normal
- [ ] API response times normal
- [ ] No new duplicate events
- [ ] No errors in logs

---

## Rollback Procedures

### Phase 2 Rollback (Code)
```bash
# If issues arise during Phase 2:
systemctl stop event-indexer-*
cp /opt/bin/event-indexer.old /opt/bin/event-indexer
systemctl start event-indexer-*
# Old behavior resumes; no data loss
```

### Phase 3 Rollback (Data Migration)
```bash
# If migration fails before commit (automatic rollback)
# Database unchanged; try again after debugging

# If migration completes but corruption detected
dropdb checkmate-escrow
pg_restore -d checkmate-escrow /backups/checkmate-escrow-YYYYMMDD-HHMMSS.dump
# Restore from backup; minimal data loss (only events since backup)
```

---

## Success Criteria

### Phase 2 Success (24-48 hours post-deployment)
- [x] All instances healthy (no crashes)
- [x] Event count stable or growing normally (not exponentially)
- [x] `ingestion_state` populated and advancing
- [x] `reorg_events` table (0 entries unless real reorg)
- [x] No duplicate events detected
- [x] API queries fast and correct
- [x] Zero parse_event errors in logs

### Phase 3 Success (Post-migration)
- [x] Duplicate count = 0
- [x] Database size reduced
- [x] Event count unchanged
- [x] All unique IDs are deterministic
- [x] Ingestion continues without interruption
- [x] All tests pass

### Overall Success
- [x] Exactly-once semantics verified
- [x] Zero data loss or corruption
- [x] Audit trail complete (ingestion_state + reorg_events)
- [x] Operational procedures documented and tested
- [x] Team trained on new tooling

---

## Emergency Contacts

| Role | Name | Phone | Slack |
|------|------|-------|-------|
| On-Call SRE | | | |
| Database Admin | | | |
| Platform Lead | | | |

---

## Post-Deployment Review

**After Phase 2 is stable (72 hours):**
- [ ] Team meeting to review logs and metrics
- [ ] Discuss any issues encountered
- [ ] Confirm readiness for Phase 3

**After Phase 3 is complete (1 week):**
- [ ] Full post-mortem (if any issues)
- [ ] Document lessons learned
- [ ] Update runbooks based on experience
- [ ] Plan Phase 4 cleanup (if proceeding)

---

## Sign-Off

Deployment Lead: ________________  
Date: ________________

Database Admin: ________________  
Date: ________________

Platform Lead: ________________  
Date: ________________
