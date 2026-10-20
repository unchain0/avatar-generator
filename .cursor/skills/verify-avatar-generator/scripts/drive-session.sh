#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd curl
require_cmd jq
SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: drive-session.sh SCRATCH"
load_state "$SCRATCH"

"$SCRIPT_DIR/doctor.sh" "$SCRATCH" >/dev/null

EVIDENCE="$(ensure_evidence_dir session-isolation)"
TRANSCRIPT="$EVIDENCE/transcript.txt"
: >"$TRANSCRIPT"
log() { echo "$*" | tee -a "$TRANSCRIPT"; }

JAR_A="$SCRATCH/cookies-a.txt"
JAR_B="$SCRATCH/cookies-b.txt"
: >"$JAR_A"
: >"$JAR_B"

log "feature=session-isolation origin=$ORIGIN run=$RUN_ID"

curl -sS -c "$JAR_A" -b "$JAR_A" -o /dev/null "$ORIGIN/"
curl -sS -c "$JAR_B" -b "$JAR_B" -o /dev/null "$ORIGIN/"
log "bootstrapped two cookie jars"

payload='{"count":1,"seed":"session-a-only","set":"set1","background":"none","size":50,"ttl_days":1}'
code_a="$(curl -sS -c "$JAR_A" -b "$JAR_A" -o "$EVIDENCE/gen-a.json" -w "%{http_code}" \
  -H 'content-type: application/json' -d "$payload" "$ORIGIN/generate")"
log "session A POST /generate -> $code_a"
[[ "$code_a" == "200" ]] || die "session A generate failed"
id_a="$(jq -r '.avatars[0].id' "$EVIDENCE/gen-a.json")"
[[ -n "$id_a" && "$id_a" != null ]] || die "missing id_a"

# Session B gallery must not list A's card
curl -sS -c "$JAR_B" -b "$JAR_B" -o "$EVIDENCE/index-b.html" "$ORIGIN/"
if grep -q "data-id=\"$id_a\"" "$EVIDENCE/index-b.html"; then
  die "session B gallery leaked session A avatar $id_a"
fi
log "session B gallery does not list $id_a"

# Session B cannot delete A's avatar
del_b="$(curl -sS -c "$JAR_B" -b "$JAR_B" -o "$EVIDENCE/delete-b.json" -w "%{http_code}" \
  -X DELETE "$ORIGIN/avatar/$id_a")"
log "session B DELETE /avatar/$id_a -> $del_b"
[[ "$del_b" == "404" ]] || die "expected 404 when B deletes A's avatar, got $del_b"

# Session A still has it
curl -sS -c "$JAR_A" -b "$JAR_A" -o "$EVIDENCE/index-a.html" "$ORIGIN/"
grep -q "data-id=\"$id_a\"" "$EVIDENCE/index-a.html" || die "session A lost its avatar"
log "session A still owns $id_a"

# Public PNG fetch by id still works (proxy is not session-gated) — document that.
png_code="$(curl -sS -o "$EVIDENCE/avatar-a.png" -w "%{http_code}" "$ORIGIN/avatar/$id_a")"
log "unauthenticated GET /avatar/$id_a -> $png_code (expected 200; image proxy is id-based)"
[[ "$png_code" == "200" ]] || die "avatar proxy failed"

# Clear A
clr="$(curl -sS -c "$JAR_A" -b "$JAR_A" -o "$EVIDENCE/clear-a.json" -w "%{http_code}" \
  -X POST "$ORIGIN/clear-all")"
log "session A POST /clear-all -> $clr body=$(tr '\n' ' ' <"$EVIDENCE/clear-a.json")"
[[ "$clr" == "200" ]] || die "clear-all failed"

cat >"$EVIDENCE/PROOF.md" <<EOF
# Proof: session-isolation

- feature: session-isolation
- entry: two cookie jars, POST /generate, DELETE /avatar/:id, POST /clear-all
- origin: $ORIGIN
- result: pass
- observed: avatar created in session A is absent from session B gallery; B gets 404 on
  DELETE of A's id; A still lists it until clear-all. Image proxy GET /avatar/:id is
  id-based (200 without session cookie) — documented, not a session leak of the gallery.
- side effects: clear-all on A removes the batch.
EOF

log "PASS evidence=$EVIDENCE"
echo "drive-session: pass evidence=$EVIDENCE"
