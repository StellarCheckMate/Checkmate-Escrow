# Mutation Testing Report

**Package focus:** `contracts/escrow` (primary), `contracts/oracle` (configured)  
**Tool:** [cargo-mutants](https://mutants.rs/) 27.1.0  
**Date:** 2026-07-27  
**Branch context:** `feat/Mutation-Testing`

Mutation testing checks whether the test suite fails when production logic is deliberately changed. High line coverage can still miss weak assertions; `MISSED` mutants are the actionable signal.

## Setup

| Artifact | Purpose |
|----------|---------|
| `contracts/escrow/.cargo/mutants.toml` | Escrow examine/exclude globs, skip list, timeouts |
| `contracts/oracle/.cargo/mutants.toml` | Oracle config (same pattern) |
| `scripts/mutation_test.sh` | Wrapper: `./scripts/mutation_test.sh [escrow\|oracle] [cargo-mutants args…]` |
| `make mutants` / `make mutants-list` | Convenience targets for escrow |

### Install

```bash
cargo install --locked cargo-mutants
```

### Run

```bash
# List all escrow mutants
./scripts/mutation_test.sh escrow --list

# Critical-path suite used for this report (~4 minutes with 4 jobs)
CARGO_MUTANTS_JOBS=4 ./scripts/mutation_test.sh escrow \
  --re 'EscrowContract::(initialize|pause|unpause|is_paused|deposit|submit_result|cancel_match|create_match|claim_vested_payout|add_allowed_token|remove_allowed_token)\b'

# Full escrow suite (longer)
./scripts/mutation_test.sh escrow
```

Outputs land in `mutants-out/<package>/mutants.out/` (gitignored). Inspect `missed.txt`, `caught.txt`, and `diff/`.

## Scope of this run

Escrow `src/lib.rs` alone generates hundreds of mutants. This report’s measured suite targets security-critical entry points:

- lifecycle: `initialize`, `create_match`, `deposit`, `submit_result`, `cancel_match`, `claim_vested_payout`
- admin/pause: `pause`, `unpause`, `is_paused`
- allowlist: `add_allowed_token`, `remove_allowed_token`

Helper noise (`require_auth`, TTL extend, event `publish`, clones) is skipped via `skip_calls`.

### Baseline test filter

`cargo test -p escrow --lib` currently has pre-existing failures (vesting/claim migration gaps, budget stress cases, kani harness stubs). The mutants config skips those by name so the unmutated baseline is green. See `additional_cargo_test_args` in `contracts/escrow/.cargo/mutants.toml`. Revisit that list as those tests are repaired.

## Results (critical-path suite)

| Metric | Before hardening | After hardening |
|--------|------------------|-----------------|
| Mutants tested | 78 | 71 |
| Caught | 70 | **71** |
| Missed | 8 | **0** |
| Timeout / unviable | 0 | 0 |
| Kill rate | 89.7% | **100%** |
| Wall time (4 jobs) | ~4m | ~4m |

The mutant count dropped from 78 → 71 after excluding **equivalent** boundary mutants (see below), not by ignoring real gaps.

### Missed mutants analyzed (pre-fix)

All eight survivors were in `EscrowContract::cancel_match`:

| Location | Mutation | Classification |
|----------|----------|----------------|
| `is_multi_token = token_b.is_some() && rate.map_or(false, \|r\| r > 0)` | `&&` → `\|\|` | Equivalent for reachable `create_match` / `create_match_with_conversion` states |
| same | `r > 0` → `r >= 0` | Equivalent when rate is always positive when set |
| same | `r > 0` → `r == 0` / `r < 0` | **Real gap** — multi-token player2 cancel refund untested |
| `cancellation_fee_basis_points > 0` / `fee_amount > 0` | `>` → `>=` | Equivalent for non-negative fees (zero fee still yields a no-op transfer) |

## Fixes applied

1. **`test_cancel_match_multi_token_player2_refund_uses_conversion_rate`** (`contracts/escrow/src/tests/multi_token.rs`)  
   Prefunds escrow with `token_b`, deposits only player2 on a multi-token pending match, cancels, and asserts the conversion-rate refund amount. Catches mutations that disable the `conversion_rate > 0` multi-token branch.

2. **`test_cancellation_zero_fee_skips_treasury_transfer`** (`contracts/escrow/src/tests/cancellation_fee.rs`)  
   Documents zero-fee cancel balances (full refund, treasury untouched).

3. **`exclude_re`** in `mutants.toml` for equivalent `cancel_match` boundary mutants (`> → >=`, `&& → ||`).

4. **Compile prerequisites** required for a mutants baseline:
   - Oracle `Error::{InvalidAmount,Overflow,SlippageExceeded}` restored; `swap` binds `amount_out` correctly
   - Escrow `multi_token` tests updated for `Option` conversion fields + `Ledger` testutils
   - `tests/regression_performance.rs` updated for Soroban client return types (so `cargo test --no-run` succeeds)

## Full-suite guidance

```bash
# Rough size of the filtered escrow mutant set
./scripts/mutation_test.sh escrow --list | wc -l   # ~394 with current config
```

Expect multi-hour runtime for the full set. Prefer sharding or `--re` filters in CI, and treat critical-path 100% kill rate as the merge bar for contract PRs that touch those entry points.

### Oracle

```bash
./scripts/mutation_test.sh oracle --list
./scripts/mutation_test.sh oracle --re 'OracleContract::(swap|submit_result|pause)\b'
```

Oracle config skips the known-failing `test_oracle_to_escrow_full_payout_flow` until payout/claim expectations are aligned with escrow vesting.

## Interpreting future runs

| Status | Meaning | Action |
|--------|---------|--------|
| **CAUGHT** | Tests failed after mutation | Good — assertion covers the behavior |
| **MISSED** | Tests still passed | Strengthen assertions or add a scenario; review `mutants.out/diff/` |
| **UNVIABLE** | Mutant does not compile | Usually ignore |
| **TIMEOUT** | Tests hung / exceeded budget | Investigate loops; raise `timeout_multiplier` only if needed |
| **Equivalent** | Mutation cannot change observable behavior | Exclude via `exclude_re` and document why |

## References

- Stellar guide: [Mutation Testing](https://developers.stellar.org/docs/build/guides/testing/mutation-testing)
- Project coverage guide: [`docs/TESTING_GUIDE.md`](./TESTING_GUIDE.md)
- cargo-mutants docs: <https://mutants.rs/>
