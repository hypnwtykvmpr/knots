#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MAX_AGE_HOURS="${1:-${ARTIFACT_MAX_AGE_HOURS:-24}}"

case "$MAX_AGE_HOURS" in
  ''|*[!0-9]*)
    echo "MAX_AGE_HOURS must be a positive integer" >&2
    exit 2
    ;;
esac

if [ "$MAX_AGE_HOURS" -eq 0 ]; then
  echo "MAX_AGE_HOURS must be greater than zero" >&2
  exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to compute the artifact cutoff time" >&2
  exit 2
fi

STAMP="$(mktemp "${TMPDIR:-/tmp}/knots-artifact-cutoff.XXXXXX")"
trap 'rm -f "$STAMP"' EXIT

python3 - "$STAMP" "$MAX_AGE_HOURS" <<'PY'
import os
import sys
import time

stamp = sys.argv[1]
hours = int(sys.argv[2])
cutoff = time.time() - hours * 3600
os.utime(stamp, (cutoff, cutoff))
PY

removed=0

if [ -d "$ROOT/target" ]; then
  while IFS= read -r -d '' artifact; do
    if [ "$artifact" -newer "$STAMP" ]; then
      continue
    fi

    if find "$artifact" -type f -newer "$STAMP" -print -quit | grep -q .; then
      continue
    fi

    echo "Reaping stale artifact tree: ${artifact#$ROOT/}"
    rm -rf "$artifact"
    removed=1
  done < <(find "$ROOT/target" -mindepth 1 -maxdepth 1 -type d -print0)
fi

if [ "$removed" -eq 0 ]; then
  echo "No stale build artifacts older than ${MAX_AGE_HOURS}h."
fi
