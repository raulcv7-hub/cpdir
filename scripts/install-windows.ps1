<#
.SYNOPSIS
Installs cpdir.exe for the current Windows user without touching the Registry by default.
.DESCRIPTION
Installs cpdir into %LOCALAPPDATA%\Programs\cpdir (or %USERPROFILE%\.cargo\bin when -UseCargoInstall
is passed) and ensures the directory is on the User PATH. When executed from a cloned repository,
it compiles the locked release binary. When executed standalone (e.g. via irm | iex), it downloads
the latest verified release asset from GitHub. Explorer context-menu registration remains strictly
opt-in via -WithContextMenu.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\cpdir"),
    [switch]$UseCargoInstall,
    [switch]$WithContextMenu,
    [switch]$FromRelease,
    [switch]$NoPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoOwner = "raulcv7-hub"
$repoName = "cpdir"

$hasLocalRepo = $false
if ($PSScriptRoot) {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    if (Test-Path -LiteralPath (Join-Path $repoRoot "Cargo.toml")) {
        $hasLocalRepo = $true
    }
}

if ($hasLocalRepo -and -not $FromRelease) {
    $null = Get-Command cargo -ErrorAction Stop

    Push-Location $repoRoot
    try {
        if ($UseCargoInstall) {
            Write-Host "Installing cpdir via cargo install --locked..."
            & cargo install --path . --locked --force
            if ($LASTEXITCODE -ne 0) {
                throw "cargo install failed with exit code $LASTEXITCODE"
            }
            $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
            $installedExe = Join-Path (Join-Path $cargoHome "bin") "cpdir.exe"
        }
        else {
            Write-Host "Building cpdir release binary (--locked)..."
            & cargo build --release --locked
            if ($LASTEXITCODE -ne 0) {
                throw "cargo build failed with exit code $LASTEXITCODE"
            }

            $builtExe = Join-Path $repoRoot "target\release\cpdir.exe"
            if (-not (Test-Path -LiteralPath $builtExe)) {
                throw "Release binary not found at $builtExe"
            }

            $null = New-Item -ItemType Directory -Path $InstallDir -Force
            $installedExe = Join-Path $InstallDir "cpdir.exe"
            Copy-Item -LiteralPath $builtExe -Destination $installedExe -Force
        }
    }
    finally {
        Pop-Location
    }
}
else {
    Write-Host "Fetching latest release from https://github.com/$repoOwner/$repoName..."
    $zipUrl = "https://github.com/$repoOwner/$repoName/releases/latest/download/cpdir-windows-x86_64.zip"
    $shaUrl = "https://github.com/$repoOwner/$repoName/releases/latest/download/cpdir-windows-x86_64.zip.sha256"

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("cpdir_install_" + [System.Guid]::NewGuid().ToString("N"))
    $null = New-Item -ItemType Directory -Path $tempDir -Force
    try {
        $zipFile = Join-Path $tempDir "cpdir-windows-x86_64.zip"
        $shaFile = Join-Path $tempDir "cpdir-windows-x86_64.zip.sha256"

        Invoke-WebRequest -Uri $zipUrl -OutFile $zipFile -UseBasicParsing
        Invoke-WebRequest -Uri $shaUrl -OutFile $shaFile -UseBasicParsing

        $expectedHash = ((Get-Content -LiteralPath $shaFile -Raw).Trim() -split "\s+")[0].ToLowerInvariant()
        $actualHash = (Get-FileHash -LiteralPath $zipFile -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expectedHash -ne $actualHash) {
            throw "Checksum verification failed! Expected: $expectedHash, got: $actualHash"
        }

        Expand-Archive -LiteralPath $zipFile -DestinationPath $tempDir -Force
        $extractedExe = Join-Path $tempDir "cpdir.exe"
        if (-not (Test-Path -LiteralPath $extractedExe)) {
            throw "Extracted archive did not contain cpdir.exe"
        }

        $null = New-Item -ItemType Directory -Path $InstallDir -Force
        $installedExe = Join-Path $InstallDir "cpdir.exe"
        Copy-Item -LiteralPath $extractedExe -Destination $installedExe -Force
    }
    finally {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not $NoPath -and -not $UseCargoInstall) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @()
    if (-not [string]::IsNullOrWhiteSpace($userPath)) {
        $entries = $userPath -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    }

    $normalizedTarget = $InstallDir.TrimEnd("\")
    $alreadyPresent = $entries | Where-Object { $_.TrimEnd("\") -ieq $normalizedTarget }
    if (-not $alreadyPresent) {
        $updatedPath = ($entries + $InstallDir) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $updatedPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "Added $InstallDir to User PATH. Open a new terminal to invoke 'cpdir'."
    }
}

Write-Host "Installed cpdir binary at: $installedExe"

if ($WithContextMenu) {
    Write-Host "Registering optional HKCU Windows Explorer context menu..."
    & $installedExe install --yes
    if ($LASTEXITCODE -ne 0) {
        throw "cpdir install --yes failed with exit code $LASTEXITCODE"
    }
}
else {
    Write-Host "Registry untouched. Run 'cpdir' from any folder in a terminal (or 'cpdir install' if you later want Explorer integration)."
}
