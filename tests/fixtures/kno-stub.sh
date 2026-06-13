#!/usr/bin/env bash
set -euo pipefail

args=("$@")
subcmd=""
for ((i = 0; i < ${#args[@]}; i++)); do
  case "${args[$i]}" in
    -C)
      i=$((i + 1))
      ;;
    --json|-j)
      ;;
    *)
      subcmd="${args[$i]}"
      break
      ;;
  esac
done

if [[ " ${args[*]} " == *" MISSING "* ]]; then
  echo "error: knot 'MISSING' not found" >&2
  exit 1
fi

case "$subcmd" in
  ls)
    echo '{"data":[{"id":"k1","title":"demo","state":"ready","updated_at":"t","type":"work","tags":[]}],"total":1,"offset":0,"limit":50,"has_more":false}'
    ;;
  show)
    echo '{"id":"k1","title":"demo","state":"ready","updated_at":"t","type":"work","tags":[]}'
    ;;
  poll)
    echo '{"id":"k1","title":"demo","state":"ready_for_implementation","updated_at":"t","type":"work","tags":[]}'
    ;;
  new)
    echo '{"id":"k-new","title":"New","state":"ready_for_implementation","updated_at":"t","type":"work","tags":[]}'
    ;;
  update)
    if [[ " ${args[*]} " == *" k-new "* ]]; then
      echo '{"id":"k-new","title":"New","state":"ready_for_implementation","updated_at":"t","type":"work","priority":3,"tags":[]}'
    else
      echo '{"id":"k1","title":"updated","state":"ready_for_implementation","updated_at":"t","type":"work","tags":[]}'
    fi
    ;;
  claim)
    if [[ " ${args[*]} " == *" --e2e "* ]]; then
      echo '{"id":"k1","title":"demo","state":"planning","prompt":"do x","e2e":true,"workflow_boundary_kind":"e2e_continuation","lease_id":"L1"}'
    else
      echo '{"id":"k1","title":"demo","state":"planning","prompt":"do x","e2e":false,"workflow_boundary_kind":"single_action","lease_id":"L1"}'
    fi
    ;;
  next)
    echo '{"id":"k1","title":"demo","state":"ready_for_review","updated_at":"t","type":"work","tags":[]}'
    ;;
  rollback)
    echo '{"id":"k1","state":"implementation","target_state":"ready_for_implementation","owner_kind":"agent","reason":"rolled back","dry_run":false}'
    ;;
  sync)
    echo '{"status":"deferred","active_leases":1}'
    ;;
  lease)
    echo '{"id":"L1","title":"Lease: mcp-session","state":"active","agent_info":{"model":"test-model"}}'
    ;;
  *)
    echo '{}'
    ;;
esac
