# Checkmate-Escrow — Competitive Chess Betting on Stellar

[![Code Coverage](https://github.com/meetdarc-tech/Checkmate-Escrow/actions/workflows/coverage.yml/badge.svg)](https://github.com/meetdarc-tech/Checkmate-Escrow/actions/workflows/coverage.yml)

A trustless chess wagering platform built on Stellar Soroban smart contracts. Players stake tokens before a match, and the winner is automatically paid out the moment the game ends — no middleman, no delays, no trust required.


## 🎯 What is Checkmate-Escrow?

Checkmate-Escrow combines competitive chess with Stellar's fast settlement to create a fully on-chain betting platform for casual and high-stakes matches.

Players:

- Stake tokens into a Soroban escrow contract before a match begins
- Play their game on Lichess or Chess.com as normal
- Receive automatic payouts the instant the match result is verified on-chain

A custom Oracle bridges the Chess.com / Lichess API to the smart contract, verifying match results and triggering payouts without any manual intervention.

This makes Checkmate-Escrow:

✅ Trustless (no platform can withhold or delay winnings)  
✅ Transparent (all stakes and payouts are verifiable on-chain)  
✅ Instant (Stellar's fast finality means payouts settle in seconds)  
✅ Accessible (anyone with a Stellar wallet can participate)

## 🚀 Features

- **Create a Match**: Set stake amount, token address, and link a Lichess/Chess.com game ID
- **Flexible Token Support**: Any Stellar token address is accepted by default; once the admin adds at least one token via `add_allowed_token`, only allowlisted tokens are accepted for new matches
- **Escrow Stakes**: Both players deposit funds into the contract before the game starts
- **Oracle Integration**: Real-time result verification via Lichess/Chess.com APIs
- **Automatic Payouts**: Winner receives the full pot the moment the result is confirmed
- **Draw Handling**: Stakes are returned to both players in the event of a draw
- **Admin Controls**: Pause/unpause, oracle rotation, admin transfer, and match timeout configuration
- **Transparent**: All escrow balances and payout history are verifiable on-chain

## 🗺️ Match Lifecycle

Matches move through the following states:

```
Pending ──► Active ──► Completed
   │                       ▲
   └──► Cancelled ◄─────────
         (expire_match / cancel_match)
```

| State       | Description                                              |
|-------------|----------------------------------------------------------|
| `Pending`   | Match created; awaiting deposits from both players       |
| `Active`    | Both players have deposited; game is in progress         |
| `Completed` | Oracle submitted result; payout executed                 |
| `Cancelled` | Cancelled before activation, or expired after timeout    |

### Events Reference

The table below is generated from all `env.events().publish` calls in the
contract source.  For a machine-readable listing, see the
[Soroban contract spec](contracts/escrow/contract-spec.json).

| Topic (namespace / name) | Emitted by | Payload |
|---|---|---|
| `escrow` / `init` | `initialize` | `(oracle_address, admin_address)` |
| `admin` / `paused` | `pause` | `()` |
| `admin` / `unpaused` | `unpause` | `()` |
| `escrow` / `stablecoin_mode` | `set_protocol_config` | `(new_mode, admin)` |
| `admin` / `protocol_config_set` | `set_protocol_config` | `admin` |
| `admin` / `ref_share` | `set_referral_share_bps` | `(basis_points, admin)` |
| `admin` / `min_stake` | `set_minimum_stake` | `amount` |
| `admin` / `token_add` | `add_allowed_token` | `(token, admin)` |
| `admin` / `tok_rm` | `remove_allowed_token` | `(token, admin)` |
| `admin` / `tok_blacklist` | `add_token_to_blacklist` | `(token, reason, admin)` |
| `admin` / `tok_unblacklist` | `remove_token_from_blacklist` | `token` |
| `admin` / `freeze` | `admin_freeze_player` | `player` |
| `admin` / `unfreeze` | `admin_unfreeze_player` | `player` |
| `admin` / `fee_tiers_set` | `set_fee_tiers` | `(tiers.len(), admin)` |
| `admin` / `sc_issuer_add` | `add_stablecoin_issuer` | `issuer` |
| `admin` / `sc_issuer_rm` | `remove_stablecoin_issuer` | `issuer` |
| `match` / `created` | `create_match` | `(match_id, player1, player2, stake_amount, referrer)` |
| `match` / `bracket_created` | `create_match_tournament` | `(match_id, bracket_id, round, player1, player2, stake_amount)` |
| `match` / `deposit` | `deposit` | `(match_id, player, previous_state)` |
| `match` / `activated` | `deposit` | `match_id` |
| `escrow` / `deposit` | `deposit` | `(match_id, player, stake_amount)` |
| `match` / `completed` | `settle_result` | `(match_id, winner, payout_amount)` |
| `match` / `pending_result` | `settle_result` | `(match_id, winner, deadline)` |
| `match` / `cancelled` | `cancel_match` | `match_id` |
| `match` / `paused` | `pause_match` | `match_id` |
| `match` / `resumed` | `resume_match` | `match_id` |
| `match` / `expired` | `expire_match` | `match_id` |
| `match` / `rb_vote` | `dispute_and_rollback_match` | `(match_id, disputer)` |
| `match` / `rollback` | `dispute_and_rollback_match` | `(match_id, disputer, reason)` |
| `match` / `heartbeat` | `heartbeat_match` | `(match_id, player, timestamp)` |
| `match` / `snapshot` | `record_snapshot` | `(match_id, index, commitment)` |
| `match` / `adm_stall` | `admin_resolve_stalled_match` | `(match_id, resolution)` |
| `match` / `truncated` | `collect_matches_by_state` | `(state, max_results)` |
| `player` / `snapshot` | `record_player_snapshot` | `(player, index, balance)` |
| `dispute` / `created` | `dispute_oracle_result` | `(dispute_id, match_id, disputer, evidence_hash, bond)` |
| `dispute` / `voted` | `vote_on_dispute` | `(dispute_id, voter, vote, snapshot_weight)` |
| `dispute` / `resolved` | `resolve_dispute_by_vote` | `(dispute_id, match_id, state, winner, total_votes, quorum)` |
| `dispute` / `oracle_slash_signal` | `mark_dispute_for_oracle_slash` | `(dispute_id, oracle, slash_amount)` |
| `admin` / `match_recovered` | `admin_recover_match` | `(match_id, admin)` |
| `admin` / `propose` | `propose_admin` | `new_admin` |
| `admin` / `xfer` | `accept_admin` | `pending_admin` |
| `admin` / `prop_cxl` | `cancel_admin_proposal` | `admin` |
| `admin` / `timeout` | `set_match_timeout` | `(old_timeout, seconds)` |
| `admin` / `max_stake` | `set_maximum_stake` | `amount` |
| `admin` / `dispute_period` | `set_dispute_period` | `period` |
| `admin` / `dispute_bond` | `set_dispute_bond_basis_points` | `basis_points` |
| `admin` / `min_hold_duration` | `set_minimum_hold_duration` | `duration` |
| `admin` / `quorum_basis_points` | `set_quorum_basis_points` | `basis_points` |
| `admin` / `oracle_up` | `update_oracle` | `(old_oracle, new_oracle)` |
| `match` / `ora_dead` | `check_oracle_deadlock` | `(match_id, current_confirmations, required)` |
| `admin` / `rot_temp` | `rotate_oracle_temporary` | `(old_oracle, new_oracle, duration_seconds)` |
| `admin` / `rot_prop` | `propose_oracle_rotation` | `(old_oracle, new_oracle)` |
| `admin` / `ora_add` | `add_approved_oracle` | `oracle` |
| `admin` / `ora_rm` | `remove_approved_oracle` | `oracle` |
| `admin` / `req_conf` | `set_required_confirmations` | `count` |
| `match` / `claim` | `claim_vested_payout` | `(match_id, player, amount_claimed, payout_token)` |
| `upgrade` / `sched` | `schedule_upgrade` | `(new_wasm_hash, scheduled_at)` |
| `upgrade` / `cancel` | `cancel_upgrade` | `()` |
| `upgrade` / `exec` | `execute_upgrade` | `new_wasm_hash` |
| `upgrade` / `migrated` | `migrate_state` | `(current, target_version)` |

#### Oracle contract events

| Topic (namespace / name) | Emitted by | Payload |
|---|---|---|
| `oracle` / `init` | `initialize` | `admin` |
| `oracle` / `stake` | `register_oracle_with_stake` | `(oracle_address, stake_amount, token)` |
| `oracle` / `result` | `submit_result` / `submit_batch_results` / `submit_oracle_result` | `(match_id, result)` |
| `oracle` / `batch` | `submit_batch_results` | `len` |
| `oracle` / `vote` | `submit_oracle_result` | `(match_id, oracle, result)` |
| `oracle` / `finalzd` | `submit_oracle_result` | `(match_id, submitters_len, threshold)` |
| `oracle` / `disputed` | `submit_oracle_result` | `match_id` |
| `oracle` / `minority` | `submit_oracle_result` / `resolve_disputed_match` | `(match_id, oracle)` |
| `oracle` / `equivoc` | `submit_oracle_result` | `(match_id, oracle)` |
| `oracle` / `resolved` | `resolve_disputed_match` | `(match_id, result)` |
| `oracle` / `slashstg` | `slash_oracle` | `(oracle_address, match_id, slash_amount, admin)` |
| `oracle` / `slash` | `finalize_slash` | `(oracle_address, match_id, slash_amount)` |
| `admin` / `slashcxl` | `admin_cancel_slash` | `(oracle_address, match_id, admin)` |
| `oracle` / `deact` | `deactivate_slow_oracle` | `oracle_address` |
| `oracle` / `thresh` | `set_consensus_threshold` | `threshold` |
| `oracle` / `ratelim` | `set_oracle_rate_limits` | `(oracle, hourly_limit, daily_limit)` |
| `admin` / `admin_rot` | `update_admin` | `(old_admin, new_admin)` |
| `admin` / `paused` | `pause` | `()` |
| `admin` / `unpaused` | `unpause` | `()` |
| `admin` / `slash_gp` | `set_slashing_grace_period` | `(grace_period_ledgers, admin)` |
| `oracle` / `deleted` | `delete_result` | `match_id` |

## 🛠️ Quick Start

**New to Checkmate-Escrow?** Start with the [Local Development Setup](docs/local-dev.md) guide for step-by-step instructions on building, testing, and running the full stack locally — including [running the oracle service locally](docs/local-dev.md#running-the-oracle-service-locally) against a mock Lichess/Chess.com server.

### Prerequisites

- Rust (1.70+)
- Soroban CLI
- Stellar CLI

### Build

```bash
./scripts/build.sh
```

### Test

```bash
./scripts/test.sh
```

### Setup Environment

Copy the example environment file:

```bash
cp .env.example .env
```

Configure your environment variables in `.env`:

```env
# Network configuration
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org

# Contract addresses (after deployment)
CONTRACT_ESCROW=<your-contract-id>
CONTRACT_ORACLE=<your-contract-id>

# Oracle configuration
LICHESS_API_TOKEN=<your-lichess-api-token>
CHESSDOTCOM_API_KEY=<your-chessdotcom-api-key>

# Frontend configuration
VITE_STELLAR_NETWORK=testnet
VITE_STELLAR_RPC_URL=https://soroban-testnet.stellar.org
```

Network configurations are defined in `environments.toml` — see the [environments.toml reference](docs/local-dev.md#environmentstoml) in the local development guide for a full field-by-field breakdown of each network:

- `testnet` — Stellar testnet (free funds via Friendbot, recommended for development)
- `mainnet` — Stellar mainnet (real funds, production only)
- `futurenet` — Stellar futurenet (preview of upcoming protocol features)
- `standalone` — Local development node (fully isolated, no external connectivity)

### Deploy to Testnet

```bash
# Configure your testnet identity first
stellar keys generate deployer --network testnet

# Deploy
./scripts/deploy_testnet.sh
```

### Run Demo

Follow the step-by-step guide in `demo/demo-script.md`

## 🎓 New here? Start with the Interactive Tutorial

Brand new to Checkmate-Escrow? The **[Interactive Tutorial](docs/tutorial-step-by-step.md)**
takes you from zero to a completed, paid-out match on **testnet** in under 15
minutes — no real funds at risk:

1. **Create a match** — register a wager on-chain
2. **Deposit funds** — fund the escrow
3. **Check the result** — verify the outcome and watch the payout

Prefer to learn by watching or testing yourself?

- 🎬 **Video walkthroughs:** the [tutorial](docs/tutorial-step-by-step.md) includes a Video walkthroughs section (links added as recordings are published)
- 🧩 **Interactive quiz & checklist:** [tutorial-quiz.md](docs/tutorial-quiz.md)
- 🧪 **Testnet practice mode:** the whole tutorial runs on free testnet funds — no real money at risk

## 📖 Documentation

- [Changelog](CHANGELOG.md) — release history and notable changes
- [Interactive Tutorial](docs/tutorial-step-by-step.md) — guided, hands-on intro for new users
- [Tutorial Quiz & Checklist](docs/tutorial-quiz.md) — verify your understanding
- [FAQ](docs/faq.md) — common questions and answers
- [Glossary](docs/glossary.md) — key terms (escrow, oracle, match states, Soroban, wave-ready, and more) for new contributors
- [Architecture Overview](docs/architecture.md)
- [Oracle Design](docs/oracle.md) — comprehensive oracle documentation covering result verification, consensus, and platform integration:
  - [Lichess Integration](docs/oracle.md#lichess-integration) — API setup, game ID format, result mapping
  - [Chess.com Integration](docs/oracle.md#chess-com-integration) — API key setup, rate limits, result field mapping, error handling
  - [Platform Comparison Table](docs/oracle.md#platform-api-comparison-lichess-vs-chessdotcom) — Lichess vs Chess.com API differences
- [WebSocket API](docs/websocket-api.md) — real-time match event protocol (protocol v1), connection lifecycle, subscription model, React hook
- [Threat Model & Security](docs/security.md)
- [Security Policy & Vulnerability Disclosure](SECURITY.md) — how to report vulnerabilities, response SLA, and bug bounty
- [Balance-Privacy Model](docs/privacy-model.md) — what the balance-snapshot APIs hide from non-admins, and what they don't
- [Roadmap](docs/roadmap.md)
- [Deployment Guide](docs/deployment.md)
- [Mainnet Deployment Checklist](docs/mainnet-deployment-checklist.md) — step-by-step checklist for safe mainnet launches
- [Monitoring Setup](docs/monitoring-setup.md) — Prometheus metrics, Grafana dashboard, and alerting rules
- [Error Codes Reference](docs/error-codes.md) — every contract error code, its cause, and how to recover

## 🎓 Smart Contract API

### Match Management

```
create_match(player1, player2, stake_amount, token, game_id, platform) -> u64
get_match(match_id) -> Match
cancel_match(match_id, caller) -> Result<(), Error>
expire_match(match_id) -> Result<(), Error>
get_player_matches(player) -> Vec<u64>
get_pending_matches() -> Vec<Match>
get_active_matches() -> Vec<Match>
get_completed_matches() -> Vec<Match>
get_completed_matches_paginated(offset, limit) -> Vec<Match>
```

- `create_match` must be authorized by `player1`.
- `cancel_match` may be called by either matched player.
- `expire_match` may be called by anyone once the match timeout elapses.

### Escrow

```
deposit(match_id, player) -> Result<(), Error>
is_funded(match_id) -> Result<bool, Error>
get_escrow_balance(match_id) -> Result<i128, Error>
```

#### `is_funded` vs `get_escrow_balance`

These two functions answer different questions and are easy to confuse:

- **`is_funded(match_id)`** — returns `true` only when *both* players have deposited their stake (i.e. the match has transitioned to `Active`). It reflects deposit flags, not token balances. Use this to gate game-start logic.

- **`get_escrow_balance(match_id)`** — returns the total token amount currently held in escrow for the match: `0`, `1×stake`, or `2×stake` depending on how many players have deposited. Once a match is `Completed` or `Cancelled` (funds already paid out or refunded), this always returns `0` regardless of on-chain token balances.

Examples:

| Scenario                              | `is_funded` | `get_escrow_balance` |
|---------------------------------------|-------------|----------------------|
| Only player1 deposited                | `false`     | `1 × stake_amount`   |
| Both players deposited (Active)       | `true`      | `2 × stake_amount`   |
| Match completed (payout done)         | `true`      | `0`                  |
| Match cancelled (refunds done)        | `false`     | `0`                  |

### Contract Metadata

```
get_contract_version() -> String
```

- Returns the crate version from `Cargo.toml` (e.g. `"0.1.0"`) as a semver-formatted string. Clients can use this to detect version mismatches against a deployed contract.

### Oracle & Payouts

```
get_oracle_address() -> Address
submit_result(match_id, winner, caller) -> Result<(), Error>
submit_result_with_oracle_record(match_id, winner, game_id) -> Result<(), Error>
submit_result_batch(results: Vec<(match_id, winner)>, caller) -> Result<Vec<Option<Error>>, Error>
```

- `get_oracle_address()` returns the oracle address currently configured on the contract. This is a view function that requires no authentication and is intended for off-chain clients, frontends, and monitoring tools to verify oracle configuration without reading raw contract storage. Returns `Error::Unauthorized` if the contract has not been initialized.

- `submit_result_batch` settles multiple matches in a single call, reducing oracle transaction overhead. `caller` must be the configured oracle. Each match is processed independently — a failure on one (e.g. `NotFunded`, `InvalidState`) does not stop the rest from being processed. The returned `Vec` has one entry per input entry, in the same order: `None` on success, `Some(Error)` on failure for that match. (Soroban's contract ABI has no `Result` element type, so `Option<Error>` is the on-chain equivalent of `Result<(), Error>` here.)

- `submit_result` is called by the configured oracle address and requires oracle authorization.
- `submit_result_with_oracle_record` is the canonical oracle integration path and stores the verified `game_id` for audit.

`submit_result` verifies the caller, records the winner, and immediately executes the payout (or refund on draw) in a single transaction. There are no separate `verify_result` or `execute_payout` functions.

### Admin Controls

```
set_match_timeout(seconds) -> Result<(), Error>
get_match_timeout() -> Result<u64, Error>
set_maximum_stake(amount: Option<i128>) -> Result<(), Error>
set_protocol_config(config: ProtocolConfig) -> Result<(), Error>
get_protocol_config() -> Result<ProtocolConfig, Error>
admin_freeze_player(player, reason) -> Result<(), Error>
admin_unfreeze_player(player) -> Result<(), Error>
is_player_frozen(player) -> bool
get_frozen_players() -> Vec<Address>
```

All admin-control functions below require authorization from the configured admin address.

- **Match timeout** — `set_match_timeout` sets how long (in seconds) a `Pending` match may wait for both deposits before anyone can call `expire_match` on it. Value must fall within `[MIN_MATCH_TIMEOUT_SECONDS, MAX_MATCH_TIMEOUT_SECONDS]` (1–90 days), enforced with `Error::InvalidTimeout`. `get_match_timeout` returns the current value in seconds. The timeout is stored in `ProtocolConfig::match_timeout_seconds` and defaults to `DEFAULT_MATCH_TIMEOUT_SECONDS` (30 days).
- **Maximum stake** — `set_maximum_stake` caps the `stake_amount` accepted by `create_match` and its variants. Pass `Some(amount)` to set a cap (rejected with `Error::InvalidAmount` if `amount <= 0`), or `None` to remove the cap (the default). `create_match` rejects any `stake_amount` above the configured cap with `Error::InvalidAmount`. Read the current cap via `get_protocol_config().maximum_stake`.
- **Protocol fee** — `ProtocolConfig::protocol_fee_bps` (basis points, default `0`) and `ProtocolConfig::fee_recipient` control a platform fee taken out of the winner's payout: `protocol_fee = stake_amount * 2 * protocol_fee_bps / 10_000`, transferred to `fee_recipient` when `submit_result` resolves a winner. Draw refunds are never charged this fee. Set both fields via `set_protocol_config`; `protocol_fee_bps` above `10_000` (100%) is rejected with `Error::InvalidAmount`.
- **Full config read** — `get_protocol_config` returns the complete `ProtocolConfig` struct as currently stored on-chain (treasury, vesting duration, cancellation fee, stablecoin-only mode, maximum stake, match timeout, protocol fee, and fee recipient). Off-chain tools and the frontend can call this single function instead of making multiple separate admin queries to reconstruct contract state.
- **Player freeze** — `admin_freeze_player(player, reason)` blocks a single player from creating matches (any `create_match` variant, including being named as the opponent) and depositing, without pausing the contract for anyone else. The `reason` is stored on-chain for auditability, and `admin_unfreeze_player(player)` reverses it. A freeze deliberately does **not** block fund recovery — the frozen player can still cancel/expire pending matches and claim payouts, and existing `Active` matches settle normally. Rejections surface as `Error::ContractPaused` (the error enum is at its 50-variant XDR cap, so this reuses the closest existing code). Query state via `is_player_frozen(player)` and `get_frozen_players()`.

## 🧪 Testing

Comprehensive test suite covering:

✅ Match creation and configuration  
✅ Deposit validation and escrow locking  
✅ Oracle result submission and verification  
✅ Winner payout and draw refund logic  
✅ Cancellation and edge cases  
✅ Error handling and security checks

Run tests:

```bash
cargo test
```

## 🌍 Why This Matters

**The Problem**: Current chess betting and tournament prize payouts are slow and rely entirely on the platform's honesty. Players have no guarantee their winnings will be paid out fairly or on time.

**The Solution**: By holding stakes in a Soroban smart contract and automating payouts via a verified Oracle, Checkmate-Escrow removes the need to trust any third party.

**Blockchain Benefits**:

- No platform can withhold or manipulate payouts
- Transparent stake and payout history for every match
- Programmable rules enforced by smart contracts
- Accessible to anyone with a Stellar wallet

**Target Users**:

- Competitive chess players looking for trustless wagering
- Chess clubs and tournament organizers
- Casual players wanting skin-in-the-game matches
- Developers building on Stellar/Soroban

## 🗺️ Roadmap

- **v1.0 (Current)**: Token-allowlist escrow, Lichess Oracle integration, basic match flow
- **v1.1**: Chess.com Oracle, expanded token support
- **v2.0**: Multi-game tournaments, bracket payouts
- **v3.0**: Frontend UI with wallet integration
- **v4.0**: Mobile app, ELO-based matchmaking, leaderboards

See [docs/roadmap.md](docs/roadmap.md) for details.

## 🤝 Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

See our [Code of Conduct](CODE_OF_CONDUCT.md) and [Contributing Guidelines](CONTRIBUTING.md).

## 🌊 Drips Wave Contributors

This project participates in Drips Wave — a contributor funding program! Check out:

- [Wave Contributor Guide](docs/wave-guide.md) — How to earn funding for contributions
- [Wave-Ready Issues](https://github.com/issues?q=label%3Awave-ready) — Funded issues ready to tackle
- GitHub Issues labeled with `wave-ready` — Earn 100–200 points per issue

Issues are categorized as:

- `trivial` (100 points) — Documentation, simple tests, minor fixes
- `medium` (150 points) — Oracle helpers, validation logic, moderate features
- `high` (200 points) — Core escrow logic, Oracle integrations, security enhancements

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Stellar Development Foundation](https://stellar.org) for Soroban
- [Lichess](https://lichess.org) for their open API
- [Chess.com](https://chess.com) for their developer platform
- Drips Wave for supporting public goods funding

## Handsoff notes

<!-- handsoff-issue-1528 -->
- #1528: Security: `deposit_batch` and `submit_result_batch` keep partial state writes when an entry fails

<!-- handsoff-issue-1530 -->
- #1530: Fix: `settle_result` updates stats and payout counters before the dispute period ends

<!-- handsoff-issue-1562 -->
- #1562: Performance: `get_bracket_matches` scans every match ever created
