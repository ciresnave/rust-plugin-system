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
if (!(Test-Path $plugins_out)) { New-Item -ItemType Directory -Path $plugins_out | Out-Null }

$plugins = Get-ChildItem -Directory -Path "$root\plugins" | Where-Object { Test-Path (Join-Path $_.FullName 'Cargo.toml') }
foreach ($p in $plugins) {
  Write-Host "Processing plugin: $($p.Name)"
  if (-not $SkipBuild) {
    Write-Host "Building plugin: $($p.Name)"
    Push-Location $p.FullName
    cargo build
    Pop-Location
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

  if (-not $built) {
    Write-Warning "Could not find built artifact for plugin $($p.Name)"
  }
}

Write-Host "Plugins copied to: $plugins_out"