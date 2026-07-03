param(
    [switch]$Sanity,
    [switch]$SkipTests,
    [switch]$SkipCoverage
)

$ErrorActionPreference = 'Stop'

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Command
    )

    Write-Output "== $Name =="
    & $Command
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Invoke-Step 'fmt' { cargo fmt --all -- --check }
Invoke-Step 'changesets' { npm run check-changesets }
Invoke-Step 'check' { cargo check --all-targets --all-features }
Invoke-Step 'clippy' { cargo clippy --all-targets --all-features -- -D warnings }
Invoke-Step 'file sizes' { & .\scripts\repo\Check-FileSizes.ps1 }

if (-not $SkipTests) {
    Invoke-Step 'tests' { cargo test --all-targets --all-features }
    Invoke-Step 'release tests' { npm run test-release }
}

if (-not $SkipCoverage) {
    Invoke-Step 'coverage' {
        & .\scripts\repo\Invoke-Coverage.ps1
    }
}
