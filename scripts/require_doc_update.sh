#!/usr/bin/env bash
# Fails if a PR modifies the escrow/oracle contracts' public-interface files
# without also touching one of the doc-conformance-relevant files. This is
# the "required CI gate" from issue #1067: it does not check that the docs
# were updated *correctly* (that's scripts/check_doc_conformance.sh's job),
# only that a PR touching contract surface didn't skip docs entirely.
#
# Usage: require_doc_update.sh <base-ref> <head-ref>
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <base-ref> <head-ref>" >&2
  exit 2
fi

BASE_REF="$1"
HEAD_REF="$2"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Contract files whose public-interface changes must come with a doc update.
GATED_FILES=(
  "contracts/escrow/src/lib.rs"
  "contracts/escrow/src/types.rs"
  "contracts/oracle/src/lib.rs"
)

# Any of these counts as "a corresponding doc-conformance update".
DOC_FILES=(
  "docs/architecture.md"
  "docs/security.md"
  "docs/roadmap.md"
  "docs/oracle.md"
  "docs/doc-conformance.md"
  "contracts/escrow/formal_spec.json"
)

CHANGED="$(git diff --name-only "$BASE_REF" "$HEAD_REF")"

touches_contract=false
for f in "${GATED_FILES[@]}"; do
  if grep -qxF "$f" <<< "$CHANGED"; then
    touches_contract=true
    echo "Contract surface changed: $f"
  fi
done

if [[ "$touches_contract" == false ]]; then
  echo "No gated contract files changed; doc-update gate does not apply."
  exit 0
fi

touches_docs=false
for f in "${DOC_FILES[@]}"; do
  if grep -qxF "$f" <<< "$CHANGED"; then
    touches_docs=true
    echo "Doc-conformance file updated: $f"
  fi
done

if [[ "$touches_docs" == false ]]; then
  cat >&2 <<'EOF'

ERROR: This PR modifies a contract's public-interface file
(contracts/escrow/src/lib.rs, contracts/escrow/src/types.rs, or
contracts/oracle/src/lib.rs) without touching any doc-conformance file
(docs/architecture.md, docs/security.md, docs/roadmap.md, docs/oracle.md,
docs/doc-conformance.md, or contracts/escrow/formal_spec.json).

If your change adds/removes/renames a public function, a MatchState
variant, a Match/Dispute field, or a configurable-parameter bound, update
the relevant doc(s) in the same PR. If your change genuinely has no
doc-visible effect (e.g. an internal refactor with no interface change),
touch docs/doc-conformance.md with a one-line note explaining why, or ask
a maintainer to bypass this gate.

See docs/doc-conformance.md for details.
EOF
  exit 1
fi

echo "OK: contract surface change is accompanied by a doc update."
