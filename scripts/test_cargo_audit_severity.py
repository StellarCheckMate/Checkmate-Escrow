"""Regression tests for cargo-audit severity classification (issue #2025)."""

from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from cargo_audit_severity import classify_advisory, is_blocking, main, parse_cvss_vector


def advisory(**overrides):
    base = {
        "id": "RUSTSEC-2025-0001",
        "title": "high impact vulnerability",
        "severity": None,
        "informational": None,
    }
    base.update(overrides)
    return base


def report_with(*advisories):
    return {
        "vulnerabilities": {
            "list": [{"advisory": item} for item in advisories]
        }
    }


def run_main(report):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "audit-report.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        output = io.StringIO()
        with redirect_stdout(output):
            code = main([str(path)])
    return code, output.getvalue()


class SeverityClassificationTests(unittest.TestCase):
    def test_cvss_only_high_advisory_is_blocking(self):
        item = advisory(cvss="CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N")
        self.assertEqual(classify_advisory(item), "high")
        self.assertTrue(is_blocking(classify_advisory(item)))

    def test_cvss_only_high_advisory_fails_the_job(self):
        item = advisory(
            id="RUSTSEC-2025-HIGH",
            cvss="CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N",
        )
        code, output = run_main(report_with(item))
        self.assertEqual(code, 1)
        self.assertIn("RUSTSEC-2025-HIGH", output)
        self.assertIn("HIGH/CRITICAL advisories found", output)

    def test_scope_changed_with_confidentiality_high_is_critical(self):
        item = advisory(cvss="CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N")
        self.assertEqual(classify_advisory(item), "critical")

    def test_low_impact_cvss_vector_does_not_block(self):
        item = advisory(cvss="CVSS:3.1/AV:L/AC:H/PR:H/UI:R/S:U/C:L/I:N/A:N")
        self.assertEqual(classify_advisory(item), "medium")
        code, output = run_main(report_with(item))
        self.assertEqual(code, 0)
        self.assertIn("below HIGH severity", output)

    def test_explicit_severity_still_blocks(self):
        item = advisory(severity="high")
        self.assertTrue(is_blocking(classify_advisory(item)))
        code, _ = run_main(report_with(item))
        self.assertEqual(code, 1)

    def test_highest_signal_wins_when_fields_disagree(self):
        item = advisory(severity="low", cvss="CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N")
        self.assertEqual(classify_advisory(item), "high")

    def test_numeric_score_boundaries(self):
        self.assertEqual(classify_advisory(advisory(cvss_score=6.9)), "medium")
        self.assertEqual(classify_advisory(advisory(cvss_score=7.0)), "high")
        self.assertEqual(classify_advisory(advisory(cvss={"score": 9.1})), "critical")

    def test_malformed_cvss_is_unclassified_and_reported(self):
        self.assertEqual(parse_cvss_vector("not-a-vector"), {})
        item = advisory(cvss="not-a-vector")
        self.assertEqual(classify_advisory(item), "unknown")
        code, output = run_main(report_with(item))
        self.assertEqual(code, 0)
        self.assertIn("severity=unknown", output)

    def test_bad_report_returns_actionable_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "missing.json"
            output = io.StringIO()
            with redirect_stdout(output):
                code = main([str(path)])
        self.assertEqual(code, 2)
        self.assertIn("Could not read cargo audit report", output.getvalue())

    def test_clean_report_succeeds(self):
        code, output = run_main({"vulnerabilities": {"list": []}})
        self.assertEqual(code, 0)
        self.assertEqual(output, "")


if __name__ == "__main__":
    unittest.main()
