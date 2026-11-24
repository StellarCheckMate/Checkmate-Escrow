#!/usr/bin/env python3
"""Classify cargo-audit advisories by severity (issue #2025).

RUSTSEC advisories do not all carry a `severity` field: some expose only a
CVSS vector such as `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N`. Treating
those as unknown made a confidentiality-high advisory non-blocking even though
the CI comment claimed a `C:H` fallback. This module parses the vector (and
numeric scores when present) so the job fails on high-impact advisories.

Exit codes: 0 = no blocking advisory, 1 = blocking advisory found, 2 = bad input.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Tuple

SEVERITY_RANK = {
    "unknown": 0,
    "low": 1,
    "medium": 2,
    "high": 3,
    "critical": 4,
}

# CVSS impact metrics that are high enough to block a merge on their own.
HIGH_IMPACT_METRICS = ("C:H", "I:H", "A:H")


def _rank(severity: str) -> int:
    return SEVERITY_RANK.get(severity, 0)


def _normalise_severity(value: Any) -> str:
    if not isinstance(value, str):
        return "unknown"
    token = value.strip().lower()
    if token in ("critical", "high"):
        return token
    if token in ("medium", "moderate"):
        return "medium"
    if token == "low":
        return "low"
    return "unknown"


def parse_cvss_vector(vector: str) -> Dict[str, str]:
    """Return ``{metric: value}`` for a CVSS v3.x vector string.

    A malformed or non-CVSS string returns an empty mapping rather than raising:
    the caller reports it as an unclassified advisory instead of silently
    treating it as safe.
    """
    if not isinstance(vector, str) or not vector.strip().upper().startswith("CVSS:"):
        return {}
    metrics: Dict[str, str] = {}
    for token in vector.strip().split("/")[1:]:
        if ":" not in token:
            continue
        key, value = token.split(":", 1)
        key = key.strip().upper()
        if key:
            metrics[key] = value.strip().upper()
    return metrics


def severity_from_cvss_vector(vector: str) -> str:
    """Classify a CVSS vector by its impact metrics."""
    metrics = parse_cvss_vector(vector)
    if not metrics:
        return "unknown"

    high_impact = any(metrics.get(metric[0]) == metric[-1] for metric in HIGH_IMPACT_METRICS)
    if not high_impact:
        return "medium" if "L" in (metrics.get("C"), metrics.get("I"), metrics.get("A")) else "unknown"

    # Confidentiality loss paired with another high impact or a scope change
    # is treated as critical; a single high-impact metric is high.
    if metrics.get("C") == "H" and (
        metrics.get("I") == "H" or metrics.get("A") == "H" or metrics.get("S") == "C"
    ):
        return "critical"
    return "high"


def severity_from_score(score: Any) -> str:
    """Classify a numeric CVSS base score using the standard bands."""
    if isinstance(score, bool) or score is None:
        return "unknown"
    try:
        value = float(score)
    except (TypeError, ValueError):
        return "unknown"
    if not 0 <= value <= 10:
        return "unknown"
    if value >= 9.0:
        return "critical"
    if value >= 7.0:
        return "high"
    if value >= 4.0:
        return "medium"
    return "low"


def classify_advisory(advisory: Dict[str, Any]) -> str:
    """Return the highest severity supported by an advisory's fields."""
    if not isinstance(advisory, dict):
        return "unknown"

    severities = [
        _normalise_severity(advisory.get("severity")),
        _normalise_severity(advisory.get("informational")),
    ]

    cvss = advisory.get("cvss")
    if isinstance(cvss, str):
        # A string may be a base score ("7.5") or a vector.
        if cvss.strip().upper().startswith("CVSS:"):
            severities.append(severity_from_cvss_vector(cvss))
        else:
            severities.append(severity_from_score(cvss))
    elif isinstance(cvss, dict):
        for key in ("score", "base_score", "baseScore"):
            if key in cvss:
                severities.append(severity_from_score(cvss[key]))
        vector = cvss.get("vector") or cvss.get("vectorString") or cvss.get("vector_string")
        if isinstance(vector, str):
            severities.append(severity_from_cvss_vector(vector))

    for key in ("cvss_score", "cvssScore", "score"):
        if key in advisory:
            severities.append(severity_from_score(advisory[key]))

    return max(severities, key=_rank)


def is_blocking(severity: str) -> bool:
    return severity in ("high", "critical")


def analyze_report(report: Dict[str, Any]) -> Tuple[List[Tuple[str, str, str]], List[str]]:
    """Return ``(advisories, blocking_ids)`` for a cargo-audit JSON report."""
    vulnerabilities = report.get("vulnerabilities", {}) if isinstance(report, dict) else {}
    items = vulnerabilities.get("list", []) if isinstance(vulnerabilities, dict) else []
    if not isinstance(items, list):
        items = []

    advisories: List[Tuple[str, str]] = []
    blocking: List[str] = []
    for item in items:
        advisory = item.get("advisory", {}) if isinstance(item, dict) else {}
        advisory_id = str(advisory.get("id", "unknown")) if isinstance(advisory, dict) else "unknown"
        title = str(advisory.get("title", "")) if isinstance(advisory, dict) else ""
        severity = classify_advisory(advisory)
        advisories.append((advisory_id, title, severity))
        if is_blocking(severity):
            blocking.append(advisory_id)
    return advisories, blocking


def main(argv: List[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    path = Path(args[0]) if args else Path("audit-report.json")
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"::error::Could not read cargo audit report {path}: {error}")
        return 2

    advisories, blocking = analyze_report(report)
    for advisory_id, title, severity in advisories:
        print(f"advisory {advisory_id}: {title} (severity={severity})")

    if blocking:
        print(f"::error::HIGH/CRITICAL advisories found: {', '.join(blocking)}")
        return 1
    if advisories:
        print(f"::warning::{len(advisories)} advisory(ies) found below HIGH severity; not blocking merge.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
