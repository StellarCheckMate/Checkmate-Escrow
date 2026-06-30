#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root_dir"

required_files=(
  LICENSE
  CODE_OF_CONDUCT.md
  README.md
  CONTRIBUTING.md
  Cargo.toml
  environments.toml
  .env.example
  .github/pull_request_template.md
  docs/architecture.md
  docs/oracle.md
  docs/security.md
  docs/roadmap.md
  docs/wave-guide.md
)

fail=0
for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "MISSING: $file"
    fail=1
  fi
done

if [[ -f CODE_OF_CONDUCT.md && ! -s CODE_OF_CONDUCT.md ]]; then
  echo "FAIL: CODE_OF_CONDUCT.md is empty"
  fail=1
fi

check_links() {
  local source_file="$1"
  grep -oE '\[[^]]+\]\([^)]*\)' "$source_file" | while IFS= read -r match; do
    local target="${match#*\(}"
    target="${target%\)}"
    if [[ "$target" =~ ^https?:// ]] || [[ "$target" =~ ^mailto: ]] || [[ "$target" =~ ^# ]]; then
      continue
    fi
    if [[ "$target" == /* ]]; then
      target=".${target}"
    fi
    if [[ ! -e "$target" ]]; then
      echo "BROKEN LINK: $source_file -> $target"
      fail=1
    fi
  done
}

check_links README.md
check_links CONTRIBUTING.md

if [[ "$fail" -ne 0 ]]; then
  echo "Repository health check failed"
  exit 1
fi

echo "Repository health check passed"
