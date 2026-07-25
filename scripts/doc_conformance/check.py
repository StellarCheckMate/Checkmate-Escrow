#!/usr/bin/env python3
"""Doc-code conformance checker for Checkmate-Escrow.

Derives verifiable facts from `contracts/escrow/src/{lib,types}.rs` and
`contracts/oracle/src/lib.rs`, then diffs them against claims embedded in
`docs/security.md`, `docs/architecture.md`, `docs/oracle.md`, and
`contracts/escrow/formal_spec.json`. See `docs/doc-conformance.md` for the
full explanation of what is and isn't checked, and the annotation
convention used for claims that can't be fully automated.

Usage:
    python3 scripts/doc_conformance/check.py [--repo-root PATH] [--json]

Exit code is non-zero if any error-level finding is produced.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from extract_facts import ContractFacts, load_contract_facts  # noqa: E402

ANNOTATION_RE = re.compile(
    r"<!--\s*doc-conformance:\s*verified\s+path=(?P<path>\S+)\s+line=(?P<line>\d+)\s+"
    r"sha256=(?P<hash>[0-9a-f]{64})\s*-->"
)


@dataclass
class Finding:
    severity: str  # "error" | "warn"
    check: str
    message: str


def line_hash(line: str) -> str:
    return hashlib.sha256(line.strip().encode("utf-8")).hexdigest()


def check_match_states(facts: ContractFacts, arch_text: str) -> list[Finding]:
    findings = []
    for state in facts.match_states:
        token = f"`{state}`"
        occurrences = arch_text.count(token)
        # Expect the variant to appear in both the state table and the
        # transition table (>= 2), not just a passing prose mention.
        if occurrences < 2:
            findings.append(
                Finding(
                    "error",
                    "match-states",
                    f"MatchState variant '{state}' appears only {occurrences}x as "
                    f"`{state}` in docs/architecture.md (expected >= 2: state table "
                    "+ transition table). Contract has "
                    f"{len(facts.match_states)} states: {facts.match_states}.",
                )
            )
    return findings


def check_match_struct_fields(facts: ContractFacts, arch_text: str) -> list[Finding]:
    findings = []
    m = re.search(
        r"### `Match` Struct(?P<body>.*?)(?=\n### )", arch_text, re.DOTALL
    )
    if not m:
        return [
            Finding(
                "error",
                "match-fields",
                "docs/architecture.md has no '### `Match` Struct' section to check "
                "fields against.",
            )
        ]
    body = m.group("body")
    for f_name in facts.match_fields:
        if f_name in ("player1_deposited", "player2_deposited"):
            # Documented as intentionally-internal via a callout, not a table row.
            if f_name not in body:
                findings.append(
                    Finding(
                        "error",
                        "match-fields",
                        f"Match field '{f_name}' is not mentioned anywhere in the "
                        "`Match` Struct section (expected at least the internal-field "
                        "callout).",
                    )
                )
            continue
        if f"`{f_name}`" not in body:
            findings.append(
                Finding(
                    "error",
                    "match-fields",
                    f"Match field '{f_name}' (contracts/escrow/src/types.rs) has no "
                    "row in docs/architecture.md's `Match` Struct table.",
                )
            )
    return findings


def check_function_coverage(
    fns: dict, doc_text: str, *, doc_name: str, contract_name: str
) -> list[Finding]:
    findings = []
    for name in fns:
        if f"`{name}`" not in doc_text:
            findings.append(
                Finding(
                    "error",
                    "function-coverage",
                    f"{contract_name} function '{name}' has no `{name}` code-span "
                    f"reference anywhere in {doc_name}.",
                )
            )
    return findings


def check_timeout_bounds(facts: ContractFacts, security_text: str) -> list[Finding]:
    findings = []

    def fmt(n: int) -> str:
        return f"{n:,}"

    if fmt(facts.timeout_min) not in security_text:
        findings.append(
            Finding(
                "error",
                "timeout-bounds",
                f"MIN_MATCH_TIMEOUT_LEDGERS ({fmt(facts.timeout_min)}) is not "
                "mentioned in docs/security.md.",
            )
        )
    if fmt(facts.timeout_max) not in security_text:
        findings.append(
            Finding(
                "error",
                "timeout-bounds",
                f"MAX_MATCH_TIMEOUT_LEDGERS ({fmt(facts.timeout_max)}) is not "
                "mentioned in docs/security.md.",
            )
        )

    stale_patterns = [
        r"fixed\s+timeout",
        r"hardcoded[^.]{0,40}timeout",
        r"timeout[^.]{0,40}hardcoded",
        r"~?24[\s-]hour",
    ]
    for pat in stale_patterns:
        if re.search(pat, security_text, re.IGNORECASE):
            findings.append(
                Finding(
                    "error",
                    "timeout-bounds",
                    f"docs/security.md contains a stale fixed/hardcoded-timeout "
                    f"claim matching /{pat}/i. Match timeout is admin-configurable "
                    "via set_match_timeout — see contracts/escrow/src/lib.rs "
                    "MIN_MATCH_TIMEOUT_LEDGERS/MAX_MATCH_TIMEOUT_LEDGERS.",
                )
            )
    return findings


def check_native_token_claim(security_text: str) -> list[Finding]:
    if re.search(r"no native token support", security_text, re.IGNORECASE):
        return [
            Finding(
                "error",
                "token-support",
                "docs/security.md still contains a 'No Native Token Support' style "
                "claim. The contract supports multi-token matches via "
                "create_match_with_conversion (contracts/escrow/src/lib.rs) — this "
                "claim is stale.",
            )
        ]
    return []


def check_annotations(repo_root: Path, doc_paths: list[Path]) -> list[Finding]:
    findings = []
    for doc_path in doc_paths:
        text = doc_path.read_text()
        for m in ANNOTATION_RE.finditer(text):
            cited_path = repo_root / m.group("path")
            cited_line_no = int(m.group("line"))
            expected_hash = m.group("hash")
            rel_doc = doc_path.relative_to(repo_root)
            if not cited_path.is_file():
                findings.append(
                    Finding(
                        "error",
                        "annotations",
                        f"{rel_doc}: annotation cites missing file "
                        f"'{m.group('path')}'.",
                    )
                )
                continue
            lines = cited_path.read_text().splitlines()
            if not (1 <= cited_line_no <= len(lines)):
                findings.append(
                    Finding(
                        "error",
                        "annotations",
                        f"{rel_doc}: annotation cites {m.group('path')}:"
                        f"{cited_line_no}, which is out of range "
                        f"(file has {len(lines)} lines).",
                    )
                )
                continue
            actual_hash = line_hash(lines[cited_line_no - 1])
            if actual_hash != expected_hash:
                findings.append(
                    Finding(
                        "error",
                        "annotations",
                        f"{rel_doc}: content at {m.group('path')}:{cited_line_no} "
                        "has changed since this annotation was verified "
                        f"(expected sha256={expected_hash}, got {actual_hash}). "
                        "Re-verify the doc claim and update the annotation's hash.",
                    )
                )
    return findings


def check_formal_spec_cross_reference(
    facts: ContractFacts, repo_root: Path
) -> list[Finding]:
    findings = []
    spec_path = repo_root / "contracts/escrow/formal_spec.json"
    if not spec_path.is_file():
        return [
            Finding(
                "error",
                "formal-spec",
                "contracts/escrow/formal_spec.json not found; the state-machine "
                "cross-reference check (see the related state-machine-"
                "verification effort) cannot run.",
            )
        ]
    try:
        spec = json.loads(spec_path.read_text())
    except json.JSONDecodeError as e:
        return [
            Finding(
                "error", "formal-spec", f"formal_spec.json is not valid JSON: {e}"
            )
        ]

    spec_states = {s["state"] for s in spec.get("match_states", [])}
    code_states = set(facts.match_states)
    missing_in_spec = code_states - spec_states
    extra_in_spec = spec_states - code_states
    if missing_in_spec:
        findings.append(
            Finding(
                "error",
                "formal-spec",
                f"MatchState variant(s) {sorted(missing_in_spec)} exist in "
                "contracts/escrow/src/types.rs but are missing from "
                "formal_spec.json's match_states.",
            )
        )
    if extra_in_spec:
        findings.append(
            Finding(
                "error",
                "formal-spec",
                f"formal_spec.json's match_states lists {sorted(extra_in_spec)}, "
                "which no longer exist as MatchState variants in "
                "contracts/escrow/src/types.rs.",
            )
        )

    for ep in spec.get("entry_points", []):
        name = ep.get("name")
        if name and name not in facts.escrow_fns:
            findings.append(
                Finding(
                    "error",
                    "formal-spec",
                    f"formal_spec.json entry_points references '{name}', which is "
                    "not a public function on EscrowContract.",
                )
            )
    return findings


def run(repo_root: Path) -> list[Finding]:
    facts = load_contract_facts(repo_root)

    architecture_path = repo_root / "docs/architecture.md"
    security_path = repo_root / "docs/security.md"
    oracle_path = repo_root / "docs/oracle.md"

    architecture_text = architecture_path.read_text()
    security_text = security_path.read_text()
    oracle_text = oracle_path.read_text()

    findings: list[Finding] = []
    findings += check_match_states(facts, architecture_text)
    findings += check_match_struct_fields(facts, architecture_text)
    findings += check_function_coverage(
        facts.escrow_fns,
        architecture_text,
        doc_name="docs/architecture.md",
        contract_name="EscrowContract",
    )
    findings += check_function_coverage(
        facts.oracle_fns,
        oracle_text + architecture_text,
        doc_name="docs/oracle.md (or docs/architecture.md)",
        contract_name="OracleContract",
    )
    findings += check_timeout_bounds(facts, security_text)
    findings += check_native_token_claim(security_text)
    findings += check_annotations(
        repo_root,
        [architecture_path, security_path, oracle_path, repo_root / "docs/roadmap.md"],
    )
    findings += check_formal_spec_cross_reference(facts, repo_root)
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="Repository root (default: inferred from this script's location).",
    )
    parser.add_argument(
        "--json", action="store_true", help="Emit findings as JSON instead of text."
    )
    args = parser.parse_args()

    findings = run(args.repo_root)
    errors = [f for f in findings if f.severity == "error"]

    if args.json:
        print(
            json.dumps(
                [f.__dict__ for f in findings],
                indent=2,
            )
        )
    else:
        if not findings:
            print("doc-conformance: OK — no drift detected.")
        for f in findings:
            print(f"[{f.severity.upper()}] ({f.check}) {f.message}")
        print(
            f"\ndoc-conformance: {len(errors)} error(s), "
            f"{len(findings) - len(errors)} warning(s)."
        )

    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
