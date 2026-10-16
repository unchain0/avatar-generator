#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd cargo
require_cmd curl
require_cmd ss

cd "$REPO_ROOT"

if [[ ! -x "$REPO_ROOT/target/debug/avatar-generator" ]]; then
  echo "building avatar-generator..."
  cargo build -q
fi
BIN="$REPO_ROOT/target/debug/avatar-generator"
[[ -x "$BIN" ]] || die "binary missing after build: $BIN"

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="$SCRATCH_ROOT/$RUN_ID"
mkdir -p "$SCRATCH/data"
touch "$SCRATCH/cookies.txt"
chmod 700 "$SCRATCH"
chmod 600 "$SCRATCH/cookies.txt"

PORT="$(pick_free_port)"
ORIGIN="http://127.0.0.1:${PORT}"
DATA_DIR="$SCRATCH/data"
STATIC_DIR="$REPO_ROOT/static"

if [[ -f "$SCRATCH/state.env" ]]; then
  die "scratch already initialized: $SCRATCH"
fi

# Build a clean env for the child. Only pass through optional ROBOHASH_BASE_URL.
CHILD_ENV=(
  "PORT=$PORT"
  "DATA_DIR=$DATA_DIR"
  "STATIC_DIR=$STATIC_DIR"
  "RUST_LOG=${RUST_LOG:-info,avatar_generator=debug}"
  "TTL_DAYS=${TTL_DAYS:-1}"
  "TRUST_PROXY=false"
  "PATH=$PATH"
  "HOME=${HOME:-/tmp}"
)
if [[ -n "${ROBOHASH_BASE_URL:-}" ]]; then
  CHILD_ENV+=("ROBOHASH_BASE_URL=$ROBOHASH_BASE_URL")
fi

env -i "${CHILD_ENV[@]}" "$BIN" >"$SCRATCH/server.log" 2>&1 &
PID=$!

cat >"$SCRATCH/state.env" <<EOF
RUN_ID=$RUN_ID
PORT=$PORT
ORIGIN=$ORIGIN
PID=$PID
BIN=$BIN
DATA_DIR=$DATA_DIR
STATIC_DIR=$STATIC_DIR
COOKIE_JAR=$SCRATCH/cookies.txt
REPO_ROOT=$REPO_ROOT
STARTED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
chmod 600 "$SCRATCH/state.env"

ready=0
for _ in $(seq 1 60); do
  if ! pid_alive "$PID"; then
    echo "server exited during startup; log:" >&2
    tail -n 50 "$SCRATCH/server.log" >&2 || true
    "$SCRIPT_DIR/cleanup.sh" "$SCRATCH" || true
    die "server died before ready"
  fi
  body="$(curl -sS --max-time 1 "$ORIGIN/healthz" 2>/dev/null || true)"
  if [[ "$body" == "ok" ]]; then
    ready=1
    break
  fi
  sleep 0.5
done

if [[ "$ready" -ne 1 ]]; then
  echo "timeout waiting for healthz; log:" >&2
  tail -n 50 "$SCRATCH/server.log" >&2 || true
  "$SCRIPT_DIR/cleanup.sh" "$SCRATCH" || true
  die "not ready"
fi

echo "ready origin=$ORIGIN pid=$PID data=$DATA_DIR"
echo "SCRATCH=$SCRATCH"
