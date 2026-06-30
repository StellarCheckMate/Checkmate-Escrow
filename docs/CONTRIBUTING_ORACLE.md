# Contributing to the Oracle Service

This guide explains patterns and conventions for contributors working on the off-chain oracle service that bridges Chess.com and Lichess APIs to the Checkmate-Escrow smart contracts.

## Why this guide exists

`CONTRIBUTING.md` covers general workflow and testing, but oracle development requires an understanding of:

- Local oracle setup and environment configuration
- Running and writing oracle integration tests
- Adding support for new chess platform clients (e.g., Chess.com, Lichess, etc.)
- API rate limiting and error handling for external services
- Verification and signature patterns for result submission

Use this document whenever you are modifying the oracle service, adding platform support, or changing result verification logic.

## Local Setup

### Prerequisites

- Rust 1.70 or later
- Stellar CLI (for contract interaction)
- Soroban CLI (for contract testing)
- A testnet Stellar account with some XLM (free from friendbot)
- API credentials for supported chess platforms (Lichess and/or Chess.com)

### Oracle Service Installation

```bash
# Clone the repository
git clone https://github.com/Jay989810/checkmate-escrow.git
cd checkmate-escrow

# Build the oracle service
cargo build -p oracle --release

# Alternatively, build all packages
./scripts/build.sh
```

### Environment Configuration

Copy the example environment file:

```bash
cp .env.example .env
```

Configure the following for the oracle service:

```env
# Stellar configuration
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org

# Oracle account (your oracle service's Stellar address)
ORACLE_ACCOUNT=<your-oracle-public-key>
ORACLE_SECRET_KEY=<your-oracle-secret-key>

# Deployed contract addresses
CONTRACT_ESCROW=<deployed-contract-id>

# Chess platform API credentials
LICHESS_API_TOKEN=<your-lichess-api-token>
CHESSDOTCOM_API_KEY=<your-chess-com-api-key>

# Oracle service configuration
ORACLE_POLL_INTERVAL=5  # seconds
ORACLE_LOG_LEVEL=info   # debug, info, warn, error
ORACLE_VERIFY_TLS=true  # set to false for local testing only
```

### Running the Oracle Service

```bash
# Run the oracle service in development mode
cargo run -p oracle

# With environment-specific settings
STELLAR_NETWORK=testnet cargo run -p oracle

# With debug logging
RUST_LOG=oracle=debug cargo run -p oracle
```

The oracle service will:
1. Poll for pending matches in the escrow contract
2. Query Chess.com or Lichess for game results
3. Verify results meet security checks
4. Submit verified results back to the smart contract

## Oracle Testing

### Running Integration Tests

Integration tests verify the oracle's ability to:
- Connect to external chess APIs
- Parse game results correctly
- Validate match data
- Submit results to contracts

```bash
# Run all oracle tests
cargo test -p oracle

# Run with output
cargo test -p oracle -- --nocapture

# Run specific test
cargo test -p oracle test_lichess_client

# Run only integration tests (requires network access)
cargo test -p oracle --test '*' -- --test-threads=1
```

### Test Conventions

#### Test Naming

Use descriptive test names following the pattern:

```rust
test_<component>_<action>_<expected_result>
```

Examples:
- `test_lichess_client_fetches_game_correctly`
- `test_chess_com_client_parses_result_for_incomplete_game_returns_none`
- `test_result_verifier_validates_authorized_oracle_succeeds`
- `test_result_submission_with_invalid_signature_fails`

#### Mocking External APIs

When testing oracle logic, mock external chess platform APIs to avoid dependencies on live services:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Mock Chess.com API response
    struct MockChessComClient {
        game_response: Option<GameResult>,
    }

    impl MockChessComClient {
        fn new_with_result(result: GameResult) -> Self {
            Self {
                game_response: Some(result),
            }
        }

        fn fetch_game(&self, game_id: &str) -> Result<Option<GameResult>, ApiError> {
            Ok(self.game_response.clone())
        }
    }

    #[test]
    fn test_oracle_uses_mocked_chess_com_api() {
        let mock_client = MockChessComClient::new_with_result(
            GameResult {
                winner: Some("player1".to_string()),
                status: GameStatus::Completed,
            },
        );

        let result = mock_client.fetch_game("abc123");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
```

#### Environment-Based Testing

For tests that require actual API calls (integration tests), use environment variables or conditional compilation:

```rust
#[test]
#[ignore]  // Run with `cargo test -- --ignored` to include
fn test_lichess_api_real_endpoint() {
    // Only run this test if LICHESS_API_TOKEN is set
    let token = std::env::var("LICHESS_API_TOKEN").expect("LICHESS_API_TOKEN not set");
    
    let client = LichessClient::new(&token);
    let result = client.fetch_game("real-game-id");
    
    assert!(result.is_ok());
}
```

### Test Coverage Guidance

When testing oracle components, cover:

- **Happy paths**: Successful API calls and result submissions
- **API errors**: Timeout, rate limit, and network errors
- **Data validation**: Invalid game IDs, incomplete results, draw handling
- **Authorization**: Incorrect oracle signatures, unauthorized callers
- **Contract interaction**: Result submission success and failure cases

Examples of strong oracle tests:

- `test_lichess_client_handles_invalid_game_id_returns_error`
- `test_chess_com_client_handles_rate_limit_with_backoff`
- `test_oracle_result_verification_validates_game_state`
- `test_submit_result_with_invalid_oracle_signature_fails`
- `test_draw_detection_correctly_identifies_equal_outcomes`

## Adding a New Chess Platform Client

If you want to add support for a new chess platform (e.g., a third-party tournament API), follow these steps:

### 1. Define the Platform Trait

Create a trait that abstracts the chess platform API:

```rust
// In oracle/src/platform/mod.rs

pub trait ChessPlatformClient: Send + Sync {
    /// Fetch the result of a game by ID
    fn fetch_game(&self, game_id: &str) -> Result<Option<GameResult>, PlatformError>;
    
    /// Verify the game ID format is valid for this platform
    fn validate_game_id(&self, game_id: &str) -> bool;
    
    /// Get the platform name (e.g., "lichess", "chess.com")
    fn platform_name(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct GameResult {
    pub winner: Option<String>,  // None for draws
    pub status: GameStatus,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameStatus {
    InProgress,
    Completed,
    Aborted,
}
```

### 2. Implement the Client

Create a new module for your platform:

```rust
// In oracle/src/platform/new_platform.rs

use super::{ChessPlatformClient, GameResult, GameStatus, PlatformError};
use reqwest::Client as HttpClient;

pub struct NewPlatformClient {
    http_client: HttpClient,
    api_key: String,
    base_url: String,
}

impl NewPlatformClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http_client: HttpClient::new(),
            api_key,
            base_url: "https://api.newplatform.com".to_string(),
        }
    }
}

impl ChessPlatformClient for NewPlatformClient {
    fn fetch_game(&self, game_id: &str) -> Result<Option<GameResult>, PlatformError> {
        // Implement API call
        // Parse response
        // Return GameResult or error
        todo!()
    }

    fn validate_game_id(&self, game_id: &str) -> bool {
        // Implement platform-specific validation
        todo!()
    }

    fn platform_name(&self) -> &'static str {
        "new_platform"
    }
}
```

### 3. Register the Client

Add the new platform to the platform registry:

```rust
// In oracle/src/platform/mod.rs

pub mod lichess;
pub mod chess_com;
pub mod new_platform;  // Add this

pub fn create_client(platform: &str, api_key: &str) -> Result<Box<dyn ChessPlatformClient>, PlatformError> {
    match platform {
        "lichess" => Ok(Box::new(lichess::LichessClient::new(api_key))),
        "chess.com" => Ok(Box::new(chess_com::ChessComClient::new(api_key))),
        "new_platform" => Ok(Box::new(new_platform::NewPlatformClient::new(api_key))),  // Add this
        _ => Err(PlatformError::UnsupportedPlatform(platform.to_string())),
    }
}
```

### 4. Write Integration Tests

Create comprehensive tests for the new platform:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_platform_client_validates_game_id() {
        let client = NewPlatformClient::new("test_key".to_string());
        assert!(client.validate_game_id("valid_game_id"));
        assert!(!client.validate_game_id(""));
    }

    #[test]
    fn test_new_platform_client_fetches_game_correctly() {
        let client = NewPlatformClient::new("test_key".to_string());
        let result = client.fetch_game("test_game_id");
        
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_new_platform_client_handles_completed_game() {
        // Test game completion logic
    }

    #[test]
    fn test_new_platform_client_handles_draw() {
        // Test draw detection
    }

    #[test]
    fn test_new_platform_client_handles_api_error() {
        // Test error handling
    }
}
```

### 5. Update Documentation

Add the new platform to:
- `docs/oracle.md` — architecture and supported platforms
- `README.md` — feature list and roadmap
- `.env.example` — new required API credentials
- This guide — if new patterns are introduced

## Oracle Result Verification

Result verification is critical for security. Follow these patterns:

### Authorization

Only the authorized oracle address can submit results:

```rust
pub fn submit_result(
    env: &Env,
    match_id: u64,
    winner: Address,  // Or None for draw
) -> Result<(), Error> {
    // Verify the caller is the authorized oracle
    let oracle = env.storage().get(&DataKey::OracleAddress)
        .ok_or(Error::OracleNotSet)?;
    
    env.require_auth(&oracle);
    
    // Proceed with result submission
    // ...
}
```

### Result Integrity

Verify game results before submission:

```rust
pub fn verify_game_result(
    game_result: &GameResult,
    match_info: &Match,
) -> Result<(), VerificationError> {
    // Check game status is complete
    if game_result.status != GameStatus::Completed {
        return Err(VerificationError::GameNotComplete);
    }
    
    // Verify both players were participants
    let winner = game_result.winner.as_ref()
        .ok_or(VerificationError::DrawNotAllowed)?;
    
    if winner != &match_info.player1 && winner != &match_info.player2 {
        return Err(VerificationError::InvalidWinner);
    }
    
    // Verify game was played on correct platform
    if game_result.platform != match_info.platform {
        return Err(VerificationError::PlatformMismatch);
    }
    
    Ok(())
}
```

### Rate Limiting and Backoff

When querying external APIs, handle rate limits gracefully:

```rust
pub async fn fetch_with_backoff(
    client: &dyn ChessPlatformClient,
    game_id: &str,
    max_retries: usize,
) -> Result<Option<GameResult>, PlatformError> {
    let mut retries = 0;
    let mut backoff_ms = 100;
    
    loop {
        match client.fetch_game(game_id) {
            Ok(result) => return Ok(result),
            Err(PlatformError::RateLimit { retry_after }) => {
                if retries >= max_retries {
                    return Err(PlatformError::RateLimit { retry_after });
                }
                
                let wait_ms = retry_after.as_millis() as u64;
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                retries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Running Tests in CI

The oracle tests run in GitHub Actions as part of the main test suite. Before submitting a PR:

1. Run all tests locally:
   ```bash
   cargo test -p oracle
   ```

2. Verify your code passes linting:
   ```bash
   cargo fmt -p oracle
   cargo clippy -p oracle
   ```

3. For integration tests with live APIs:
   ```bash
   cargo test -p oracle -- --ignored --nocapture
   ```

## Common Issues and Troubleshooting

### "API credentials not configured"

Ensure your `.env` file is set up correctly and sourced:

```bash
source .env
cargo run -p oracle
```

### Rate limit errors

If the oracle is hitting rate limits:
- Increase `ORACLE_POLL_INTERVAL` in `.env`
- Implement exponential backoff in the platform client
- Check API documentation for current rate limits

### Contract interaction failures

If result submission fails:
- Verify the oracle address is authorized in the escrow contract
- Check the match ID exists and is in a valid state
- Ensure the Stellar account has sufficient XLM for fees

### Test timeouts

If tests timeout when hitting live APIs:
- Use mock clients for unit tests
- Mark integration tests with `#[ignore]`
- Increase timeout in test configuration

## Code Review Checklist

Before submitting a PR that modifies the oracle service, ensure:

- [ ] All existing tests pass: `cargo test -p oracle`
- [ ] New platform clients implement the `ChessPlatformClient` trait
- [ ] Integration tests follow naming conventions (`test_<component>_<action>_<result>`)
- [ ] API errors are handled gracefully with backoff
- [ ] Authorization is verified before result submission
- [ ] Rate limiting and retry logic is implemented
- [ ] Documentation is updated for new platforms or API changes
- [ ] Code passes formatting and linting: `cargo fmt && cargo clippy`
- [ ] PR description explains the changes and any new dependencies

## Performance Considerations

- **API call latency**: Chess platform APIs may take 1-5 seconds to respond
- **Contract submission**: Result submission transactions take 5-30 seconds to finalize on testnet
- **Polling frequency**: Default 5-second poll interval balances responsiveness and API quota usage
- **Database queries**: Oracle caches frequently accessed match data to reduce RPC calls

## Where to Update This Guide

If you add a new platform client, introduce a new verification pattern, or change oracle architecture, update this guide to reflect the changes.

## Getting Help

- Ask questions in GitHub Discussions
- Review existing platform clients (Lichess, Chess.com) for patterns
- Check `docs/oracle.md` for architecture details
- Open an issue for bugs or improvement suggestions
