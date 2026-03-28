#!/usr/bin/env bash
set -e

echo "Running tests..."
cargo test
echo "All tests passed."
