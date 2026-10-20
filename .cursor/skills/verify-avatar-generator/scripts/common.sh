#!/usr/bin/env bash
# Shared helpers for verify-avatar-generator. Sourced by launch/doctor/drive/cleanup.
set -euo pipefail

SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$SKILL_DIR/../../.." && pwd)"
EVIDENCE_ROOT="$SKILL_DIR/evidence"
SCRATCH_ROOT="/tmp/avatar-generator-verify"

die() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

load_state() {
  local scratch="${1:-}"
  [[ -n "$scratch" ]] || die "SCRATCH path required"
  [[ -f "$scratch/state.env" ]] || die "missing state.env in $scratch"
  # shellcheck disable=SC1091
  source "$scratch/state.env"
  : "${PORT:?PORT missing in state.env}"
  : "${ORIGIN:?ORIGIN missing in state.env}"
  : "${PID:?PID missing in state.env}"
  : "${DATA_DIR:?DATA_DIR missing in state.env}"
  : "${RUN_ID:?RUN_ID missing in state.env}"
  COOKIE_JAR="${COOKIE_JAR:-$scratch/cookies.txt}"
}

pid_alive() {
  local pid="$1"
  [[ -d "/proc/$pid" ]] || return 1
  # still the same start? /proc/pid exists
  return 0
}

pid_cmd_ok() {
  local pid="$1"
  [[ -r "/proc/$pid/cmdline" ]] || return 1
  tr '\0' ' ' <"/proc/$pid/cmdline" | grep -q 'avatar-generator'
}

port_owner_pid() {
  local port="$1"
  # Prefer ss; fall back empty
  if command -v ss >/dev/null 2>&1; then
    ss -ltnp "sport = :$port" 2>/dev/null | sed -n 's/.*pid=\([0-9]\+\).*/\1/p' | head -n1
  fi
}

pick_free_port() {
  local port
  for port in $(seq 18100 18200); do
    if ! ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "[:.]$port$"; then
      # also try binding briefly with bash /dev/tcp is not reliable for bind
      if command -v python3 >/dev/null 2>&1; then
        if python3 - "$port" <<'PY'
import socket, sys
p=int(sys.argv[1])
s=socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("127.0.0.1", p))
except OSError:
    sys.exit(1)
s.close()
sys.exit(0)
PY
        then
          echo "$port"
          return 0
        fi
      else
        echo "$port"
        return 0
      fi
    fi
  done
  die "no free port in 18100-18200"
}

http_get() {
  # http_get PATH [extra curl args...]
  local path="$1"
  shift || true
  curl -sS -b "$COOKIE_JAR" -c "$COOKIE_JAR" "$@" "${ORIGIN}${path}"
}

http_code() {
  local method="$1"
  local path="$2"
  shift 2 || true
  curl -sS -o /tmp/avatar-verify-body.$$ -w "%{http_code}" \
    -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
    -X "$method" "$@" "${ORIGIN}${path}"
  # caller may read /tmp/avatar-verify-body.$$ — cleaned by drive scripts into evidence
}

ensure_evidence_dir() {
  local feature="$1"
  local dir="$EVIDENCE_ROOT/${feature}-${RUN_ID}"
  mkdir -p "$dir"
  echo "$dir"
}
