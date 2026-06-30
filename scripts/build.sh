#!/usr/bin/env bash
set -euo pipefail

echo "Building contracts..."
cargo build --target wasm32-unknown-unknown --release
echo "Build complete."
