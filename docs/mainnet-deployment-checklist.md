# Mainnet Deployment Checklist

This checklist must be completed in full — and signed off by the responsible parties — before any Checkmate-Escrow deployment goes live on Stellar mainnet. Work through each section sequentially. Do not skip items; mark them `[N/A]` with a justification note if genuinely not applicable.

> **How to use this checklist**
> 1. Copy this file to a private deployment log (e.g. a confidential issue, Notion page, or internal wiki).
> 2. Check off each item as you complete it.
> 3. Record the signer, date, and any relevant artefact (tx hash, commit SHA, audit report link) next to each sign-off block.
> 4. Do not proceed to the next section until the current section is fully checked.

---

## Table of Contents

1. [Pre-Launch: Security Audit](#1-pre-launch-security-audit)
2. [Key Management & Access Control](#2-key-management--access-control)
3. [Contract Build & Verification](#3-contract-build--verification)
4. [Oracle Setup & Rate Limiting](#4-oracle-setup--rate-limiting)
5. [Token Allowlist Configuration](#5-token-allowlist-configuration)
6. [Pre-Launch Testing](#6-pre-launch-testing)
7. [Deployment Execution](#7-deployment-execution)
8. [Post-Deployment Verification](#8-post-deployment-verification)
9. [Post-Launch Monitoring](#9-post-launch-monitoring)
10. [Rollback Procedures](#10-rollback-procedures)
11. [Sign-Off](#11-sign-off)

---

## 1. Pre-Launch: Security Audit

All items in this section must be complete before the deployment window opens.

- [ ] An independent security audit of `contracts/escrow/src` and `contracts/oracle/src` has been completed by a qualified third-party auditor.
- [ ] The audit report has been reviewed by the lead developer and all **critical** and **high** findings have been remediated or explicitly accepted with written justification.
- [ ] Audit findings and mitigations are documented in the deployment log with a link to the audit report.
- [ ] The internal [Security Audit Checklist](SECURITY_AUDIT_CHECKLIST.md) has been completed and all items are checked.
- [ ] Formal verification checks have passed: `./scripts/check_tla.sh` exits 0.
- [ ] `cargo deny check` passes with no unresolved advisories.
- [ ] No `TODO`, `FIXME`, or `HACK` comments remain in production contract code (`contracts/`).
- [ ] All dependency versions in `Cargo.lock` have been reviewed for known CVEs (run `cargo audit`).
- [ ] The [Threat Model & Security](security.md) document is up to date and reflects the final architecture.

**Audit sign-off**

| Role | Name | Date | Artefact |
|------|------|------|----------|
| Lead Developer | | | |
| Security Auditor | | | Audit report URL: |

---

## 2. Key Management & Access Control

- [ ] A dedicated **deployer keypair** has been generated for the mainnet deployment and is stored in a hardware security module (HSM) or hardware wallet (Ledger / Trezor).
- [ ] The deployer keypair is **not** the same key as the admin or oracle operational keypairs.
- [ ] The **escrow admin keypair** is stored in the HSM / hardware wallet with restricted access (minimum two keyholders required for use).
- [ ] The **oracle operational keypair** is stored in a secrets manager (e.g. AWS Secrets Manager, HashiCorp Vault) and rotated at least every 90 days. Rotation schedule is documented.
- [ ] Key access is limited to named, trusted individuals. A key-access registry is maintained and current.
- [ ] A **key rotation runbook** exists and has been tested on testnet. See [runbook-rotation.md](runbook-rotation.md).
- [ ] Emergency contact list for keyholders is documented and accessible to all signers.
- [ ] Deployer keypair will be zeroed / archived after contract initialisation is confirmed.

---

## 3. Contract Build & Verification

- [ ] The release build has been produced from a clean, tagged Git commit on `main`:
  ```bash
  git tag v1.0.0 && git push origin v1.0.0
  ./scripts/build.sh
  ```
- [ ] The WASM bytecodes are deterministically reproducible: a second build from the same commit produces byte-identical WASM files.
- [ ] WASM checksums have been recorded:
  - `escrow.wasm` SHA-256: `______________________________________________________`
  - `oracle.wasm` SHA-256: `______________________________________________________`
- [ ] The checksums above match the files uploaded to mainnet (verify with `stellar contract inspect`).
- [ ] All CI checks pass on the tagged commit (GitHub Actions green).
- [ ] Test coverage meets or exceeds the minimums in [TESTING_GUIDE.md](TESTING_GUIDE.md):
  - Line coverage ≥ 95 %
  - Branch coverage ≥ 90 %

---

## 4. Oracle Setup & Rate Limiting

- [ ] Oracle service is deployed to a production-grade host (not a developer laptop).
- [ ] Oracle service environment variables are sourced from the secrets manager, not from `.env` files committed to version control.
- [ ] `LICHESS_API_TOKEN` is a production token with appropriate rate limits; confirm the Lichess account is in good standing.
- [ ] `CHESSDOTCOM_API_KEY` is a production key (if Chess.com oracle is enabled for this release).
- [ ] Oracle poll interval (`ORACLE_POLL_INTERVAL_SECS`) is set to a value that stays within platform API rate limits:
  - Lichess: free tier allows ~200 requests/minute; recommended `ORACLE_POLL_INTERVAL_SECS=30`.
  - Chess.com: verify your tier's rate limit before setting.
- [ ] HTTP retry logic and exponential back-off are confirmed active in oracle config.
- [ ] Dead-letter queue is configured and monitored for failed submission retries.
- [ ] Oracle service health endpoint (`GET /health`) returns `healthy` in production environment.
- [ ] Oracle Prometheus metrics endpoint (`GET /metrics`) is reachable from the Prometheus scraper.

---

## 5. Token Allowlist Configuration

- [ ] The token allowlist strategy for mainnet has been decided and documented:
  - [ ] **Allowlist active**: specific tokens will be added via `add_allowed_token` before launch.
  - [ ] **Open mode**: allowlist remains inactive (any token accepted). *Note: this is higher risk and requires additional monitoring.*
- [ ] If allowlist mode:
  - [ ] All permitted token contract addresses have been verified on the Stellar mainnet ledger.
  - [ ] XLM native asset contract address for mainnet has been confirmed.
  - [ ] USDC or other stablecoin contract addresses have been confirmed with the issuing organisation.
  - [ ] Test transaction using each allowed token has been executed on testnet with the exact mainnet contract addresses.
- [ ] Match timeout has been set to an appropriate value for mainnet (recommended: 518,400 ledgers / 30 days).

---

## 6. Pre-Launch Testing

Complete all tests on the Stellar **testnet** using configuration as close to production as possible, then repeat the smoke tests on **mainnet** immediately after contract initialisation (before opening to users).

### Testnet Smoke Tests (required before mainnet deploy)

- [ ] Full end-to-end match lifecycle: create → deposit (both players) → submit result → payout verified.
- [ ] Draw scenario: submit draw result → both players refunded correct amounts.
- [ ] Cancel flow: cancel before both deposits → both players refunded.
- [ ] Expire flow: advance past match timeout → `expire_match` executes correctly.
- [ ] Admin pause: pause contract → all write operations rejected → unpause → operations resume.
- [ ] Oracle key rotation: rotate oracle address → new oracle submits result → payout correct.
- [ ] Invalid token rejection: attempt `create_match` with a token not on the allowlist → `InvalidToken` error returned.
- [ ] `submit_result_with_oracle_record` stores correct `game_id` on-chain; verify via `get_oracle_record`.
- [ ] Duplicate `game_id` rejected with `DuplicateGameId`.

### Load & Edge Cases

- [ ] Concurrent deposits from both players in the same ledger handled correctly.
- [ ] Verify no double-payout is possible: second `submit_result` call on a completed match returns `MatchAlreadyCompleted`.

---

## 7. Deployment Execution

Execute these steps in order. Record the transaction hash and contract ID for each step.

- [ ] **Step 1** — Deploy `OracleContract` to mainnet:
  ```bash
  stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
    --source <DEPLOYER_KEYPAIR> \
    --network mainnet
  ```
  Oracle Contract ID: `_____________________________________________`
  Deploy TX Hash:     `_____________________________________________`

- [ ] **Step 2** — Initialize `OracleContract`:
  ```bash
  stellar contract invoke \
    --id $ORACLE_CONTRACT_ID \
    --source <DEPLOYER_KEYPAIR> \
    --network mainnet \
    -- initialize \
    --admin <ORACLE_ADMIN_ADDRESS> \
    --deployer <DEPLOYER_ADDRESS>
  ```
  Init TX Hash: `_____________________________________________`

- [ ] **Step 3** — Deploy `EscrowContract` to mainnet:
  ```bash
  stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
    --source <DEPLOYER_KEYPAIR> \
    --network mainnet
  ```
  Escrow Contract ID: `_____________________________________________`
  Deploy TX Hash:     `_____________________________________________`

- [ ] **Step 4** — Initialize `EscrowContract`:
  ```bash
  stellar contract invoke \
    --id $ESCROW_CONTRACT_ID \
    --source <DEPLOYER_KEYPAIR> \
    --network mainnet \
    -- initialize \
    --oracle $ORACLE_CONTRACT_ID \
    --admin <ESCROW_ADMIN_ADDRESS> \
    --deployer <DEPLOYER_ADDRESS>
  ```
  Init TX Hash: `_____________________________________________`

- [ ] **Step 5** — Add allowed tokens (if allowlist mode is active):
  ```bash
  stellar contract invoke \
    --id $ESCROW_CONTRACT_ID \
    --source <ESCROW_ADMIN_KEYPAIR> \
    --network mainnet \
    -- add_allowed_token \
    --token <TOKEN_CONTRACT_ADDRESS>
  ```
  Confirm with `get_allowed_tokens`; list matches expected tokens: ✓

- [ ] **Step 6** — Set match timeout:
  ```bash
  stellar contract invoke \
    --id $ESCROW_CONTRACT_ID \
    --source <ESCROW_ADMIN_KEYPAIR> \
    --network mainnet \
    -- set_match_timeout \
    --timeout 518400
  ```
  TX Hash: `_____________________________________________`

- [ ] **Step 7** — Update oracle service configuration with the live contract IDs:
  - `CONTRACT_ESCROW=<ESCROW_CONTRACT_ID>`
  - `CONTRACT_ORACLE=<ORACLE_CONTRACT_ID>`
  - `STELLAR_NETWORK=mainnet`
  - Confirm oracle service restarts cleanly and `/health` returns `healthy`.

- [ ] **Step 8** — Update frontend / event indexer with mainnet contract IDs and restart.

- [ ] **Step 9** — Archive / delete the deployer keypair as planned.

---

## 8. Post-Deployment Verification

Run these checks immediately after deployment, before announcing availability to users.

- [ ] `stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_contract_state` returns expected initial state (not paused, no matches).
- [ ] Oracle service logs show successful connection to mainnet Stellar RPC.
- [ ] Event indexer is ingesting mainnet events (check `/health` and first few blocks).
- [ ] Grafana dashboard loads and shows the correct mainnet contract IDs in the title / annotations.
- [ ] Run a **live mainnet smoke test** with minimal stakes (1 XLM each player):
  - [ ] Create match → TX confirmed.
  - [ ] Both players deposit → match becomes Active.
  - [ ] Submit result via oracle → payout TX confirmed on-chain.
  - [ ] Payout amounts verified correct in Stellar explorer.
- [ ] No unexpected `Paused`, `MatchNotFound`, or token-transfer errors appear in oracle logs.

---

## 9. Post-Launch Monitoring

- [ ] Prometheus is scraping all production targets (oracle, event indexer, WebSocket server) — verify at http://prometheus:9090/targets.
- [ ] Grafana **Contract Health** dashboard is visible and all panels show data.
- [ ] Alertmanager is configured and test alert has been fired and received by the on-call channel.
- [ ] All critical and warning alert rules from [`monitoring/prometheus/alerts.yml`](../monitoring/prometheus/alerts.yml) are active.
- [ ] On-call rotation is set up and the first responder has been briefed on:
  - [runbook-pause.md](runbook-pause.md) — responding to contract pause
  - [runbook-rotation.md](runbook-rotation.md) — oracle key rotation
  - [monitoring-setup.md](monitoring-setup.md) — dashboard usage
- [ ] A post-launch review is scheduled for 24 hours after go-live to review initial metrics.
- [ ] A full post-launch review is scheduled for 7 days after go-live.

---

## 10. Rollback Procedures

Soroban smart contracts are immutable once deployed. There is no "undo" for a deployment. Rollback options are:

### Option A — Emergency Pause (preferred first response)

If a critical issue is detected:

1. Admin calls `pause` on the escrow contract immediately.
2. No new matches can be created; no deposits or result submissions are accepted.
3. Investigate the issue without further fund exposure.
4. If the issue is resolvable (e.g. oracle bug, frontend bug), fix and unpause.
5. See [runbook-pause.md](runbook-pause.md) for the full pause runbook.

### Option B — Oracle Rotation (oracle key compromise)

If the oracle key is compromised:

1. Admin calls `update_oracle` with a new, secure oracle keypair.
2. Old oracle can no longer submit results.
3. Deploy new oracle service with the new keypair.
4. See [runbook-rotation.md](runbook-rotation.md) for the full rotation runbook.

### Option C — Deploy New Contract Version

If the contract code itself has a critical vulnerability:

1. Pause the existing contract immediately (Option A).
2. Communicate transparently with users: all staked funds remain safe in the paused contract.
3. Deploy and verify the new contract version (repeat this checklist).
4. Provide a migration path for any active matches: announce a refund window, then call `cancel_match` / `expire_match` for remaining active matches (requires admin action or waiting for timeout).
5. Redirect oracle service and frontend to the new contract address.
6. Announce the migration and new contract address publicly.

> **Important**: Always exhaust Option A and B before considering Option C. A deploy of a new contract requires users to trust the new address and may disrupt active matches.

### Communication Template

When an incident requires user communication:

```
[CHECKMATE-ESCROW NOTICE — {DATE}]

We have identified an issue affecting [component]. 
The escrow contract has been [paused / oracle rotated].

Impact: [describe what users cannot do]
Funds: All staked funds are safe and inaccessible to any party except their owners.
ETA: We expect to resolve this by [time estimate].
Updates: Follow this channel / [status page URL] for updates.
```

---

## 11. Sign-Off

All responsible parties must sign off before the deployment is considered complete and before the system is opened to public use.

| Role | Full Name | Date (UTC) | Signature / Approval Link |
|------|-----------|------------|---------------------------|
| Lead Developer | | | |
| Security Reviewer | | | |
| DevOps / Infrastructure | | | |
| Product Owner | | | |
| (Auditor — if required) | | | |

**Deployment declared production-ready on**: _________________________ (UTC)

---

*This checklist was introduced in [#979](https://github.com/StellarCheckMate/Checkmate-Escrow/issues/979). Update it as the deployment process evolves.*
