# Performance Report — Escrow Contract

## Overview

This report documents the gas (CPU instruction) usage, memory usage, and
execution time of the escrow contract's core operations, and how those costs
scale as the contract accumulates matches over its lifetime.

Results are produced by the benchmarking suite in
[`contracts/escrow/tests/benchmarks.rs`](../contracts/escrow/tests/benchmarks.rs),
run via [`scripts/benchmark.sh`](../scripts/benchmark.sh). Each run overwrites
[`reports/performance/benchmark-results.json`](../reports/performance/benchmark-results.json);
the numbers below are a committed snapshot from that file, captured against
the v0.1.0 contract.

## How to reproduce

```bash
bash scripts/benchmark.sh
# or directly:
cargo test -p escrow --test benchmarks -- --nocapture
```

The suite measures CPU instructions and memory bytes via the Soroban test
host's budget tracker, and wall-clock time via `std::time::Instant`. Each
measured call is preceded by a budget reset so that setup work (creating
filler matches, minting tokens) is excluded from the reported cost.

> **Note on accuracy**: per the Soroban SDK's own documentation, CPU
> instructions and memory measured against native Rust test execution are
> "likely to be underestimated... compared to running the WASM equivalent."
> These numbers are directionally reliable for comparing operations and
> spotting scaling trends, but absolute values should be re-validated against
> testnet (`soroban contract invoke` / `stellar contract invoke`) before being
> quoted as production gas costs.

## Baseline costs (single call, minimal state)

| Operation | CPU instructions | Memory (bytes) | Wall time (µs) |
|---|---|---|---|
| `deposit` (1st deposit, stays `Pending`) | 284,551 | 42,528 | ~1,600 |
| `cancel_match` (`Pending`, 1 deposit refunded) | 280,540 | 40,132 | ~1,600 |
| `submit_result` (1 active match) | 333,701 | 45,021 | ~1,900 |

These three are comparable in cost: each does one token transfer and one
persistent-storage read/write of a single `Match` record.

## Scaling: deposit, submit_result, get_active_matches

| Operation | n (active/total matches) | CPU instructions | Memory (bytes) | Wall time (µs) |
|---|---|---|---|---|
| `deposit` (activation: 2nd deposit) | 1 | 348,599 | 52,842 | ~3,000 |
| `deposit` (activation: 2nd deposit) | 10 | 479,550 | 112,944 | ~2,600 |
| `deposit` (activation: 2nd deposit) | 100 | 1,608,087 | 713,964 | ~12,000 |
| `submit_result` | 1 | 333,713 | 45,054 | ~1,700 |
| `submit_result` | 10 | 472,084 | 106,164 | ~2,700 |
| `submit_result` | 100 | 1,730,403 | 752,904 | ~11,300 |
| `get_active_matches` | 1 | 91,460 | 10,595 | ~700 |
| `get_active_matches` | 10 | 413,269 | 45,452 | ~1,900 |
| `get_active_matches` | 100 | 3,944,983 | 429,662 | ~22,200 |

CPU cost for all three roughly scales linearly with `n`, growing
**5–6x between n=1 and n=10, and ~4–10x again between n=10 and n=100**.

For contrast, `cancel_match` (which never touches the active-match index)
stayed an order of magnitude cheaper than `deposit`/`submit_result` at every
`n` in the same test run, despite being measured against an equally-sized
match history — see [`benchmark-results.json`](../performance/benchmark-results.json)
in `reports/performance/` for the raw `cancel_match` series.

## Identified performance / DoS vectors

1. **`ActiveMatches` index is a single re-read-and-rewritten vector.**
   The `deposit` call that activates a match (both players funded) and the
   `submit_result` call that completes one both go through an internal helper
   that loads the *entire* active-matches list, mutates it, and writes it back
   in full. Cost grows linearly with the number of concurrently active
   matches. An attacker who opens and funds many matches (each only costs a
   token transfer to themselves via two addresses they control) can inflate
   the cost of every subsequent `deposit`/`submit_result` call for *all*
   players, not just their own.

   *Mitigation directions*: replace the single vector with a keyed/indexed
   structure (e.g., one storage entry per active match, or a bounded set with
   O(1) removal), or cap the number of concurrently active matches.

2. **`get_active_matches` / `get_pending_matches` / `get_live_matches` scan
   the full match history.** These read-only entry points loop over every
   match ID ever issued (`0..match_count`) and filter by state in the
   contract itself, rather than reading only the relevant index. Cost grows
   with the *total number of matches ever created*, not the number currently
   active or pending — and this total only ever increases, so these calls get
   strictly more expensive over the contract's lifetime. `get_*_paginated`
   variants exist and should be preferred by all callers (this is already
   noted as the recommended path in the contract's docs), but the unbounded
   variants remain callable by anyone and contribute to overall network load.

   *Mitigation directions*: deprecate/remove the unbounded variants, or cap
   their result size, or maintain a real active/pending index instead of
   scanning the full match table.

3. **`cancel_match` and other single-record operations stayed flat in CPU
   cost across n in this suite's design**, since they only touch the target
   match's own storage entry, confirming the cost growth above is specific to
   the shared `ActiveMatches` index and the full-history scans, not a general
   property of the storage backend.

## CI integration

`scripts/benchmark.sh` is intended to be run on a schedule (or on
`chore/performance-benchmarking`-labeled PRs) to catch regressions; compare
the freshly generated `reports/performance/benchmark-results.json` against
this document's baseline table and flag any operation whose CPU instructions
regress by more than 10% at the same `n`.

## Deployment guidance for integrators

- Expect `deposit`/`submit_result` cost to rise with the number of
  simultaneously active matches; do not assume a flat per-call gas budget if
  the platform anticipates high concurrent match volume.
- Prefer the paginated read methods (`get_active_matches_paginated`,
  `get_pending_matches_paginated`, `get_player_matches_paginated`) over their
  unbounded counterparts in any production integration.
- `cancel_match` and `create_match` costs do not grow with contract history
  size and are safe to budget for as constant-cost operations.
