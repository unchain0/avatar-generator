#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd curl
require_cmd jq
SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: drive-generate.sh SCRATCH"
load_state "$SCRATCH"

"$SCRIPT_DIR/doctor.sh" "$SCRATCH" >/dev/null

EVIDENCE="$(ensure_evidence_dir generate-avatars)"
TRANSCRIPT="$EVIDENCE/transcript.txt"
: >"$TRANSCRIPT"
log() { echo "$*" | tee -a "$TRANSCRIPT"; }

log "feature=generate-avatars origin=$ORIGIN run=$RUN_ID"

# Establish session cookie the way a browser would.
curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -o "$EVIDENCE/index-before.html" "$ORIGIN/" >/dev/null
log "GET / (session bootstrap)"

# Invalid count must be rejected (UI validates 1..50; server does too).
bad_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/generate-invalid.json" -w "%{http_code}" \
  -H 'content-type: application/json' \
  -d '{"count":0,"seed":"x","set":"set1","background":"none","size":200,"ttl_days":1}' \
  "$ORIGIN/generate")"
log "POST /generate count=0 -> $bad_code body=$(tr '\n' ' ' <"$EVIDENCE/generate-invalid.json")"
[[ "$bad_code" == "400" || "$bad_code" == "422" || "$bad_code" == "400" ]] \
  || [[ "$bad_code" =~ ^4 ]] || die "expected 4xx for invalid count, got $bad_code"

# Valid batch: 2 avatars, custom seed, set2, bg1, size 100.
payload='{"count":2,"seed":"verify-skill-seed","set":"set2","background":"bg1","size":100,"ttl_days":1}'
gen_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/generate.json" -w "%{http_code}" \
  -H 'content-type: application/json' \
  -d "$payload" \
  "$ORIGIN/generate")"
log "POST /generate valid -> $gen_code"
[[ "$gen_code" == "200" ]] || {
  cat "$EVIDENCE/generate.json" >&2
  die "generate failed with $gen_code"
}

count="$(jq -r '.count' "$EVIDENCE/generate.json")"
[[ "$count" == "2" ]] || die "expected count 2, got $count"
id1="$(jq -r '.avatars[0].id' "$EVIDENCE/generate.json")"
id2="$(jq -r '.avatars[1].id' "$EVIDENCE/generate.json")"
[[ -n "$id1" && "$id1" != null && -n "$id2" && "$id2" != null ]] || die "missing avatar ids"
log "avatars id1=$id1 id2=$id2"

# Gallery HTML must list both cards after generate (UI reloads).
curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -o "$EVIDENCE/index-after.html" "$ORIGIN/" >/dev/null
grep -q "data-id=\"$id1\"" "$EVIDENCE/index-after.html" || die "gallery missing card $id1"
grep -q "data-id=\"$id2\"" "$EVIDENCE/index-after.html" || die "gallery missing card $id2"
grep -q "/avatar/$id1" "$EVIDENCE/index-after.html" || die "gallery missing img for $id1"
log "GET / gallery contains both cards"

# Proxied PNG
png_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/avatar-${id1}.png" -w "%{http_code}" \
  -D "$EVIDENCE/avatar-${id1}.headers" \
  "$ORIGIN/avatar/$id1")"
log "GET /avatar/$id1 -> $png_code bytes=$(wc -c <"$EVIDENCE/avatar-${id1}.png")"
[[ "$png_code" == "200" ]] || die "avatar png failed"
# PNG magic
head -c 8 "$EVIDENCE/avatar-${id1}.png" | od -An -tx1 | grep -q '89 50 4e 47' \
  || die "avatar body is not PNG"
grep -qi 'content-type: image/png' "$EVIDENCE/avatar-${id1}.headers" \
  || die "avatar missing image/png content-type"

# Per-avatar download attachment
dl_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/download-${id1}.png" -w "%{http_code}" \
  -D "$EVIDENCE/download-${id1}.headers" \
  "$ORIGIN/download/$id1")"
log "GET /download/$id1 -> $dl_code"
[[ "$dl_code" == "200" ]] || die "download failed"
grep -qi "content-disposition:.*${id1}\\.png" "$EVIDENCE/download-${id1}.headers" \
  || die "download missing content-disposition filename"

# Download-all ZIP
zip_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/download-all.zip" -w "%{http_code}" \
  -D "$EVIDENCE/download-all.headers" \
  "$ORIGIN/download-all")"
log "GET /download-all -> $zip_code bytes=$(wc -c <"$EVIDENCE/download-all.zip")"
[[ "$zip_code" == "200" ]] || die "download-all failed"
# ZIP local file header magic PK\x03\x04
head -c 2 "$EVIDENCE/download-all.zip" | od -An -tx1 | grep -q '50 4b' \
  || die "download-all body is not ZIP"

# Side effect: files under DATA_DIR
found="$(find "$DATA_DIR" -type f -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
log "DATA_DIR png files=$found"
[[ "$found" -ge 2 ]] || die "expected cached PNGs under DATA_DIR"

# Delete one
del_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/delete-${id2}.json" -w "%{http_code}" \
  -X DELETE "$ORIGIN/avatar/$id2")"
log "DELETE /avatar/$id2 -> $del_code body=$(tr '\n' ' ' <"$EVIDENCE/delete-${id2}.json")"
[[ "$del_code" == "200" ]] || die "delete failed"
jq -e '.success == true' "$EVIDENCE/delete-${id2}.json" >/dev/null

# Clear remaining
clr_code="$(curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  -o "$EVIDENCE/clear-all.json" -w "%{http_code}" \
  -X POST "$ORIGIN/clear-all")"
log "POST /clear-all -> $clr_code body=$(tr '\n' ' ' <"$EVIDENCE/clear-all.json")"
[[ "$clr_code" == "200" ]] || die "clear-all failed"
jq -e '.success == true' "$EVIDENCE/clear-all.json" >/dev/null

curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -o "$EVIDENCE/index-cleared.html" "$ORIGIN/" >/dev/null
if grep -q 'avatar-card' "$EVIDENCE/index-cleared.html"; then
  die "gallery still has cards after clear-all"
fi
grep -q 'Nenhum avatar ainda' "$EVIDENCE/index-cleared.html" \
  || die "empty gallery copy missing after clear-all"
log "GET / after clear-all shows empty gallery"

cat >"$EVIDENCE/PROOF.md" <<EOF
# Proof: generate-avatars

- feature: generate-avatars
- entry: POST /generate (same JSON body as static/app.js), gallery reload, download routes, clear-all
- origin: $ORIGIN
- result: pass
- payload: count=2 seed=verify-skill-seed set=set2 background=bg1 size=100 ttl_days=1
- ids: $id1, $id2
- observed: invalid count → 4xx; valid generate → count 2; gallery cards; PNG magic on /avatar; attachment on /download; ZIP on /download-all; delete + clear-all empty the gallery.
- side effects: ≥2 PNG files under DATA_DIR during the run; gallery empty HTML after clear-all.
EOF

log "PASS evidence=$EVIDENCE"
echo "drive-generate: pass evidence=$EVIDENCE"
