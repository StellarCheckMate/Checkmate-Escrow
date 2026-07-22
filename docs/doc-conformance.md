# Doc-Code Conformance Checking

This document describes the mechanism that prevents documentation from
silently drifting away from the escrow and oracle contracts' actual public
interface — the recurring failure mode that motivated
[issue #1067](https://github.com/StellarCheckMate/Checkmate-Escrow/issues/1067):
`docs/security.md` claiming a hardcoded ~24-hour match timeout when
`set_match_timeout` has supported a configurable `[17,280, 1,555,200]`-ledger
range since before that claim was written, `docs/architecture.md` omitting
struct fields and contract functions that had shipped, and `docs/roadmap.md`
claiming a feature had landed complete while its settlement path was still
unsound.

## What this is (and isn't)

This is a **presence-and-consistency checker**, not a correctness prover. It
answers "does every fact I can mechanically extract from source have a
matching mention in the docs, and does every doc claim I can't extract
automatically still match what it cited?" It does **not** verify that a
doc's prose *correctly describes* a function's behavior — that's a human
review responsibility this tool supports but doesn't replace.

Formal correctness properties of the state machine itself (reachability,
invariants like "no double payout") are a separate, complementary effort —
see `contracts/escrow/formal_spec.json` and the related state-machine-
verification work. This checker treats that file as **cross-referenced
ground truth for `MatchState` variants and entry points**, not something it
duplicates: see [Cross-reference with formal_spec.json](#cross-reference-with-formal_specjson)
below. When the two disagree, that disagreement is itself a finding — see
the worked example in that section.

## Running it locally

```bash
make doc-conformance        # or: bash scripts/check_doc_conformance.sh
make doc-conformance-test   # or: bash scripts/test_doc_conformance.sh
```

The checker exits non-zero if any error-level finding is produced. Pass
`--json` to `scripts/doc_conformance/check.py` for machine-readable output.

## What's checked automatically

Facts are extracted from source by `scripts/doc_conformance/extract_facts.py`
(regex/scanning — no `syn`/AST dependency, since CI only guarantees a plain
`python3`) and diffed against docs by `scripts/doc_conformance/check.py`:

| Check | Source of truth | Doc(s) checked |
|---|---|---|
| `match-states` | `MatchState` enum variants (`contracts/escrow/src/types.rs`) | `docs/architecture.md` — each variant must appear (as `` `Variant` ``) at least twice, matching the pattern of a states table + a transitions table. |
| `match-fields` | `Match` struct fields (`types.rs`) | `docs/architecture.md`'s `### \`Match\` Struct` section — every field needs a row (or, for `player1_deposited`/`player2_deposited`, the documented internal-field callout). |
| `function-coverage` | Every `pub fn` in `EscrowContract`'s and `OracleContract`'s `impl` blocks | `docs/architecture.md` (escrow) and `docs/oracle.md` + `docs/architecture.md` combined (oracle) — every function name needs at least one `` `fn_name` `` code-span mention somewhere. |
| `timeout-bounds` | `MIN_MATCH_TIMEOUT_LEDGERS` / `MAX_MATCH_TIMEOUT_LEDGERS` (`contracts/escrow/src/lib.rs`) | `docs/security.md` must mention both bounds, and must not contain a "fixed/hardcoded timeout" or "~24 hour" style claim. |
| `token-support` | — | `docs/security.md` must not contain a "No Native Token Support" style claim (the contract supports allowlisted SAC tokens including multi-token matches via `create_match_with_conversion`). |
| `formal-spec` | `contracts/escrow/formal_spec.json` | Cross-checked against the same `MatchState` variants and the escrow contract's function set — see below. |
| `annotations` | Cited source line's current content | Any doc carrying a `doc-conformance` annotation (see below) — the cited line must still hash to what was recorded. |

## The annotation convention

Some doc claims aren't mechanically extractable as a single fact (e.g. "the
admin-oversight bullet list in the Oracle Compromise section is still
accurate"). For these, cite the exact source location backing the claim
with a structured HTML comment immediately above it:

```html
<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=41 sha256=0c23a067e8485bb2d30c198995a18b78f9591bc66506f988f1e1399cab01f590 -->
```

The checker re-reads `path:line` and compares its stripped content against
the recorded `sha256`. If the cited line has since changed — even in a way
that's still true, like a comment edit — the hash won't match and the
checker fails with a clear pointer to re-verify the claim and update the
annotation. This deliberately trades some false positives (a harmlessly
reworded comment triggers a required-but-cheap re-check) for the property
that **a silently drifted claim can never pass CI indefinitely**.

To compute a new/updated hash locally:

```bash
python3 -c "
import hashlib
line = open('contracts/escrow/src/lib.rs').read().splitlines()[LINE_NUMBER - 1]
print(hashlib.sha256(line.strip().encode()).hexdigest())
"
```

## Cross-reference with formal_spec.json

Rather than re-deriving a second state-transition model (which would just
create a second thing to keep in sync), this checker treats
`contracts/escrow/formal_spec.json` — the artifact produced by the related
state-machine-verification effort — as ground truth for `MatchState`
variants and entry-point names, and checks it *against the contract source*
the same way it checks the docs:

- Every `MatchState` variant in `types.rs` must appear in the spec's
  `match_states`, and vice versa (a spec state that no longer exists in
  code is exactly as stale as a doc claim that no longer exists in code).
- Every `entry_points[].name` in the spec must be a real public function on
  `EscrowContract`.

This isn't hypothetical: running this cross-check while building this gate
caught that `formal_spec.json`'s `resolve_dispute_by_vote` entry claimed an
overturned dispute vote transitions the match to `Cancelled`. The actual
code (`contracts/escrow/src/lib.rs`, `resolve_dispute_by_vote`) always sets
`match.state = Completed` — an overturned vote pays out a `Draw` refund
through the normal `Completed` path, it does not cancel the match. That
entry has been corrected as part of this change. `docs/architecture.md`'s
transition table already described the real behavior correctly, which is
exactly the kind of divergence between two "authoritative" artifacts this
cross-check exists to catch before it's a third, contradictory source of
truth.

## Required CI gate

Three jobs run in `.github/workflows/doc-conformance.yml`:

1. **`conformance-check`** — runs the checker against the repo's current
   state on every push/PR. This is the "docs must currently be accurate"
   gate.
2. **`conformance-self-test`** — runs `scripts/doc_conformance/tests/` (see
   below), so a change to the checker itself that silently breaks its
   ability to detect drift is caught the same way a change that breaks the
   docs is.
3. **`require-doc-update`** (PRs only) — `scripts/require_doc_update.sh`
   fails if a PR modifies `contracts/escrow/src/lib.rs`,
   `contracts/escrow/src/types.rs`, or `contracts/oracle/src/lib.rs` without
   also touching at least one of `docs/architecture.md`, `docs/security.md`,
   `docs/roadmap.md`, `docs/oracle.md`, `docs/doc-conformance.md`, or
   `contracts/escrow/formal_spec.json`. This is a coarse presence check (did
   *a* doc file change at all), not a substitute for job 1 — it exists so a
   PR can't skip documentation entirely, and job 1 exists so what gets
   written is actually correct.

## Test suite (positive controls)

`scripts/doc_conformance/tests/test_check.py` runs the checker against a
small synthetic fixture repo (`tests/fixtures/good_repo/` — not this
repo's real docs/contracts, so these tests stay fast and don't need
updating every time the real docs change). `TestGoodRepoPasses` is the
negative control: the clean fixture must produce zero findings.
`TestPositiveControls` has one test per check category, each applying
exactly one deliberate mutation (delete a state's second doc mention, add
an undocumented struct field, add an undocumented function, reintroduce a
"hardcoded timeout" claim, reintroduce a "no native token" claim, stale an
annotation's cited line, and diverge `formal_spec.json` from source in two
different ways) and asserting the checker's `run()` reports a
matching-category error. Run with:

```bash
make doc-conformance-test
```

## Known limitations

- **Regex-based extraction, not an AST.** `extract_facts.py` scans for
  `impl NAME { ... }`, `enum NAME { ... }`, `struct NAME { ... }`, and
  `pub fn` blocks with brace/paren counting. It will misparse source that
  uses unusual formatting it wasn't written against (e.g. a struct/enum
  defined via macro rather than literal `struct`/`enum` syntax). If a check
  errors with a "not found" `ValueError` rather than a normal finding,
  that's this limitation surfacing — treat it as "the checker needs an
  update to follow the refactor," not as green.
- **Function coverage only checks the name is mentioned**, not that its
  documented signature/description matches. A doc row for `deposit` with a
  stale parameter list still satisfies `function-coverage`; only the
  annotation mechanism (cited line + hash) catches that class of drift, and
  only where an annotation was actually added.
- **`match-states` requiring "at least twice"** is a heuristic proxy for
  "appears in both a states table and a transitions table," not a
  structural parse of Markdown tables. A doc that mentions a state twice in
  unrelated prose would pass; this trade-off was chosen to avoid coupling
  the checker to today's exact Markdown table layout.
- **The oracle contract's `set_rate`/`get_rate`/`swap` functions** predate
  full doc-comment coverage in source (see the note in `docs/oracle.md`).
  They pass `function-coverage` because they're named in the doc's function
  reference table, but their error-handling documentation there is
  advisory, not sourced from `# Errors` doc comments like the rest of the
  contract.
- **`require_doc_update.sh` is presence-only.** It cannot tell a genuine
  doc update from a one-character whitespace touch to a gated file; that
  gap is intentional (a stricter check would need to understand diff
  *semantics*, which is exactly the harder problem `conformance-check`
  handles for the specific fact classes above, not this gate).
