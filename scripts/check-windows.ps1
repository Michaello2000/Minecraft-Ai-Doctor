# Minecraft Ai Doctor - read-only Windows environment preflight.
# No network calls, installations, writes, or reading Minecraft logs.
# Example: powershell -NoProfile -File .\scripts\check-windows.ps1

$ErrorActionPreference = 'Continue'
Write-Host "=== Minecraft Ai Doctor / Windows setup check ===" -ForegroundColor Cyan
Write-Host "Time: $(Get-Date -Format o)"
Write-Host "OS: $([Environment]::OSVersion.VersionString)"
Write-Host "Architecture: $env:PROCESSOR_ARCHITECTURE"
Write-Host "User context: local read-only check (no file contents opened)"
Write-Host ''

function Show-Tool {
    param([string]$Name, [string[]]$VersionArgs = @('--version'))
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $command) {
        Write-Host "[MISSING] $Name" -ForegroundColor Yellow
        return
    }
    $location = if ($command.Source) { $command.Source } else { $command.Definition }
    Write-Host "[FOUND]   $Name : $location" -ForegroundColor Green
    if ($VersionArgs.Count -gt 0) {
        try {
            $out = & $Name @VersionArgs 2>&1 | Select-Object -First 2
            foreach ($line in $out) { Write-Host "          $line" }
        } catch {
            Write-Host "          (version unavailable: $($_.Exception.Message))"
        }
    }
}

Show-Tool 'git'
Show-Tool 'rustup'
Show-Tool 'rustc'
Show-Tool 'cargo'
Show-Tool 'codex'
Show-Tool 'winget' @('--version')
Write-Host ''

$rustup = Get-Command rustup -ErrorAction SilentlyContinue
if ($rustup) {
    try { Write-Host "Rust active toolchain: $(& rustup show active-toolchain 2>&1)" }
    catch { Write-Host "Rust toolchain check failed: $($_.Exception.Message)" }
}

# For Visual Studio installations, vswhere is more reliable than cl.exe in PATH.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) {
    try {
        $vsPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vsPath) {
            Write-Host "[FOUND]   MSVC build tools via Visual Studio: $vsPath" -ForegroundColor Green
        } else {
            Write-Host '[CHECK]   Visual Studio exists, but MSVC C++ tools were not detected.' -ForegroundColor Yellow
        }
    } catch { Write-Host "[CHECK] MSVC detection failed: $($_.Exception.Message)" }
} else {
    Write-Host '[CHECK] Visual Studio installer (vswhere) not found. MSVC may be absent or standalone.' -ForegroundColor Yellow
}
if (Get-Command cl.exe -ErrorAction SilentlyContinue) { Write-Host '[FOUND]   cl.exe on current PATH' -ForegroundColor Green }

# Windows SDK is usually installed with the MSVC C++ workload.
try {
    $kit = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
    if ($kit -and $kit.KitsRoot10 -and (Test-Path -LiteralPath $kit.KitsRoot10)) {
        $sdkInclude = Join-Path $kit.KitsRoot10 'Include'
        $versions = @(Get-ChildItem -LiteralPath $sdkInclude -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name)
        if ($versions.Count -gt 0) {
            Write-Host "[FOUND]   Windows SDK versions: $($versions -join ', ')" -ForegroundColor Green
        } else {
            Write-Host '[CHECK]   Windows Kits folder exists, but no SDK include versions found.' -ForegroundColor Yellow
        }
    } else {
        Write-Host '[CHECK]   Windows SDK registry installation not detected.' -ForegroundColor Yellow
    }
} catch { Write-Host "[CHECK] Windows SDK detection failed: $($_.Exception.Message)" }

$paths = [ordered]@{
    'Prism standard data root' = (Join-Path $env:APPDATA 'PrismLauncher')
    'Prism standard instances' = (Join-Path $env:APPDATA 'PrismLauncher\instances')
    'Lunar home' = (Join-Path $env:USERPROFILE '.lunarclient')
    'Lunar profiles' = (Join-Path $env:USERPROFILE '.lunarclient\profiles')
    'Lunar 1.21 logs' = (Join-Path $env:USERPROFILE '.lunarclient\profiles\1.21\logs')
}
Write-Host ''
Write-Host '--- Expected launcher data directories ---'
foreach ($entry in $paths.GetEnumerator()) {
    if (Test-Path -LiteralPath $entry.Value -PathType Container) {
        Write-Host "[FOUND]   $($entry.Key)" -ForegroundColor Green
    } else {
        Write-Host "[NOT FOUND] $($entry.Key)" -ForegroundColor Yellow
    }
}

Write-Host ''
Write-Host '--- Common executable locations (heuristic only) ---'
$possiblePrism = @(
    (Join-Path $env:ProgramFiles 'PrismLauncher\prismlauncher.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'PrismLauncher\prismlauncher.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\PrismLauncher\prismlauncher.exe')
)
foreach ($p in $possiblePrism) {
    if (Test-Path -LiteralPath $p -PathType Leaf) { Write-Host '[FOUND]   Prism Launcher executable (common path)' -ForegroundColor Green }
}
Write-Host ''
Write-Host 'Done. This check does not prove that every portable/custom install was found.' -ForegroundColor Cyan
Write-Host 'If you share results, review them first; tool output may contain local paths or usernames.'
