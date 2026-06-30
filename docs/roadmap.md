# Checkmate-Escrow Roadmap

This document outlines the planned development phases for Checkmate-Escrow, expanding on the high-level roadmap in the README.

---

## v1.0 — Core Escrow & Lichess Integration (Complete)

The foundation of trustless chess wagering on Stellar.

### Features

- **Multi-token escrow**: Players stake any Stellar token (XLM, USDC, or custom assets) via a generic token interface
- **Match lifecycle management**: Create, deposit, activate, complete, cancel, and expire matches
- **Lichess Oracle integration**: Automated result verification via Lichess public API
- **Chess.com platform support**: `Platform::ChessDotCom` variant supported in match creation
- **Winner payouts**: Automatic distribution of the full pot to the winner
- **Draw handling**: Stakes returned to both players when games end in a draw
- **Cancellation logic**: Players can cancel unfunded matches before both deposits are made
- **Match expiry**: Pending matches that exceed a configurable ledger timeout can be expired by anyone, refunding any deposits
- **Token allowlist**: Admin can restrict which tokens are accepted via `add_allowed_token`
- **Contract pause/unpause**: Admin can halt `create_match`, `deposit`, and `submit_result` in an emergency
- **Admin management**: Two-step admin transfer (`propose_admin` / `accept_admin`) and direct `transfer_admin`
- **Match indexing**: `get_player_matches` and `get_active_matches` for efficient on-chain lookups
- **Basic security**: Admin-gated oracle submission, duplicate game ID prevention

### Oracle Features

- **Result submission**: Admin submits verified match results on-chain
- **Result queries**: Public `has_result` and admin-gated `has_result_admin`
- **Result deletion**: Admin can remove a previously submitted result via `delete_result`
- **Oracle pause/unpause**: Admin can halt result submission independently of the escrow contract
- **Admin rotation**: `update_admin` rotates the oracle admin address

### Status

✅ Complete — deployed to testnet

---

## v1.1 — Chess.com Oracle & Token Interface Standardization

Expand the off-chain oracle service to cover Chess.com and standardize the token integration.

### Features

- **Chess.com Oracle**: Implement the off-chain API client for Chess.com result verification
- **Platform validation**: Validate that game IDs match the declared platform format (numeric strings for Chess.com)
- **Token interface documentation**: Document the generic token transfer pattern for integrators

### Technical Changes

- Add Chess.com API client to the oracle off-chain service
- Update validation to handle Chess.com game ID format
- Document token integration patterns

### Timeline

Q2 2026

---

## v2.0 — Tournament Support

Enable multi-game tournaments with bracket-style payouts.

### Features

- **Tournament creation**: Define multi-round tournaments with entry fees
- **Bracket management**: Track tournament structure and progression
- **Prize pool distribution**: Configurable payout splits (e.g., 60% winner, 30% runner-up, 10% third place)
- **Batch result submission**: Oracle can submit multiple match results in a single transaction
- **Tournament state tracking**: Monitor active tournaments, completed rounds, and remaining matches

### Technical Changes

- New `Tournament` contract with bracket logic
- Link multiple `Match` instances to a single tournament
- Implement prize pool calculation and distribution
- Add tournament admin controls (pause, cancel, modify structure)

### Use Cases

- Chess club tournaments with on-chain prize pools
- Online tournaments with automatic payouts
- Bracket-style competitions with transparent prize distribution

### Timeline

Q3-Q4 2026

---

## v3.0 — Frontend UI & Wallet Integration

Make Checkmate-Escrow accessible to non-technical users.

### Features

- **Web application**: React-based frontend for creating and managing matches
- **Wallet integration**: Support for Freighter, Albedo, and other Stellar wallets
- **Match browser**: View active matches, tournament brackets, and historical results
- **Real-time updates**: WebSocket integration for live match status
- **User profiles**: Track match history, win/loss records, and earnings
- **Mobile-responsive design**: Optimized for desktop and mobile browsers

### Technical Stack

- React + TypeScript
- Stellar SDK for wallet integration
- TailwindCSS for styling
- WebSocket server for real-time updates

### Timeline

Q1-Q2 2027

---

## v4.0 — Mobile App & Matchmaking

Native mobile experience with intelligent player matching.

### Features

- **Native mobile apps**: iOS and Android applications
- **ELO-based matchmaking**: Match players of similar skill levels
- **Leaderboards**: Global and regional rankings based on match performance
- **Push notifications**: Alerts for match invitations, deposits, and results
- **In-app wallet**: Simplified Stellar wallet for casual users
- **Social features**: Friend lists, match challenges, and chat

### Technical Changes

- React Native mobile app
- ELO rating calculation and storage
- Matchmaking algorithm based on rating and stake preferences
- Push notification service integration
- Simplified wallet creation and management

### Use Cases

- Casual players finding opponents at their skill level
- Competitive players climbing leaderboards
- Mobile-first chess betting experience

### Timeline

Q3-Q4 2027

---

## Future Considerations

Beyond v4.0, potential features include:

- **Multi-chain support**: Bridge to other blockchain networks
- **Streaming integration**: Automatic result verification from Twitch/YouTube chess streams
- **Team tournaments**: Multi-player team-based competitions
- **Staking rewards**: Earn yield on escrowed funds during active matches
- **Governance token**: Community-driven platform decisions
- **Sponsorship integration**: Allow sponsors to fund prize pools

---

## Contributing to the Roadmap

Have ideas for features or improvements? Open an issue or discussion on GitHub. We welcome community input on prioritization and new feature proposals.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for details on how to contribute.
