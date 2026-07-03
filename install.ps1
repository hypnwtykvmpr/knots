param(
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

$DefaultRepo = 'hypnwtykvmpr/knots'
$Repo = if ($env:KNOTS_GITHUB_REPO) { $env:KNOTS_GITHUB_REPO } else { $DefaultRepo }
$InstallDir = if ($env:KNOTS_INSTALL_DIR) {
    $env:KNOTS_INSTALL_DIR
} elseif ($env:LOCALAPPDATA) {
    Join-Path $env:LOCALAPPDATA 'Programs\knots'
} else {
    Join-Path $env:USERPROFILE '.local\bin'
}
$DownloadBase = if ($env:KNOTS_RELEASE_DOWNLOAD_BASE) { $env:KNOTS_RELEASE_DOWNLOAD_BASE } else { 'https://github.com' }
$ApiBase = if ($env:KNOTS_GITHUB_API_BASE) { $env:KNOTS_GITHUB_API_BASE } else { 'https://api.github.com' }
$RequestedVersion = if ($env:KNOTS_VERSION) { $env:KNOTS_VERSION } else { '' }
$ParentPid = 0
if ($env:KNOTS_PARENT_PID -and [int]::TryParse($env:KNOTS_PARENT_PID, [ref]$ParentPid)) {
    $ParentPid = [int]$env:KNOTS_PARENT_PID
}

function Show-Usage {
    @'
kno installer

Environment variables:
  KNOTS_GITHUB_REPO              owner/repo source (default: hypnwtykvmpr/knots)
  KNOTS_VERSION                  release tag (example: v0.1.0). default: latest
  KNOTS_INSTALL_DIR              target dir (default: %LOCALAPPDATA%\Programs\knots)
  KNOTS_RELEASE_DOWNLOAD_BASE    override download base for release assets
  KNOTS_GITHUB_API_BASE          override API base for latest release lookup
  KNOTS_PARENT_PID               kno process id to wait on before swapping a locked binary
'@
}

function Resolve-Version {
    if ($RequestedVersion) {
        $tag = $RequestedVersion
    } else {
        $apiUrl = ($ApiBase.TrimEnd('/')) + "/repos/$Repo/releases/latest"
        $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ 'User-Agent' = 'kno-installer' }
        $tag = [string]$release.tag_name
        if (-not $tag) {
            throw "failed to resolve latest release tag from $apiUrl"
        }
    }
    if ($tag.StartsWith('v')) { $tag } else { "v$tag" }
}

function Resolve-TargetSuffix {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
    switch ($arch) {
        'x64' { 'windows-x86_64' }
        'x86' { throw 'unsupported Windows architecture x86' }
        'arm64' { 'windows-x86_64' }
        default { throw "unsupported Windows architecture $arch" }
    }
}

function Read-Checksum {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$AssetName
    )
    foreach ($line in Get-Content -LiteralPath $Path) {
        $parts = $line -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1] -eq $AssetName) {
            return $parts[0].ToLowerInvariant()
        }
    }
    throw "checksum entry for $AssetName was not found"
}

function Verify-Checksum {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected
    )
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) {
        throw "checksum verification failed for $Path"
    }
}

function Install-KnoBinary {
    param(
        [Parameter(Mandatory = $true)][string]$ExtractedBinary,
        [Parameter(Mandatory = $true)][string]$DestinationDir
    )
    New-Item -ItemType Directory -Force -Path $DestinationDir | Out-Null

    $destination = Join-Path $DestinationDir 'knots.exe'
    $alias = Join-Path $DestinationDir 'kno.exe'
    $staging = Join-Path $DestinationDir 'knots.exe.new'

    if (Test-Path -LiteralPath $destination) {
        Copy-Item -LiteralPath $destination -Destination (Join-Path $DestinationDir 'knots.previous.exe') -Force
    }
    if (Test-Path -LiteralPath $alias) {
        Copy-Item -LiteralPath $alias -Destination (Join-Path $DestinationDir 'kno.previous.exe') -Force
    }

    Copy-Item -LiteralPath $ExtractedBinary -Destination $staging -Force
    Move-Item -LiteralPath $staging -Destination $destination -Force

    if (Test-Path -LiteralPath $alias) {
        Remove-Item -LiteralPath $alias -Force
    }
    try {
        New-Item -ItemType HardLink -Path $alias -Target $destination | Out-Null
    } catch {
        Copy-Item -LiteralPath $destination -Destination $alias -Force
    }
}

function Schedule-InstallAfterParentExit {
    param(
        [Parameter(Mandatory = $true)][string]$ExtractedBinary,
        [Parameter(Mandatory = $true)][string]$DestinationDir,
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [Parameter(Mandatory = $true)][string]$TempDir
    )
    $helper = Join-Path $TempDir 'complete-kno-install.ps1'
    @'
param(
    [Parameter(Mandatory = $true)][string]$ExtractedBinary,
    [Parameter(Mandatory = $true)][string]$DestinationDir,
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [string]$CleanupDir = ''
)

$ErrorActionPreference = 'Stop'
if ($ProcessId -gt 0) {
    Wait-Process -Id $ProcessId -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

New-Item -ItemType Directory -Force -Path $DestinationDir | Out-Null
$destination = Join-Path $DestinationDir 'knots.exe'
$alias = Join-Path $DestinationDir 'kno.exe'
$staging = Join-Path $DestinationDir 'knots.exe.new'

if (Test-Path -LiteralPath $destination) {
    Copy-Item -LiteralPath $destination -Destination (Join-Path $DestinationDir 'knots.previous.exe') -Force
}
if (Test-Path -LiteralPath $alias) {
    Copy-Item -LiteralPath $alias -Destination (Join-Path $DestinationDir 'kno.previous.exe') -Force
}

Copy-Item -LiteralPath $ExtractedBinary -Destination $staging -Force
Move-Item -LiteralPath $staging -Destination $destination -Force
if (Test-Path -LiteralPath $alias) {
    Remove-Item -LiteralPath $alias -Force
}
try {
    New-Item -ItemType HardLink -Path $alias -Target $destination | Out-Null
} catch {
    Copy-Item -LiteralPath $destination -Destination $alias -Force
}

if ($CleanupDir -and (Test-Path -LiteralPath $CleanupDir)) {
    # The helper script itself lives in $CleanupDir; PowerShell has fully
    # parsed this file by now, so the directory can be removed safely.
    Set-Location -LiteralPath ([IO.Path]::GetTempPath())
    Remove-Item -LiteralPath $CleanupDir -Recurse -Force -ErrorAction SilentlyContinue
}
'@ | Set-Content -LiteralPath $helper -Encoding UTF8

    # Start-Process joins -ArgumentList entries with spaces into one command
    # line; every path must carry embedded quotes or spaced paths shatter
    # into separate tokens and the helper's parameter binding fails.
    $powershellExe = (Get-Process -Id $PID).Path
    Start-Process -FilePath $powershellExe -WindowStyle Hidden -ArgumentList @(
        '-NoProfile',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        ('"{0}"' -f $helper),
        '-ExtractedBinary',
        ('"{0}"' -f $ExtractedBinary),
        '-DestinationDir',
        ('"{0}"' -f $DestinationDir),
        '-ProcessId',
        $ProcessId,
        '-CleanupDir',
        ('"{0}"' -f $TempDir)
    ) | Out-Null
}

function Ensure-UserPath {
    param([Parameter(Mandatory = $true)][string]$Directory)

    # Read and write the raw registry value so REG_EXPAND_SZ entries like
    # %USERPROFILE%\bin are preserved unexpanded instead of being flattened.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $current = ''
        $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        if ($key -and ($null -ne $key.GetValue('Path'))) {
            $current = $key.GetValue(
                'Path', '',
                [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $kind = $key.GetValueKind('Path')
        }
        $parts = @()
        if ($current) {
            $parts = $current -split ';' | Where-Object { $_ }
        }
        if ($parts -notcontains $Directory) {
            $next = (($parts + $Directory) -join ';')
            $key.SetValue('Path', $next, $kind)
            $env:Path = $env:Path + ';' + $Directory
            return $true
        }
        return $false
    } finally {
        if ($key) { $key.Dispose() }
    }
}

if ($Help) {
    Show-Usage
    exit 0
}

$resolvedTag = Resolve-Version
$targetSuffix = Resolve-TargetSuffix
$assetFile = "knots-$resolvedTag-$targetSuffix.zip"
$checksumsFile = "knots-$resolvedTag-checksums.txt"
$assetUrl = ($DownloadBase.TrimEnd('/')) + "/$Repo/releases/download/$resolvedTag/$assetFile"
$checksumsUrl = ($DownloadBase.TrimEnd('/')) + "/$Repo/releases/download/$resolvedTag/$checksumsFile"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('knots-install-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    $assetPath = Join-Path $tmp $assetFile
    $checksumsPath = Join-Path $tmp $checksumsFile
    Invoke-WebRequest -UseBasicParsing -Uri $assetUrl -OutFile $assetPath
    Invoke-WebRequest -UseBasicParsing -Uri $checksumsUrl -OutFile $checksumsPath
    $expected = Read-Checksum -Path $checksumsPath -AssetName $assetFile
    Verify-Checksum -Path $assetPath -Expected $expected

    $extractDir = Join-Path $tmp 'extract'
    Expand-Archive -LiteralPath $assetPath -DestinationPath $extractDir -Force
    $extracted = Get-ChildItem -LiteralPath $extractDir -Filter 'knots.exe' -Recurse |
        Select-Object -First 1
    if (-not $extracted) {
        throw "expected knots.exe in $assetFile"
    }

    $scheduled = $false
    try {
        Install-KnoBinary -ExtractedBinary $extracted.FullName -DestinationDir $InstallDir
    } catch {
        if ($ParentPid -le 0) {
            throw
        }
        Schedule-InstallAfterParentExit `
            -ExtractedBinary $extracted.FullName `
            -DestinationDir $InstallDir `
            -ProcessId $ParentPid `
            -TempDir $tmp
        $scheduled = $true
    }

    $pathUpdated = Ensure-UserPath -Directory $InstallDir
    "{0,13}  {1}" -f 'kno', (Join-Path $InstallDir 'kno.exe')
    "{0,13}  {1}" -f 'compat', (Join-Path $InstallDir 'knots.exe')
    "{0,13}  {1}" -f 'version', $resolvedTag
    if ($scheduled) {
        'install scheduled after the current kno process exits'
    }
    if ($pathUpdated) {
        "$InstallDir added to the user PATH; open a new terminal if this one cannot find kno."
    }
} finally {
    if (-not $scheduled) {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}
