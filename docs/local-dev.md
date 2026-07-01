# 🛠️ Local Development Setup

Get Checkmate-Escrow running on your machine in minutes. This guide covers setting up the smart contracts, frontend, and supporting services.

## Prerequisites

| Tool | Version | Check |
|------|---------|-------|
| [Rust](https://www.rust-lang.org/tools/install) | 1.70+ | `rustc --version` |
| [Soroban CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-or-update-soroban-cli) | Latest | `soroban --version` |
| [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) | Latest | `stellar --version` |
| [Node.js](https://nodejs.org/) | 18+ | `node --version` |
| [npm](https://www.npmjs.com/get-npm) | 9+ | `npm --version` |
| `wasm32` target | — | `rustup target add wasm32-unknown-unknown` |

## Quick Start

### 1. Clone the repository

```bash
git clone https://github.com/StellarCheckMate/Checkmate-Escrow.git
cd Checkmate-Escrow
```

### 2. Build smart contracts

```bash
./scripts/build.sh
```

This compiles both the Escrow and Oracle contracts to WebAssembly. The build output goes to `target/wasm32-unknown-unknown/release/`.

### 3. Run contract tests

```bash
./scripts/test.sh
```

Tests cover match creation, escrow logic, oracle integration, and edge cases.

### 4. Set up the frontend

```bash
cd frontend
npm install
npm run dev
```

The frontend runs at `http://localhost:5173` by default.

### 5. Start the event indexer

```bash
cd services/event-indexer
cargo run --release
```

The event indexer tracks on-chain events and indexes them for quick queries. Configuration is in `services/event-indexer/src/config.rs`.

#### Using Docker Compose

Alternatively, run the event indexer in a container via Docker Compose. Make sure `.env` is set up first (see [Environment variables](#environment-variables)):

```bash
docker compose up --build
```

This builds the `event-indexer` service from `services/event-indexer/Dockerfile`, persists its SQLite database in a named Docker volume, and exposes the API on `http://localhost:8080`. Environment variables are sourced from `.env` at the repo root, with sensible defaults applied for anything not set (see `docker-compose.yml`).

## Configuration

### Environment variables

Copy the example environment file and configure as needed:

```bash
cp .env.example .env
```

Key variables:

```env
# Stellar network (testnet, mainnet, futurenet, standalone)
STELLAR_NETWORK=testnet

# After deploying contracts locally
CONTRACT_ESCROW=<your-contract-id>
CONTRACT_ORACLE=<your-contract-id>

# Oracle credentials (for testing with real APIs)
LICHESS_API_TOKEN=<optional>
CHESSDOTCOM_API_KEY=<optional>

# Frontend
VITE_STELLAR_NETWORK=testnet
VITE_STELLAR_RPC_URL=https://soroban-testnet.stellar.org
```

### Using a local Stellar network

For isolated testing, you can run against a local Stellar node:

1. **Start Soroban/Stellar in standalone mode:**

```bash
docker run --rm -it \
  -p 8000:8000 \
  stellar/quickstart:latest \
  --standalone
```

2. **Point environment to local network:**

```bash
export STELLAR_NETWORK=standalone
export STELLAR_RPC_URL=http://localhost:8000
```

3. **Deploy contracts locally:**

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source deployer \
  --network standalone
```

## Project Structure

```
Checkmate-Escrow/
├── contracts/
│   ├── escrow/          # Main escrow smart contract
│   │   ├── src/
│   │   └── tests/
│   └── oracle/          # Oracle contract for result verification
│       └── src/
├── oracle-service/      # Oracle service (Lichess/Chess.com integration)
├── services/
│   └── event-indexer/   # Event indexer for on-chain event tracking
├── frontend/            # React + TypeScript frontend
├── scripts/             # Build, test, and deployment scripts
├── docs/                # Documentation
└── demo/                # Demo walkthrough scripts
```

## Development Workflows

### Test-driven development

```bash
# Write a test in contracts/escrow/tests/
# Watch for failures:
cargo watch -x test --manifest-path contracts/escrow/Cargo.toml

# Fix the code
# Verify the test passes
```

### Frontend development with contract testing

Terminal 1 — Watch contract changes:
```bash
cd contracts/escrow
cargo watch -x test
```

Terminal 2 — Frontend dev server:
```bash
cd frontend
npm run dev
```

### Oracle service integration

Test Oracle result submission locally:

```bash
cd oracle-service
cargo test -- --nocapture
```

See `oracle-service/tests/` for integration tests with Chess.com and Lichess APIs.

## Troubleshooting

### Build fails with "cannot find -lwasmvm"

The Wasm build target is missing. Install it:

```bash
rustup target add wasm32-unknown-unknown
```

### "STELLAR_NETWORK not found in environments.toml"

Set your network environment variable and ensure `environments.toml` includes your chosen network. Default networks are in the file; add custom ones if needed.

### Frontend won't connect to contracts

Check that:
1. Contracts are deployed: verify `CONTRACT_ESCROW` and `CONTRACT_ORACLE` in `.env`
2. The network matches: `STELLAR_NETWORK` and `VITE_STELLAR_NETWORK` should be the same
3. RPC URL is reachable: test with `curl $STELLAR_RPC_URL`

### Tests hang or timeout

Increase the timeout in Cargo.toml or run tests with a longer timeout:

```bash
cargo test -- --test-threads=1 --nocapture
```

## Next Steps

- [Interactive Tutorial](tutorial-step-by-step.md) — Deploy to testnet and run a full match
- [Architecture Overview](architecture.md) — Understand the design
- [Testing Guide](TESTING_GUIDE.md) — Deep dive into test patterns
- [Deployment Guide](deployment.md) — Deploy to mainnet

## Getting Help

- Check [docs/](.) for architecture and API reference
- Review [GitHub Issues](https://github.com/StellarCheckMate/Checkmate-Escrow/issues) for known issues
- See [Contributing Guidelines](../CONTRIBUTING.md) for code style and PR process
