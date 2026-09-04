# cargo audit / cargo deny advisories in CI

## Problem

`deny.toml` was present but the CI pipeline didn't fail specifically on
HIGH/CRITICAL vulnerability severity, and `cargo deny check` only ran as one
combined command, making it unclear whether the advisories database was
actually being checked as its own gate.

## What changed

- `.github/workflows/ci.yml`:
  - The "Run cargo audit" step now runs `cargo audit --json`, parses the
    report, and fails the build (`exit 1`) only when a HIGH or CRITICAL
    severity advisory is present. Lower-severity advisories are printed as
    a `::warning::` instead of blocking the merge.
  - `cargo deny check` was split into two explicit steps: `cargo deny check
    advisories` (complementary vulnerability DB coverage to cargo-audit)
    and `cargo deny check bans licenses sources` (the rest of the existing
    policy).
- `deny.toml`: documented the advisory database source
  (`db-urls = ["https://github.com/rustsec/advisory-db"]`) under
  `[advisories]`.

## Why

cargo-audit and cargo-deny pull from overlapping but not identical
advisory data; running both, and gating on severity rather than any match,
means real HIGH/CRITICAL issues block merges without every low-severity
advisory becoming a manual override drill.
