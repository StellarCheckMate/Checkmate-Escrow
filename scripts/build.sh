#!/usr/bin/env bash
set -e

echo "Building contracts..."
cargo build --target wasm32-unknown-unknown --release
echo "Build complete."
