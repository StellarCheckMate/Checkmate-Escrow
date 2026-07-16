### 1. Formally-Verified Anti-Double-Spend Reconciliation Across Independently-Merged Payout Paths
**Labels:** `security`, `bug`, `storage`, `high`
**Priority:** Critical
**Estimated Time:** 40-80 hours

**Description:**
`submit_result` (`contracts/escrow/src/lib.rs:721-824`) marks `m.state = Completed` and `m.vested_at = Some(...)` unconditionally, then — since `get_dispute_period` defaults to `0` (`lib.rs:1782-1787`) — calls `execute_payout` immediately (`lib.rs:774-777`). The same `execute_payout` is also reachable via `finalize_match` (`lib.rs:1435-1489`) and `resolve_dispute_by_vote` (`lib.rs:1686-1759`). None of these three call sites ever set `m.player1_claimed`/`m.player2_claimed`. Those flags are touched *only* inside `claim_vested_payout` (`lib.rs:2369-2457`), which independently re-transfers the same pot once `state == Completed` and vesting has elapsed. A player can call `submit_result`'s payout path and then separately call `claim_vested_payout` and get paid twice — either erroring on this match's now-empty escrow, or (more dangerously) succeeding by draining the contract's pooled token balance belonging to *other, unrelated* active matches. This traces to an unreconciled merge of two independently-developed feature branches (`feature/stake-vesting` and `feature/oracle-dispute-period`), each of which assumed sole ownership of the payout path — the class of bug that survives review because each branch's own tests pass in isolation.

**Tasks:**
- Unify `execute_payout`, `submit_result`'s immediate-payout branch, `finalize_match`, `resolve_dispute_by_vote`, and `claim_vested_payout` behind one single, idempotent state-transition function with an explicit `Paid`/`Unpaid` invariant enforced at the type level, not just a convention
- Prove (formally, e.g. via Kani or a hand-built exhaustive state-machine enumerator — not just unit tests) that no reachable sequence of public calls can invoke a payout-moving function twice for the same match
- Add a contract-wide accounting invariant check (sum of per-match escrowed balances never exceeds the contract's actual token balance) runnable as a property test across randomized call sequences
- Add a regression test reproducing the exact double-payout/cross-match-drain sequence described above, proving it is now rejected
- Fix the test-suite compilation break this bug currently hides (`tests/mod.rs:51-53`, `tests/lifecycle.rs:2136-2138,2151-2153` call `update_protocol_config` with a single-field struct against `ProtocolConfig`'s real 3-field definition at `types.rs:38-44` — the whole suite doesn't currently build)
- Document the unified payout state machine in `docs/match-lifecycle.md`, replacing the current transition table
- Add a CI gate that fails if any function outside the unified payout path calls a token `transfer` for match funds

---

### 2. Unauthenticated Oracle `swap()` Fund-Drain Remediation & Trust-Minimized AMM Redesign
**Labels:** `security`, `bug`, `oracle`, `high`
**Priority:** Critical
**Estimated Time:** 40-70 hours

**Description:**
`swap` (`contracts/oracle/src/lib.rs:751-789`) computes `amount_out` from a stored rate and calls `client_out.transfer(&env.current_contract_address(), &recipient, &amount_out)` with **no `require_auth()` anywhere in the function**, and it **never collects `token_in`** from any caller. Anyone can call this today to drain the Oracle contract's entire token balance — including staked oracle bonds registered via `register_oracle_with_stake` (`lib.rs:67-135`) — to any address they choose, for free, with no funds ever provided in return. There is zero test coverage of `swap` in `contracts/oracle/src/tests.rs` despite ~1,700 lines of otherwise-thorough oracle tests. This is a live, unauthenticated fund-drain vulnerability, not a hypothetical.

**Tasks:**
- Immediately gate `swap` behind proper `require_auth()` and an atomic, guaranteed-execution `token_in` collection before any `token_out` transfer (single-transaction atomicity, no partial-fill fund loss)
- Replace the ad-hoc stored-rate model with a formally specified pricing mechanism (constant-product or an oracle-fed rate with staleness/deviation bounds — pick one and justify it), including slippage protection for callers
- Add reentrancy/callback-safety analysis for the transfer ordering (checks-effects-interactions or Soroban-equivalent) and a test proving a malicious token contract callback cannot re-enter `swap` to double-extract funds
- Add exhaustive tests: unauthenticated call rejected, insufficient `token_in` rejected, correct 2-sided settlement, slippage-bound enforcement, and the specific drain sequence from the description now failing
- Add a fuzz target exercising `swap` against adversarial rate/amount/recipient combinations
- Audit every other Oracle-contract entry point for the same missing-auth pattern and report findings even if no other instance is found
- Document the swap mechanism's trust assumptions and economic design in `docs/oracle.md`

---

### 3. Sybil/Flash-Loan-Resistant Bonded Dispute Governance with Automatic Slashing Linkage
**Labels:** `security`, `oracle`, `enhancement`, `high`
**Priority:** High
**Estimated Time:** 40-80 hours

**Description:**
`dispute_oracle_result` (`contracts/escrow/src/lib.rs:1499-1596`) only checks `evidence_hash.len() == 0` (`lib.rs:1531-1532`) — any non-empty byte string passes, with no bond required to open a dispute, so there is no anti-spam/anti-griefing cost. `vote_on_dispute` (`lib.rs:1605-1677`) weights votes by `token::Client::balance(&voter)` read **live at vote time**, with no historical snapshot, no minimum lock duration, and no quorum — an address can acquire tokens, vote, and move them elsewhere immediately (flash-loan-style vote manipulation). `resolve_dispute_by_vote`'s tie-break (`lib.rs:1718-1726`, `no_votes >= yes_votes` upholds the original result) means a dispute with **zero participation** silently rubber-stamps whatever the oracle said. Separately, `slash_oracle` (`contracts/oracle/src/lib.rs:101-135`) exists but is purely a manual, admin-unilateral action — an "Overturned" dispute outcome never automatically triggers it.

**Tasks:**
- Require a bonded stake to open a dispute, refunded on a successful overturn and forfeited (to a treasury or the counterparty) on a rejected dispute
- Replace live-balance vote weighting with a snapshot taken at dispute-open time, plus a minimum pre-dispute holding-duration requirement to defeat flash-loan/just-in-time acquisition
- Add an explicit quorum requirement (minimum total participating weight) before a vote can resolve either direction; define and implement the no-quorum fallback precisely (not a silent uphold)
- Wire an "Overturned" resolution to automatically invoke `slash_oracle` for the implicated oracle registration, removing the manual admin step
- Build a cost-of-attack model quantifying what it would take to manipulate a dispute vote under the new bonding/snapshot/quorum rules vs. the current scheme, validated with a Monte Carlo simulation across representative stake distributions
- Add tests: dispute-without-bond rejected, flash-acquired-balance vote weight rejected/discounted, no-quorum resolution behaves per spec, successful overturn triggers slashing automatically
- Document the full governance model in a new `docs/dispute-governance.md`

---

## Smart Contracts

### 4. Multi-Token Settlement Correctness & Deviation-Bounded Conversion-Rate Oracle
**Labels:** `security`, `bug`, `storage`, `enhancement`
**Priority:** High
**Estimated Time:** 40-70 hours

**Description:**
`execute_payout` (`contracts/escrow/src/lib.rs:1412-1430`) always pays out `2 × stake_amount` of `m.token` (token_a) regardless of whether the match was created via `create_match_with_conversion` — unlike `cancel_match`/`expire_match` (`lib.rs:886-918`, `1067-1086`), which correctly branch on `is_multi_token` and refund `token_b` at `conversion_rate`. A completed multi-token match therefore pays the winner in the wrong token/amount, either draining token_a liquidity meant for other matches or leaving player2's actual token_b deposit permanently stuck. Compounding this, `create_match_with_conversion` (`lib.rs:541-557`) contains the comment `// Simplified: trust the provided rate` and never validates the caller-supplied rate against any real price source, despite its own doc-comment describing a "±5% oracle rate check" that doesn't exist in the code — an unchecked economic-manipulation vector. The existing tests in `contracts/escrow/src/tests/multi_token.rs:192-278` assert correct cross-token payout behavior but call the **Oracle contract's** `submit_result` (`contracts/oracle/src/lib.rs:146-213`), which never touches escrow or moves tokens at all — meaning the multi-token payout path has never actually been exercised by a passing test.

**Tasks:**
- Make `execute_payout` multi-token-aware: correctly settle in each player's actual deposited token at the match's recorded conversion rate, mirroring the correctness already present in `cancel_match`/`expire_match`
- Replace the trust-the-caller rate with real validation against a bounded-deviation reference price source, rejecting or clamping rates outside a configurable tolerance band
- Add price-staleness handling (a rate quoted too long ago must be rejected or re-fetched, not silently used)
- Fix the disconnected test suite so `multi_token.rs` tests actually exercise `escrow_client.submit_result` end-to-end, not the unrelated Oracle contract call
- Add tests proving the fund-drain/stuck-deposit scenario described above is fixed, plus a manipulated-rate rejection test
- Add gas benchmarks for multi-token settlement vs. the existing single-token path
- Update `docs/roadmap.md`'s "Multi-token escrow — shipped in v1.0" claim to accurately reflect the fixed, verified state

---

### 5. Byzantine Multi-Oracle Consensus Replacing the Single-Admin Trust Root
**Labels:** `security`, `oracle`, `enhancement`, `research`
**Priority:** High
**Estimated Time:** 60-100 hours

**Description:**
Despite look-alike staking machinery (`register_oracle_with_stake`/`slash_oracle`, `contracts/oracle/src/lib.rs:67-135`), the entire system's "oracle" is a single `Admin` address — every mutating call is gated by `admin.require_auth()`, and `submit_result`'s stake check (`lib.rs:171-179`) only ever looks up `OracleRegistration(admin.clone())`, since only the admin can call `submit_result` in the first place. There is no m-of-n signature aggregation, no independent-oracle quorum, and no BFT voting among multiple result feeds — the staking/slashing system is structurally decorative rather than load-bearing, and `docs/security.md`'s own "Known Limitations" section correctly names this as "Single Oracle" but doesn't describe how much of the surrounding staking machinery is inert as a result.

**Tasks:**
- Design and implement a genuine m-of-n oracle result-submission scheme: multiple independently-registered, independently-staked oracle addresses each submit a signed result; the contract accepts a match result only once a threshold of matching submissions is reached
- Handle disagreement explicitly: define what happens when submitted results conflict (majority wins with minority slashed, escalate to the dispute-voting system, or a documented alternative) — do not leave it undefined
- Make the existing staking/slashing machinery load-bearing: a result-submitting oracle without adequate stake must be rejected, and provable equivocation (submitting conflicting results for the same match) must trigger automatic slashing
- Preserve backward compatibility for the current admin-oracle deployment as a degenerate n=1 configuration, with tests proving both modes
- Add tests for: threshold reached → payout proceeds, threshold not reached → match stays pending, conflicting submissions → correct dispute/slash path, single malicious-minority oracle cannot force an incorrect result
- Add gas benchmarks for consensus-checking at 3/5/10/25 registered oracles
- Document the consensus protocol, its Byzantine-fault-tolerance bound (how many colluding oracles it survives), and migration path from single-oracle in `docs/oracle.md`

---

### 6. Formal Model-Checked Verification of the Match State Machine
**Labels:** `security`, `testing`, `storage`, `research`
**Priority:** High
**Estimated Time:** 50-90 hours

**Description:**
The escrow contract's real `MatchState` has 6 variants (`contracts/escrow/src/types.rs:5-12`), but `docs/architecture.md`'s transition table (lines 60-73) documents only 4 and omits the dispute/voting flow, pause/resume, tiers, multi-token conversion, and player-balance history entirely — anyone integrating against this "stable" doc would miss most of the actual contract surface. More importantly, no artifact in the repository proves the state machine is actually safe: Issue #1 above demonstrates a real double-payout path that no existing test caught, precisely because the invariants were never made explicit and machine-checked in the first place.

**Tasks:**
- Formally enumerate every public entry point, every `MatchState` transition it can cause, and every field it mutates, as a machine-readable state-transition specification (not prose)
- Build a model-checking harness (Kani, or a custom exhaustive/randomized state-machine explorer over the contract's public interface) that verifies safety invariants: no double payout, no fund loss, no unreachable-but-fundable state, monotonic state progression where required
- Run the harness against the current codebase and report every invariant violation found (expect at least the Issue #1 bug; do not simply assume no others exist — this is a genuine search)
- Fix or explicitly document (with justification) every violation found
- Regenerate `docs/architecture.md`'s transition table directly from the formal spec so the two cannot drift again, and add a CI check that fails if the code's reachable transitions and the documented spec diverge
- Add the harness as a required CI gate for any future PR touching `contracts/escrow/src/lib.rs`
- Publish the safety-invariant list and proof methodology in `docs/formal-verification.md`, including explicit non-goals (what the harness does not guarantee)

---

## Integration

### 7. Fully Automated Fetch→Verify→Sign→Submit Oracle Pipeline with Durable Retry Queue
**Labels:** `oracle`, `integration`, `enhancement`, `devops`
**Priority:** High
**Estimated Time:** 50-90 hours

**Description:**
The "automated oracle" described in `docs/oracle.md:13-18` does not exist. `oracle-service/src/main.rs` is 36 lines total and only serves a hardcoded `/health` endpoint; there is no code anywhere in `oracle-service/` that calls `EscrowContract::submit_result` or signs/submits a Soroban transaction — `oracle-service/Cargo.toml` explicitly comments `soroban-sdk` as "Optional: used only if/when you wire to Soroban for submissions." Today, someone must manually hold the oracle key and call `submit_result` by hand. Separately, `docs/oracle.md:227-238` describes retry-with-exponential-backoff for transient errors, but `chess_com_client.rs`'s and `lichess_client.rs`'s `fetch_result` (`chess_com_client.rs:96-137`, `lichess_client.rs:79-123`) simply propagate the first error with no retry loop, no persisted "pending verification" state, and no queue — a transient API blip permanently drops a match's result unless a human notices and retries manually.

**Tasks:**
- Build the actual fetch→verify→sign→submit pipeline: poll active matches needing results, fetch from Chess.com/Lichess, verify the result maps correctly to the on-chain match, sign and submit a `submit_result` transaction to Soroban
- Add a durable, restart-surviving queue of pending verifications with exponential-backoff retry, matching what the docs already claim
- Add a dead-letter path for matches that exhaust retries, with alerting and manual-replay tooling
- Implement secure key management for the oracle-signing key (no plaintext key in process memory/env beyond what's unavoidable; document the threat model)
- Fix the copy-paste bug where `lichess_client.rs:9` uses `ChessComError` instead of the unused, purpose-built `LichessError` (`errors.rs:27-49`), so Lichess failures no longer surface as "invalid chess.com game id"
- Add integration tests against mocked Chess.com/Lichess endpoints proving the full pipeline end-to-end, including transient-failure-then-recovery
- Update `docs/oracle.md` and `docs/security.md` to describe what's actually implemented, removing claims that no longer apply

---

### 8. Concurrent Token-Bucket Rate Limiting & Multi-Provider Failover Architecture
**Labels:** `oracle`, `integration`, `performance`, `enhancement`
**Priority:** Medium
**Estimated Time:** 40-70 hours

**Description:**
Both chess-platform clients rate-limit via a single `Arc<Mutex<Instant>>` spacing gate (`chess_com_client.rs:85-94`, `lichess_client.rs:69-77`) — not a token bucket, with no per-game concurrency — hard-capping system-wide throughput at 0.5 req/s no matter how many matches need verification simultaneously. There is no failover or redundancy: if Chess.com or Lichess has an outage, every match on that platform simply stalls with no alternate path.

**Tasks:**
- Replace the single-mutex spacing gate with a real token-bucket (or leaky-bucket) rate limiter permitting configurable burst and sustained-rate limits, correctly shared across concurrent match-verification tasks
- Support per-provider concurrency limits distinct from the global rate limit (e.g. N concurrent in-flight requests, still capped at the provider's rate ceiling)
- Add a pluggable multi-provider architecture so a given match's verification can retry through an alternate data source (e.g. a secondary API mirror) if the primary is unavailable, with clear precedence/tie-breaking rules if sources disagree
- Add backpressure signaling so the oracle pipeline (Issue #7) can distinguish "rate limited, retry later" from "genuinely failed"
- Add load tests proving correct behavior at a stated target throughput (e.g. verifying 500 concurrent matches) without violating provider rate limits
- Add tests for provider-outage failover and for rate-limit-exceeded backoff behavior
- Document the rate-limiting/failover design and its configuration knobs in `docs/oracle.md`

---

### 9. Cryptographically Verifiable Health & Liveness Monitoring Replacing Decorative Health Checks
**Labels:** `oracle`, `devops`, `integration`, `testing`
**Priority:** Medium
**Estimated Time:** 40-70 hours

**Description:**
`docs/oracle.md:1-11` describes the `/health` endpoint as monitoring "connectivity and uptime," but `oracle-service/src/main.rs`'s `health_check` (lines 13-20) returns a hardcoded `status: "healthy"` and a literal placeholder `contract_address: "CB..."` unconditionally — it performs no actual connectivity check to Stellar RPC, the escrow contract, or the chess-platform APIs it's meant to front. An operator watching this endpoint has no real signal when the service is actually broken.

**Tasks:**
- Replace the hardcoded response with genuine liveness checks: real Stellar RPC connectivity, real escrow-contract read (proving the configured contract address is actually reachable and correct), and real upstream chess-API reachability
- Design a health-check response schema that distinguishes degraded-but-functional from fully-down, per dependency, not a single boolean
- Wire the resulting signal into an actual alerting path (not just an HTTP endpoint nobody polls) — integrate with whatever monitoring exists in `docs/repository-health-checklist.md` or specify what's needed
- Add a chaos/fault-injection test suite that kills each dependency (RPC, contract, chess API) independently and proves the health check correctly reflects each failure mode
- Add a synthetic end-to-end canary check that periodically exercises the real fetch→verify→submit pipeline (Issue #7) against a known test match, proving liveness beyond mere connectivity
- Add regression tests proving the old hardcoded-response behavior cannot silently return (i.e. the health check must fail if RPC is actually unreachable)
- Document the monitoring architecture and alerting runbook alongside `docs/runbook-pause.md` and `docs/runbook-rotation.md`

---

## Performance

### 10. Gas-Bounded On-Chain Indexing Replacing O(n) Active-Match & Player-History Scans
**Labels:** `storage`, `performance`, `enhancement`
**Priority:** High
**Estimated Time:** 50-90 hours

**Description:**
`ActiveMatches` (`contracts/escrow/src/lib.rs:1221-1241`) is a single `Vec<u64>` fully re-read and rewritten on every activating `deposit` and every `submit_result`/`finalize_match`/`resolve_dispute_by_vote` — `docs/performance-report.md:71-85` already documents that an attacker opening many self-funded matches inflates this list and degrades every other player's transaction cost, with no cap or fix in the code. `collect_matches_by_state` (`lib.rs:2143-2167`), backing `get_pending_matches`/`get_active_matches`/`get_live_matches`, iterates the entire match history on every call regardless of total size (`docs/performance-report.md:87-100`), and the unbounded variants remain publicly callable alongside the paginated ones rather than being deprecated. Separately, `require_player_tier_for_stake` → `completed_match_count` (`lib.rs:1140-1195`) walks a player's entire match history on **every** `create_match` and `deposit` call — an unaddressed cost-growth vector the existing benchmark suite never measures at realistic per-player history sizes.

**Tasks:**
- Replace the flat `ActiveMatches` vector with a properly indexed on-chain structure giving O(log n) or O(1) insert/remove, with a documented, enforced upper bound on simultaneously-open matches per address to close the inflation attack
- Replace or cap the unbounded `collect_matches_by_state` scans; either deprecate them in favor of the paginated variants with a hard migration deadline, or bound their per-call cost with a documented ceiling
- Maintain an incrementally-updated per-player completed-match counter (updated once at completion) instead of recomputing `completed_match_count` by full history walk on every stake-gated call
- Preserve all externally-observable query results exactly (same matches returned, same ordering guarantees) — this is an internal data-structure change, not a behavior change
- Add gas benchmarks at 100/1,000/10,000/100,000 total matches and 10/100/1,000 matches-per-player, proving flat or logarithmic cost growth where the current code is linear
- Add a regression test reproducing the documented list-inflation attack, proving the new structure's per-call cost no longer scales with attacker-created match count
- Update `docs/performance-report.md` with the new measured complexity and remove the now-resolved "known issue" entries it currently documents

---

### 11. Exactly-Once, Reorg-Safe Event Ingestion Pipeline
**Labels:** `events`, `performance`, `bug`, `storage`
**Priority:** Critical
**Estimated Time:** 50-90 hours

**Description:**
`parse_event` (`services/event-indexer/src/rpc.rs:159-221`) assigns each event a **random** `id: Uuid::new_v4()` (line 206) instead of a deterministic key derived from `(ledger, tx hash, event index)`. `poll_events`/`event_poller` (`rpc.rs:97-157`) sets `last_ledger` to the just-processed batch's max ledger and passes that same value back as the next call's inclusive `start_ledger` (lines 112-114, 134). `db.insert_event` (`db.rs:56-59`) uses `INSERT OR REPLACE` keyed on that random `id`, which can never deduplicate a re-fetched event since its key differs every time it's re-ingested. Net effect: **every event on the most-recently-seen ledger is re-ingested as a brand-new row on every single poll interval**, forever — an unbounded, silent data-integrity bug in what's supposed to be the platform's authoritative off-chain read model. This is a genuinely hard exactly-once-semantics-over-an-at-least-once-source problem, not a one-line fix, and its own integration tests (`services/event-indexer/tests/integration_tests.rs:72-89`) are literal no-ops (`assert!(true, "... placeholder")`) that would never have caught it.

**Tasks:**
- Derive a deterministic, stable event ID from `(ledger sequence, transaction hash, event index within tx)` and use it as the true dedup key, replacing the random UUID
- Fix the polling boundary so a re-fetched ledger's events are recognized as already-seen rather than treated as new (correct inclusive/exclusive handling, or an explicit "already ingested" check keyed on the new deterministic ID)
- Handle ledger reorgs explicitly: define and implement what happens when a previously-ingested ledger's contents change (invalidate and re-ingest vs. append-only with supersession — pick one and justify it)
- Prove idempotency formally or via exhaustive property testing: replaying the same ledger range any number of times must produce byte-identical indexed state
- Replace `tests/integration_tests.rs`'s placeholder assertions with real tests exercising `parse_event`, the polling boundary, and `db.insert_event` against a realistic RPC-response fixture, including a repeated-poll-of-the-same-ledger scenario
- Add a migration/backfill plan and tooling to deduplicate any already-corrupted historical data in a live deployment
- Document the ingestion guarantee precisely (exactly-once at the logical-event level, idempotent replay, defined reorg behavior) in a new `services/event-indexer/docs/ingestion-guarantees.md`

---

### 12. Horizontally-Scalable, Leader-Elected, Sharded Event Indexer
**Labels:** `events`, `performance`, `devops`, `enhancement`
**Priority:** High
**Estimated Time:** 50-90 hours

**Description:**
`services/event-indexer` is architected as exactly one process: a single SQLite file behind a `Mutex<Connection>` (`db.rs:7-17`), one poller and one API server in one binary (`main.rs`), and `config.rs` exposes no clustering/HA knobs at all. Given Issue #11's ingestion bug, running multiple naive replicas today would only multiply the corruption rate — this issue assumes #11 is fixed and builds real horizontal scalability on top of a now-idempotent ingestion path. Separately, `EventCache::insert` (`cache.rs:19-25`) evicts via `self.events.iter().next()` — undefined `HashMap` iteration order — despite being treated elsewhere as a bounded cache with sensible eviction semantics.

**Tasks:**
- Replace the single SQLite file with a storage backend safe for concurrent multi-instance access (a proper relational store, or a sharded/partitioned scheme with clear ownership boundaries)
- Add leader election (or partition ownership) so multiple indexer instances can run concurrently without duplicate or conflicting ingestion, building on Issue #11's idempotency guarantee as the correctness backstop, not a replacement for it
- Replace the arbitrary cache eviction with a genuine LRU (or documented alternative) policy, with a test proving eviction order is deterministic and correct under a controlled access sequence
- Add horizontal read scaling for `api.rs`'s query endpoints against the shared backing store
- Add a multi-instance integration test proving no duplicate ingestion and correct failover if the current leader/owner dies mid-poll
- Add a load test at a stated target (e.g. sustained ingestion + concurrent query load across N instances) with measured latency/throughput
- Document the scaling architecture, failure modes, and operational runbook for adding/removing instances

---

## Database Design

### 13. Cryptographic Balance-Privacy Redesign: Commitment-Based Disclosure Distinguishing "Pruned" from "Zero"
**Labels:** `security`, `storage`, `enhancement`
**Priority:** Medium
**Estimated Time:** 40-70 hours

**Description:**
`get_balance_snapshots`/`get_latest_snapshot` (`contracts/escrow/src/lib.rs:1916-1994`) redact `stake_amount`/`escrow_balance` to `0` for non-admins but still expose `player1_deposited`/`player2_deposited`/`ledger`/`token`/`token_symbol` — an observer can correlate these against the token contract's own public balance history to reconstruct the amounts the redaction was meant to hide. Separately, the player-balance ring buffer (`get_balance_at_timestamp`, `lib.rs:2092-2141`, docstring `2080-2091`, capacity 32 via `MAX_PLAYER_SNAPSHOTS`) returns `0` both when a player genuinely never had a snapshot **and** when the ring buffer has pruned away the requested history — indistinguishable outcomes, a real problem if this data is ever used as dispute or compliance evidence.

**Tasks:**
- Replace the naive zero-redaction with a real cryptographic commitment scheme (e.g. a Pedersen-style commitment or hash-commitment to the true balance) so non-admins see a verifiable commitment rather than a value correlatable via side-channel metadata
- Remove or generalize the remaining correlatable fields (`player1_deposited`/`player2_deposited`/etc.) so redaction is actually effective, not partial
- Add a distinguishable "data unavailable (pruned)" signal separate from a genuine zero balance in `get_balance_at_timestamp`, changing its return type/contract as needed
- Consider (and justify the decision either way) increasing `MAX_PLAYER_SNAPSHOTS` or adding an off-chain archival path for pruned history that preserves auditability without unbounded on-chain storage growth
- Add a test proving an observer cannot reconstruct a redacted balance from the remaining exposed fields plus public token-transfer history
- Add a test proving pruned-vs-zero is now distinguishable
- Document the privacy model and its precise guarantees/non-guarantees in `docs/privacy-model.md`

---

### 14. Rebuild the Broken Escrow Test Suite + Adversarial Economic-Security Attack Simulation Harness
**Labels:** `testing`, `security`, `high`
**Priority:** High
**Estimated Time:** 50-90 hours

**Description:**
Commit `b922b8b` ("fix: complete escrow contract compilation and resolve build issues") reworked `contracts/escrow/src/{lib,types}.rs` (adding dispute/tier/multi-token/vesting fields) but did not update the test suite to match — `tests/mod.rs:51-53` and `tests/lifecycle.rs:2136-2138,2151-2153` still call `update_protocol_config` with a shape that doesn't match the real `ProtocolConfig` (`types.rs:38-44`), meaning **none** of the ~230 existing tests currently run, including whichever of them might have caught Issue #1's double-payout bug. Beyond simply restoring compilation, this codebase's dispute-voting (Issue #3) and multi-oracle (Issue #5) economics have never been quantified: there is no cost-of-attack model for vote manipulation, oracle collusion, or match-tier gaming anywhere in the repository.

**Tasks:**
- Fix every compilation break in the test suite so all existing tests actually build and run against the current contract API, without silently weakening any assertion to make it pass
- Triage every test that now fails for real (as opposed to failing to compile) and determine whether it's revealing a genuine regression (like Issue #1) or needs updating for intentional behavior changes — document each decision
- Build a parameterized adversarial-cost simulation: given a slice of stake distribution, dispute-vote-manipulation cost (pre- and post-Issue-#3 fix), and oracle-collusion cost (pre- and post-Issue-#5 fix), compute what an attacker would need to spend to force an incorrect payout
- Validate the model with Monte Carlo simulation across representative configurations (small/large stake pools, few/many oracles)
- Add a queryable benchmark report (not just pass/fail) showing before/after attack-cost deltas once Issues #3 and #5 land
- Wire the full, now-passing test suite plus the new simulation harness into CI as required gates
- Document the methodology, assumptions, and known limitations in `docs/economic-security-model.md`

---

### 15. Automated Doc-Code Conformance Verification Gate (Spec-Drift CI)
**Labels:** `documentation`, `testing`, `devops`, `enhancement`
**Priority:** Medium
**Estimated Time:** 40-70 hours

**Description:**
Documentation across this repository has drifted materially from the code it describes, in ways that would mislead an integrator or auditor: `docs/security.md`'s "Known Limitations" (lines 185-212) claims a hardcoded ~24-hour timeout, while the real `set_match_timeout` supports a fully configurable `[17,280, 1,555,200]`-ledger range (`lib.rs:28-32, 1270-1291`) and no 24h default exists anywhere; it also claims "No Native Token Support" despite the (currently broken, per Issue #4) multi-token conversion feature existing. `docs/architecture.md`'s transition table is missing 2 of 6 real `MatchState` variants and omits the dispute/voting/pause/tier/multi-token/balance-history surfaces entirely. `docs/roadmap.md` claims multi-token escrow shipped complete in v1.0 with no signal that its payout path is unsound. This is not a one-time doc fix — the goal is a durable mechanism preventing this class of drift from recurring silently, as it evidently already has multiple times.

**Tasks:**
- Correct every drift identified above across `docs/security.md`, `docs/architecture.md`, and `docs/roadmap.md` as a baseline
- Build a machine-checkable conformance mechanism: derive verifiable facts about the contract's actual public interface (function signatures, `MatchState` variants and reachable transitions, configurable-parameter bounds) directly from source, and diff them against claims embedded in the docs
- Where full automated extraction isn't feasible for a given doc claim, add a structured, explicitly-labeled "verified against `<file>:<line>` as of `<commit>`" annotation convention, and a CI check that the cited line still says what the annotation claims
- Wire this as a required CI gate that fails a PR modifying `contracts/escrow/src/{lib,types}.rs` or `contracts/oracle/src/lib.rs` without a corresponding doc-conformance update
- Cross-reference this gate's transition-table check against the formal spec produced in Issue #6, so the two efforts reinforce rather than duplicate each other
- Add a test suite proving the conformance checker itself correctly flags a deliberately-introduced drift (positive control)
- Document the conformance-checking system and its coverage/limitations in `docs/doc-conformance.md`

---
