#requires -Version 7.0
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?$')]
    [string]$Version
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe' }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'Cargo is required.' }
$null = Get-Command corepack -ErrorAction Stop
$null = Get-Command node -ErrorAction Stop
$utf8 = [Text.UTF8Encoding]::new($false)
$originals = @{}
$updates = @{}
Push-Location $root
try {
    $paths = @('package.json') + @(Get-ChildItem packages -Filter package.json -Recurse |
        Where-Object { $_.FullName -notmatch '[\\/](node_modules|dist|target)[\\/]' } |
        ForEach-Object { $_.FullName })
    $names = @{}
    foreach ($path in $paths) {
        $json = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
        $names[$json.name] = $true
    }
    foreach ($path in $paths) {
        $text = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path))
        $json = $text | ConvertFrom-Json -AsHashtable
        $json.version = $Version
        foreach ($section in 'dependencies','devDependencies','optionalDependencies','peerDependencies') {
            if (-not $json.ContainsKey($section)) { continue }
            foreach ($name in @($json[$section].Keys)) {
                if ($names.ContainsKey($name) -and $json[$section][$name] -notlike 'workspace:*') {
                    $json[$section][$name] = $Version
                }
            }
        }
        $updates[(Resolve-Path -LiteralPath $path).Path] = ($json | ConvertTo-Json -Depth 100) + "`n"
    }
    $manifestPaths = @('Cargo.toml') + @(Get-ChildItem crates -Filter Cargo.toml -Recurse | ForEach-Object { $_.FullName })
    foreach ($path in $manifestPaths) {
        $absolute = (Resolve-Path -LiteralPath $path).Path
        $text = [IO.File]::ReadAllText($absolute)
        if ($path -eq 'Cargo.toml') {
            $text = [regex]::Replace($text, '(?m)(\[workspace\.package\]\s*\r?\nversion\s*=\s*")[^"]+', { param($m) $m.Groups[1].Value + $Version })
        } else {
            $text = [regex]::Replace($text, '(?m)^(xcss-[\w-]+\s*=\s*\{[^\r\n]*version\s*=\s*")[^"]+', { param($m) $m.Groups[1].Value + '=' + $Version })
        }
        $updates[$absolute] = $text
    }
    if (-not $PSCmdlet.ShouldProcess($root, "Set all release packages and crates to $Version and update lockfiles")) { return }
    foreach ($path in @($updates.Keys) + @((Join-Path $root 'Cargo.lock'), (Join-Path $root 'pnpm-lock.yaml'))) {
        $originals[$path] = [IO.File]::ReadAllBytes($path)
    }
    try {
        foreach ($path in $updates.Keys) { [IO.File]::WriteAllText($path, $updates[$path], $utf8) }
        & $cargo update --workspace --offline
        if ($LASTEXITCODE) { throw 'Cargo lockfile update failed.' }
        & corepack pnpm install --lockfile-only --ignore-scripts
        if ($LASTEXITCODE) { throw 'pnpm lockfile update failed.' }
        & node npm/verify-release.mjs
        if ($LASTEXITCODE) { throw 'Release version verification failed.' }
    } catch {
        foreach ($path in $originals.Keys) { [IO.File]::WriteAllBytes($path, $originals[$path]) }
        throw
    }
    Write-Host "Release versions updated to $Version. Review and commit the changes. No tag was created."
} finally { Pop-Location }
