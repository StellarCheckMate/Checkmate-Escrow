#!/usr/bin/env bash
# Runs cspell against docs/ Markdown files and Rust contract source comments.
# Exits non-zero if any spelling errors are found.
#
# Usage: bash scripts/spellcheck.sh
# Requires: npx (Node.js) with cspell available via npx.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "Running spellcheck on docs/ Markdown files..."
npx --yes cspell@8 \
  --config .cspell.json \
  --no-progress \
  --show-suggestions \
  "docs/**/*.md" \
  "README.md" \
  "CONTRIBUTING.md" \
  "CHANGELOG.md" \
  "SECURITY.md"

echo ""
echo "Running spellcheck on Rust contract doc-comments..."
# Extract only doc-comment lines (/// and //!) from Rust source to avoid
# flagging identifiers, keywords, and macro tokens as spelling errors.
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

find contracts -name "*.rs" | while IFS= read -r rs_file; do
  rel="${rs_file#"$REPO_ROOT/"}"
  out_file="$TMP_DIR/$(echo "$rel" | tr '/' '_').txt"
  grep -E '^\s*(///|//!)' "$rs_file" \
    | sed 's|^\s*///\s*||; s|^\s*//!\s*||' \
    > "$out_file" || true
done

if find "$TMP_DIR" -name "*.txt" -size +0 | grep -q .; then
  npx --yes cspell@8 \
    --config .cspell.json \
    --no-progress \
    --show-suggestions \
    "$TMP_DIR/**/*.txt"
fi

echo ""
echo "Spellcheck passed."
