# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),

## [Unreleased]

### Added
- Added `cursor` field to `GET /events` responses echoing the `ledger_sequence` of the last returned event, so poll-based clients can detect skipped ledgers between requests (closes #2000).
- Added unit tests in `auth.rs` asserting that a missing API key and a malformed API key are both rejected by the authentication middleware (closes #2001).
- Added a `cargo machete` unused-dependency check to CI and documented local usage in `CONTRIBUTING.md` and `docs/local-dev.md` (closes #2029).
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- Re-PR: changelog update -->


[!NOTE] Added retrospective entries for major features since v0.1.0: oracle consensus, vesting, disputes, player freeze. Closes #1777.

### Added
- `confirm_upgrade` — multi-sig confirmation step for contract upgrades. A second authorized address must call `confirm_upgrade` after `schedule_upgrade` and before `execute_upgrade`. This ensures that a single compromised admin key cannot unilaterally deploy arbitrary WASM — both the admin key and the upgrader key must cooperate to execute an upgrade. `execute_upgrade` returns `Error::UpgradeNotConfirmed` (#58) if the confirmation is absent. The confirmation is automatically cleared by both `cancel_upgrade` and `execute_upgrade`.
- `Error::UpgradeNotConfirmed` (#58) — new error code returned by `execute_upgrade` when the multi-sig confirmation step has not been completed.

### Changed
- `execute_upgrade` now requires a prior `confirm_upgrade` call. Existing upgrade flows must add a `confirm_upgrade(upgrader)` step between `schedule_upgrade` and `execute_upgrade`. See [docs/upgrade-safety.md](docs/upgrade-safety.md) for the updated procedure.
- `cancel_upgrade` now also clears any recorded `confirm_upgrade` confirmation so it cannot carry over to a subsequent upgrade cycle.
- `docs/upgrade-safety.md` — updated upgrade lifecycle diagram and procedure to include the `confirm_upgrade` step.
- `docs/security.md` — added "Contract Upgrade Multi-Sig" section documenting the threat model and recommendations.
- `docs/error-codes.md` — added entry for error code #58 `UpgradeNotConfirmed`.

### Added
- `admin_resolve_stalled_match` — admin-gated recovery function for Active matches stuck after the 24-hour player rollback window expires and the oracle has failed to submit a result for more than 7 days. Provides a bounded recovery path so funds are never permanently locked in the event of prolonged oracle outage or lost signing keys.
- `ADMIN_STALL_WINDOW_SECONDS` constant (7 days) — configurable threshold after which admin can invoke stall resolution, long enough not to compete with the player-initiated 24-hour rollback window.
- `admin_freeze_player` / `admin_unfreeze_player` — targeted per-player freeze that blocks a single address from creating matches and depositing, without pausing the contract for other users. The freeze reason is stored on-chain; `is_player_frozen` / `get_frozen_players` expose state. Fund-recovery paths (cancel/expire/claim) and oracle settlement of existing matches are deliberately unaffected.
- Rate limits subsection in the Chess.com Integration section of `docs/oracle.md`, documenting the PubAPI's serial/parallel throttling model, cache invalidation behavior, the oracle client's conservative 30 req/min token-bucket configuration, and how to request higher limits. Closes #1800.

### Fixed
- Active matches can no longer be cancelled; attempts return `InvalidState`, and the create-match form resets when the connected wallet account changes.
- Bracket match lookups and state-filtered match queries use maintained indexes instead of scanning the full match history.
- Active matches with no oracle response are no longer permanently unrecoverable after the 24-hour `dispute_and_rollback_match` window elapses. The new admin recovery function closes the gap identified in Issue #1274, ensuring funds can always be recovered after a bounded stall period.

## [1.0.0] - 2026-06-29

### Added

- Soroban escrow smart contract for trustless chess wagering on Stellar
- Match lifecycle management: `Pending → Active → Completed / Cancelled` states
- `create_match` — register a wager with stake amount, token address, and Lichess/Chess.com game ID
- `deposit` — both players lock funds into escrow before the game starts
- `submit_result` / `submit_result_with_oracle_record` — oracle submits verified result and triggers automatic payout in a single transaction
- `cancel_match` — either player can cancel before the match becomes active
- `expire_match` — anyone can expire a match after the configured timeout elapses; stakes are refunded
- Draw handling — both players receive their stake back on a draw result
- Flexible token support — any Stellar token is accepted by default; once an admin allowlists at least one token, only allowlisted tokens are accepted
- `add_allowed_token` admin function for token allowlist management
- `get_match`, `get_player_matches`, `get_pending_matches`, `get_active_matches` query helpers
- `is_funded` and `get_escrow_balance` escrow inspection helpers
- Oracle contract for bridging Lichess API results on-chain
- Admin controls: `pause` / `unpause`, `update_oracle`, `transfer_admin`, configurable match timeout
- On-chain event emission for all state transitions (see README Events Reference)
- Oracle service (Rust) polling Lichess for game results and submitting them to the contract
- Frontend scaffold (React + Vite + TypeScript) with Stellar wallet integration
- Event indexer service for querying historical match and payout data
- Deployment scripts for testnet and mainnet (`scripts/deploy.sh`, `scripts/deploy_testnet.sh`)
- Comprehensive test suite covering match creation, deposits, payouts, draws, cancellations, and error paths
- Documentation: architecture overview, oracle design, threat model & security, deployment guide, error codes reference, interactive tutorial, and FAQ

[Unreleased]: https://github.com/your-org/Checkmate-Escrow/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/your-org/Checkmate-Escrow/releases/tag/v1.0.0
# Re-PR for changelog
