#!/usr/bin/env bash
set -u -o pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

URL="${KNO_MCP_URL:-https://manhattan.tailfd2e8e.ts.net/mcp}"
TAILNET_URL="${KNO_MCP_TAILNET_URL:-$URL}"
SSH_HOST="${KNO_MCP_SSH_HOST:-manhattan}"
SERVICE_NAME="${KNO_MCP_SERVICE_NAME:-kno-mcp}"
MACBOOK_REPO="${KNO_MCP_MACBOOK_REPO:-$ROOT_DIR}"
LOCAL_SERVICE_URL="${KNO_MCP_SERVICE_LOCAL_URL:-http://127.0.0.1:7777/mcp}"
REMOTE_TOKEN_FILE="${KNO_MCP_REMOTE_TOKEN_FILE:-/etc/kno-mcp/token}"
ALLOW_RESTART_CHECK="${KNO_MCP_ALLOW_RESTART_CHECK:-0}"
SSH_TIMEOUT_SECONDS="${KNO_MCP_SSH_TIMEOUT_SECONDS:-8}"
PROBE_CLIENT_NAME="${KNO_MCP_PROBE_CLIENT_NAME:-sandbox-probe}"
PROBE_CLIENT_VERSION="${KNO_MCP_PROBE_CLIENT_VERSION:-1.0.0}"
PROBE_CLIENT_PROVIDER="${KNO_MCP_PROBE_CLIENT_PROVIDER:-sandbox-probe-provider}"

TOTAL=0
FAILED=0

usage() {
  cat <<'USAGE'
Verify the external Phase 2 gates from docs/mcp-server-design.html.

Environment:
  KNO_MCP_URL                  MCP URL reachable from this host.
  KNO_MCP_TAILNET_URL          Tailnet URL for V2.5a; defaults to KNO_MCP_URL.
  KNO_MCP_TOKEN                Bearer token for local HTTP probes.
  KNO_MCP_TOKEN_FILE           File containing the bearer token.
  KNO_MCP_SSH_HOST             SSH host for Manhattan; default: manhattan.
  KNO_MCP_SERVICE_NAME         systemd service name; default: kno-mcp.
  KNO_MCP_SERVICE_LOCAL_URL    Service-host URL for V2.4c.
  KNO_MCP_REMOTE_TOKEN_FILE    Token file on the service host for V2.4c.
  KNO_MCP_SSH_TIMEOUT_SECONDS  Seconds before an SSH probe is failed.
  KNO_MCP_PROBE_CLIENT_NAME     Sandbox client name for V2.6 identity proof.
  KNO_MCP_PROBE_CLIENT_VERSION  Sandbox client version for V2.6 identity proof.
  KNO_MCP_PROBE_CLIENT_PROVIDER Sandbox provider title for V2.6 identity proof.
  KNO_MCP_PUBLIC_PROBE_CMD     Non-tailnet curl command; must fail for V2.5b.
  KNO_MCP_MACBOOK_REPO         MacBook repo used after remote MCP mutation.
  KNO_MCP_ALLOW_RESTART_CHECK  Set to 1 to run V2.4b's kill/restart check.

The script prints GO or NO-GO for V2.4a-c, V2.5a-b, and V2.6a-b.
It exits 0 only when every checked gate is GO.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

record_go() {
  local gate="$1"
  local detail="$2"
  TOTAL=$((TOTAL + 1))
  printf 'GO %s %s\n' "$gate" "$detail"
}

record_nogo() {
  local gate="$1"
  local detail="$2"
  TOTAL=$((TOTAL + 1))
  FAILED=$((FAILED + 1))
  printf 'NO-GO %s %s\n' "$gate" "$detail"
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

redact_status() {
  sed -E 's/(Bearer )[[:alnum:]_.~+\/=-]+/\1<redacted>/g'
}

shell_quote() {
  printf '%q' "$1"
}

load_token() {
  if [[ -n "${KNO_MCP_TOKEN:-}" ]]; then
    printf '%s' "$KNO_MCP_TOKEN"
    return 0
  fi
  if [[ -n "${KNO_MCP_TOKEN_FILE:-}" && -r "${KNO_MCP_TOKEN_FILE:-}" ]]; then
    tr -d '\r\n' < "$KNO_MCP_TOKEN_FILE"
    return 0
  fi
  return 1
}

ssh_run() {
  local stdout stderr pid waited status
  stdout="$(mktemp)"
  stderr="$(mktemp)"
  ssh -o BatchMode=yes -o ConnectTimeout=5 "$SSH_HOST" "$@" \
    >"$stdout" 2>"$stderr" &
  pid=$!
  waited=0
  while kill -0 "$pid" >/dev/null 2>&1; do
    if [[ "$waited" -ge "$SSH_TIMEOUT_SECONDS" ]]; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
      cat "$stdout" "$stderr"
      rm -f "$stdout" "$stderr"
      printf 'SSH timed out after %ss\n' "$SSH_TIMEOUT_SECONDS"
      return 124
    fi
    sleep 1
    waited=$((waited + 1))
  done
  wait "$pid"
  status=$?
  cat "$stdout" "$stderr"
  rm -f "$stdout" "$stderr"
  return "$status"
}

json_request() {
  local method="$1"
  local params="$2"
  jq -nc --arg method "$method" --argjson params "$params" \
    '{jsonrpc:"2.0",id:1,method:$method,params:$params}'
}

http_post() {
  local url="$1"
  local body="$2"
  local token="$3"
  local output="$4"
  local code
  code=$(curl -sS --max-time 10 \
    -H "Authorization: Bearer ${token}" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -o "$output" \
    -w '%{http_code}' \
    -XPOST "$url" \
    -d "$body" 2>"$output.stderr")
  local status=$?
  if [[ $status -ne 0 ]]; then
    printf 'curl exit %s: ' "$status"
    redact_status < "$output.stderr"
    return 1
  fi
  if [[ "$code" != "200" ]]; then
    printf 'HTTP %s: ' "$code"
    head -c 300 "$output" | redact_status
    return 1
  fi
  return 0
}

initialize_body() {
  jq -nc \
    --arg name "$PROBE_CLIENT_NAME" \
    --arg version "$PROBE_CLIENT_VERSION" \
    --arg title "$PROBE_CLIENT_PROVIDER" \
    '{
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: {
        name: $name,
        version: $version,
        title: $title
      }
    }
  }'
}

tool_body() {
  local name="$1"
  local args="$2"
  json_request "tools/call" "$(jq -nc --arg name "$name" --argjson args "$args" \
    '{name:$name,arguments:$args}')"
}

tool_call() {
  local name="$1"
  local args="$2"
  local output="$3"
  local body
  body=$(tool_body "$name" "$args")
  http_post "$URL" "$body" "$TOKEN" "$output"
}

check_requirements() {
  local missing=0
  for cmd in curl jq ssh; do
    if ! need_cmd "$cmd"; then
      record_nogo "pre.${cmd}" "missing required command"
      missing=1
    fi
  done
  if ! TOKEN="$(load_token)"; then
    TOKEN=""
  fi
  return "$missing"
}

check_service_active() {
  local out
  if ! out=$(ssh_run "systemctl is-active $(shell_quote "$SERVICE_NAME")"); then
    record_nogo "V2.4a" "SSH/systemctl failed: $(printf '%s' "$out" | head -n 1)"
    return
  fi
  if [[ "$out" == "active" ]]; then
    record_go "V2.4a" "systemctl is-active returned active"
  else
    record_nogo "V2.4a" "expected active, got: $out"
  fi
}

check_restart() {
  if [[ "$ALLOW_RESTART_CHECK" != "1" ]]; then
    record_nogo "V2.4b" "restart check not run; set KNO_MCP_ALLOW_RESTART_CHECK=1"
    return
  fi

  local remote
  read -r -d '' remote <<'REMOTE' || true
set -u
service="$1"
pid="$(systemctl show "$service" --property=MainPID --value 2>/dev/null)"
case "$pid" in ''|0) echo "missing MainPID"; exit 1;; esac
kill "$pid" || exit 1
sleep 5
systemctl is-active "$service"
REMOTE

  local out
  if ! out=$(ssh_run "bash -s -- $(shell_quote "$SERVICE_NAME")" <<<"$remote"); then
    record_nogo "V2.4b" "restart probe failed: $(printf '%s' "$out" | head -n 1)"
    return
  fi
  if [[ "$out" == "active" ]]; then
    record_go "V2.4b" "service restarted and returned active"
  else
    record_nogo "V2.4b" "expected active after restart, got: $out"
  fi
}

check_service_host_initialize() {
  local remote
  read -r -d '' remote <<'REMOTE' || true
set -u
url="$1"
token_file="$2"
token="$(tr -d '\r\n' < "$token_file")" || exit 1
body="$(cat <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {},
    "clientInfo": {
      "name": "service-host-probe",
      "version": "1.0.0"
    }
  }
}
JSON
)"
curl -fsS --max-time 10 \
  -H "Authorization: Bearer ${token}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -XPOST "$url" \
  -d "$body"
REMOTE

  local out
  out=$(ssh_run \
    "bash -s -- $(shell_quote "$LOCAL_SERVICE_URL") $(shell_quote "$REMOTE_TOKEN_FILE")" \
    <<<"$remote")
  if [[ $? -ne 0 ]]; then
    record_nogo "V2.4c" "service-host initialize failed: $(printf '%s' "$out" | head -n 1)"
    return
  fi
  if jq -e '.result.serverInfo.name == "kno-mcp"' >/dev/null 2>&1 <<<"$out"; then
    record_go "V2.4c" "service-host initialize returned serverInfo"
  else
    record_nogo "V2.4c" "initialize response did not contain kno-mcp serverInfo"
  fi
}

check_tailnet_initialize() {
  if [[ -z "$TOKEN" ]]; then
    record_nogo "V2.5a" "missing KNO_MCP_TOKEN or KNO_MCP_TOKEN_FILE"
    return
  fi

  local body output
  body=$(initialize_body)
  output="$(mktemp)"
  if ! http_post "$TAILNET_URL" "$body" "$TOKEN" "$output"; then
    record_nogo "V2.5a" "$(cat "$output.stderr" 2>/dev/null | head -n 1)"
    rm -f "$output" "$output.stderr"
    return
  fi
  if jq -e '.result.serverInfo.name == "kno-mcp"' "$output" >/dev/null; then
    record_go "V2.5a" "tailnet initialize returned serverInfo"
  else
    record_nogo "V2.5a" "initialize response did not contain kno-mcp serverInfo"
  fi
  rm -f "$output" "$output.stderr"
}

check_non_tailnet_probe() {
  if [[ -z "${KNO_MCP_PUBLIC_PROBE_CMD:-}" ]]; then
    record_nogo "V2.5b" "missing KNO_MCP_PUBLIC_PROBE_CMD from a non-tailnet host"
    return
  fi
  if bash -c "$KNO_MCP_PUBLIC_PROBE_CMD" >/tmp/kno-mcp-public-probe.out 2>&1; then
    record_nogo "V2.5b" "non-tailnet probe succeeded; endpoint is reachable"
  else
    record_go "V2.5b" "non-tailnet probe failed as expected"
  fi
}

macbook_claim_identity_visible() {
  local knot_id="$1"
  local show_out status
  show_out="$(mktemp)"

  if ! kno -C "$MACBOOK_REPO" sync >/dev/null 2>&1; then
    rm -f "$show_out" "$show_out.stderr"
    return 1
  fi
  if ! kno -C "$MACBOOK_REPO" show "$knot_id" -j \
    >"$show_out" 2>"$show_out.stderr"; then
    rm -f "$show_out" "$show_out.stderr"
    return 1
  fi

  jq -e \
    --arg name "$PROBE_CLIENT_NAME" \
    --arg version "$PROBE_CLIENT_VERSION" \
    --arg provider "$PROBE_CLIENT_PROVIDER" \
    '
      .lease_agent.agent_type == "api"
      and .lease_agent.provider == $provider
      and .lease_agent.agent_name == $name
      and .lease_agent.model == $name
      and .lease_agent.model_version == $version
      and (
        [
          .step_history[]?
          | select(
              .status == "started"
              and .agent_name == $name
              and .agent_model == $name
              and .agent_version == $version
            )
        ]
        | length >= 1
      )
    ' "$show_out" >/dev/null
  status=$?
  rm -f "$show_out" "$show_out.stderr"
  return "$status"
}

check_sandbox_claim_and_next() {
  if [[ -z "$TOKEN" ]]; then
    record_nogo "V2.6a" "missing KNO_MCP_TOKEN or KNO_MCP_TOKEN_FILE"
    record_nogo "V2.6b" "missing KNO_MCP_TOKEN or KNO_MCP_TOKEN_FILE"
    return
  fi
  if ! need_cmd kno; then
    record_nogo "V2.6a" "missing local kno for MacBook convergence check"
    record_nogo "V2.6b" "missing local kno for MacBook convergence check"
    return
  fi

  local title init_out create_out claim_out next_out knot_id next_state
  title="mcp-external-probe-$(date -u +%Y%m%d%H%M%S)-$$"
  init_out="$(mktemp)"
  create_out="$(mktemp)"
  claim_out="$(mktemp)"
  next_out="$(mktemp)"

  if ! http_post "$URL" "$(initialize_body)" "$TOKEN" "$init_out"; then
    record_nogo "V2.6a" "initialize failed before sandbox-equivalent calls"
    record_nogo "V2.6b" "initialize failed before sandbox-equivalent calls"
    rm -f "$init_out" "$init_out.stderr" "$create_out" "$claim_out" "$next_out"
    return
  fi
  rm -f "$init_out" "$init_out.stderr"

  local create_args
  create_args="$(jq -nc --arg title "$title" '{title:$title}')"
  if ! tool_call "knots_create" "$create_args" "$create_out"; then
    record_nogo "V2.6a" "knots_create failed"
    record_nogo "V2.6b" "knots_create failed"
    rm -f "$create_out" "$create_out.stderr" "$claim_out" "$next_out"
    return
  fi
  knot_id="$(jq -r '.result.structuredContent.id // empty' "$create_out")"
  if [[ -z "$knot_id" ]]; then
    record_nogo "V2.6a" "created knot id missing from structuredContent"
    record_nogo "V2.6b" "created knot id missing from structuredContent"
    rm -f "$create_out" "$create_out.stderr" "$claim_out" "$next_out"
    return
  fi

  if ! tool_call "knots_claim" "$(jq -nc --arg id "$knot_id" '{id:$id}')" "$claim_out"; then
    record_nogo "V2.6a" "knots_claim failed for $knot_id"
    record_nogo "V2.6b" "claim prerequisite failed"
    rm -f "$create_out" "$create_out.stderr" "$claim_out" "$claim_out.stderr" "$next_out"
    return
  fi
  if jq -e '.result.structuredContent.workflow_boundary_kind == "single_action"' \
    "$claim_out" >/dev/null; then
    if macbook_claim_identity_visible "$knot_id"; then
      record_go "V2.6a" "claim returned single_action and MacBook sees MCP identity"
    else
      record_nogo "V2.6a" "claim worked, but MacBook lease identity evidence is missing"
    fi
  else
    record_nogo "V2.6a" "claim did not return single_action boundary"
  fi

  if ! tool_call "knots_next" "$(jq -nc --arg id "$knot_id" '{id:$id}')" "$next_out"; then
    record_nogo "V2.6b" "knots_next failed for $knot_id"
    rm -f "$create_out" "$create_out.stderr" "$claim_out" "$claim_out.stderr"
    rm -f "$next_out" "$next_out.stderr"
    return
  fi
  next_state="$(jq -r '.result.structuredContent.state // empty' "$next_out")"
  if [[ -z "$next_state" ]]; then
    record_nogo "V2.6b" "next response missing advanced state"
    rm -f "$create_out" "$create_out.stderr" "$claim_out" "$claim_out.stderr"
    rm -f "$next_out" "$next_out.stderr"
    return
  fi
  if kno -C "$MACBOOK_REPO" sync >/dev/null 2>&1 &&
    kno -C "$MACBOOK_REPO" show "$knot_id" -j |
      jq -e --arg state "$next_state" '.state == $state' >/dev/null; then
    record_go "V2.6b" "MacBook sees advanced state $next_state"
  else
    record_nogo "V2.6b" "MacBook does not see advanced state $next_state"
  fi
  rm -f "$create_out" "$create_out.stderr" "$claim_out" "$claim_out.stderr"
  rm -f "$next_out" "$next_out.stderr"
}

check_requirements || true
check_service_active
check_restart
check_service_host_initialize
check_tailnet_initialize
check_non_tailnet_probe
check_sandbox_claim_and_next

printf 'SUMMARY %d checked, %d NO-GO\n' "$TOTAL" "$FAILED"
if [[ "$FAILED" -eq 0 ]]; then
  exit 0
fi
exit 1
