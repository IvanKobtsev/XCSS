#requires -Version 7.0
[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'Medium')]
param(
    [switch]$DryRun,
    [ValidateRange(30, 3600)][int]$RegistryTimeoutSeconds = 300
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $candidate = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
    if (Test-Path -LiteralPath $candidate) { $cargo = $candidate }
    else { throw 'Cargo is unavailable. Install the Rust toolchain before publishing.' }
}
$crates = @(
    'xcss-source', 'xcss-syntax', 'xcss-analysis', 'xcss-typescript',
    'xcss-language-service', 'xcss-format', 'xcss-codegen', 'xcss-core',
    'xcss-resolver', 'xcss-cli'
)

function Invoke-Cargo([string[]]$Arguments) {
    & $cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') failed ($LASTEXITCODE)." }
}

function Test-Published([string]$Name, [string]$Version) {
    try {
        $response = Invoke-RestMethod -Uri "https://crates.io/api/v1/crates/$Name/$Version" `
            -Headers @{ 'User-Agent' = 'XCSS-release-script (https://github.com/IvanKobtsev/XCSS)' }
        if ($response.version.yanked) { throw "$Name@$Version is yanked; choose a new version." }
        return $true
    } catch {
        if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { return $false }
        throw
    }
}

Push-Location $root
try {
    $metadataText = & $cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Could not read Cargo metadata.' }
    $metadata = ($metadataText -join "`n") | ConvertFrom-Json
    $packages = @{}
    foreach ($package in $metadata.packages) { $packages[$package.name] = $package }
    foreach ($name in $crates) {
        if (-not $packages.ContainsKey($name)) { throw "Missing workspace crate: $name" }
        if ($null -ne $packages[$name].publish -and $packages[$name].publish.Count -eq 0) {
            throw "$name has publishing disabled."
        }
    }
    foreach ($name in $crates) {
        $version = $packages[$name].version
        if ($WhatIfPreference) {
            $null = $PSCmdlet.ShouldProcess("$name@$version", 'Dry-run, publish, and wait for registry availability')
            continue
        }
        if (Test-Published $name $version) {
            Write-Host "Skipping $name@$version (already published)."
            continue
        }
        Write-Host "Validating $name@$version..."
        Invoke-Cargo @('publish', '--dry-run', '--locked', '-p', $name)
        if ($DryRun) { continue }
        if (-not $PSCmdlet.ShouldProcess("$name@$version on crates.io", 'Publish')) { continue }
        Invoke-Cargo @('publish', '--locked', '-p', $name)
        $deadline = [DateTime]::UtcNow.AddSeconds($RegistryTimeoutSeconds)
        while (-not (Test-Published $name $version)) {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw "Timed out waiting for $name@$version. Rerun the script to resume."
            }
            Start-Sleep -Seconds 5
        }
        Write-Host "Published $name@$version."
    }
    Write-Host 'Finished.'
} finally {
    Pop-Location
}
