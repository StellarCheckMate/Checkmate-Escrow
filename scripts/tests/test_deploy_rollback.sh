#!/usr/bin/env bash
# Smoke test for the `scripts/deploy.sh --rollback` path.
#
# Stubs out the `stellar` CLI so this can run without network access or real
# credentials, and verifies:
#   - `--rollback` without CONTRACT_ESCROW fails fast with a clear error
#   - `--rollback` invokes `stellar contract invoke ... -- cancel_upgrade`
#     when confirmed
#   - `--upgrade` and `--rollback` together is rejected as invalid usage
#
# Run manually with: bash scripts/tests/test_deploy_rollback.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEPLOY_SH="$REPO_ROOT/scripts/deploy.sh"
STUB_DIR="$(mktemp -d)"
trap 'rm -rf "$STUB_DIR"' EXIT

INVOKE_LOG="$STUB_DIR/invoke.log"

# Stub `stellar` so cancel_upgrade doesn't hit a real network. Also stub
# `cargo`/`rustc` checks by pointing PATH at the real toolchain (they're
# just version-checked, not invoked against the network).
cat > "$STUB_DIR/stellar" <<STUB
#!/usr/bin/env bash
echo "stellar \$*" >> "$INVOKE_LOG"
case "\$1" in
    keys) echo "GSTUBBEDADDRESS" ;;
    contract)
        if [[ "\$2" == "invoke" ]]; then
            echo "cancel_upgrade invoked"
        fi
        ;;
    --version) echo "stellar-cli 21.0.0 (stub)" ;;
esac
exit 0
STUB
chmod +x "$STUB_DIR/stellar"
export PATH="$STUB_DIR:$PATH"

pass=0
fail=0

assert_contains() {
    local haystack="$1" needle="$2" desc="$3"
    if [[ "$haystack" == *"$needle"* ]]; then
        echo "  ok - $desc"
        pass=$((pass + 1))
    else
        echo "  FAIL - $desc (expected to contain: $needle)"
        fail=$((fail + 1))
    fi
}

echo "Test: --rollback without CONTRACT_ESCROW fails"
: > "$INVOKE_LOG"
output=$(cd "$REPO_ROOT" && DEPLOYER_KEYPAIR=deployer ORACLE_ADMIN=x ESCROW_ADMIN=y \
    bash "$DEPLOY_SH" testnet --rollback 2>&1) || true
assert_contains "$output" "CONTRACT_ESCROW required for --rollback" \
    "rejects rollback with no CONTRACT_ESCROW"

echo "Test: --upgrade and --rollback together is rejected"
output=$(cd "$REPO_ROOT" && DEPLOYER_KEYPAIR=deployer ORACLE_ADMIN=x ESCROW_ADMIN=y \
    bash "$DEPLOY_SH" testnet --upgrade --rollback 2>&1) || true
assert_contains "$output" "mutually exclusive" \
    "rejects combining --upgrade and --rollback"

echo "Test: --rollback invokes cancel_upgrade when confirmed"
: > "$INVOKE_LOG"
output=$(cd "$REPO_ROOT" && DEPLOYER_KEYPAIR=deployer ORACLE_ADMIN=x ESCROW_ADMIN=y \
    CONTRACT_ESCROW=CSTUBESCROW123 \
    bash -c "echo rollback | bash '$DEPLOY_SH' testnet --rollback" 2>&1) || true
assert_contains "$(cat "$INVOKE_LOG")" "cancel_upgrade" \
    "calls stellar contract invoke with cancel_upgrade"
assert_contains "$output" "Rollback complete" \
    "reports rollback completion"

echo ""
echo "$pass passed, $fail failed"
[[ "$fail" -eq 0 ]]
