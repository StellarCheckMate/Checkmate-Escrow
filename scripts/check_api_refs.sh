#!/usr/bin/env bash
# Checks that every contract function name referenced in docs exists in the codebase.
# Only flags names that appear in function-call style: name( in code blocks.
# Exits non-zero if any stale API names are found.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Build allowlist from actual public contract functions
ALLOWLIST=$(grep -rh "pub fn " \
  "$REPO_ROOT/contracts/escrow/src/lib.rs" \
  "$REPO_ROOT/contracts/oracle/src/lib.rs" \
  | grep -oP 'pub fn \K[a-z_]+' | sort -u)

# SDK / CLI / Rust stdlib calls that are not contract functions — skip these
EXCLUDE="require_auth|from_str|to_string|cost_estimate|invoke_contract|call_contract"

# Docs to scan
DOCS=$(find "$REPO_ROOT/docs" "$REPO_ROOT/demo" -name "*.md"; echo "$REPO_ROOT/README.md")

errors=0

while IFS= read -r file; do
  while IFS= read -r name; do
    if ! echo "$ALLOWLIST" | grep -qx "$name"; then
      echo "STALE API: '$name' in $file"
      errors=$((errors + 1))
    fi
  done < <(grep -oP '`?\K[a-z][a-z_]+(?=\()' "$file" \
           | grep '_' \
           | grep -vE "^($EXCLUDE)$" \
           | sort -u)
done <<< "$DOCS"

if [[ $errors -gt 0 ]]; then
  echo "$errors stale API reference(s) found."
  exit 1
fi

echo "All API references OK."
