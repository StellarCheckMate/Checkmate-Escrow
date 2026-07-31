#!/usr/bin/env bash
# Run the TLC model checker over the match state-machine specification.
#
# Usage:
#   scripts/check_tla.sh              # check every model in specs/tla
#   scripts/check_tla.sh MatchStateMachine.cfg   # check one model
#
# Requirements:
#   - Java 11 or newer on PATH.
#   - tla2tools.jar. Resolution order:
#       1. $TLA_TOOLS_JAR
#       2. specs/tla/.tools/tla2tools.jar (cached by a previous run)
#       3. downloaded from the tlaplus GitHub release (needs network access)
#
# Why -deadlock is passed:
#   The model bounds the clock with MaxLedger. Once every match is terminal and
#   the clock has stopped, no action is enabled — TLC would report that as a
#   deadlock even though it is just the end of the bounded model. Deadlock
#   detection is therefore disabled; liveness is expressed by the explicit
#   properties in the spec instead.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPEC_DIR="$REPO_ROOT/specs/tla"
TOOLS_DIR="$SPEC_DIR/.tools"
SPEC="MatchStateMachine.tla"
TLA_VERSION="${TLA_VERSION:-v1.7.4}"
DOWNLOAD_URL="https://github.com/tlaplus/tlaplus/releases/download/${TLA_VERSION}/tla2tools.jar"

# ── Locate tla2tools.jar ──────────────────────────────────────────────────────
resolve_jar() {
  if [[ -n "${TLA_TOOLS_JAR:-}" ]]; then
    if [[ ! -f "$TLA_TOOLS_JAR" ]]; then
      echo "TLA_TOOLS_JAR is set but $TLA_TOOLS_JAR does not exist" >&2
      exit 1
    fi
    echo "$TLA_TOOLS_JAR"
    return
  fi

  local cached="$TOOLS_DIR/tla2tools.jar"
  if [[ -f "$cached" ]]; then
    echo "$cached"
    return
  fi

  echo "tla2tools.jar not found — downloading ${TLA_VERSION}…" >&2
  mkdir -p "$TOOLS_DIR"
  if ! curl -fsSL "$DOWNLOAD_URL" -o "$cached"; then
    cat >&2 <<EOF
Failed to download tla2tools.jar.

Fetch it manually and re-run, e.g.:
  mkdir -p $TOOLS_DIR
  curl -L $DOWNLOAD_URL -o $cached

Or point TLA_TOOLS_JAR at an existing copy.
EOF
    exit 1
  fi
  echo "$cached"
}

if ! command -v java >/dev/null 2>&1; then
  echo "java not found on PATH — TLC needs Java 11 or newer" >&2
  exit 1
fi

JAR="$(resolve_jar)"

# ── Select models ─────────────────────────────────────────────────────────────
if [[ $# -gt 0 ]]; then
  CONFIGS=("$@")
else
  CONFIGS=()
  while IFS= read -r cfg; do
    CONFIGS+=("$(basename "$cfg")")
  done < <(find "$SPEC_DIR" -maxdepth 1 -name '*.cfg' | sort)
fi

if [[ ${#CONFIGS[@]} -eq 0 ]]; then
  echo "No .cfg models found in $SPEC_DIR" >&2
  exit 1
fi

# ── Check each model ──────────────────────────────────────────────────────────
failures=0

for cfg in "${CONFIGS[@]}"; do
  echo
  echo "──────────────────────────────────────────────────────────────────"
  echo "TLC: $SPEC with $cfg"
  echo "──────────────────────────────────────────────────────────────────"

  if (cd "$SPEC_DIR" && java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC \
        -config "$cfg" \
        -workers auto \
        -deadlock \
        "$SPEC"); then
    echo "PASS: $cfg"
  else
    echo "FAIL: $cfg"
    failures=$((failures + 1))
  fi
done

echo
if [[ $failures -gt 0 ]]; then
  echo "$failures model(s) failed. See the TLC output above for the counter-example trace."
  exit 1
fi

echo "All TLA+ models passed."
