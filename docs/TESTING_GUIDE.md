# Testing & Coverage Guide

## Overview

This guide provides comprehensive instructions for running tests, generating coverage reports, and interpreting security test results for the Checkmate-Escrow smart contract.

## Quick Start

### Running All Tests

```bash
cd contracts/escrow
cargo test --lib
```

### Running Security Tests Only

```bash
cd contracts/escrow
cargo test --lib security::
```

### Running with Output

```bash
cd contracts/escrow
cargo test --lib -- --nocapture
```

## Test Organization

The test suite is organized into logical modules:

### 1. **Admin Tests** (`tests/admin.rs`)
- Pause/unpause functionality
- Token allowlist management
- Admin authorization checks

### 2. **Lifecycle Tests** (`tests/lifecycle.rs`)
- Match creation
- Deposit handling
- Match completion and cancellation
- State transitions

### 3. **Authorization & Access Control** (`tests/admin.rs`)
- Player authorization
- Oracle authorization
- Admin-only functions
- Role-based access

### 4. **Invariants Tests** (`tests/invariants.rs`)
- Token balance conservation
- Match state consistency
- Player validation
- Arithmetic safety

### 5. **Events Tests** (`tests/events.rs`)
- Event emission verification
- Event data correctness
- Event sequencing

### 6. **Security Tests** (`tests/security.rs`) ⭐
- **Fuzz Testing**: Stake amounts, game IDs, player addresses
- **Authorization Fuzzing**: Access control boundary testing
- **Attack Vectors**:
  - Double deposit attacks
  - Invalid state transition attacks
  - Allowlist bypass attempts
  - Contract pause bypass attempts
  - Same player attacks
  - Contract as player attacks
  - Duplicate game ID attacks
  - Overflow protection testing
- **Invariant Validation**: Token preservation, state consistency

## Coverage Reports

### Generating Coverage Reports

Using tarpaulin (recommended):

```bash
cd contracts/escrow
cargo install cargo-tarpaulin  # One-time setup
cargo tarpaulin --out Html --output-dir coverage
```

Using llvm-cov:

```bash
cd contracts/escrow
cargo install cargo-llvm-cov  # One-time setup
cargo llvm-cov --html
```

### Coverage Thresholds

The project enforces minimum coverage levels:

- **Line Coverage**: 90% minimum
- **Branch Coverage**: 85% minimum

These thresholds are enforced in CI/CD pipeline (`coverage.yml`).

### Interpreting Coverage Reports

1. **Html Report**: Open `coverage/index.html` in a browser
   - Green: Fully covered
   - Yellow/Orange: Partially covered
   - Red: Not covered

2. **LCOV Report**: Can be imported into IDEs (VS Code, JetBrains)
   ```bash
   cargo tarpaulin --out Lcov
   ```

3. **Command Line Summary**:
   ```bash
   cargo tarpaulin --out Stdout
   ```

## Test Categories

### By Concern

#### Authorization & Access Control
```bash
cargo test --lib admin
cargo test --lib security::test_security_unauthorized
cargo test --lib security::test_security_oracle
```

#### State Management
```bash
cargo test --lib lifecycle
cargo test --lib invariants
```

#### Token Operations
```bash
cargo test --lib token_allowlist
```

#### Security & Attack Prevention
```bash
cargo test --lib security
```

### By Function

#### Match Creation
```bash
cargo test --lib create_match
```

#### Deposit Functionality
```bash
cargo test --lib deposit
```

#### Result Submission
```bash
cargo test --lib submit_result
```

#### Match Cancellation
```bash
cargo test --lib cancel_match
```

## Security Test Details

### Fuzzing Tests

The security module includes comprehensive fuzz tests for critical inputs:

#### Stake Amount Fuzzing
- Minimum valid amounts (1)
- Normal amounts (100)
- Large amounts (i128::MAX / 2)
- Invalid amounts (0, negative)
- Overflow boundary testing

#### Game ID Fuzzing
- Empty strings (rejected)
- Minimum length (1 byte)
- Typical length (8 bytes for Lichess)
- Maximum length (64 bytes)
- Over-length strings (rejected)

#### Player Address Fuzzing
- Same player validation
- Contract as player validation
- Invalid player rejection

### Attack Vector Tests

#### Double Deposit Attack
Ensures players cannot deposit twice:
```rust
test_security_double_deposit_attack()
```

#### Invalid State Transition
Prevents operations in wrong states:
```rust
test_security_cancel_completed_match_attack()
test_security_cancel_active_match_attack()
```

#### Allowlist Bypass
Validates token allowlist enforcement:
```rust
test_security_allowlist_bypass_attempt()
```

#### Pause Contract Bypass
Ensures pause blocks all critical operations:
```rust
test_security_create_match_when_paused()
test_security_deposit_when_paused()
test_security_submit_result_when_paused()
```

### Invariant Tests

Run invariant-based tests:
```bash
cargo test --lib invariant
```

## CI/CD Integration

### GitHub Actions

Two workflows handle testing and coverage:

1. **CI Workflow** (`ci.yml`)
   - Runs on: push to main/master, all PRs
   - Tasks:
     - Unit tests
     - WASM build
     - Link validation
     - API reference checks

2. **Coverage Workflow** (`coverage.yml`)
   - Runs on: push/PR to main/master with contract changes
   - Tasks:
     - Generates coverage report
     - Enforces minimum thresholds (90% line coverage)
     - Comments on PRs with coverage results
     - Uploads coverage artifact

### Local Pre-commit Testing

Run this before pushing:

```bash
#!/bin/bash
cd contracts/escrow
cargo test --lib || exit 1
cargo tarpaulin --out Stdout || exit 1
echo "✅ All tests passed and coverage meets requirements"
```

## Troubleshooting

### Test Failures

1. **Timeout errors**: Increase timeout in Cargo.toml
   ```toml
   [profile.test]
   opt-level = 1
   ```

2. **Mock auth issues**: Ensure `env.mock_all_auths()` is called
   ```rust
   env.mock_all_auths();  // Required for Soroban tests
   ```

3. **Token balance mismatches**: Verify mint amounts in setup
   ```rust
   asset_client.mint(&player1, &1000);  // Sufficient balance required
   ```

### Coverage Not Meeting Thresholds

1. **Identify uncovered code**:
   ```bash
   cargo tarpaulin --out Html
   # Open coverage/index.html and look for red lines
   ```

2. **Add tests for uncovered paths**:
   - Look at red lines in coverage report
   - Add test cases that exercise those paths
   - Document why path is uncovered (if intentional)

3. **Check if code is dead**:
   ```bash
   cargo dead-code  # Or use clippy lints
   ```

## Best Practices

### Writing Tests

1. **Use descriptive names**: `test_security_double_deposit_attack`
2. **Test one thing**: Each test should have a single assertion or tight group
3. **Use fixtures**: Leverage `setup()`, `setup_with_funded_match()`
4. **Verify both success and failure paths**

### Security Testing

1. **Fuzz inputs**: Test boundary values, invalid ranges
2. **Test authorization**: Verify auth checks are enforced
3. **Test state transitions**: Ensure invalid transitions are blocked
4. **Test invariants**: Verify contract invariants hold

### Coverage Maintenance

1. **Keep coverage > 90%**: Non-negotiable minimum
2. **Document uncovered code**: Add comments explaining why
3. **Prioritize security functions**: Higher coverage for auth, state, transfers
4. **Regular audits**: Periodically review coverage trends

## Performance Benchmarks

### Test Execution Time

```
Full test suite: ~10-15s
Security tests only: ~3-5s
Coverage generation: ~20-30s
```

Times vary based on machine specs and background processes.

## Test Data & Fixtures

### Standard Setup
```rust
let (env, contract_id, oracle, player1, player2, token, admin) = setup();
```

Provides:
- Initialized contract
- Two players with 1000 tokens each
- Ready-to-use oracle and admin addresses

### With Funded Match
```rust
let (env, contract_id, oracle, player1, player2, token, admin, match_id) = setup_with_funded_match();
```

Adds:
- Active match (both players deposited)
- Ready for result submission

## Coverage Badges

Generate badge for README:

```markdown
[![Coverage Status](https://img.shields.io/badge/coverage-95%25-brightgreen)]()
```

Update after running coverage reports.

## References

- [Cargo Testing Guide](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Tarpaulin Documentation](https://github.com/xd009642/tarpaulin)
- [Soroban Testing](https://developers.stellar.org/docs/smart-contracts/getting-started/hello-world#test)
- [Quickcheck for Property Testing](https://docs.rs/quickcheck/latest/quickcheck/)
