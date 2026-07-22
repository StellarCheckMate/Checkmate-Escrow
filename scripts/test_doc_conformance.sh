#!/usr/bin/env bash
# Runs the doc-conformance checker's own self-tests (positive + negative
# controls proving it actually flags deliberately introduced drift).
# See docs/doc-conformance.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 -m unittest discover -s "$REPO_ROOT/scripts/doc_conformance/tests" -v
