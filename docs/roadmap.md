# Checkmate-Escrow Roadmap

This document outlines the planned development phases for Checkmate-Escrow, expanding on the high-level roadmap in the README.

---

## v1.0 — Core Escrow & Lichess Integration (Current)

The foundation of trustless chess wagering on Stellar.

### Features

- **XLM-only escrow**: Players stake native XLM tokens in a Soroban smart contract
- **Match lifecycle management**: Create, deposit, activate, and complete matches
- **Lichess Oracle integration**: Automated result verification via Lichess public API
- **Winner payouts**: Automatic distribution of the full pot to the winner
- **Draw handling**: Stakes returned to both players when games end in a draw
- **Cancellation logic**: Players can cancel unfunded matches before both deposits are made
- **Basic security**: Admin-gated oracle submission, duplicate game ID prevention

### Status

✅ Complete — deployed to testnet

---

## v1.1 — Multi-Token Support & Chess.com

Expand token support and add a second platform integration.

### Features

- **USDC support**: Allow players to stake USDC instead of XLM
- **Custom token support**: Enable any Stellar asset as stake currency
- **Chess.com Oracle**: Verify results from Chess.com games via their public API
- **Platform validation**: Ensure game IDs match the declared platform
- **Token interface standardization**: Abstract token operations for easier extension

### Technical Changes

- Add `token_address` parameter to `create_match`
- Implement generic token transfer logic using Stellar token interface
- Add Chess.com API client to oracle service
- Update validation to handle Chess.com game ID format (numeric strings)

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
