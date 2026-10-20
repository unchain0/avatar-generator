#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd curl
SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: drive-home.sh SCRATCH"
load_state "$SCRATCH"

"$SCRIPT_DIR/doctor.sh" "$SCRATCH" >/dev/null

EVIDENCE="$(ensure_evidence_dir home-health)"
TRANSCRIPT="$EVIDENCE/transcript.txt"
: >"$TRANSCRIPT"
log() { echo "$*" | tee -a "$TRANSCRIPT"; }

log "feature=home-health origin=$ORIGIN run=$RUN_ID"

code="$(curl -sS -o "$EVIDENCE/healthz.txt" -w "%{http_code}" "$ORIGIN/healthz")"
body="$(cat "$EVIDENCE/healthz.txt")"
log "GET /healthz -> $code body=$(printf %s "$body" | tr '\n' ' ')"
[[ "$code" == "200" && "$body" == "ok" ]] || die "healthz failed"

code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -D "$EVIDENCE/index.headers" \
  -o "$EVIDENCE/index.html" -w "%{http_code}" "$ORIGIN/")"
log "GET / -> $code"
[[ "$code" == "200" ]] || die "index failed"
grep -q 'id="generateForm"' "$EVIDENCE/index.html" || die "missing generateForm"
grep -q 'id="gallery"' "$EVIDENCE/index.html" || die "missing gallery"
grep -q 'Gerador de Avatares RoboHash' "$EVIDENCE/index.html" || die "missing brand"
grep -qi 'set-cookie:.*avatar_session_id=' "$EVIDENCE/index.headers" \
  || die "missing avatar_session_id Set-Cookie"

# Empty gallery copy
grep -q 'Nenhum avatar ainda' "$EVIDENCE/index.html" \
  || grep -q 'avatar-card' "$EVIDENCE/index.html" \
  || die "gallery neither empty nor populated"

for asset in app.js style.css; do
  acode="$(curl -sS -o "$EVIDENCE/static-$asset" -w "%{http_code}" "$ORIGIN/static/$asset")"
  log "GET /static/$asset -> $acode"
  [[ "$acode" == "200" ]] || die "static $asset failed"
done

cat >"$EVIDENCE/PROOF.md" <<EOF
# Proof: home-health

- feature: home-health
- entry: GET /healthz, GET /, GET /static/*
- origin: $ORIGIN
- result: pass
- observed: healthz body \`ok\`; index HTML has generateForm + gallery + pt-BR brand;
  Set-Cookie avatar_session_id present; static assets 200.
- side effects: session cookie jar at cookies (not committed); no avatars written.
EOF

log "PASS evidence=$EVIDENCE"
echo "drive-home: pass evidence=$EVIDENCE"
