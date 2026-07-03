param(
    [string]$BaseRef = 'origin/main'
)

$ErrorActionPreference = 'Stop'
$thresholdFile = '.ci/coverage-threshold.txt'

if (-not (Test-Path -LiteralPath $thresholdFile)) {
    Write-Error "error: $thresholdFile is missing"
    exit 1
}

$currentValue = (Get-Content -Raw -LiteralPath $thresholdFile).Trim()
if ($currentValue -notmatch '^[0-9]+$') {
    Write-Error "error: $thresholdFile must contain an integer percentage"
    exit 1
}

& git cat-file -e "$BaseRef^{commit}" 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Output "Base ref '$BaseRef' not available; skipping threshold regression check."
    exit 0
}

$baseSpec = $BaseRef + ':' + $thresholdFile
$baseValue = (& git show $baseSpec 2>$null)
if ($LASTEXITCODE -ne 0) {
    Write-Output "No $thresholdFile on $BaseRef; skipping threshold regression check."
    exit 0
}

$baseValue = ($baseValue | Out-String).Trim()
if ($baseValue -notmatch '^[0-9]+$') {
    Write-Error "error: ${BaseRef}:$thresholdFile does not contain an integer percentage"
    exit 1
}

if ([int]$currentValue -lt [int]$baseValue) {
    Write-Error "error: coverage threshold regression: $currentValue < $baseValue"
    exit 1
}

Write-Output "Coverage threshold check passed: current=$currentValue, base=$baseValue"
