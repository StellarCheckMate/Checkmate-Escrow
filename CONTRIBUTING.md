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
- Update README.md if adding new features or changing setup steps

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
