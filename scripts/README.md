# scripts

Build automation, release tooling, and git hooks.

## scripts/repo/

- **`pre-push-sanity.sh`** — runs `make sanity` before every push
- **`install-hooks.sh`** — installs the managed pre-push hook
- **`check-file-sizes.sh`** — enforces < 500 lines per .rs file
- **`check-coverage-threshold.sh`** — prevents coverage regressions
- **`require-changeset.sh`** — ensures changesets are present for releases
- **`publish-public.sh`** — publish release artifacts to the public channel
- **`reap-stale-artifacts.sh`** — prunes stale `target/` build trees
- Windows (PowerShell) counterparts used by `make` when `OS=Windows_NT`:
  `Install-Hooks.ps1`, `Pre-Push-Sanity.ps1`, `Check-FileSizes.ps1`,
  `Check-CoverageThreshold.ps1`, `Invoke-Coverage.ps1`, `Reap-StaleArtifacts.ps1`

## scripts/mcp/

- **`install-systemd-service.sh`** — installs or dry-runs the Linux
  `kno-mcp` systemd service used by the Phase 2 Manhattan deployment.
  Set `KNO_MCP_GIT_URL` to clone or fetch the dedicated service checkout.
  Set `KNO_MCP_TAILSCALE_SERVE=1` to expose the localhost service through the
  HTTPS MagicDNS endpoint used by the external verifier.
- **`verify-phase2-external.sh`** — verifies the external MCP Phase 2 gates
  from `docs/mcp-server-design.html` (Manhattan service, tailnet reachability,
  and sandbox-style claim/next convergence). The V2.6 probe defaults to
  `sandbox-probe` identity and can be named with `KNO_MCP_PROBE_CLIENT_NAME`,
  `KNO_MCP_PROBE_CLIENT_VERSION`, and `KNO_MCP_PROBE_CLIENT_PROVIDER`. Set
  `KNO_MCP_SSH_TRANSPORT=tailscale` when Manhattan requires Tailscale SSH
  host-key handling instead of plain OpenSSH. Set `KNO_MCP_SERVICE_SCOPE=user`
  for a lingering user-systemd `kno-mcp` service.

## scripts/release/

- **`check-changesets.mjs`** — validates changeset package keys before release
- **`notes-from-changelog.sh`** — extract GitHub Release notes from CHANGELOG.md
- **`sync-cargo-version.mjs`** — sync version between Cargo.toml and package.json
- **`channel-install.sh`** — install from a named release channel
- **`channel-use.sh`** — switch the active release channel
- **`smoke-install.sh`** — post-install smoke test
