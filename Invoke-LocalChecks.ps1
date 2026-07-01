param(
    [switch]$Sanity,
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
Invoke-Step 'clippy' { cargo clippy --all-targets --all-features -- -D warnings }
Invoke-Step 'file sizes' { & .\scripts\repo\Check-FileSizes.ps1 }
Invoke-Step 'tests' { cargo test --all-targets --all-features }
Invoke-Step 'release tests' { npm run test-release }

if (-not $SkipCoverage) {
    $tarpaulin = Get-Command cargo-tarpaulin -ErrorAction SilentlyContinue
    if (-not $tarpaulin) {
        Write-Error 'cargo-tarpaulin is required. Install with: cargo install cargo-tarpaulin --locked'
        exit 1
    }
    Invoke-Step 'coverage' {
        New-Item -ItemType Directory -Force -Path coverage | Out-Null
        cargo tarpaulin --engine llvm --all-features --workspace --timeout 120 --out Xml `
            --output-dir coverage --fail-under (Get-Content -Raw .ci\coverage-threshold.txt).Trim()
    }
}
