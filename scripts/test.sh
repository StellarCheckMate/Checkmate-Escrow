#!/usr/bin/env bash
set -euo pipefail

echo "Running tests..."
cargo test
echo "All tests passed."
