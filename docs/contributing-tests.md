# Writing Tests for Checkmate-Escrow

This guide explains how to write tests for the Checkmate-Escrow smart contracts using Soroban's test environment.

## Quick Start

To run tests:

```bash
cargo test                    # Run all tests
cargo test -p escrow          # Run escrow contract tests only
cargo test test_my_function   # Run specific test by name
cargo test -- --nocapture    # Show println! output during tests
```

---

## Test Structure

### Directory Organization

Tests are organized in the `contracts/escrow/src/tests/` directory:

```
contracts/escrow/src/tests/
├── mod.rs                     # Test module hub, setup() fixture
├── helpers.rs                 # Shared helper functions
├── lifecycle.rs               # Lifecycle tests (initialize, state transitions)
├── integration.rs             # Full end-to-end flows
├── admin.rs                   # Admin operations and authorization
├── events.rs                  # Event verification
├── validation.rs              # Input validation
├── deposit.rs                 # Deposit-specific tests
└── ...                        # Other feature-specific test modules
```

Each module tests a specific area of functionality. New features should have their own test module or be added to an existing one.

### Module Pattern

Every test module follows this pattern:

```rust
use super::*;  // Import everything from parent (mod.rs)

#[test]
fn test_something() {
    // Test implementation
}
```

The `use super::*;` imports are critical — they bring in the Soroban SDK types, contract types, and test setup fixtures.

---

## Soroban Test Environment

### Initializing the Environment

Every test starts by creating a Soroban test environment:

```rust
let env = Env::default();
env.mock_all_auths();  // Mock all authorization checks
```

**What does this do?**
- `Env::default()` creates an in-memory Soroban test environment
- `mock_all_auths()` disables actual cryptographic signature verification, so you don't need to sign transactions. Perfect for unit tests.

### Registering the Contract

After creating the environment, register your contract:

```rust
let contract_id = env.register_contract(None, EscrowContract);
let client = EscrowContractClient::new(&env, &contract_id);
```

**What does this do?**
- `register_contract(None, EscrowContract)` deploys the contract to the test environment and returns its ID
- `EscrowContractClient` provides a type-safe client for calling contract functions

### Setting Up the Full Environment

For most tests, use the shared `setup()` fixture in `mod.rs`:

```rust
let (env, contract_id, oracle, player1, player2, token_addr, admin) = setup();
let client = EscrowContractClient::new(&env, &contract_id);
```

**What does `setup()` do?**
- Creates the Soroban environment with `mock_all_auths()`
- Registers the contract and initializes it
- Creates test addresses for oracle, two players, and admin
- Creates and mints a mock token with 1000 units per player
- Returns all addresses for use in tests

This is the fastest way to get started with a fully-initialized contract.

---

## Mocking Addresses

### Generating Random Addresses

```rust
let address = Address::generate(&env);
```

This creates a unique mock address that can sign transactions in the test environment.

### Using Addresses as Signers

When a contract function requires authorization (via `require_auth()`), you sign with the address:

```rust
let player = Address::generate(&env);

// This will pass authorization checks for `player`
client.deposit(&match_id, &player);
```

With `mock_all_auths()` enabled, you don't need to actually sign — just pass the address and the auth check succeeds.

### Typical Address Roles

In most tests, create addresses for different roles:

```rust
let admin = Address::generate(&env);
let oracle = Address::generate(&env);
let player1 = Address::generate(&env);
let player2 = Address::generate(&env);
```

---

## Working with Tokens

### Creating and Minting Tokens

The `setup()` fixture automatically creates a mock token contract. For custom tests:

```rust
// Register a mock Stellar token contract
let token = env.register_stellar_asset_contract_v2(admin.clone());

// Get a token client
let tc = soroban_sdk::token::Client::new(&env, &token);

// Mint tokens to a player
tc.mint(&player1, &1000);
```

### Checking Balances

```rust
let tc = soroban_sdk::token::Client::new(&env, &token);
let balance = tc.balance(&player1);
assert_eq!(balance, 1000);
```

---

## Helper Functions

The `helpers.rs` module provides utilities to reduce boilerplate:

### Match Creation Helpers

```rust
// Create a match with default stake (100) and Lichess platform
let match_id = create_default_match(
    &client, &env, &player1, &player2, &token, "my_game_id"
);

// Create a match with custom stake
let match_id = create_match_with_stake(
    &client, &env, &player1, &player2, &token, "my_game_id", 250
);
```

### Funding a Match

```rust
// Deposit for both players in sequence
fund_match(&client, match_id, &player1, &player2);
```

### Full Match Lifecycle Helper

```rust
// Create → fund → submit result → claim payout all in one call
run_full_match(
    &client, &env, &player1, &player2, &token, "game_id", &Winner::Player1
);
```

### Balance Snapshots

```rust
// Capture balances before an operation
let before = BalanceSnapshot::capture(&env, &token, &player1, &player2, &contract_id);

// Do something...
client.deposit(&match_id, &player1);

// Verify invariant: total tokens should remain constant
assert_total_balance(&env, &token, &player1, &player2, &contract_id, before.total());
```

---

## Testing Error Cases

### Using the `try_` Pattern (Preferred)

Always prefer the `try_` variant to assert the *exact* error:

```rust
#[test]
fn test_create_match_with_zero_stake_returns_invalid_amount() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &0,  // Zero stake should fail
        &token,
        &String::from_str(&env, "game"),
        &Platform::Lichess,
    );

    // Assert the exact error variant
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}
```

**Why?** The `try_` pattern:
- Asserts the *exact* error, not just "something panicked"
- Makes failures easier to diagnose
- Prevents accidental test passes due to unrelated panics

### Avoiding `#[should_panic]`

Only use `#[should_panic]` when the contract panics with a string message (rare):

```rust
#[test]
#[should_panic(expected = "assertion failed")]
fn test_internal_assertion_fails() {
    // ... only if the contract panics with a plain string
}
```

---

## Event Verification

### Capturing and Verifying Events

```rust
#[test]
fn test_match_creation_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "event_test_game"),
        &Platform::Lichess,
    );

    // Capture all events from the environment
    let events = env.events().all();

    // Find the match creation event (topic is ("match", "created"))
    let created_event = events.iter().find(|e| {
        e.0.topics() == (
            Symbol::new(&env, "match"),
            symbol_short!("created"),
        )
    });

    assert!(created_event.is_some(), "match creation event not emitted");
}
```

---

## State Verification

### Reading Contract State

```rust
// Get a match to verify its state
let m = client.get_match(&match_id);
assert_eq!(m.state, MatchState::Pending);
assert_eq!(m.player1, player1);
assert_eq!(m.stake_amount, 100);

// Verify escrow balance
assert_eq!(client.get_escrow_balance(&match_id), 0);

// Check if funding is complete
assert!(!client.is_funded(&match_id));  // Only one player has deposited
```

### Checking Admin/Oracle State

```rust
// Verify admin is set correctly
assert_eq!(client.get_admin(), admin);

// Verify oracle is set
assert_eq!(client.get_oracle(), oracle);
```

---

## Test Naming Convention

Use descriptive names that follow this pattern:

```
test_<function>_<condition>_<expected_result>
```

**Examples:**
- `test_create_match_with_zero_stake_returns_invalid_amount`
- `test_deposit_after_both_players_fund_returns_already_funded`
- `test_submit_result_before_funding_returns_not_funded`
- `test_cancel_match_on_active_match_returns_match_already_active`

This makes it obvious what the test does without reading the implementation.

---

## Common Testing Patterns

### Pattern: Setup → Action → Verify

```rust
#[test]
fn test_deposit_increases_escrow_balance() {
    // Setup
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let match_id = create_default_match(&client, &env, &player1, &player2, &token, "test_game");

    // Action
    client.deposit(&match_id, &player1);

    // Verify
    assert_eq!(client.get_escrow_balance(&match_id), 100);
}
```

### Pattern: Error Case with `try_`

```rust
#[test]
fn test_action_with_invalid_input_fails() {
    // Setup
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Action & Verify
    let result = client.try_action_with_invalid_input(...);
    assert!(result.is_err());  // or assert_eq!(result, Err(Ok(Error::SomeError)))
}
```

### Pattern: Multi-Step Lifecycle

```rust
#[test]
fn test_full_match_lifecycle() {
    // Setup
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Step 1: Create match
    let match_id = client.create_match(...);
    assert_eq!(client.get_match(&match_id).state, MatchState::Pending);

    // Step 2: Fund match
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);

    // Step 3: Submit result
    client.submit_result(&match_id, &Winner::Player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
}
```

---

## Debugging Tests

### Viewing Output During Tests

```bash
cargo test test_name -- --nocapture
```

Add `println!` statements in your test to debug:

```rust
#[test]
fn test_something() {
    let (env, contract_id, ...) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(...);
    println!("Created match: {}", match_id);

    let m = client.get_match(&match_id);
    println!("Match state: {:?}", m.state);
    println!("Escrow balance: {}", client.get_escrow_balance(&match_id));
}
```

### Inspecting Errors

When a test fails, Rust's assertion output shows:

```
assertion failed: `(left == right)`
  left: `Err(Ok(InvalidAmount))`,
 right: `Ok(100)`
```

This tells you exactly what error was returned vs. what was expected.

---

## Running Tests in CI

The repository runs tests in CI via GitHub Actions. Before pushing:

```bash
# Run all tests
cargo test

# Run tests for a specific contract
cargo test -p escrow
cargo test -p oracle

# Run a specific test
cargo test test_create_match
```

All tests must pass before a PR can be merged.

---

## Annotated Example: Trivial Test

Here's a complete, minimal test that verifies a single behavior:

```rust
/// This test verifies that creating a match with zero stake fails with InvalidAmount.
///
/// It's a trivial test because it:
/// 1. Has a single action (create_match with stake=0)
/// 2. Verifies one specific error case
/// 3. Doesn't depend on other contract state
#[test]
fn test_create_match_with_zero_stake_returns_invalid_amount() {
    // --- Setup ---
    // Create the test environment and contract
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // --- Action ---
    // Attempt to create a match with invalid stake amount
    let result = client.try_create_match(
        &player1,
        &player2,
        &0,  // <- Invalid: stake must be > 0
        &token,
        &String::from_str(&env, "game_id"),
        &Platform::Lichess,
    );

    // --- Verify ---
    // Assert that we got the exact error we expected
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Expected InvalidAmount error for zero stake"
    );
}
```

---

## Annotated Example: Medium Complexity Test

Here's a complete test that exercises multiple contract functions:

```rust
/// This test verifies the full match lifecycle for a player 1 victory:
/// 1. Create a match (Pending state)
/// 2. Both players deposit (Active state)
/// 3. Oracle submits result (Completed state)
/// 4. Winner receives the full pot
///
/// It's medium complexity because it:
/// 1. Has multiple sequential actions (4 contract calls)
/// 2. Verifies state transitions at each step
/// 3. Checks both contract state and token balances
/// 4. Uses the try_ pattern for fallible operations
#[test]
fn test_full_lifecycle_winner_receives_pot() {
    // --- Setup ---
    // Initialize the test environment with two players and a token
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let stake: i128 = 100;

    // --- Step 1: Create Match ---
    // Create a match with stake of 100 units per player
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "full_lifecycle_game"),
        &Platform::Lichess,
    );

    // Verify the match is in Pending state
    let match_state = client.get_match(&match_id);
    assert_eq!(
        match_state.state,
        MatchState::Pending,
        "New match must start in Pending state"
    );
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "Escrow should be empty before deposits"
    );

    // --- Step 2: Both Players Deposit ---
    // Record balances before deposits
    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    // Player 1 deposits their stake
    client.deposit(&match_id, &player1);

    // Match should still be Pending (only one player has deposited)
    let match_state = client.get_match(&match_id);
    assert_eq!(
        match_state.state,
        MatchState::Pending,
        "Match should still be Pending after one deposit"
    );

    // Player 2 deposits their stake (this activates the match)
    client.deposit(&match_id, &player2);

    // Verify the match is now Active (both players funded)
    let match_state = client.get_match(&match_id);
    assert_eq!(
        match_state.state,
        MatchState::Active,
        "Match must be Active after both players deposit"
    );
    assert_eq!(
        client.get_escrow_balance(&match_id),
        stake * 2,
        "Escrow should hold both stakes"
    );
    assert!(
        client.is_funded(&match_id),
        "Match must report is_funded=true after both deposits"
    );

    // --- Step 3: Oracle Submits Result ---
    // The oracle (oracle from setup) submits that player1 won
    client.submit_result(&match_id, &Winner::Player1);

    // --- Step 4: Verify Payout ---
    // After result submission, the match should be Completed
    let match_state = client.get_match(&match_id);
    assert_eq!(
        match_state.state,
        MatchState::Completed,
        "Match must be Completed after result submission"
    );

    // Escrow should be empty (payout already executed)
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "Escrow must be empty after payout"
    );

    // Verify token distribution:
    // - Winner (player1) receives both stakes (p1_before - stake + pot)
    // - Loser (player2) loses their stake (p2_before - stake)
    let p1_after = tc.balance(&player1);
    let p2_after = tc.balance(&player2);

    assert_eq!(
        p1_after,
        p1_before + stake * 2,
        "Winner should receive the full pot (both stakes)"
    );
    assert_eq!(
        p2_after,
        p2_before - stake,
        "Loser should lose their stake"
    );
}
```

---

## Tips for Writing Good Tests

1. **One assertion per test, when possible** — Easier to understand what failed
2. **Use descriptive test names** — Should be able to read the name and know what's tested
3. **Add comments for non-obvious steps** — Future maintainers will thank you
4. **Use the `try_` pattern** — More precise error testing
5. **Verify state at multiple points** — Catch issues early in the lifecycle
6. **Test error cases** — Happy path only gets you so far
7. **Use helper functions** — Reduces boilerplate and improves readability
8. **Keep setup minimal** — Only create what the test needs
9. **Test invariants** — Like "total tokens never change"
10. **Document your test with a doc comment** — Explain the "why"

---

## See Also

- [Soroban Rust SDK Documentation](https://soroban.stellar.org/docs)
- [Smart Contract Testing Guide](contributing-contracts.md)
- [Error Codes Reference](error-codes.md)

