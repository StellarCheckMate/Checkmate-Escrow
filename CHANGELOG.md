# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
