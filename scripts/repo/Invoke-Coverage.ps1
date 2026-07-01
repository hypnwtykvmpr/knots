param(
    [string]$TargetDir = 'target\sanity-coverage',
    [string]$OutputDir = 'coverage',
    [string]$ThresholdFile = '.ci\coverage-threshold.txt'
)

$ErrorActionPreference = 'Stop'

function Invoke-Checked {
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

function Get-RustHostTriple {
    $verbose = & rustc -vV
    if ($LASTEXITCODE -ne 0) {
        Write-Error 'rustc -vV failed while resolving LLVM tools'
        exit $LASTEXITCODE
    }
    $hostLine = $verbose | Where-Object { $_ -like 'host:*' } | Select-Object -First 1
    if (-not $hostLine) {
        Write-Error 'rustc -vV did not report a host triple'
        exit 1
    }
    return $hostLine.Split(':', 2)[1].Trim()
}

function Resolve-LlvmTool {
    param([string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $sysroot = (& rustc --print sysroot).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $sysroot) {
        Write-Error "rustc --print sysroot failed while resolving $Name"
        exit 1
    }

    $host = Get-RustHostTriple
    $candidate = Join-Path $sysroot "lib\rustlib\$host\bin\$Name"
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }

    Write-Error "$Name was not found on PATH or in the active Rust toolchain"
    exit 1
}

function Read-CoverageThreshold {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        Write-Error "coverage threshold file is missing: $Path"
        exit 1
    }

    $value = (Get-Content -Raw -LiteralPath $Path).Trim()
    if ($value -notmatch '^[0-9]+$') {
        Write-Error "$Path must contain an integer percentage"
        exit 1
    }
    return [double]::Parse($value, [System.Globalization.CultureInfo]::InvariantCulture)
}

function Get-CoverageObjects {
    param([string]$BuildDir)

    $depsDir = Join-Path $BuildDir 'debug\deps'
    if (-not (Test-Path -LiteralPath $depsDir)) {
        Write-Error "coverage build output is missing: $depsDir"
        exit 1
    }

    $objects = @(Get-ChildItem -LiteralPath $depsDir -Filter '*.exe' -File)
    $rootBins = @('knots.exe', 'kno-mcp.exe', 'kno_mcp.exe')
    foreach ($name in $rootBins) {
        $path = Join-Path (Join-Path $BuildDir 'debug') $name
        if (Test-Path -LiteralPath $path) {
            $objects += Get-Item -LiteralPath $path
        }
    }

    if ($objects.Count -eq 0) {
        Write-Error 'no executable coverage objects were produced'
        exit 1
    }
    return $objects
}

function Convert-ToObjectArgs {
    param([System.IO.FileInfo[]]$Objects)

    $args = @()
    foreach ($object in $Objects) {
        $args += @('-object', $object.FullName)
    }
    return $args
}

function Get-LineCoverage {
    param([string[]]$ReportLines)

    $total = $ReportLines | Where-Object { $_ -match '^\s*TOTAL\s+' } | Select-Object -Last 1
    if (-not $total) {
        Write-Error 'llvm-cov report did not include a TOTAL row'
        exit 1
    }

    $parts = $total.Trim() -split '\s+'
    if ($parts.Count -lt 10) {
        Write-Error "unable to parse llvm-cov TOTAL row: $total"
        exit 1
    }
    $percent = $parts[9].TrimEnd('%')
    return [double]::Parse($percent, [System.Globalization.CultureInfo]::InvariantCulture)
}

$threshold = Read-CoverageThreshold -Path $ThresholdFile
$llvmCov = Resolve-LlvmTool -Name 'llvm-cov.exe'
$llvmProfdata = Resolve-LlvmTool -Name 'llvm-profdata.exe'
$root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')
$targetRoot = Join-Path $root $TargetDir
$outputRoot = Join-Path $root $OutputDir
$profileRoot = Join-Path $targetRoot "profiles-$PID"
$profileList = Join-Path $targetRoot 'profraw-files.txt'
$profdata = Join-Path $targetRoot 'coverage.profdata'
$summaryPath = Join-Path $outputRoot 'llvm-cov-summary.txt'
$jsonPath = Join-Path $outputRoot 'llvm-cov-summary.json'

New-Item -ItemType Directory -Force -Path $targetRoot | Out-Null
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
if (Test-Path -LiteralPath $profileRoot) {
    Remove-Item -LiteralPath $profileRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $profileRoot | Out-Null

$previousIncremental = $env:CARGO_INCREMENTAL
$previousTargetDir = $env:CARGO_TARGET_DIR
$previousRustFlags = $env:RUSTFLAGS
$previousProfileFile = $env:LLVM_PROFILE_FILE

try {
    $env:CARGO_INCREMENTAL = '0'
    $env:CARGO_TARGET_DIR = $targetRoot
    $env:RUSTFLAGS = '-C instrument-coverage'
    $env:LLVM_PROFILE_FILE = Join-Path $profileRoot 'knots-%p-%m.profraw'

    Invoke-Checked 'coverage tests' {
        cargo test --all-targets --all-features -- --test-threads=1
    }
}
finally {
    $env:CARGO_INCREMENTAL = $previousIncremental
    $env:CARGO_TARGET_DIR = $previousTargetDir
    $env:RUSTFLAGS = $previousRustFlags
    $env:LLVM_PROFILE_FILE = $previousProfileFile
}

$profiles = @(Get-ChildItem -LiteralPath $profileRoot -Filter '*.profraw' -File)
if ($profiles.Count -eq 0) {
    Write-Error 'coverage tests did not produce any LLVM profile files'
    exit 1
}

$profiles | ForEach-Object { $_.FullName } | Set-Content -LiteralPath $profileList
Invoke-Checked 'merge coverage profiles' {
    & $llvmProfdata merge -sparse --input-files $profileList -o $profdata
}

$objects = Get-CoverageObjects -BuildDir $targetRoot
$objectArgs = Convert-ToObjectArgs -Objects $objects
$ignoreRegex = '[\\/](\.cargo|\.rustup)[\\/]|[\\/]tests[\\/]|' +
    '(^|[\\/])src[\\/].*tests?.*\.rs$'

$report = & $llvmCov report @objectArgs --instr-profile $profdata `
    --ignore-filename-regex $ignoreRegex --summary-only
if ($LASTEXITCODE -ne 0) {
    $report | Write-Output
    exit $LASTEXITCODE
}
$report | Tee-Object -FilePath $summaryPath

$json = & $llvmCov export @objectArgs --instr-profile $profdata `
    --ignore-filename-regex $ignoreRegex --summary-only
if ($LASTEXITCODE -ne 0) {
    $json | Write-Output
    exit $LASTEXITCODE
}
$json | Set-Content -LiteralPath $jsonPath

$lineCoverage = Get-LineCoverage -ReportLines $report
if ($lineCoverage -lt $threshold) {
    Write-Error "line coverage $lineCoverage% is below required $threshold%"
    exit 1
}

Write-Output "Coverage passed: line coverage $lineCoverage% >= $threshold%"
