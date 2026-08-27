#!/usr/bin/env bash
set -euo pipefail

echo "Building contracts..."
cargo build -p escrow -p oracle --target wasm32-unknown-unknown --release
echo "Build complete."
