<#
PowerShell helper to build all plugin crates into ./plugins_out
Usage: .\scripts\build_plugins.ps1
#>

param(
  [string]$buildProfile = 'debug',
  [switch]$SkipBuild
)

# Determine workspace root (parent of the scripts/ directory)
$script_dir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$root = Split-Path -Parent $script_dir
Set-Location $root

$plugins_out = Join-Path $root 'plugin-host' 'plugins_out'
# Clean plugins_out to avoid stale artifacts from previous runs
if (Test-Path $plugins_out) {
  Write-Host "Cleaning existing plugins_out: $plugins_out"
  Get-ChildItem -Path $plugins_out -File | Remove-Item -Force
}
else {
  New-Item -ItemType Directory -Path $plugins_out | Out-Null
}

$plugins = Get-ChildItem -Directory -Path "$root\plugins" | Where-Object { Test-Path (Join-Path $_.FullName 'Cargo.toml') }
foreach ($p in $plugins) {
  Write-Host "Processing plugin: $($p.Name)"
    if (-not $SkipBuild) {
    Write-Host "Building plugin: $($p.Name) (target-dir: $($p.FullName)\target)"
    # Use --manifest-path and --target-dir to ensure artifacts are placed under the plugin's own target dir
    cargo build --manifest-path (Join-Path $p.FullName 'Cargo.toml') --target-dir (Join-Path $p.FullName 'target')
  }

  # Try common artifact names
  $artifact_candidates = @()
  if ($IsWindows) {
    $artifact_candidates += "$($p.Name).dll"
    $artifact_candidates += "$($p.Name -replace '-', '_').dll"
  }
  elseif ($IsMacOS) {
    $artifact_candidates += "lib$($p.Name).dylib"
    $artifact_candidates += "lib$($p.Name -replace '-', '_').dylib"
  }
  else {
    $artifact_candidates += "lib$($p.Name).so"
    $artifact_candidates += "lib$($p.Name -replace '-', '_').so"
  }

  $built = $false
  foreach ($candidate in $artifact_candidates) {
    $src = Join-Path $p.FullName "target\$buildProfile\$candidate"
    if (Test-Path $src) {
      Copy-Item -Path $src -Destination $plugins_out -Force
      Write-Host "Copied $candidate to plugins_out"
      $built = $true
      break
    }
  }

  # On macOS create a .so shim if only .dylib exists and tests expect .so
  if (-not $IsWindows -and $IsMacOS) {
    $dylib = Join-Path $p.FullName "target\$buildProfile\lib$($p.Name).dylib"
    $dylib_alt = Join-Path $p.FullName "target\$buildProfile\lib$($p.Name -replace '-', '_').dylib"
    $so = Join-Path $p.FullName "target\$buildProfile\lib$($p.Name).so"
    $so_alt = Join-Path $p.FullName "target\$buildProfile\lib$($p.Name -replace '-', '_').so"
    if ((Test-Path $dylib) -and -not (Test-Path $so)) {
      Copy-Item -Path $dylib -Destination $so -Force
      Write-Host "Created .so shim from .dylib: $so"
      if (Test-Path $so) { Copy-Item -Path $so -Destination $plugins_out -Force; Write-Host "Copied lib*.so to plugins_out" }
    }
    elseif ((Test-Path $dylib_alt) -and -not (Test-Path $so_alt)) {
      Copy-Item -Path $dylib_alt -Destination $so_alt -Force
      Write-Host "Created .so shim from .dylib: $so_alt"
      if (Test-Path $so_alt) { Copy-Item -Path $so_alt -Destination $plugins_out -Force; Write-Host "Copied lib*_alt.so to plugins_out" }
    }
  }

  if (-not $built) {
    Write-Warning "Could not find built artifact for plugin $($p.Name)"
  }
}

Write-Host "Plugins copied to: $plugins_out"