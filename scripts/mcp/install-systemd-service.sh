#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="${KNO_MCP_SERVICE_NAME:-kno-mcp}"
SERVICE_USER="${KNO_MCP_USER:-kno-mcp}"
BINARY="${KNO_MCP_BINARY:-/usr/local/bin/kno-mcp}"
KNO_BIN="${KNO_MCP_KNO_BIN:-/usr/local/bin/kno}"
REPO="${KNO_MCP_REPO:-/var/lib/knots/repo}"
BIND="${KNO_MCP_BIND:-127.0.0.1:7777}"
TOKEN_FILE="${KNO_MCP_TOKEN_FILE:-/etc/kno-mcp/token}"
SYNC_INTERVAL_SECONDS="${KNO_MCP_SYNC_INTERVAL_SECONDS:-15}"
LEASE_TIMEOUT_SECONDS="${KNO_MCP_LEASE_TIMEOUT_SECONDS:-600}"
SERVICE_FILE="${KNO_MCP_SERVICE_FILE:-/etc/systemd/system/${SERVICE_NAME}.service}"
DRY_RUN=0

usage() {
  cat <<'USAGE'
Install or update the kno-mcp systemd service on a Linux service host.

Usage:
  scripts/mcp/install-systemd-service.sh [--dry-run]

Environment:
  KNO_MCP_SERVICE_NAME             systemd service name, default: kno-mcp
  KNO_MCP_USER                     service user, default: kno-mcp
  KNO_MCP_BINARY                   kno-mcp binary path, default: /usr/local/bin/kno-mcp
  KNO_MCP_KNO_BIN                  kno binary path, default: /usr/local/bin/kno
  KNO_MCP_REPO                     served repo path, default: /var/lib/knots/repo
  KNO_MCP_BIND                     bind addr:port, default: 127.0.0.1:7777
  KNO_MCP_TOKEN_FILE               bearer token file, default: /etc/kno-mcp/token
  KNO_MCP_TOKEN                    token to write when token file does not exist
  KNO_MCP_SYNC_INTERVAL_SECONDS    background sync interval, default: 15
  KNO_MCP_LEASE_TIMEOUT_SECONDS    MCP lease timeout, default: 600
  KNO_MCP_SERVICE_FILE             unit path, default: /etc/systemd/system/kno-mcp.service

Set KNO_MCP_BIND to the Manhattan tailnet IP/port for the Phase 2 deployment.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

require_root() {
  if [[ "$DRY_RUN" == "0" && "$(id -u)" != "0" ]]; then
    printf 'run as root, or pass --dry-run to preview the unit\n' >&2
    exit 1
  fi
}

reject_whitespace() {
  local name="$1"
  local value="$2"
  if [[ "$value" =~ [[:space:]] ]]; then
    printf '%s must not contain whitespace: %s\n' "$name" "$value" >&2
    exit 1
  fi
}

validate_config() {
  reject_whitespace KNO_MCP_SERVICE_NAME "$SERVICE_NAME"
  reject_whitespace KNO_MCP_USER "$SERVICE_USER"
  reject_whitespace KNO_MCP_BINARY "$BINARY"
  reject_whitespace KNO_MCP_KNO_BIN "$KNO_BIN"
  reject_whitespace KNO_MCP_REPO "$REPO"
  reject_whitespace KNO_MCP_BIND "$BIND"
  reject_whitespace KNO_MCP_TOKEN_FILE "$TOKEN_FILE"
  reject_whitespace KNO_MCP_SERVICE_FILE "$SERVICE_FILE"
}

unit_text() {
  cat <<UNIT
[Unit]
Description=Knots MCP server
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
Group=${SERVICE_USER}
WorkingDirectory=${REPO}
Environment=HOME=/var/lib/knots
ExecStart=${BINARY} --repo ${REPO} --kno-bin ${KNO_BIN} \\
  --lease-timeout-seconds ${LEASE_TIMEOUT_SECONDS} serve \\
  --bind ${BIND} --token-file ${TOKEN_FILE} \\
  --sync-interval-seconds ${SYNC_INTERVAL_SECONDS}
Restart=always
RestartSec=2
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=/var/lib/knots ${REPO}

[Install]
WantedBy=multi-user.target
UNIT
}

ensure_paths() {
  install -d -m 0755 /etc/kno-mcp
  install -d -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0755 /var/lib/knots
  install -d -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0755 "$REPO"
}

ensure_user() {
  if id "$SERVICE_USER" >/dev/null 2>&1; then
    return
  fi
  useradd --system --home /var/lib/knots --shell /usr/sbin/nologin "$SERVICE_USER"
}

ensure_token() {
  if [[ -r "$TOKEN_FILE" ]]; then
    return
  fi
  if [[ -z "${KNO_MCP_TOKEN:-}" ]]; then
    printf '%s does not exist; set KNO_MCP_TOKEN to create it\n' "$TOKEN_FILE" >&2
    exit 1
  fi
  umask 077
  printf '%s\n' "$KNO_MCP_TOKEN" > "$TOKEN_FILE"
  chown root:"$SERVICE_USER" "$TOKEN_FILE"
  chmod 0640 "$TOKEN_FILE"
}

install_unit() {
  unit_text > "$SERVICE_FILE"
  chmod 0644 "$SERVICE_FILE"
  systemctl daemon-reload
  systemctl enable --now "$SERVICE_NAME"
  systemctl --no-pager --full status "$SERVICE_NAME"
}

require_root
validate_config

if [[ "$DRY_RUN" == "1" ]]; then
  unit_text
  exit 0
fi

ensure_user
ensure_paths
ensure_token
install_unit
