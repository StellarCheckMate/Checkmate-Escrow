#!/usr/bin/env bash
set -euo pipefail

echo "Building release WASM artifacts..."
# wasm32v1-none is the Soroban target: core wasm 1.0 only. Building for
# wasm32-unknown-unknown on recent Rust emits reference-types / multi-value
# instructions that the Soroban host validator rejects at deployment — the
# E2E suite would catch exactly this.
cargo build --target wasm32v1-none --release -p escrow -p oracle

echo "Running unit tests..."
cargo test

echo "Running E2E tests against the release WASM..."
cargo test -p e2e-tests

echo "All tests passed."
