#!/usr/bin/env bash
# Runs the doc-code conformance checker (scripts/doc_conformance/check.py).
# See docs/doc-conformance.md for what this validates and why.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 "$REPO_ROOT/scripts/doc_conformance/check.py" --repo-root "$REPO_ROOT" "$@"
