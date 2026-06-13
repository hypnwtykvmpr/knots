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
    cat <<'JSON'
{
  "data": [
    {
      "id": "k1",
      "title": "demo",
      "state": "ready",
      "updated_at": "t",
      "type": "work",
      "tags": []
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 50,
  "has_more": false
}
JSON
    ;;
  show)
    cat <<'JSON'
{
  "id": "k1",
  "title": "demo",
  "state": "ready",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
JSON
    ;;
  poll)
    cat <<'JSON'
{
  "id": "k1",
  "title": "demo",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
JSON
    ;;
  new)
    cat <<'JSON'
{
  "id": "k-new",
  "title": "New",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
JSON
    ;;
  update)
    if [[ " ${args[*]} " == *" k-new "* ]]; then
      cat <<'JSON'
{
  "id": "k-new",
  "title": "New",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "priority": 3,
  "tags": []
}
JSON
    else
      cat <<'JSON'
{
  "id": "k1",
  "title": "updated",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
JSON
    fi
    ;;
  claim)
    lease_present=false
    if [[ " ${args[*]} " == *" --lease L1 "* ]]; then
      lease_present=true
    fi
    if [[ " ${args[*]} " == *" --e2e "* ]]; then
      cat <<JSON
{
  "id": "k1",
  "title": "demo",
  "state": "planning",
  "prompt": "do x",
  "e2e": true,
  "workflow_boundary_kind": "e2e_continuation",
  "lease_id": "L1",
  "lease_present": $lease_present
}
JSON
    else
      cat <<JSON
{
  "id": "k1",
  "title": "demo",
  "state": "planning",
  "prompt": "do x",
  "e2e": false,
  "workflow_boundary_kind": "single_action",
  "lease_id": "L1",
  "lease_present": $lease_present
}
JSON
    fi
    ;;
  next)
    cat <<'JSON'
{
  "id": "k1",
  "title": "demo",
  "state": "ready_for_review",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
JSON
    ;;
  rollback)
    cat <<'JSON'
{
  "id": "k1",
  "state": "implementation",
  "target_state": "ready_for_implementation",
  "owner_kind": "agent",
  "reason": "rolled back",
  "dry_run": false
}
JSON
    ;;
  push)
    if [[ "${KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION:-}" == "1" ]]; then
      echo '{"allow_active_leases":true}'
    else
      echo '{"allow_active_leases":false}'
    fi
    ;;
  sync)
    echo '{"status":"deferred","active_leases":1}'
    ;;
  lease)
    if [[ " ${args[*]} " == *" --agent-name other-client "* ]]; then
      echo '{"id":"L2","title":"mcp-session","state":"active","agent_info":{"model":"other"}}'
    else
      echo '{"id":"L1","title":"mcp-session","state":"active","agent_info":{"model":"test-model"}}'
    fi
    ;;
  *)
    echo '{}'
    ;;
esac
