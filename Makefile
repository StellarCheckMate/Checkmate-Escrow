SHELL := /bin/bash
.DEFAULT_GOAL := help

.PHONY: help build test lint fmt deploy-testnet frontend-dev frontend-test

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@echo "  build            Build Soroban contracts"
	@echo "  test             Run Rust tests"
	@echo "  lint             Run Rust lints via cargo clippy"
	@echo "  fmt              Format Rust code"
	@echo "  deploy-testnet   Deploy contracts to Stellar testnet"
	@echo "  frontend-dev     Start the frontend development server"
	@echo "  frontend-test    Run frontend tests"

build:
	bash scripts/build.sh

test:
	bash scripts/test.sh

lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

deploy-testnet:
	bash scripts/deploy_testnet.sh

frontend-dev:
	cd frontend && npm ci && npm run dev

frontend-test:
	cd frontend && npm ci && npm test
