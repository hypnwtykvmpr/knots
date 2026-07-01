param(
    [int]$MaxAgeHours = $(if ($env:ARTIFACT_MAX_AGE_HOURS) {
        [int]$env:ARTIFACT_MAX_AGE_HOURS
    } else {
        24
    })
)

$ErrorActionPreference = 'Stop'
if ($MaxAgeHours -le 0) {
    Write-Error 'MAX_AGE_HOURS must be greater than zero'
    exit 2
}

$root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')
$target = Join-Path $root 'target'
$cutoff = (Get-Date).AddHours(-$MaxAgeHours)
$removed = $false

if (Test-Path -LiteralPath $target) {
    Get-ChildItem -LiteralPath $target -Directory | ForEach-Object {
        $newerFile = Get-ChildItem -LiteralPath $_.FullName -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.LastWriteTime -gt $cutoff } |
            Select-Object -First 1
        if ($_.LastWriteTime -le $cutoff -and -not $newerFile) {
            $relative = Resolve-Path -Relative -LiteralPath $_.FullName
            Write-Output "Reaping stale artifact tree: $relative"
            Remove-Item -LiteralPath $_.FullName -Recurse -Force
            $removed = $true
        }
    }
}

if (-not $removed) {
    Write-Output "No stale build artifacts older than ${MaxAgeHours}h."
}
