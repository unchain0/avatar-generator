#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd curl
require_cmd ss

SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: doctor.sh SCRATCH"
load_state "$SCRATCH"

fail() {
  echo "doctor: not ready: $*" >&2
  exit 1
}

pid_alive "$PID" || fail "pid $PID is not alive"
pid_cmd_ok "$PID" || fail "pid $PID command line is not avatar-generator"

owner="$(port_owner_pid "$PORT" || true)"
if [[ -n "$owner" && "$owner" != "$PID" ]]; then
  # Accept a child thread/worker showing as owner only if it shares the same process group ancestry.
  if [[ -r "/proc/$owner/status" ]]; then
    ppid="$(awk '/^PPid:/{print $2}' "/proc/$owner/status")"
    if [[ "$owner" != "$PID" && "$ppid" != "$PID" ]]; then
      fail "port $PORT owned by pid $owner, expected $PID"
    fi
  fi
fi

# Confirm something is listening on the port.
ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "[:.]$PORT$" \
  || fail "nothing listening on port $PORT"

hz="$(curl -sS --max-time 3 "$ORIGIN/healthz" || true)"
[[ "$hz" == "ok" ]] || fail "healthz body is '$hz', expected 'ok'"

code="$(curl -sS -o "$SCRATCH/doctor-index.html" -w "%{http_code}" --max-time 5 "$ORIGIN/" || true)"
[[ "$code" == "200" ]] || fail "GET / returned $code"
grep -q 'id="generateForm"' "$SCRATCH/doctor-index.html" || fail "index missing #generateForm"
grep -q 'id="gallery"' "$SCRATCH/doctor-index.html" || fail "index missing #gallery"
grep -q 'Gerador de Avatares RoboHash' "$SCRATCH/doctor-index.html" \
  || fail "index missing default pt-BR brand string"

for asset in app.js style.css; do
  acode="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 3 "$ORIGIN/static/$asset" || true)"
  [[ "$acode" == "200" ]] || fail "GET /static/$asset returned $acode"
done

[[ -d "$DATA_DIR" ]] || fail "DATA_DIR missing: $DATA_DIR"
[[ -w "$DATA_DIR" ]] || fail "DATA_DIR not writable: $DATA_DIR"

echo "doctor: ok origin=$ORIGIN pid=$PID data=$DATA_DIR"
