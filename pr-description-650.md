# Add CODEOWNERS for reviewer routing

## Summary

Adds `.github/CODEOWNERS` to route PR reviews automatically based on the changed paths.

## Changes

- **`.github/CODEOWNERS`** — new file mapping ownership areas:
  - `contracts/escrow/` → `@StellarCheckMate/contracts`
  - `contracts/oracle/` → `@StellarCheckMate/contracts`
  - `docs/` → `@StellarCheckMate/docs`
  - `scripts/` and `.github/` → `@StellarCheckMate/maintainers`
  - Root config files (`Cargo.toml`, `environments.toml`) → appropriate teams
  - Global fallback `*` → `@StellarCheckMate/maintainers`

## Why

With separate escrow, oracle, and docs areas, GitHub's CODEOWNERS file ensures the right reviewers are automatically requested on every PR, speeding up outside contributions without requiring manual triage.

## Testing

No code changes — reviewer routing is enforced by GitHub on PR open. Replace placeholder team slugs with real GitHub usernames or teams before merging.

closes #650
