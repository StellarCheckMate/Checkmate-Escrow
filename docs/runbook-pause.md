# Runbook: Pause & Incident Response

This runbook describes when and how to pause the Checkmate-Escrow contracts, what users experience during a pause, and the steps to investigate and recover from an incident. Keep this document accessible to all on-call operators — speed matters in a live incident.

---

## Table of Contents

- [When to Pause](#when-to-pause)
- [Pause Procedures](#pause-procedures)
  - [Pause the Escrow Contract](#pause-the-escrow-contract)
  - [Pause the Oracle Contract](#pause-the-oracle-contract)
  - [Pause Both Contracts](#pause-both-contracts)
- [User-Facing Effects During a Pause](#user-facing-effects-during-a-pause)
- [Investigation Steps](#investigation-steps)
- [Recovery Procedures](#recovery-procedures)
  - [Unpause After False Alarm or Resolution](#unpause-after-false-alarm-or-resolution)
  - [Rotate Compromised Keys While Paused](#rotate-compromised-keys-while-paused)
- [Rollback & Escalation](#rollback--escalation)
- [Post-Incident Checklist](#post-incident-checklist)
- [Event Reference](#event-reference)

---

## When to Pause

Pause immediately when any of the following is confirmed or suspected:

| Trigger | Contract(s) to pause |
|---|---|
| Oracle submitting incorrect results | Oracle (blocks new results), Escrow (blocks result acceptance) |
| Oracle admin key compromised | Oracle + Escrow |
| Escrow admin key compromised | Escrow |
| Unexpected contract behavior / funds movement anomaly | Both |
| Dependency outage (Lichess/Chess.com API down indefinitely) | Oracle |
| Planned migration or contract upgrade | Both |
| Security audit finding requiring emergency patch | Both |

If in doubt, pause both contracts. A false alarm is recoverable with one `unpause` call; a missed incident is not.

---

## Pause Procedures

### Pause the Escrow Contract

**Who can execute:** Escrow admin only.

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

Verify:

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --network <network> \
  -- is_paused
# Expected output: true
```

---

### Pause the Oracle Contract

**Who can execute:** Oracle admin only.

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

Verify:

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --network <network> \
  -- is_paused
# Expected output: true
```

---

### Pause Both Contracts

Submit both pause calls. They are independent and can be submitted in rapid succession:

```bash
# Pause escrow
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause

# Pause oracle
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

> In a severe incident, pause escrow first — it controls fund movement. Oracle can follow immediately after.

---

## User-Facing Effects During a Pause

### Escrow Contract Paused

| Operation | Status | User impact |
|---|---|---|
| `create_match` | **Blocked** (error: `ContractPaused`) | Players cannot open new matches |
| `deposit` | **Blocked** (error: `ContractPaused`) | Players cannot fund pending matches |
| `submit_result` | **Blocked** (error: `ContractPaused`) | Completed games do not resolve; stakes remain locked |
| `get_match` | **Allowed** | Players can still read match state |
| `cancel_match` | **Allowed** | Players can cancel and recover stakes from pending (unfunded) matches |
| `expire_match` | **Allowed** | Timed-out matches can still be expired and stakes returned |

Active matches (both players deposited, game in progress) will remain locked until the contract is unpaused. Players should be notified via your status page that payouts are temporarily delayed.

### Oracle Contract Paused

| Operation | Status | User impact |
|---|---|---|
| `submit_result` | **Blocked** (error: `ContractPaused`) | No new match results can be recorded |
| `delete_result` | **Blocked** (error: `ContractPaused`) | Existing results cannot be removed |
| `has_result` | **Allowed** | Result existence checks still work |
| `get_result` | **Allowed** | Stored results are still readable |

Pausing the oracle alone does not block the escrow contract. Active matches whose results are already stored in the oracle can still be resolved on escrow unless escrow is also paused.

---

## Investigation Steps

Once paused, work through these steps while the contracts are held in safe state.

### 1. Establish a timeline

```bash
# Stream recent contract events to identify first anomalous event
stellar events \
  --contract-id $ESCROW_CONTRACT_ID \
  --network <network> \
  --start-ledger <LEDGER_BEFORE_INCIDENT>

stellar events \
  --contract-id $ORACLE_CONTRACT_ID \
  --network <network> \
  --start-ledger <LEDGER_BEFORE_INCIDENT>
```

Key events to look for: `(oracle, result)`, `(match, completed)`, `(admin, xfer)`, `(admin, admin_rot)`, `(admin, oracle_up)`.

### 2. Audit current contract state

```bash
# Confirm current admin and oracle
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_admin
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_oracle
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- get_admin

# Spot-check affected matches
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --network <network> \
  -- get_match --match_id <MATCH_ID>

# Spot-check oracle results
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --network <network> \
  -- get_result --match_id <MATCH_ID>
```

### 3. Categorize the incident

| Category | Indicators | Next step |
|---|---|---|
| False alarm | No unauthorized events, state matches expected | [Unpause after false alarm](#unpause-after-false-alarm-or-resolution) |
| Oracle key compromise | `admin.admin_rot` or `oracle, result` events from unexpected address | [Rotate oracle keys while paused](#rotate-compromised-keys-while-paused) |
| Escrow admin key compromise | `admin.xfer`, `admin.oracle_up`, or `add_allowed_token` from unexpected address | [Rotate escrow admin while paused](#rotate-compromised-keys-while-paused) |
| Incorrect match results | `match.completed` events with wrong winner; oracle submitted wrong data | Rotate oracle, dispute affected matches, consider contract migration |
| Systemic bug | Multiple incorrect states, unexpected error patterns | Keep paused, escalate to contract developers |

---

## Recovery Procedures

### Unpause After False Alarm or Resolution

Only unpause when the incident is fully resolved and the root cause is understood.

**Unpause escrow:**

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- unpause
```

**Unpause oracle:**

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- unpause
```

Verify both contracts are no longer paused:

```bash
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- is_paused
# Expected: false
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- is_paused
# Expected: false
```

Confirm end-to-end functionality with a test match before announcing recovery.

---

### Rotate Compromised Keys While Paused

If a key compromise is confirmed, rotate the affected key before unpausing. The contracts remain paused throughout this process.

**Rotate the oracle address (escrow admin key must be safe):**

See [runbook-rotation.md — Rotate the Oracle Address](runbook-rotation.md#rotate-the-oracle-address-escrow). Skip the pause step — the contract is already paused. Unpause only after verifying the new oracle is functional.

**Rotate the escrow admin (requires current escrow admin key — use the recovery path if it's compromised):**

See [runbook-rotation.md — Rotate the Escrow Admin](runbook-rotation.md#rotate-the-escrow-admin-two-step).

**Rotate the oracle contract admin:**

See [runbook-rotation.md — Rotate the Oracle Contract Admin](runbook-rotation.md#rotate-the-oracle-contract-admin-direct). Skip the pause step.

> If the escrow admin key is lost or compromised and no recovery admin exists, there is no on-chain path to recover admin authority. Plan for this by maintaining a pre-provisioned recovery address in a cold-storage multi-sig before incident.

---

## Rollback & Escalation

### Accidental unpause

If a contract is unpaused prematurely, re-pause immediately:

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

There is no side effect from multiple pause/unpause cycles. The state is a simple boolean in instance storage.

### Escalation path

| Severity | Action |
|---|---|
| Minor (single bad result, contained) | Pause oracle, rotate oracle admin, document the result, unpause |
| Moderate (funds at risk but no loss yet) | Pause both contracts, engage security team, rotate keys |
| Severe (active fund loss, admin compromise) | Pause both contracts, do not unpause, engage contract developers for migration assessment |

---

## Post-Incident Checklist

Complete this checklist before marking the incident resolved:

- [ ] Root cause identified and documented
- [ ] Affected match states audited and accounted for
- [ ] Compromised key(s) rotated and old key(s) revoked from all systems
- [ ] Both contracts verified unpaused (if recovery is complete)
- [ ] `admin.paused` and `admin.unpaused` events confirmed in ledger
- [ ] Affected users notified with accurate timeline and outcome
- [ ] Monitoring alerts updated to catch the same pattern earlier
- [ ] Post-mortem written and shared with the team
- [ ] Key management procedures reviewed and improved if gaps found

---

## Event Reference

Events emitted during pause operations:

| Event topic | Contract | Data | Meaning |
|---|---|---|---|
| `(admin, paused)` | Either | — | Contract paused |
| `(admin, unpaused)` | Either | — | Contract unpaused |

Events to monitor for incident detection:

| Event topic | Contract | Why it matters |
|---|---|---|
| `(admin, oracle_up)` | Escrow | Oracle address changed — expected only during planned rotation |
| `(admin, xfer)` | Escrow | Admin transferred — expected only during planned rotation |
| `(admin, admin_rot)` | Oracle | Oracle admin changed — expected only during planned rotation |
| `(oracle, result)` | Oracle | Result submitted — verify submitter matches expected oracle admin |
| `(match, completed)` | Escrow | Payout executed — verify winner matches platform result |
