# Contributing to Checkmate-Escrow

Thanks for your interest in contributing to Checkmate-Escrow! This guide will help you get started.

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Soroban CLI
- Stellar CLI
- Git

### Setup

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/your-username/checkmate-escrow.git
   cd checkmate-escrow
   ```
3. Set up your environment:
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```
4. Build the project:
   ```bash
   ./scripts/build.sh
   ```
5. Run tests to verify your setup:
   ```bash
   ./scripts/test.sh
   ```

## Development Workflow

### Creating a Branch

Create a feature branch from `main`:

```bash
git checkout -b feature/your-feature-name
```

Use descriptive branch names:
- `feature/` for new features
- `fix/` for bug fixes
- `docs/` for documentation updates
- `refactor/` for code refactoring

### Making Changes

1. Write clear, concise commit messages
2. Keep commits focused on a single change
3. Add tests for new functionality
4. Update documentation as needed

### Testing

Run the full test suite before submitting:

```bash
cargo test
```

For specific contract tests:

```bash
cargo test -p escrow
cargo test -p oracle
```

### Code Style

- Follow Rust standard formatting: `cargo fmt`
- Run clippy for linting: `cargo clippy`
- Keep functions small and focused
- Add comments for complex logic
- Use descriptive variable names

### Documentation

- Update relevant documentation in `docs/` for architectural changes
- Add inline comments for non-obvious code
- Add or update Soroban contract guidance in `docs/contributing-contracts.md` when changing contract storage, TTL, or auth behavior
- Add or update oracle guidance in `docs/CONTRIBUTING_ORACLE.md` when changing the oracle service, adding platform clients, or modifying result verification
- Update README.md if adding new features or changing setup steps
- Check repository health and link validity with `./scripts/repository_health_check.sh`

See [docs/repository-health-checklist.md](docs/repository-health-checklist.md) for checklist details.

## Updating the Changelog

Every pull request **must** include a changelog entry. This keeps the release history accurate and helps reviewers understand the impact of a change at a glance.

### Where to add your entry

Open `CHANGELOG.md` and add your entry under the `## [Unreleased]` section at the top of the file. If that section does not exist yet, add it directly below the introductory paragraph.

### Entry format

The project follows the [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) convention. Use one of these subsection headings — add only the ones that apply:

```markdown
## [Unreleased]

### Added
- Short description of a new feature or capability.

### Changed
- Short description of a change to existing behavior.

### Fixed
- Short description of a bug that was fixed.

### Removed
- Short description of something that was removed.
```

### Full entry template

Copy the block below into `CHANGELOG.md` under `## [Unreleased]` and delete the subsections you don't need:

```markdown
### Added
- <!-- Describe new features or functions introduced by this PR -->

### Changed
- <!-- Describe modifications to existing behavior, APIs, or configuration -->

### Fixed
- <!-- Describe bugs or incorrect behavior that this PR corrects -->

### Removed
- <!-- Describe features, flags, or APIs that were deleted -->
```

### Guidelines

- Write entries from the perspective of a **user or integrator**, not an implementer. Explain *what changed* and *why it matters*, not *how* the code was restructured.
- Use the past tense and start with a capital letter: `Added support for …`, `Fixed incorrect payout when …`
- One bullet per logical change. If a PR touches multiple independent areas, add one bullet for each.
- Do **not** include internal refactors or test-only changes unless they affect observable behavior.
- When a version is released, maintainers will move `[Unreleased]` entries into a new versioned section (e.g. `## [1.1.0] - 2026-08-01`). You don't need to do this yourself.

## Submitting a Pull Request

1. Push your branch to your fork:
   ```bash
   git push origin feature/your-feature-name
   ```
2. Open a Pull Request against the `main` branch
3. Fill out the PR template with:
   - Clear description of changes
   - Related issue numbers (if applicable)
   - Testing performed
   - Screenshots (for UI changes)
4. Wait for review and address feedback

### PR Guidelines

- Keep PRs focused on a single feature or fix
- Ensure all tests pass
- Update documentation
- Respond to review comments promptly
- Squash commits if requested

### Code Owners and Review Requirements

The repository uses a [`CODEOWNERS`](.github/CODEOWNERS) file to automatically request reviews from designated maintainers when changes touch critical areas:

- **Smart contracts** (`/contracts/escrow/`, `/contracts/oracle/`): Reviewed by @StellarCheckMate/contracts team
- **CI/CD and build configuration** (`/.github/`, `/scripts/`): Reviewed by @StellarCheckMate/maintainers
- **Documentation** (`/docs/`): Reviewed by @StellarCheckMate/docs team
- **Other changes**: Reviewed by @StellarCheckMate/maintainers

When a PR touches files in these paths, GitHub automatically requests review from the designated owners. This ensures that security-critical contract code and infrastructure changes receive appropriate scrutiny. All requested reviews must be approved before merging.

## Issue Labels

We use a shared label taxonomy to keep issue and PR triage consistent. See [docs/label-taxonomy.md](docs/label-taxonomy.md) for definitions of labels like `good first issue`, `wave-ready`, and `help-wanted`.

## Coding Standards

### Rust Conventions

- Use `snake_case` for functions and variables
- Use `PascalCase` for types and enums
- Prefer explicit error handling over panics
- Use `Result<T, Error>` for fallible operations
- Document public APIs with doc comments

### Smart Contract Patterns

- Validate all inputs at function entry
- Use appropriate storage types (instance, persistent, temporary)
- Extend TTL for long-lived data
- Emit events for state changes
- Require authentication for privileged operations

### Testing Standards

- Write unit tests for all public functions
- Test error cases and edge conditions
- Use descriptive test names: `test_function_name_condition_expected_result`
- Mock external dependencies
- Verify events are emitted correctly

### Testing Conventions: Prefer `try_` Over `#[should_panic]`

When testing that a contract function returns a specific error, prefer the typed
`try_` variant over `#[should_panic]`. The `try_` approach asserts the *exact*
error variant, making failures easier to diagnose and preventing tests from
accidentally passing due to an unrelated panic.

**Avoid** — `#[should_panic]` only checks that *something* panicked:

```rust
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_create_match_with_zero_stake_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.create_match(&player1, &player2, &0, &token,
        &String::from_str(&env, "game"), &Platform::Lichess);
}
```

**Prefer** — `try_` asserts the exact error variant:

```rust
#[test]
fn test_create_match_with_zero_stake_returns_invalid_amount() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(&player1, &player2, &0, &token,
        &String::from_str(&env, "game"), &Platform::Lichess);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}
```

Use `#[should_panic]` only for cases where the contract panics with a plain
string message rather than a typed error (e.g. double-initialization):

```rust
#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_double_initialize_fails() {
    // ...
    client.initialize(&oracle, &admin);
    client.initialize(&oracle, &admin); // panics with a string, not a typed Error
}
```

## Contributing by Component

### Smart Contract Changes

For changes to `contracts/escrow` or `contracts/oracle`, see [docs/contributing-contracts.md](docs/contributing-contracts.md) for detailed guidance on:
- Authorization patterns and `require_auth`
- Storage tiers and state layout
- TTL management
- Contract initialization and upgrade safety
- Events and observability

### Oracle Service Changes

For changes to the off-chain oracle service, see [docs/CONTRIBUTING_ORACLE.md](docs/CONTRIBUTING_ORACLE.md) for detailed guidance on:
- Local oracle setup and environment configuration
- Running and writing oracle integration tests
- Adding support for new chess platform clients
- Result verification and security patterns
- API rate limiting and error handling

## Drips Wave Contributions

Checkmate-Escrow participates in Drips Wave contributor funding. Issues labeled `wave-ready` are eligible for funding:

- `trivial` (100 points): Documentation, simple tests, minor fixes
- `medium` (150 points): Oracle helpers, validation logic, moderate features
- `high` (200 points): Core escrow logic, Oracle integrations, security enhancements

See [docs/wave-guide.md](docs/wave-guide.md) for details on earning funding.

## Getting Help

- Open an issue for bugs or feature requests
- Join discussions in existing issues
- Ask questions in pull request comments

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
