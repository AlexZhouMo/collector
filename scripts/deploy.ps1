# Rebuild, deploy, clean up, and start Collector on Windows.
#
#   powershell -ExecutionPolicy Bypass -File scripts\deploy.ps1            # build + clean + start
#   powershell -ExecutionPolicy Bypass -File scripts\deploy.ps1 -CleanOnly # cleanup only
#   powershell -ExecutionPolicy Bypass -File scripts\deploy.ps1 -DryRun    # list what cleanup would delete
#   powershell -ExecutionPolicy Bypass -File scripts\deploy.ps1 -KeepCache # keep cargo cache (faster next build)
#   powershell -ExecutionPolicy Bypass -File scripts\deploy.ps1 -NoStart   # do not launch the app
#
# The app is run straight from src-tauri\target\release\collector.exe, so "deploy" means
# replacing that exe. After a successful build everything in the build tree that the running
# app does not need is deleted (about 2 GB of cargo intermediates). The price is that the next
# build is a full ~6 minute compile; pass -KeepCache to keep the cache.
#
# CLEANUP IS A FIXED WHITELIST. It deletes only the paths named in $Junk below, and only if they
# resolve to <root>\dist or somewhere under <root>\src-tauri\target. Do NOT add wildcard or
# recursive scans of the project here: an earlier version did (Get-ChildItem -Include is silently
# ignored with -LiteralPath on PowerShell 5.1) and wiped the whole working tree.
#
# Kept ASCII-only on purpose: Windows PowerShell 5.1 reads BOM-less scripts as ANSI.
param(
    [switch]$CleanOnly,
    [switch]$DryRun,
    [switch]$KeepCache,
    [switch]$NoStart
)

$ErrorActionPreference = "Stop"
$Root    = Split-Path -Parent $PSScriptRoot
$Dist    = Join-Path $Root "dist"
$Target  = Join-Path $Root "src-tauri\target"
$Release = Join-Path $Target "release"
$Exe     = Join-Path $Release "collector.exe"

if (-not (Test-Path -LiteralPath (Join-Path $Root "src-tauri\tauri.conf.json"))) {
    throw "not a Collector checkout: $Root"
}

# Paths that are safe to delete after a build. Nothing else is ever touched.
$Junk = @(
    $Dist,                                   # frontend bundle, already embedded in the exe
    (Join-Path $Target  "debug"),            # cargo check / tauri dev output
    (Join-Path $Release "bundle"),           # installer bundles
    (Join-Path $Release "incremental"),
    (Join-Path $Release "examples"),
    (Join-Path $Release "collector.d"),
    (Join-Path $Release "collector.pdb")
)
if (-not $KeepCache) {
    # Cargo intermediates. .fingerprint must go together with deps/build.
    $Junk += (Join-Path $Release "deps"), (Join-Path $Release "build"), (Join-Path $Release ".fingerprint")
}

function Get-SizeBytes([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 0 }
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer) { return $item.Length }
    $sum = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
    if ($null -eq $sum) { return 0 }
    return $sum
}

function Test-Deletable([string]$Full) {
    if ($Full -eq $Dist) { return $true }
    return $Full.StartsWith($Target + "\", [System.StringComparison]::OrdinalIgnoreCase)
}

function Invoke-Cleanup {
    if ($DryRun) { "Dry run - nothing will be deleted:" } else { "Cleaning build leftovers..." }
    $freed = 0
    foreach ($p in $Junk) {
        if (-not (Test-Path -LiteralPath $p)) { continue }
        $full = (Resolve-Path -LiteralPath $p).Path
        if (-not (Test-Deletable $full)) { throw "refusing to delete outside build output: $full" }
        $size = Get-SizeBytes $full
        if (-not $DryRun) { Remove-Item -LiteralPath $full -Recurse -Force -Confirm:$false }
        $freed += $size
        "  {0,9:N1} MB  {1}" -f ($size / 1MB), $full.Substring($Root.Length + 1)
    }
    if ($DryRun) { "Would free {0:N1} MB." -f ($freed / 1MB) }
    else { "Freed {0:N1} MB. Build tree is now {1:N1} MB." -f ($freed / 1MB), ((Get-SizeBytes $Target) / 1MB) }
}

function Import-MsvcEnv {
    if (Get-Command link.exe -ErrorAction SilentlyContinue) { return }
    $bat = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "vcvarsarm64.bat" } else { "vcvars64.bat" }
    $found = Get-ChildItem -Path "${env:ProgramFiles(x86)}\Microsoft Visual Studio", "$env:ProgramFiles\Microsoft Visual Studio" `
        -Filter $bat -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $found) { throw "MSVC build tools not found ($bat). Install Visual Studio Build Tools with the C++ workload." }
    cmd /c "`"$($found.FullName)`" >nul && set" | ForEach-Object {
        $i = $_.IndexOf("=")
        if ($i -gt 0) { Set-Item -Path ("env:" + $_.Substring(0, $i)) -Value $_.Substring($i + 1) }
    }
}

function Stop-App {
    Get-Process collector -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Exe } |
        Stop-Process -Force -Confirm:$false
    Start-Sleep -Seconds 1
}

$build = -not ($CleanOnly -or $DryRun)

if ($build) {
    Import-MsvcEnv
    Stop-App
    Push-Location $Root
    try {
        & node scripts/fetch-ffmpeg.mjs      # no-op when the sidecars are already present
        if ($LASTEXITCODE -ne 0) { throw "fetch-ffmpeg failed (exit $LASTEXITCODE)" }
        & npx tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "build failed (exit $LASTEXITCODE); nothing was cleaned" }
    } finally {
        Pop-Location
    }
}

Invoke-Cleanup

if ($build -and -not $NoStart) {
    Start-Process -FilePath $Exe -WorkingDirectory $Release
    "Started $Exe"
}
