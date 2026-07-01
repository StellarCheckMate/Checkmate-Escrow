#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="C:\"
ENV_FILE="/.env"

if [[ ! -f "" ]]; then
  echo "[verify-contract] Missing "
  exit 1
fi

set -a
source ""
set +a

if [[ -z {CONTRACT_ESCROW:-}" ]]; then
  echo "[verify-contract] CONTRACT_ESCROW is not set in "
  exit 1
fi

NETWORK=""
case "" in
  mainnet|testnet) ;;
  *)
    echo "[verify-contract] Unsupported NETWORK ''"
    exit 1
    ;;
esac

if [[ "" == "mainnet" ]]; then
  EXPLORER_URL="https://stellar.expert/explorer/public/contract/"
else
  EXPLORER_URL="https://stellar.expert/explorer/testnet/contract/"
fi

echo "[verify-contract] Contract: "
echo "[verify-contract] Network: "
echo "[verify-contract] Stellar Expert: "

if command -v stellar >/dev/null 2>&1; then
  echo "[verify-contract] Fetching contract info with stellar CLI..."
  if stellar contract info "" --network "" >/tmp/verify_contract_info.txt 2>/tmp/verify_contract_error.txt; then
    echo "[verify-contract] Success: contract is reachable and the CLI returned contract info."
    echo "[verify-contract] --- contract info ---"
    cat /tmp/verify_contract_info.txt
  else
    echo "[verify-contract] Warning: stellar contract info failed."
    if [[ -s /tmp/verify_contract_error.txt ]]; then
      cat /tmp/verify_contract_error.txt >&2
    fi
  fi
else
  echo "[verify-contract] Warning: stellar CLI not installed; skipping contract info fetch."
fi

echo "[verify-contract] Open the Stellar Expert URL above to inspect the deployed contract."