#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/common.sh"

require_cmd curl
SCRATCH="${1:-}"
[[ -n "$SCRATCH" ]] || die "usage: drive-language.sh SCRATCH"
load_state "$SCRATCH"

"$SCRIPT_DIR/doctor.sh" "$SCRATCH" >/dev/null

EVIDENCE="$(ensure_evidence_dir language-switch)"
TRANSCRIPT="$EVIDENCE/transcript.txt"
: >"$TRANSCRIPT"
log() { echo "$*" | tee -a "$TRANSCRIPT"; }

log "feature=language-switch origin=$ORIGIN run=$RUN_ID"

# Default pt-BR (no lang cookie)
curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" -o "$EVIDENCE/index-pt.html" "$ORIGIN/"
grep -q 'Gerador de Avatares RoboHash' "$EVIDENCE/index-pt.html" || die "pt-BR title missing"
grep -q '>Gerar<' "$EVIDENCE/index-pt.html" || die "pt-BR generate button missing"
log "default language is pt-BR"

# Switch to EN the way static/app.js does: document.cookie = "lang=en; ..."
curl -sS -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
  --cookie "lang=en" \
  -o "$EVIDENCE/index-en.html" "$ORIGIN/"
# curl --cookie adds to request; also persist
printf 'localhost\tFALSE\t/\tFALSE\t0\tlang\ten\n' >>"$COOKIE_JAR" || true
curl -sS -b "lang=en" -c "$COOKIE_JAR" -o "$EVIDENCE/index-en.html" "$ORIGIN/"
grep -q 'RoboHash Avatar Generator' "$EVIDENCE/index-en.html" || die "en title missing"
grep -q '>Generate<' "$EVIDENCE/index-en.html" || die "en generate button missing"
grep -q 'html lang="en"' "$EVIDENCE/index-en.html" || die "html lang not en"
log "lang=en serves English copy"

# Back to pt-br
curl -sS -b "lang=pt-br" -o "$EVIDENCE/index-pt2.html" "$ORIGIN/"
grep -q 'Gerador de Avatares RoboHash' "$EVIDENCE/index-pt2.html" || die "pt-BR restore failed"
log "lang=pt-br restores Portuguese copy"

cat >"$EVIDENCE/PROOF.md" <<EOF
# Proof: language-switch

- feature: language-switch
- entry: GET / with Cookie lang=en|pt-br (same as data-lang buttons in the topbar)
- origin: $ORIGIN
- result: pass
- observed: default HTML is pt-BR (\`Gerar\`, brand string); \`lang=en\` yields
  \`RoboHash Avatar Generator\` / \`Generate\` / \`html lang="en"\`; pt-br restores.
- side effects: none beyond the lang cookie.
EOF

log "PASS evidence=$EVIDENCE"
echo "drive-language: pass evidence=$EVIDENCE"
