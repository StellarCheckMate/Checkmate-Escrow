#!/usr/bin/env bash
# Validates external (https://) Markdown links in README.md, docs/, and demo/.
# Skips anchors-only links (#fragment). Uses curl HEAD requests with a timeout.
# Exits non-zero if any broken links are found.
#
# Usage: bash scripts/check_external_links.sh
# Set CHECK_EXTERNAL_LINKS_TIMEOUT to override per-request timeout (default: 10s).
# Set CHECK_EXTERNAL_LINKS_RETRIES to override retry count on transient errors (default: 2).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TIMEOUT="${CHECK_EXTERNAL_LINKS_TIMEOUT:-10}"
RETRIES="${CHECK_EXTERNAL_LINKS_RETRIES:-2}"

FILES=$(find "$REPO_ROOT/docs" "$REPO_ROOT/demo" -name "*.md" 2>/dev/null; echo "$REPO_ROOT/README.md")

errors=0
checked=0
skipped=0

# URLs that are known to block automated HEAD requests (rate-limiting, auth walls).
# Add entries here rather than disabling the whole check.
ALLOWLIST=(
  "https://fonts.googleapis.com"
  "https://fonts.gstatic.com"
)

is_allowlisted() {
  local url="$1"
  for pattern in "${ALLOWLIST[@]}"; do
    if [[ "$url" == "$pattern"* ]]; then
      return 0
    fi
  done
  return 1
}

while IFS= read -r file; do
  [[ -f "$file" ]] || continue
  while IFS= read -r url; do
    # Strip trailing punctuation that may have been captured
    url="${url%%[)>\"\']*}"
    [[ -z "$url" ]] && continue

    if is_allowlisted "$url"; then
      echo "SKIP (allowlisted): $url"
      skipped=$((skipped + 1))
      continue
    fi

    checked=$((checked + 1))
    # Try HEAD first; fall back to GET for servers that reject HEAD
    http_code=$(curl --silent --head --max-time "$TIMEOUT" \
      --retry "$RETRIES" --retry-delay 2 \
      --user-agent "Checkmate-Escrow-CI-LinkChecker/1.0" \
      --write-out "%{http_code}" --output /dev/null \
      "$url" 2>/dev/null || echo "000")

    if [[ "$http_code" == "405" || "$http_code" == "000" ]]; then
      # 405 = HEAD not allowed; retry with GET
      http_code=$(curl --silent --max-time "$TIMEOUT" \
        --retry "$RETRIES" --retry-delay 2 \
        --user-agent "Checkmate-Escrow-CI-LinkChecker/1.0" \
        --write-out "%{http_code}" --output /dev/null \
        "$url" 2>/dev/null || echo "000")
    fi

    if [[ "$http_code" =~ ^[23] ]]; then
      echo "OK ($http_code): $url"
    elif [[ "$http_code" == "429" ]]; then
      echo "WARN (rate-limited, $http_code): $url"
    else
      echo "BROKEN ($http_code): $file -> $url"
      errors=$((errors + 1))
    fi
  done < <(grep -oP '\]\(\K(https?://[^)]+)' "$file")
done <<< "$FILES"

echo ""
echo "External link check complete: $checked checked, $skipped skipped (allowlisted), $errors broken."

if [[ $errors -gt 0 ]]; then
  echo "$errors broken external link(s) found."
  exit 1
fi

echo "All external Markdown links OK."
