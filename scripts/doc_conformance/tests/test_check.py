"""Self-tests for the doc-conformance checker.

Run with:
    python3 -m unittest discover -s scripts/doc_conformance/tests -v

Uses a small synthetic fixture repo (fixtures/good_repo) rather than this
repo's real docs/contracts, so these tests stay fast, hermetic, and stable
regardless of future edits to the real docs. `test_good_repo_passes` is the
negative control (baseline must be clean); every `test_drift_*` is a
positive control proving the checker actually flags a deliberately
introduced class of drift, per the requirement in issue #1067 that the
conformance checker be proven to catch drift, not just assumed to.
"""

from __future__ import annotations

import shutil
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import check  # noqa: E402

FIXTURES = Path(__file__).resolve().parent / "fixtures"
GOOD_REPO = FIXTURES / "good_repo"


class DriftFixture:
    """Copies fixtures/good_repo into a temp dir, optionally mutated."""

    def __init__(self, mutate=None):
        self._tmp = TemporaryDirectory()
        self.root = Path(self._tmp.name) / "repo"
        shutil.copytree(GOOD_REPO, self.root)
        if mutate:
            mutate(self.root)

    def cleanup(self):
        self._tmp.cleanup()

    def errors(self):
        return [f for f in check.run(self.root) if f.severity == "error"]


def _replace(path: Path, old: str, new: str):
    text = path.read_text()
    assert old in text, f"fixture setup error: {old!r} not found in {path}"
    path.write_text(text.replace(old, new))


class TestGoodRepoPasses(unittest.TestCase):
    """Negative control: the clean fixture must produce zero error findings."""

    def test_no_errors(self):
        fx = DriftFixture()
        try:
            errors = fx.errors()
            self.assertEqual(
                errors, [], f"expected no errors on clean fixture, got: {errors}"
            )
        finally:
            fx.cleanup()


class TestPositiveControls(unittest.TestCase):
    """Each test introduces exactly one class of drift and asserts it's caught."""

    def test_missing_match_state_variant_detected(self):
        def mutate(root: Path):
            # Drop `Cancelled` down to a single mention in architecture.md,
            # simulating a doc that never got updated after a new terminal
            # state was reviewed into the contract.
            _replace(
                root / "docs/architecture.md",
                "| `Pending` | `Cancelled` | `cancel_match` |\n",
                "",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "match-states" and "Cancelled" in e.message for e in errors),
                f"expected a match-states finding about Cancelled, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_undocumented_match_field_detected(self):
        def mutate(root: Path):
            # Add a new field to the Match struct without touching docs —
            # this is exactly the "field added, docs not updated" drift
            # class described in issue #1067.
            _replace(
                root / "contracts/escrow/src/types.rs",
                "    pub state: MatchState,\n",
                "    pub state: MatchState,\n    pub token_b: Option<Address>,\n",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "match-fields" and "token_b" in e.message for e in errors),
                f"expected a match-fields finding about token_b, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_undocumented_new_function_detected(self):
        def mutate(root: Path):
            _replace(
                root / "contracts/escrow/src/lib.rs",
                "    pub fn set_match_timeout(env: Env, timeout: u32) -> Result<(), u32> {\n        Ok(())\n    }\n",
                "    pub fn set_match_timeout(env: Env, timeout: u32) -> Result<(), u32> {\n        Ok(())\n    }\n\n"
                "    pub fn undocumented_new_fn(env: Env) -> u32 {\n        0\n    }\n",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(
                    e.check == "function-coverage" and "undocumented_new_fn" in e.message
                    for e in errors
                ),
                f"expected a function-coverage finding, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_stale_hardcoded_timeout_claim_detected(self):
        def mutate(root: Path):
            _replace(
                root / "docs/security.md",
                "Match timeout is configurable in the range 17,280 to 1,555,200 ledgers via `set_match_timeout`.",
                "Match expiration timeout is hardcoded (~24 hours).",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "timeout-bounds" for e in errors),
                f"expected a timeout-bounds finding, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_stale_no_native_token_claim_detected(self):
        def mutate(root: Path):
            _replace(
                root / "docs/security.md",
                "Multi-token matches are supported via `create_match_with_conversion`.",
                "No Native Token Support: only XLM and USDC are supported.",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "token-support" for e in errors),
                f"expected a token-support finding, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_stale_annotation_hash_detected(self):
        def mutate(root: Path):
            # Change the annotated source line's value without re-verifying
            # the doc claim — the annotation's recorded hash goes stale.
            _replace(
                root / "contracts/escrow/src/lib.rs",
                "pub const MIN_MATCH_TIMEOUT_LEDGERS: u32 = 17_280;",
                "pub const MIN_MATCH_TIMEOUT_LEDGERS: u32 = 8_640;",
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "annotations" for e in errors),
                f"expected an annotations finding, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_formal_spec_state_drift_detected(self):
        def mutate(root: Path):
            _replace(
                root / "contracts/escrow/formal_spec.json",
                '{"state": "Cancelled", "reachable_from": ["Pending"], "terminal": true}',
                '{"state": "Cancelled", "reachable_from": ["Pending"], "terminal": true},\n'
                '    {"state": "Paused", "reachable_from": ["Active"], "terminal": false}',
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "formal-spec" and "Paused" in e.message for e in errors),
                f"expected a formal-spec finding about Paused, got: {errors}",
            )
        finally:
            fx.cleanup()

    def test_formal_spec_unknown_entry_point_detected(self):
        def mutate(root: Path):
            _replace(
                root / "contracts/escrow/formal_spec.json",
                '{"name": "get_match", "state_transition": "none"}',
                '{"name": "get_match", "state_transition": "none"},\n'
                '    {"name": "nonexistent_fn", "state_transition": "none"}',
            )

        fx = DriftFixture(mutate)
        try:
            errors = fx.errors()
            self.assertTrue(
                any(e.check == "formal-spec" and "nonexistent_fn" in e.message for e in errors),
                f"expected a formal-spec finding about nonexistent_fn, got: {errors}",
            )
        finally:
            fx.cleanup()


if __name__ == "__main__":
    unittest.main()
