<#
.SYNOPSIS
Builds and installs cpdir.exe for the current Windows user without touching the Registry by default.
.DESCRIPTION
Compiles the locked release binary and installs it into %LOCALAPPDATA%\Programs\cpdir (or
%USERPROFILE%\.cargo\bin when -UseCargoInstall is passed), ensuring the target directory is on
the User PATH. Explorer context-menu registration remains strictly opt-in via -WithContextMenu.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\cpdir"),
    [switch]$UseCargoInstall,
    [switch]$WithContextMenu
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot "Cargo.toml"))) {
    throw "Could not locate Cargo.toml under $repoRoot"
}

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
}
finally {
    Pop-Location
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
