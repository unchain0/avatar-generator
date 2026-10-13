#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: cleanup.sh SCRATCH"

if [[ ! -f "$SCRATCH/state.env" ]]; then
  echo "cleanup: no state.env at $SCRATCH (nothing to stop)"
  # Still try to remove empty/partial scratch if under the expected root.
  if [[ "$SCRATCH" == "$SCRATCH_ROOT"/* && -d "$SCRATCH" ]]; then
    rm -rf "$SCRATCH"
    echo "cleanup: removed $SCRATCH"
  fi
  exit 0
fi

# shellcheck disable=SC1091
source "$SCRATCH/state.env"

if [[ -n "${PID:-}" ]] && pid_alive "$PID"; then
  if pid_cmd_ok "$PID"; then
    owner="$(port_owner_pid "${PORT:-}" || true)"
    if [[ -z "${PORT:-}" || -z "$owner" || "$owner" == "$PID" ]]; then
      echo "cleanup: SIGTERM pid=$PID"
      kill -TERM "$PID" 2>/dev/null || true
      for _ in $(seq 1 20); do
        pid_alive "$PID" || break
        sleep 0.25
      done
      if pid_alive "$PID" && pid_cmd_ok "$PID"; then
        echo "cleanup: SIGKILL pid=$PID"
        kill -KILL "$PID" 2>/dev/null || true
      fi
    else
      echo "cleanup: refusing to signal pid $PID; port ${PORT:-?} owned by $owner" >&2
    fi
  else
    echo "cleanup: pid $PID is alive but not avatar-generator; leaving it alone" >&2
  fi
else
  echo "cleanup: pid ${PID:-unknown} already gone"
fi

# Never touch evidence under the skill directory.
if [[ "$SCRATCH" == "$SCRATCH_ROOT"/* ]]; then
  rm -rf "$SCRATCH"
  echo "cleanup: removed $SCRATCH"
else
  echo "cleanup: refusing to delete unexpected path $SCRATCH" >&2
  exit 1
fi

echo "cleanup: evidence retained under $EVIDENCE_ROOT"
