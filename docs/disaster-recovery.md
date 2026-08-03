# Disaster Recovery — Checkmate-Escrow

This document describes how to back up and restore Checkmate-Escrow contract
state, and what to do when the worst happens.

---

## Table of Contents

1. [Overview](#overview)
2. [What Is Backed Up](#what-is-backed-up)
3. [What Cannot Be Recovered Automatically](#what-cannot-be-recovered-automatically)
4. [Running a Backup](#running-a-backup)
5. [Automated Scheduled Backups](#automated-scheduled-backups)
6. [Restoring State](#restoring-state)
7. [In-Flight Match Recovery Playbook](#in-flight-match-recovery-playbook)
8. [Step-by-Step Disaster Recovery Runbook](#step-by-step-disaster-recovery-runbook)
9. [Secrets and Required Configuration](#secrets-and-required-configuration)
10. [Testing the Backup/Restore Cycle](#testing-the-backuprestore-cycle)
11. [Limitations](#limitations)

---

## Overview

Soroban smart contracts store all state on-chain; there is no off-chain
database to back up. However, contract state can become inaccessible if:

- The contract is upgraded to a broken WASM and `migrate_state` cannot run.
- The deployer loses admin credentials, making recovery functions unreachable.
- A network incident requires re-deploying to a fresh contract address.

The backup strategy exports all observable contract state to a JSON snapshot
file. The restore script replays that config into a new, freshly-deployed
contract. Players with in-flight matches receive refunds through an out-of-band
process described below.

---

## What Is Backed Up

`scripts/backup_state.sh` captures the following from the live escrow contract:

| Category | Fields |
|---|---|
| Config | `admin`, `oracle`, `paused`, `match_timeout`, `contract_version`, `dispute_period` |
| Protocol config | `vesting_duration_seconds`, `cancellation_fee_basis_points`, `treasury`, `stablecoin_only_mode` |
| Token allowlist | All addresses added via `add_allowed_token` |
| Fee tiers | All `FeeTier` entries set via `set_fee_tiers` |
| Active matches | Full `Match` struct including state, players, stakes, token, platform, game\_id |
| Pending matches | Same as above |
| Escrow balances | `get_escrow_balance` per match at snapshot time |

---

## What Cannot Be Recovered Automatically

The following state cannot be replayed without player signatures:

- **Player deposits** — restoring escrow balances requires re-depositing tokens,
  which requires the player's wallet signature. These must be refunded via the
  process in [In-Flight Match Recovery Playbook](#in-flight-match-recovery-playbook).
- **Oracle records** — `OracleRecord` entries stored for completed matches are
  read-only audit data; they do not affect active match logic.
- **Balance snapshots / ring buffers** — historical `BalanceSnapshot` and
  `PlayerBalanceSnapshot` entries are audit data and do not block recovery.
- **Dispute state** — active disputes require re-opening with original evidence
  hashes; this must be done manually after recovery.
- **Completed matches** — these are terminal; their payout was already executed.

---

## Running a Backup

### Prerequisites

- `stellar` CLI installed and configured for the target network.
- `jq` installed.
- `CONTRACT_ESCROW` set (or in `.env`).

### Manual backup

```bash
# One-time backup to the default backups/ directory
./scripts/backup_state.sh testnet

# Custom output directory
./scripts/backup_state.sh testnet /var/backups/checkmate

# Mainnet
./scripts/backup_state.sh mainnet
```

The script writes a file like:

```
backups/escrow-snapshot-testnet-20260101T020000Z.json
```

### Uploading to S3

Set `S3_BUCKET` before running the script and ensure AWS credentials are
available in the environment:

```bash
export S3_BUCKET=s3://my-org-backups/checkmate-escrow
./scripts/backup_state.sh mainnet
```

---

## Automated Scheduled Backups

The workflow `.github/workflows/backup.yml` runs `backup_state.sh` daily at
**02:00 UTC** for testnet. Mainnet backups are triggered manually via the
GitHub Actions UI (`workflow_dispatch`).

### Required GitHub Secrets

| Secret | Description |
|---|---|
| `TESTNET_CONTRACT_ESCROW` | Testnet escrow contract ID |
| `TESTNET_DEPLOYER_KEYPAIR` | Stellar keypair name for testnet reads |
| `MAINNET_CONTRACT_ESCROW` | Mainnet escrow contract ID |
| `MAINNET_DEPLOYER_KEYPAIR` | Stellar keypair name for mainnet reads |
| `BACKUP_S3_BUCKET` | (optional) S3 URI for off-site storage |
| `AWS_ACCESS_KEY_ID` | (optional) AWS credentials for S3 upload |
| `AWS_SECRET_ACCESS_KEY` | (optional) AWS credentials for S3 upload |
| `AWS_REGION` | (optional) AWS region, default `us-east-1` |

Snapshots are stored as GitHub Actions artifacts for 30 days (testnet) or 90
days (mainnet) as a fallback when S3 is not configured.

---

## Restoring State

### Step 1 — Deploy a new contract

```bash
# Deploy fresh contracts to the same network
./scripts/deploy.sh testnet
# Note the new CONTRACT_ESCROW and CONTRACT_ORACLE values
```

### Step 2 — Run the restore script

```bash
export CONTRACT_ESCROW=<new_contract_id>
export CONTRACT_ORACLE=<new_oracle_id>

./scripts/restore_state.sh backups/escrow-snapshot-testnet-20260101T020000Z.json testnet
```

The script will:

1. Restore protocol config (`vesting_duration_seconds`, `cancellation_fee_basis_points`, `treasury`, `stablecoin_only_mode`).
2. Restore match timeout.
3. Re-add all allowed tokens.
4. Restore fee tiers.
5. Print a report of all in-flight matches requiring manual intervention.

### Step 3 — Verify the restore

```bash
# Verify admin
stellar contract invoke --id $CONTRACT_ESCROW --network testnet -- get_admin

# Verify oracle
stellar contract invoke --id $CONTRACT_ESCROW --network testnet -- get_oracle

# Verify protocol config
stellar contract invoke --id $CONTRACT_ESCROW --network testnet -- get_protocol_config

# Verify allowed tokens
stellar contract invoke --id $CONTRACT_ESCROW --network testnet -- get_allowed_tokens
```

### Step 4 — Update the oracle service

Set `CONTRACT_ESCROW` in the oracle-service environment or `.env` to point at
the new contract ID, then redeploy or restart the oracle service.

---

## In-Flight Match Recovery Playbook

For each match reported in the restore script output, do the following:

1. **Contact both players** via the platform (Lichess/Chess.com) message system
   and your application's notification system.

2. **Determine match status** from the chess platform:
   - If the game is still in progress: ask players to re-create the match on
     the new contract once they have completed the game.
   - If the game has a result: use the oracle to submit the result on the new
     contract so the payout is executed.
   - If the game is abandoned: both players should request a refund.

3. **Token refunds for deposits already made on the old contract**: these tokens
   are still locked in the old contract. If the old contract is still accessible
   (not destroyed), call `expire_match` once the timeout elapses. If the old
   contract is inaccessible, file a support escalation — see Security in
   [docs/security.md](security.md).

4. **Re-create the match** on the new contract if both players agree to continue:
   ```bash
   stellar contract invoke --id $NEW_CONTRACT_ESCROW --network testnet \
     -- create_match \
     --player1 <player1_address> \
     --player2 <player2_address> \
     --stake_amount <amount> \
     --token <token_address> \
     --game_id <lichess_or_chessdotcom_game_id> \
     --platform Lichess
   ```

---

## Step-by-Step Disaster Recovery Runbook

> Use this runbook when the escrow contract is unresponsive or produces
> unexpected errors.

```
1. Confirm the incident
   ─────────────────────────────────────────────────────────────────────
   stellar contract invoke --id $CONTRACT_ESCROW --network mainnet -- get_admin
   # Expected: admin address. If this panics, the contract may be corrupted.

2. Pause the contract (if still reachable)
   ─────────────────────────────────────────────────────────────────────
   stellar contract invoke --id $CONTRACT_ESCROW --source $DEPLOYER_KEYPAIR \
     --network mainnet -- pause
   # This prevents new deposits while recovery is in progress.

3. Download the most recent backup
   ─────────────────────────────────────────────────────────────────────
   # From GitHub Actions artifacts or S3:
   aws s3 ls s3://$BACKUP_BUCKET/escrow-snapshot-mainnet- | sort | tail -1
   aws s3 cp s3://$BACKUP_BUCKET/<latest_snapshot> ./recovery-snapshot.json

4. Deploy a new contract
   ─────────────────────────────────────────────────────────────────────
   ./scripts/deploy.sh mainnet
   # Record the new CONTRACT_ESCROW and CONTRACT_ORACLE values.

5. Restore config
   ─────────────────────────────────────────────────────────────────────
   export CONTRACT_ESCROW=<new_escrow_id>
   ./scripts/restore_state.sh ./recovery-snapshot.json mainnet

6. Verify (see Restoring State § Step 3)

7. Update oracle service to point at the new contract

8. Handle in-flight matches (see In-Flight Match Recovery Playbook)

9. Announce the new contract address to players and integrators

10. Monitor for 24 hours
    ─────────────────────────────────────────────────────────────────────
    Watch for unexpected match state transitions or balance discrepancies.
```

---

## Secrets and Required Configuration

The scripts do not store credentials. All sensitive values must be supplied via
environment variables or the `.env` file (never committed to version control):

| Variable | Used by | Description |
|---|---|---|
| `CONTRACT_ESCROW` | Both scripts | Deployed escrow contract ID |
| `DEPLOYER_KEYPAIR` | Both scripts | Stellar keypair name for admin calls |
| `S3_BUCKET` | `backup_state.sh` | S3 URI for off-site upload (optional) |
| `BACKUP_DIR` | `backup_state.sh` | Local output directory (default: `backups/`) |
| `BACKUP_RETENTION_DAYS` | `backup_state.sh` | Days before local pruning (default: 30) |
| `STELLAR_NETWORK` | Both scripts | `testnet` / `mainnet` (default: `testnet`) |

---

## Testing the Backup/Restore Cycle

Run a full backup → restore drill on testnet at least once per month:

```bash
# 1. Back up current testnet state
./scripts/backup_state.sh testnet

# 2. Deploy a fresh testnet contract
./scripts/deploy.sh testnet  # save new contract IDs

# 3. Restore into the new contract
export CONTRACT_ESCROW=<new_contract_id>
./scripts/restore_state.sh backups/escrow-snapshot-testnet-<timestamp>.json testnet

# 4. Verify the restore (see Restoring State § Step 3)
```

Record the date of each drill. If a drill fails, open an issue before the next
scheduled backup window.

---

## Limitations

- **Player deposits cannot be replayed automatically** — they require the
  original player wallet signatures. See the playbook above.
- **Completed matches are terminal** — their payouts have already settled;
  nothing to restore.
- **Oracle records are audit-only** — they are not needed for operational
  recovery but are lost if the old contract is destroyed.
- **Snapshot accuracy** — the snapshot reflects state at the moment the backup
  ran. Any deposits or match state changes that occur between the last backup
  and a disaster are not captured.

Mitigation: increase backup frequency (e.g. every 6 hours) and consider
running event-indexer alongside backups for finer granularity — see
[docs/EVENT_INDEXER_API.md](EVENT_INDEXER_API.md).
