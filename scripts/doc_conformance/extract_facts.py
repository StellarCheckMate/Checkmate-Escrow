"""Extract machine-checkable facts from the escrow/oracle contract source.

This module intentionally uses lightweight regex/scanning instead of a full
Rust parser (no `syn` dependency is available to a plain `python3` CI step).
It is deliberately narrow: it extracts only the handful of fact classes the
doc-conformance checker (`check.py`) needs — enum variants, struct fields,
public function signatures, and named integer constants — from the specific
files this repo's docs make claims about.

If contract source is refactored in a way this scanner can't follow (e.g. a
function signature spanning unusual formatting), `check.py` will surface a
missing-fact error rather than silently skipping the check.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class FnSig:
    name: str
    params: str
    returns: str


@dataclass
class ContractFacts:
    match_states: list[str] = field(default_factory=list)
    match_fields: list[str] = field(default_factory=list)
    dispute_state_variants: list[str] = field(default_factory=list)
    dispute_fields: list[str] = field(default_factory=list)
    snapshot_reason_variants: list[str] = field(default_factory=list)
    player_tier_variants: list[str] = field(default_factory=list)
    escrow_fns: dict[str, FnSig] = field(default_factory=dict)
    oracle_fns: dict[str, FnSig] = field(default_factory=dict)
    timeout_min: int | None = None
    timeout_max: int | None = None
    timeout_default_expr: str | None = None


def _strip_line_comments(src: str) -> str:
    # Good enough for this codebase: no `//` appears inside string/char
    # literals in the const/enum/struct/fn regions we scan.
    return re.sub(r"//[^\n]*", "", src)


def extract_enum_variants(src: str, enum_name: str) -> list[str]:
    src = _strip_line_comments(src)
    m = re.search(rf"\benum\s+{re.escape(enum_name)}\s*\{{", src)
    if not m:
        raise ValueError(f"enum {enum_name} not found")
    start = m.end()
    depth = 1
    i = start
    while depth > 0:
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
        i += 1
    body = src[start : i - 1]
    variants = []
    for raw_line in body.split(","):
        line = raw_line.strip()
        if not line:
            continue
        ident = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", line)
        if ident:
            variants.append(ident.group(1))
    return variants


def extract_struct_fields(src: str, struct_name: str) -> list[str]:
    src = _strip_line_comments(src)
    m = re.search(rf"\bstruct\s+{re.escape(struct_name)}\s*\{{", src)
    if not m:
        raise ValueError(f"struct {struct_name} not found")
    start = m.end()
    depth = 1
    i = start
    while depth > 0:
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
        i += 1
    body = src[start : i - 1]
    fields = []
    for raw_line in body.split(","):
        line = raw_line.strip()
        if not line:
            continue
        m2 = re.match(r"pub\s+([A-Za-z_][A-Za-z0-9_]*)\s*:", line)
        if m2:
            fields.append(m2.group(1))
    return fields


def extract_pub_fns(src: str, *, impl_only: bool = True) -> dict[str, FnSig]:
    """Extract `pub fn name(params) -> ret` signatures.

    When `impl_only` is True, restricts extraction to the body of the first
    `impl ... { ... }` block found (the contract's `#[contractimpl] impl`),
    so free functions / test helpers elsewhere in the file are excluded.
    """
    src = _strip_line_comments(src)

    if impl_only:
        m = re.search(r"\bimpl\s+\w+\s*\{", src)
        if not m:
            raise ValueError("no impl block found")
        start = m.end()
        depth = 1
        i = start
        while depth > 0:
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
            i += 1
        src = src[start : i - 1]

    fns: dict[str, FnSig] = {}
    for m in re.finditer(r"pub fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", src):
        name = m.group(1)
        paren_start = m.end() - 1
        depth = 0
        i = paren_start
        while True:
            if src[i] == "(":
                depth += 1
            elif src[i] == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        params = src[paren_start + 1 : i]
        rest = src[i + 1 :]
        brace_idx = rest.index("{")
        header_tail = rest[:brace_idx]
        returns = ""
        if "->" in header_tail:
            returns = header_tail.split("->", 1)[1].strip()
        fns[name] = FnSig(name=name, params=params.strip(), returns=returns)
    return fns


def extract_u32_const(src: str, const_name: str) -> int:
    src = _strip_line_comments(src)
    m = re.search(
        rf"const\s+{re.escape(const_name)}\s*:\s*u32\s*=\s*([0-9_]+)\s*;", src
    )
    if not m:
        raise ValueError(f"const {const_name} not found (or not a literal u32)")
    return int(m.group(1).replace("_", ""))


def load_contract_facts(repo_root: Path) -> ContractFacts:
    escrow_lib = (repo_root / "contracts/escrow/src/lib.rs").read_text()
    escrow_types = (repo_root / "contracts/escrow/src/types.rs").read_text()
    oracle_lib = (repo_root / "contracts/oracle/src/lib.rs").read_text()

    facts = ContractFacts()
    facts.match_states = extract_enum_variants(escrow_types, "MatchState")
    facts.match_fields = extract_struct_fields(escrow_types, "Match")
    facts.dispute_state_variants = extract_enum_variants(escrow_types, "DisputeState")
    facts.dispute_fields = extract_struct_fields(escrow_types, "Dispute")
    facts.snapshot_reason_variants = extract_enum_variants(
        escrow_types, "SnapshotReason"
    )
    facts.player_tier_variants = extract_enum_variants(escrow_types, "PlayerTier")
    facts.escrow_fns = extract_pub_fns(escrow_lib)
    facts.oracle_fns = extract_pub_fns(oracle_lib)
    facts.timeout_min = extract_u32_const(escrow_lib, "MIN_MATCH_TIMEOUT_LEDGERS")
    facts.timeout_max = extract_u32_const(escrow_lib, "MAX_MATCH_TIMEOUT_LEDGERS")
    return facts
