#!/usr/bin/env bash
# Checks that every contract function name referenced in docs exists in the codebase.
# Only flags names that appear in function-call style: name( in code blocks.
# Exits non-zero if any stale API names are found.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Build allowlist from actual public contract functions
ALLOWLIST=$(grep -rh "pub fn " \
  "$REPO_ROOT/contracts/escrow/src/lib.rs" \
  "$REPO_ROOT/contracts/oracle/src/lib.rs" \
  | grep -oP 'pub fn \K[a-z_]+' | sort -u)

# SDK / CLI / Rust stdlib calls that are not contract functions — skip these
EXCLUDE="require_auth|from_str|to_string|cost_estimate|invoke_contract|call_contract|contract_initialized|current_caller|require_player|mock_all_auths|register_contract|setup_with_funded_match|checked_mul|checked_div|checked_add|ok_or|unwrap_or|is_ok|is_some|as_millis|from_millis|as_ref|current_contract_address|extend_instance_ttl|game_id|stake_amount|escrow_balance|max_id|exactly_one_of|execute_payout|new_with_result|validate_game_id|verify_game_result|platform_name|fetch_game|fetch_with_backoff|create_client|health_check|contract_health_check|get_snapshot|try_acquire|pg_try_advisory_lock"

# Docs to scan
DOCS=$(find "$REPO_ROOT/docs" "$REPO_ROOT/demo" -name "*.md"; echo "$REPO_ROOT/README.md")

errors=0

while IFS= read -r file; do
  while IFS= read -r name; do
    if ! echo "$ALLOWLIST" | grep -qx "$name"; then
      echo "STALE API: '$name' in $file"
      errors=$((errors + 1))
    fi
  done < <(grep -oP '`?\K[a-z][a-z_]+(?=\()' "$file" \
           | grep '_' \
           | grep -vE "^($EXCLUDE)$" \
           | grep -v '^test_' \
           | sort -u)
done <<< "$DOCS"

if [[ $errors -gt 0 ]]; then
  echo "$errors stale API reference(s) found."
  exit 1
fi

echo "All API references OK."
