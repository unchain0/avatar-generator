#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SCRATCH="${1:-}"
FEATURE="${2:-}"
[[ -n "$SCRATCH" && -n "$FEATURE" ]] || {
  echo "usage: drive.sh SCRATCH <home-health|generate-avatars|session-isolation|language-switch|gallery-download>" >&2
  exit 2
}

case "$FEATURE" in
  home-health|home) exec "$SCRIPT_DIR/drive-home.sh" "$SCRATCH" ;;
  generate-avatars|generate|gallery-download) exec "$SCRIPT_DIR/drive-generate.sh" "$SCRATCH" ;;
  session-isolation|session) exec "$SCRIPT_DIR/drive-session.sh" "$SCRATCH" ;;
  language-switch|language|lang) exec "$SCRIPT_DIR/drive-language.sh" "$SCRATCH" ;;
  *)
    echo "unknown feature: $FEATURE" >&2
    exit 2
    ;;
esac
