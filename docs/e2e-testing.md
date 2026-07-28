# End-to-End Testing Guide

This guide explains how to run the Checkmate-Escrow end-to-end (E2E) test suite
against the Stellar testnet using real wallets and real contract deployments.

## What the E2E tests cover

The E2E test suite (`oracle-service/tests/e2e_tests.rs`) exercises the full
oracle pipeline with no mocks:

| Test | What it validates |
|------|-------------------|
| `e2e_testnet_rpc_is_reachable` | Testnet RPC endpoint responds and returns a valid ledger sequence |
| `e2e_soroban_client_constructs_from_testnet_config` | `SorobanClient` builds without error from real testnet addresses |
| `e2e_lichess_fetch_completed_game_result` | Lichess HTTP client fetches and parses a real completed game |
| `e2e_lichess_nonexistent_game_returns_not_found` | Invalid Lichess game ID returns `GameNotFound`, not a panic |
| `e2e_chess_com_fetch_completed_game_result` | Chess.com client fetches a public archived game |
| `e2e_provider_registry_resolves_lichess_game` | Multi-provider registry resolves a Lichess game with correct winner |
| `e2e_oracle_config_round_trip` | Oracle config key material round-trips through hex decode correctly |
| `e2e_testnet_ledger_is_progressing` | Testnet ledger advances between two polls (not stalled) |
| `e2e_queue_persists_and_reloads_entries` | Pending queue persists entries to disk and reloads them correctly |
| `e2e_dead_letter_store_round_trip` | Dead-letter store appends and reloads failed entries correctly |
| `e2e_health_check_returns_ok` | Health check response serializes/deserializes without error |

## Prerequisites

### 1. Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

### 2. Stellar CLI

```bash
cargo install stellar-cli --features opt
```

### 3. Testnet wallets

Generate and fund three testnet identities — one for the oracle, and two for
the match players:

```bash
# Oracle signing key
stellar keys generate oracle-e2e --network testnet
stellar keys show oracle-e2e         # note the G-address
stellar keys show --private oracle-e2e | xxd -p  # note the hex seed

# Player wallets
stellar keys generate player1-e2e --network testnet
stellar keys generate player2-e2e --network testnet

# Fund via Friendbot (testnet only — no real XLM needed)
curl "https://friendbot.stellar.org?addr=$(stellar keys address oracle-e2e)"
curl "https://friendbot.stellar.org?addr=$(stellar keys address player1-e2e)"
curl "https://friendbot.stellar.org?addr=$(stellar keys address player2-e2e)"
```

### 4. Deploy the contracts

Follow the [Deployment Guide](deployment.md) to deploy the escrow and oracle
contracts to testnet, then note the contract C-addresses:

```bash
./scripts/deploy_testnet.sh
# Note the output:
#   Escrow contract: C...
#   Oracle contract: C...
```

## Environment variables

Export the following variables before running the tests.  Only
`E2E_CONTRACT_ESCROW`, `E2E_CONTRACT_ORACLE`, `E2E_ORACLE_KEY_HEX`, and
`E2E_ORACLE_ADDRESS` are required; all others have sensible defaults.

```bash
# Required
export E2E_CONTRACT_ESCROW="C<56 chars>"
export E2E_CONTRACT_ORACLE="C<56 chars>"
export E2E_ORACLE_KEY_HEX="<64 hex chars>"   # 32-byte seed, hex-encoded
export E2E_ORACLE_ADDRESS="G<55 chars>"       # matches E2E_ORACLE_KEY_HEX

# Optional (defaults shown)
export E2E_RPC_URL="https://soroban-testnet.stellar.org"
export E2E_NETWORK_PHRASE="Test SDF Network ; September 2015"
export E2E_LICHESS_TOKEN=""         # Lichess bearer token for higher rate limits
export E2E_CHESSDOTCOM_KEY=""       # Chess.com developer API key
```

### Getting the oracle key hex

```bash
# Stellar CLI stores keys in ~/.config/stellar/identity/<name>.toml
# The raw 32-byte seed can be extracted as:
stellar keys show --private oracle-e2e
# Output looks like: SB... (Stellar secret key strkey)
# Convert to hex:
python3 -c "
import base64, sys
raw = sys.stdin.read().strip()
# Decode strkey (S...) → 32-byte seed
import struct
decoded = base64.b32decode(raw[1:] + '=' * ((8 - len(raw[1:]) % 8) % 8))
print(decoded[1:-2].hex())  # strip version byte and checksum
"
# Or use the stellar-strkey library directly in Rust if you prefer.
```

> **Security note**: Never commit `E2E_ORACLE_KEY_HEX` to source control.
> Use a `.env` file (listed in `.gitignore`) or inject it via your CI
> secrets manager.

## Running the tests

```bash
# All E2E tests (sequential to avoid rate-limit collisions)
cargo test -p oracle-service --test e2e_tests -- --nocapture --test-threads=1

# Single test
cargo test -p oracle-service --test e2e_tests e2e_testnet_rpc_is_reachable -- --nocapture

# Skip tests that require env vars (CI without testnet credentials)
# — tests skip automatically when E2E_CONTRACT_ESCROW is unset —
cargo test -p oracle-service --test e2e_tests -- --nocapture
```

## CI integration

Add the following job to your GitHub Actions workflow to run E2E tests in CI
with testnet credentials stored as repository secrets:

```yaml
e2e-tests:
  name: E2E Testnet Tests
  runs-on: ubuntu-latest
  if: github.event_name == 'push' && github.ref == 'refs/heads/main'
  env:
    E2E_CONTRACT_ESCROW: ${{ secrets.E2E_CONTRACT_ESCROW }}
    E2E_CONTRACT_ORACLE: ${{ secrets.E2E_CONTRACT_ORACLE }}
    E2E_ORACLE_KEY_HEX:  ${{ secrets.E2E_ORACLE_KEY_HEX }}
    E2E_ORACLE_ADDRESS:  ${{ secrets.E2E_ORACLE_ADDRESS }}
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Run E2E tests
      run: |
        cargo test -p oracle-service --test e2e_tests \
          -- --nocapture --test-threads=1
```

> **Recommended**: Run E2E tests only on `main` pushes (not on every PR) to
> avoid exhausting Friendbot and testnet rate limits.

## Funded testnet wallets in CI

For a fully automated pipeline, generate fresh testnet wallets in CI and fund
them via Friendbot:

```yaml
- name: Generate and fund testnet wallets
  run: |
    stellar keys generate ci-oracle --network testnet
    ORACLE_ADDR=$(stellar keys address ci-oracle)
    curl -s "https://friendbot.stellar.org?addr=$ORACLE_ADDR" | jq .
    echo "E2E_ORACLE_ADDRESS=$ORACLE_ADDR" >> $GITHUB_ENV

    # Export the raw key for E2E_ORACLE_KEY_HEX
    # (implement key extraction per your CI platform's secret handling)
```

## Interpreting test output

Each test prints a status line prefixed with `[e2e]`:

```
[e2e] testnet_rpc_is_reachable: latest ledger sequence=12345678
[e2e] lichess_fetch_completed_game_result: winner=Player1 OK
[e2e] SKIP: chess_com_fetch_completed_game_result: rate limited by chess.com; retry after 60s
```

- `OK` — test passed
- `SKIP` — test was skipped due to missing credentials or unavailable network;
  this is not a failure
- A test failure (non-zero exit code) indicates a real regression

## Verifying payout transactions on the testnet ledger

After a match completes on testnet, verify the payout transaction using the
Stellar Explorer or the CLI:

```bash
# Look up the winning player's transaction history
stellar operations list --account <PLAYER_G_ADDRESS> --network testnet

# Or use Stellar Expert:
# https://stellar.expert/explorer/testnet/account/<PLAYER_G_ADDRESS>
```

The payout appears as a `payment` operation from the escrow contract address to
the winner's address for the `2 × stake_amount` of the match token.

## Troubleshooting

### "required E2E environment variables not set"

The test was skipped because one or more required env vars were absent.
Set them as described in the [Environment variables](#environment-variables)
section and re-run.

### "RPC endpoint unreachable"

The testnet RPC is temporarily unavailable or your network blocks outbound HTTPS.
Check the [Stellar status page](https://status.stellar.org) and retry.

### "contract address invalid"

The `E2E_CONTRACT_ESCROW` or `E2E_CONTRACT_ORACLE` value is malformed.
Valid Soroban contract C-addresses start with `C` and are 56 characters long.

### "oracle signing key must not be all-zeros"

`E2E_ORACLE_KEY_HEX` was set to all zeros or was decoded to zeros.  Regenerate
the key with `stellar keys generate` and export the correct hex seed.

### Rate limiting

The Lichess and Chess.com tests use conservative rate limits (0.5 req/s).  If
you hit rate limits during development, add a short `sleep` between test runs
or use the `E2E_LICHESS_TOKEN` variable to authenticate for higher limits.

## Related documentation

- [Oracle Design](oracle.md) — how the oracle pipeline fetches and verifies results
- [Deployment Guide](deployment.md) — deploying contracts to testnet
- [Local Development Setup](local-dev.md) — running the full stack locally
- [Error Codes Reference](error-codes.md) — contract error codes and recovery
