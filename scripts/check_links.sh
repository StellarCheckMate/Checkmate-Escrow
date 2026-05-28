#!/usr/bin/env bash
# Validates local Markdown links in README.md, docs/, and demo/.
# Exits non-zero if any broken links are found.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FILES=$(find "$REPO_ROOT/docs" "$REPO_ROOT/demo" -name "*.md"; echo "$REPO_ROOT/README.md")

errors=0

while IFS= read -r file; do
  # Extract local links: [text](path) where path does not start with http/https/#
  while IFS= read -r link; do
    # Resolve relative to the file's directory
    target="$(dirname "$file")/$link"
    # Normalize path (remove ../ etc.)
    target="$(realpath -m "$target")"
    if [[ ! -e "$target" ]]; then
      echo "BROKEN: $file -> $link"
      errors=$((errors + 1))
    fi
  done < <(grep -oP '\]\(\K[^)]+' "$file" | grep -v '^https\?://' | grep -v '^#' | grep -v '^mailto:')
done <<< "$FILES"

if [[ $errors -gt 0 ]]; then
  echo "$errors broken link(s) found."
  exit 1
fi

echo "All local Markdown links OK."
