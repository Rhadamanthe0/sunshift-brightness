# Offline regression tests: real ZIP extraction and hashes, mocked download only.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'prepare_github_graphics.ps1')
Add-Type -AssemblyName System.IO.Compression.FileSystem
$root = Join-Path ([IO.Path]::GetTempPath()) ('ssb-graphics-test-' + [guid]::NewGuid().ToString('N'))
$old = @{}
foreach ($key in @('GITHUB_ACTIONS', 'RUNNER_ENVIRONMENT', 'RUNNER_OS', 'RUNNER_TEMP')) {
    $old[$key] = [Environment]::GetEnvironmentVariable($key)
}
$script:downloads = @{}
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, [string]$Uri, [string]$OutFile, [int]$TimeoutSec)
    if (-not $script:downloads.ContainsKey($Uri)) { throw 'Unexpected network request.' }
    Copy-Item -LiteralPath $script:downloads[$Uri] -Destination $OutFile
}
function Assert-Rejected {
    param([scriptblock] $Action, [string] $Pattern)
    $caught = $false
    try { & $Action } catch {
        if ($_.Exception.Message -notmatch $Pattern) { throw }
        $caught = $true
    }
    if (-not $caught) { throw "Expected rejection matching $Pattern" }
}
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $env:GITHUB_ACTIONS = 'true'
    $env:RUNNER_ENVIRONMENT = 'github-hosted'
    $env:RUNNER_OS = 'Windows'
    $env:RUNNER_TEMP = $root
    $manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'github-graphics.json') -Raw | ConvertFrom-Json
    foreach ($package in $manifest.packages) {
        $folder = Join-Path $root $package.directory
        $chrome = Join-Path $folder 'chrome-win64'
        New-Item -ItemType Directory -Path $chrome -Force | Out-Null
        foreach ($file in $package.files) {
            $path = Join-Path $chrome $file.name
            [IO.File]::WriteAllText($path, ('fixture ' + $package.directory + '/' + $file.name))
            $file.sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        }
        $archive = Join-Path $root ($package.directory + '.zip')
        [IO.Compression.ZipFile]::CreateFromDirectory($folder, $archive)
        $package.sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
        $script:downloads[$package.url] = $archive
    }
    $manifestPath = Join-Path $root 'manifest.json'
    $json = $manifest | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($manifestPath, $json)
    $destination = Join-Path $root 'installed'
    Install-GitHubGraphics -ManifestPath $manifestPath -DestinationRoot $destination
    if (@(Get-ChildItem -LiteralPath $destination -File -Recurse).Count -ne 6) { throw 'Incomplete fixture.' }
    foreach ($package in $manifest.packages) {
        foreach ($file in $package.files) {
            $path = Join-Path (Join-Path $destination $package.directory) $file.name
            if ((Get-FileHash -LiteralPath $path).Hash -ne $file.sha256) { throw 'Published file differs.' }
        }
    }
    Assert-Rejected { Install-GitHubGraphics -ManifestPath $manifestPath -DestinationRoot $destination } 'existing graphics'
    $env:RUNNER_ENVIRONMENT = 'self-hosted'
    Assert-Rejected { Install-GitHubGraphics -ManifestPath $manifestPath -DestinationRoot (Join-Path $root 'forbidden') } 'GitHub-hosted'
    $env:RUNNER_ENVIRONMENT = 'github-hosted'
    foreach ($mode in @('archive', 'file', 'missing', 'path')) {
        $case = $json | ConvertFrom-Json
        $pattern = 'SHA-256 mismatch'
        if ($mode -eq 'archive') { $case.packages[1].sha256 = '0' * 64 }
        if ($mode -eq 'file') { $case.packages[1].files[0].sha256 = '0' * 64 }
        if ($mode -eq 'path') {
            $case.packages[1].files[0].name = '../escape.dll'
            $pattern = 'Invalid graphics file list'
        }
        if ($mode -eq 'missing') {
            $folder = Join-Path $root 'incomplete'
            New-Item -ItemType Directory -Path $folder | Out-Null
            [IO.File]::WriteAllText((Join-Path $folder 'unrelated.txt'), 'not the required DLLs')
            $archive = Join-Path $root 'incomplete.zip'
            [IO.Compression.ZipFile]::CreateFromDirectory($folder, $archive)
            $case.packages[1].sha256 = (Get-FileHash -LiteralPath $archive).Hash
            $script:downloads[$case.packages[1].url] = $archive
            $pattern = 'Missing or duplicate graphics entry'
        }
        [IO.File]::WriteAllText($manifestPath, ($case | ConvertTo-Json -Depth 8))
        $target = Join-Path $root ('rejected-' + $mode)
        Assert-Rejected { Install-GitHubGraphics -ManifestPath $manifestPath -DestinationRoot $target } $pattern
        if (Test-Path -LiteralPath $target) { throw 'Failed verification published a fixture.' }
        if (@(Get-ChildItem -LiteralPath $root -Directory -Filter 'ssb-graphics-*').Count) { throw 'Temporary files leaked.' }
    }
    Write-Output 'PASS: complete fixture, existing destination, runner guard, archive hash, file hash, missing entry, path rejection.'
} finally {
    foreach ($key in $old.Keys) { [Environment]::SetEnvironmentVariable($key, $old[$key]) }
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
