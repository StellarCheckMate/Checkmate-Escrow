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

`docker compose up` runs the full stack — a local Stellar network, the event
indexer (API server), the oracle service, Postgres/Redis, and the WebSocket
server — with one command, no manual multi-terminal setup required.

Make sure `.env` is set up first (see [Environment variables](#environment-variables)):

```bash
cp .env.example .env
docker compose up --build
```

Services started:

| Service | Container port | Host port | Purpose |
|---|---|---|---|
| `stellar-standalone` | 8000 | 8000 | Local Soroban network + RPC (`stellar/quickstart`) |
| `postgres` | 5432 | 5432 | Event indexer storage |
| `redis` | 6379 | 6379 | Shared API response cache |
| `event-indexer-1` (API server) | 8080 | 8080 | REST API — matches, events, analytics |
| `event-indexer-2` | 8080 | 8081 | Second leader-eligible replica |
| `oracle-service` | 8000 | 8095 | Polls Lichess/Chess.com and submits results |
| `websocket-server` | 8090 | 8090 | Real-time match events |

`event-indexer-3` only starts with `docker compose --profile testing up`, for
exercising 3+ instance HA locally.

To exercise the full local flow against the contracts:

1. Start the stack: `docker compose up --build -d stellar-standalone`.
2. Build and deploy the contracts to the running standalone node (see
   `./scripts/build.sh` and [Using a local Stellar network](#using-a-local-stellar-network)
   below), using `--network standalone` so the RPC points at
   `http://localhost:8000/soroban/rpc`.
3. Set `CONTRACT_ESCROW`, `CONTRACT_ORACLE` and `ORACLE_SIGNING_KEY` in `.env`
   to the deployed contract IDs and a generated oracle key
   (`openssl rand -hex 32`).
4. Start the rest of the stack: `docker compose up --build`. The oracle
   service and event indexer will pick up the new `.env` values.

Environment variables are sourced from `.env` at the repo root, with sensible
defaults applied for anything not set (see `docker-compose.yml`).

## Configuration

### environments.toml

`environments.toml` defines the named networks available to the Stellar CLI and all project scripts. Select a network by setting `STELLAR_NETWORK` in your `.env` file or by passing `--network <name>` to any CLI command.

See also the [inline comments in `environments.toml`](../environments.toml) for a quick reference alongside the actual values.

#### Fields

| Field | Required | Description |
|---|---|---|
| `rpc_url` | Yes | HTTP(S) endpoint of the Soroban RPC node. Used by the CLI to submit transactions and query contract state. |
| `network_passphrase` | Yes | Unique string identifying the Stellar network. Every transaction is signed against this value — it must match exactly what the target node expects, or the transaction is rejected. |

#### Built-in networks

**`[testnet]`**

The Stellar public testnet. This is the right choice for development and CI. Test XLM is available for free from [Friendbot](https://friendbot.stellar.org/?addr=<your-address>) so you can deploy and interact with contracts without spending real funds.

```toml
rpc_url            = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"
```

**`[mainnet]`**

The Stellar public mainnet. Use only for production deployments. Transactions cost real XLM — always validate contracts thoroughly on testnet first.

```toml
rpc_url            = "https://soroban-mainnet.stellar.org"
network_passphrase = "Public Global Stellar Network ; September 2015"
```

**`[futurenet]`**

A preview network for upcoming Stellar protocol features. May be unstable. Use when you specifically need to test functionality not yet promoted to testnet.

```toml
rpc_url            = "https://rpc-futurenet.stellar.org"
network_passphrase = "Test SDF Future Network ; October 2022"
```

**`[standalone]`**

A fully isolated local node with no external connectivity. Ideal for fast, offline development and deterministic testing. Start it with:

```bash
stellar network start local
# or via Docker:
docker run --rm -it -p 8000:8000 stellar/quickstart:latest --standalone
```

```toml
rpc_url            = "http://localhost:8000/soroban/rpc"
network_passphrase = "Standalone Network ; February 2017"
```

#### Adding a custom network

Append a new section to `environments.toml` and use it immediately:

```toml
[my_network]
rpc_url            = "https://my-rpc-endpoint"
network_passphrase = "My Custom Network ; YYYY"
```

```bash
stellar contract deploy --network my_network ...
```

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

4. **Reset contract state** (see [Resetting Local Contract State](#resetting-local-contract-state))

## Resetting Local Contract State

During local development, you'll often need to reset contract state between iterations — whether testing breaking changes, cleaning up from failed deployments, or starting fresh with a new contract version. This section covers the main approaches.

### Option 1: Stop and restart the local Stellar node

The simplest and most thorough way to reset state is to stop and restart the standalone network. This clears all contract state and creates a clean environment.

**Using Docker:**

```bash
# Stop the current container (Ctrl+C in the terminal running it)
# Then start a fresh instance:
docker run --rm -it \
  -p 8000:8000 \
  stellar/quickstart:latest \
  --standalone
```

**Using Stellar CLI:**

```bash
# Stop the running network
stellar network stop local

# Start a fresh network
stellar network start local
```

**Expected output after restart:**

```
$ stellar network start local
Starting local network...
Network started successfully
RPC URL: http://localhost:8000/soroban/rpc
Network Passphrase: Standalone Network ; February 2017
```

### Option 2: Clear storage and redeploy contracts

If you want to keep the node running but reset the contract state, you can manually clear Soroban storage and redeploy.

**Clear all storage:**

```bash
# For standalone network, storage is in the container filesystem
# Stop the container, remove volumes, then restart
docker stop stellar-standalone
docker rm stellar-standalone
docker volume prune -f
```

**Redeploy contracts after clearing:**

```bash
# Rebuild contracts
./scripts/build.sh

# Deploy escrow contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source deployer \
  --network standalone

# Deploy oracle contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
  --source deployer \
  --network standalone
```

### Option 3: Reset specific contract instances

For development workflows that test incremental changes, you can remove individual contract instances without affecting the entire node.

**Remove a contract instance:**

```bash
# Get the contract ID
CONTRACT_ID=$(stellar contract get-id --wasm target/wasm32-unknown-unknown/release/escrow.wasm --network standalone)

# Remove the contract instance
stellar contract remove \
  --id "$CONTRACT_ID" \
  --network standalone
```

**Note:** This only removes the contract instance — you'll need to rebuild and redeploy if you made code changes.

### Common patterns

**Pattern 1: Full reset between major iterations**

```bash
# Stop and restart the network
stellar network stop local
stellar network start local

# Rebuild and redeploy
./scripts/build.sh
./scripts/deploy_local.sh  # or use your custom deploy script
```

**Pattern 2: Quick reset for minor changes**

```bash
# Just rebuild the WASM and redeploy (keeps existing network state)
./scripts/build.sh

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source deployer \
  --network standalone
```

**Pattern 3: Docker compose reset**

```bash
# Stop and remove all containers and volumes
docker compose down -v

# Rebuild and restart
docker compose up --build
```

### Verification

After resetting, verify the state is clean:

```bash
# Check network is running
stellar get-ledger --network standalone

# Confirm no contracts are deployed
stellar contract list --network standalone
# Expected: empty list or only default system contracts

# Deploy and verify
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/escrow.wasm --source deployer --network standalone
# Expected: Contract deployed successfully, contract ID printed
```

## Running the Oracle Service Locally

The oracle service (`oracle-service/`) polls active matches, checks their result on Lichess/Chess.com, and submits the verified result on-chain via Soroban RPC. It reads all configuration from environment variables (see `oracle-service/src/config.rs`) and, in debug builds, auto-loads a `.env` file from the current directory.

### Required environment variables

| Variable | Required | Description |
|----------|----------|--------------|
| `STELLAR_RPC_URL` | yes | Soroban RPC endpoint, e.g. `https://soroban-testnet.stellar.org`. |
| `STELLAR_NETWORK` | no | `testnet` (default), `mainnet`, `futurenet`, or `standalone` — used to derive the network passphrase. |
| `STELLAR_NETWORK_PASSPHRASE` | no | Overrides the passphrase derived from `STELLAR_NETWORK`. |
| `CONTRACT_ESCROW` | yes | Contract ID of the deployed escrow contract. |
| `CONTRACT_ORACLE` | yes | Contract ID of the deployed oracle contract. |
| `ORACLE_SIGNING_KEY` | yes | Hex-encoded 32-byte ed25519 seed the oracle signs transactions with. **Never commit a real key** — generate a throwaway keypair for local dev. |
| `LICHESS_API_TOKEN` | no | Personal Lichess API token; only needed for higher rate limits. |
| `CHESSDOTCOM_API_KEY` | no | Reserved for future Chess.com API auth. |
| `ORACLE_POLL_INTERVAL_SECS` | no | Poller wake interval, in seconds (default `30`). |
| `ORACLE_MAX_RETRIES` | no | Max retry attempts before an entry is dead-lettered (default `5`). |
| `ORACLE_RETRY_BASE_DELAY_SECS` | no | Base delay before the first retry; doubles each attempt (default `10`). |
| `ORACLE_QUEUE_DIR` | no | Directory for the pending/dead-letter queue files (default `./oracle-queue`). |

### Sample `.env` for local oracle development

Create `oracle-service/.env`:

```env
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
STELLAR_NETWORK=testnet
CONTRACT_ESCROW=<your-deployed-escrow-contract-id>
CONTRACT_ORACLE=<your-deployed-oracle-contract-id>

# Throwaway dev seed only — never use a mainnet key here.
ORACLE_SIGNING_KEY=0101010101010101010101010101010101010101010101010101010101010101

ORACLE_POLL_INTERVAL_SECS=10
ORACLE_MAX_RETRIES=5
ORACLE_RETRY_BASE_DELAY_SECS=5
ORACLE_QUEUE_DIR=./oracle-queue
```

### Starting the service

```bash
cd oracle-service
cargo run
```

This starts three concurrent tasks: a health/metrics HTTP endpoint on `http://localhost:8000` (`/health`, `/metrics`), a health-check poller, and the pipeline poller that watches pending matches and submits results on-chain.

### Testing against a mock Lichess/Chess.com server

Hitting the real Lichess/Chess.com APIs on every local run is slow and rate-limited. The oracle clients (`LichessClient`, `ChessComClient`) accept a configurable `api_base`, and the test suite already spins up [`wiremock`](https://docs.rs/wiremock) servers that stand in for both APIs — this is the supported way to exercise oracle result-verification logic locally without external network calls:

```bash
cd oracle-service
cargo test --test lichess_client_unit
cargo test --test chess_com_client_unit
cargo test --test pipeline_integration   # mocks both the chess API and Soroban RPC
```

Each test starts a `MockServer`, registers expected requests/responses (e.g. a completed game with a known winner), and constructs the client against the mock's local address instead of the real API host. Use these tests as a template if you need to manually reproduce a specific Lichess/Chess.com response shape — copy the `Mock::given(...)` setup from `oracle-service/tests/lichess_client_unit.rs` or `chess_com_client_unit.rs` into a scratch test to iterate against it.

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
