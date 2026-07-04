# Emergency Contract Pause And Recovery Runbook

This runbook gives operators a cold-start procedure for pausing the escrow contract, communicating with users, identifying affected matches, recovering funds, and deciding whether to unpause, roll back, or upgrade after an incident.

Use it for testnet drills and production incidents. Replace placeholder contract IDs, account names, and entrypoint names with the values from the current deployment before running commands.

## Incident Roles

- Incident commander: owns the decision to pause, unpause, upgrade, or recover funds.
- Contract operator: runs Stellar CLI commands from the approved admin account.
- Backend operator: disables unsafe API paths, captures logs, and exports affected match IDs.
- Communications owner: posts user-facing updates and support instructions.
- Reviewer: confirms each high-risk command before it is submitted.

## Prerequisites

- Stellar CLI installed and authenticated with an approved operator key.
- Current network, RPC URL, passphrase, contract ID, and admin public key.
- Access to backend logs, database read replicas, and monitoring dashboards.
- A tested backup of the current contract WASM and deployment metadata.
- A private incident channel for command review and audit notes.

Recommended environment:

    export STELLAR_NETWORK=testnet
    export STELLAR_RPC_URL=https://soroban-testnet.stellar.org
    export STELLAR_PASSPHRASE="Test SDF Network ; September 2015"
    export CHECKMATE_CONTRACT_ID=<contract-id>
    export CHECKMATE_ADMIN=admin

For mainnet, use the mainnet RPC endpoint and passphrase from the release runbook. Never paste mainnet secret keys into shared chat or issue comments.

## Decision Tree

Pause the contract immediately when any of these are true:

- Funds can move to an unintended account.
- Match settlement can be manipulated.
- A privileged key is suspected to be compromised.
- The backend is submitting malformed or duplicated settlement requests.
- An exploit is active or reproducible.

Prefer a contract upgrade instead of a pause when all of these are true:

- Funds are not at immediate risk.
- The bug is fully understood.
- The fixed WASM is already reviewed and ready.
- Users can keep using unaffected flows safely.

Prefer monitoring only when the issue is cosmetic, off-chain only, or already blocked by existing validation.

## 1. Start The Incident

1. Open an incident record with the time, reporter, affected environment, and suspected impact.
2. Assign the roles above.
3. Freeze non-essential deployments.
4. Save current contract metadata:

    stellar contract inspect --id "$CHECKMATE_CONTRACT_ID" --network "$STELLAR_NETWORK"

5. Capture current ledger height, backend version, frontend version, and indexer cursor.

## 2. Pause The Contract

Have a second operator review the command before submission.

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      pause

If the deployed contract uses a different emergency entrypoint, use the project-specific name, for example set_paused --paused true.

Confirm paused state:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      is_paused

Expected result: paused state is true, and settlement or match-mutating calls fail with the documented paused error.

## 3. Notify Users

Within 15 minutes of a production pause, publish a short status update.

Initial template:

    We have temporarily paused Checkmate Escrow while we investigate an issue affecting contract operations. Existing funds remain under review and we are not asking users to take action right now. We will post the next update by <time UTC>.

Update template:

    Update: the contract remains paused while we validate affected matches and recovery steps. We have identified <count> potentially affected matches. Next update by <time UTC>.

Resolution template:

    Resolved: Checkmate Escrow has completed recovery and normal operation has resumed. Affected users have been contacted with match-specific details. Thank you for your patience.

Do not share exploit details, private keys, raw user data, or unreviewed recovery instructions in public channels.

## 4. Identify Affected Matches

Export candidate matches from the backend or indexer for the incident window.

    psql "$DATABASE_URL" \
      --csv \
      -c "select id, creator, opponent, escrow_amount, status, updated_at from matches where updated_at >= '<incident-start-utc>' order by updated_at asc;" \
      > affected-matches.csv

For each match, record:

- Match ID.
- User accounts.
- Escrowed amount and asset.
- Last safe state.
- Last transaction hash.
- Expected destination account.
- Recovery action: no action, refund, settle, manual review.

Cross-check exported rows against contract reads:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      get_match \
      --match_id <match-id>

Mark any mismatch between backend state and contract state as manual review.

## 5. Recover Funds

Only recover funds after the affected-match list is reviewed by the incident commander and reviewer.

Refund a match when neither side can be safely settled:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      emergency_refund \
      --match_id <match-id> \
      --recipient <account>

Settle a match when the correct winner is known and documented:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      emergency_settle \
      --match_id <match-id> \
      --winner <account>

If the current contract does not expose emergency recovery entrypoints, keep the contract paused and move to the upgrade procedure. Do not attempt ad-hoc recovery through unsupported paths.

After each recovery transaction:

- Save the transaction hash.
- Re-read the match state.
- Confirm destination balances changed as expected.
- Add the result to the incident record.

## 6. Rollback Procedures

Rollback is appropriate for backend or frontend releases that triggered invalid contract calls.

1. Keep the contract paused.
2. Revert the backend or frontend deployment to the last known good version.
3. Disable scheduled jobs that could replay unsafe actions.
4. Replay only read-only checks until the contract state is confirmed.
5. Resume write traffic only after the contract is unpaused.

If rollback cannot restore safe behavior, leave the contract paused and proceed with a contract upgrade.

## 7. Upgrade The Contract

Use this path when the emergency requires patched contract code.

Build and review the patched WASM:

    cargo build --release --target wasm32-unknown-unknown

Upload the WASM:

    stellar contract upload \
      --wasm target/wasm32-unknown-unknown/release/checkmate_escrow.wasm \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK"

Invoke the upgrade entrypoint:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      upgrade \
      --wasm_hash <uploaded-wasm-hash>

Run smoke checks before unpausing:

- Paused state can be read.
- Affected match reads return expected state.
- Unauthorized accounts cannot invoke admin-only recovery paths.
- Normal settlement works in a test match after unpause on staging or testnet.

## 8. Unpause And Resume Service

Unpause only after recovery, rollback, or upgrade has been verified.

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      unpause

Confirm state:

    stellar contract invoke \
      --id "$CHECKMATE_CONTRACT_ID" \
      --source "$CHECKMATE_ADMIN" \
      --network "$STELLAR_NETWORK" \
      -- \
      is_paused

Then re-enable backend workers, frontend actions, and monitoring alerts.

## 9. Post-Incident Checklist

- Incident record includes all commands, reviewers, transaction hashes, and timestamps.
- All affected matches have a final state.
- Users with recovered or refunded funds were notified.
- Monitoring confirms no repeated failures for at least one hour.
- Root cause and follow-up issues are filed.
- This runbook is updated with any missing command or decision point discovered during the incident.
