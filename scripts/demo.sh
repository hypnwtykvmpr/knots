#!/usr/bin/env bash
# scripts/demo.sh - replayable Knots lifecycle demo.
#
# Designed for asciinema. The recording covers the smallest useful workflow:
#   git init -> kno init -> new -> ready -> poll --claim -> update -> next -> show
#
# Safety:
# - Everything happens inside temp directories created by mktemp -d.
# - The demo remote is a local bare git repository, so no network or real
#   project state is touched.
# - The temp directories are removed on EXIT.
#
# Recording it:
#   asciinema rec --overwrite -c "bash scripts/demo.sh" assets/demo.cast
#   make demo-gif    # converts to assets/demo.gif via agg

set -euo pipefail

PAUSE="${PAUSE:-0.4}"

if [ -n "${KNO_BIN:-}" ]; then
  :
elif [ -x ./target/debug/knots ]; then
  KNO_BIN=./target/debug/knots
else
  KNO_BIN=kno
fi

if ! command -v "$KNO_BIN" >/dev/null 2>&1; then
  echo "demo: '$KNO_BIN' not found on PATH and no ./target/debug/knots binary." >&2
  echo "      build it first: cargo build" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "demo: requires 'jq' on PATH to extract the claim prompt and lease." >&2
  exit 1
fi

DEMO_TMP="$(mktemp -d -t knots-demo-XXXXXX)"
REMOTE_TMP="$(mktemp -d -t knots-demo-remote-XXXXXX)"
cleanup() {
  rm -rf "$DEMO_TMP" "$REMOTE_TMP"
}
trap cleanup EXIT INT TERM

say() {
  printf '\n$ %s\n' "$*"
  sleep "$PAUSE"
}

run_kno() {
  "$KNO_BIN" --repo-root "$DEMO_TMP" "$@"
}

say "git init && git remote add origin /tmp/knots-demo-remote"
git -C "$REMOTE_TMP" init --bare -q
git -C "$DEMO_TMP" init -q
git -C "$DEMO_TMP" config user.email demo@example.test
git -C "$DEMO_TMP" config user.name "Knots Demo"
git -C "$DEMO_TMP" remote add origin "$REMOTE_TMP"
touch "$DEMO_TMP/README.md"
git -C "$DEMO_TMP" add README.md
git -C "$DEMO_TMP" commit -qm "initial repo"
git -C "$DEMO_TMP" push -q -u origin main
sleep "$PAUSE"

say "kno init"
run_kno init
sleep "$PAUSE"

say "kno new \"Tighten release checklist\" --desc \"Document the release sanity checks.\""
NEW_OUTPUT="$(
  run_kno new "Tighten release checklist" \
    --desc "Document the release sanity checks."
)"
printf '%s\n' "$NEW_OUTPUT"
KNOT_ID="$(printf '%s\n' "$NEW_OUTPUT" | awk '{print $2}')"
sleep "$PAUSE"

say "kno ready"
run_kno ready
sleep "$PAUSE"

say "kno poll --claim --json | jq -r .prompt"
CLAIM_JSON="$(run_kno poll --claim --json)"
LEASE_ID="$(printf '%s\n' "$CLAIM_JSON" | jq -r .lease_id)"
printf '%s\n' "$CLAIM_JSON" | jq -r .prompt | sed -n '1,42p'
sleep "$PAUSE"

say "kno update $KNOT_ID --add-note \"Plan: update docs, run make sanity, tag from main.\""
run_kno update "$KNOT_ID" \
  --add-note "Plan: update docs, run make sanity, tag from main."
sleep "$PAUSE"

say "kno next $KNOT_ID --expected-state planning --lease \$LEASE_ID --actor-kind agent"
run_kno next "$KNOT_ID" \
  --expected-state planning \
  --lease "$LEASE_ID" \
  --actor-kind agent
sleep "$PAUSE"

say "kno show $KNOT_ID"
run_kno show "$KNOT_ID" | sed -n '1,34p'
sleep "$PAUSE"

printf '\n# done - temp repo at %s removed on exit.\n' "$DEMO_TMP"
