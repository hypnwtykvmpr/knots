param()

$ErrorActionPreference = 'Stop'

$repoRoot = (& git rev-parse --show-toplevel 2>$null)
if (-not $repoRoot) {
    Write-Error 'error: hook installer must run inside a git repository'
    exit 1
}

$gitCommonDir = (& git rev-parse --path-format=absolute --git-common-dir)
$hooksDir = Join-Path $gitCommonDir 'hooks'
$managedHook = Join-Path $hooksDir 'pre-push'
$localHook = Join-Path $hooksDir 'pre-push.local'
$marker = 'knots-managed-pre-push-hook'

New-Item -ItemType Directory -Force -Path $hooksDir | Out-Null

if ((Test-Path -LiteralPath $managedHook) -and
    ((Get-Content -Raw -LiteralPath $managedHook) -match [regex]::Escape($marker))) {
    Write-Output "Managed pre-push hook already installed at $managedHook"
    exit 0
}

if (Test-Path -LiteralPath $managedHook) {
    if (Test-Path -LiteralPath $localHook) {
        $timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        $backup = Join-Path $hooksDir "pre-push.backup.$timestamp"
        Move-Item -LiteralPath $managedHook -Destination $backup -Force
        Write-Output "Moved existing pre-push hook to $backup"
    } else {
        Move-Item -LiteralPath $managedHook -Destination $localHook -Force
        Write-Output "Moved existing pre-push hook to $localHook"
    }
}

function ConvertTo-ShSingleQuoted {
    param([Parameter(Mandatory = $true)][string]$Value)
    return "'" + ($Value.Replace("'", "'\''")) + "'"
}

$repoForHook = ConvertTo-ShSingleQuoted -Value ($repoRoot.Replace('\', '/'))
$hooksForHook = ConvertTo-ShSingleQuoted -Value ($hooksDir.Replace('\', '/'))
$hook = @"
#!/usr/bin/env sh
set -eu
# $marker
repo_root=$repoForHook
hooks_dir=$hooksForHook
local_hook="`${hooks_dir}/pre-push.local"
sanity_script="`${repo_root}/scripts/repo/Pre-Push-Sanity.ps1"

if [ -x "`${local_hook}" ]; then
  "`${local_hook}" "`$@"
fi

if command -v pwsh.exe >/dev/null 2>&1; then
  pwsh.exe -NoProfile -ExecutionPolicy Bypass -File "`${sanity_script}" "`$@"
else
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "`${sanity_script}" "`$@"
fi
"@

Set-Content -LiteralPath $managedHook -Value $hook -NoNewline
Write-Output "Installed managed pre-push hook at $managedHook"
