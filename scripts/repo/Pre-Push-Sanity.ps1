param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$HookArgs
)

$ErrorActionPreference = 'Stop'

$repoRoot = (& git rev-parse --show-toplevel 2>$null)
if (-not $repoRoot) {
    Write-Error 'error: pre-push sanity must run inside a git repository'
    exit 1
}

Set-Location $repoRoot
Remove-Item Env:\GIT_DIR -ErrorAction SilentlyContinue
Remove-Item Env:\GIT_QUARANTINE_PATH -ErrorAction SilentlyContinue
Remove-Item Env:\GIT_WORK_TREE -ErrorAction SilentlyContinue

Write-Output 'Running make sanity before push...'
if (Test-Path -LiteralPath '.\Invoke-LocalChecks.ps1') {
    & .\Invoke-LocalChecks.ps1 -Sanity
    exit $LASTEXITCODE
}

& make sanity
exit $LASTEXITCODE
