# Offline regression for the real workflow step, including the runner exit wrapper.
# All downloads, archive operations and compressor invocations are simulated.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$workflow = [IO.File]::ReadAllText((Join-Path $root '.github/workflows/rust.yml'))
$lines = $workflow -split '\r?\n'
$name = '      - name: Enable fast Windows Cargo cache compression'
$matches = @($lines | Where-Object { $_ -ceq $name })
if ($matches.Count -ne 1) { throw 'Expected exactly one optional compressor step.' }
$index = [Array]::IndexOf($lines, $name)
if ($lines[$index + 1] -cne '        run: |') { throw 'Unexpected compressor step format.' }
$bodyLines = @()
for ($i = $index + 2; $i -lt $lines.Count; $i++) {
    if ([string]::IsNullOrWhiteSpace($lines[$i])) { $bodyLines += ''; continue }
    if (-not $lines[$i].StartsWith('          ')) { break }
    $bodyLines += $lines[$i].Substring(10)
}
$body = $bodyLines -join "`n"
$hashMatch = [regex]::Match($body, '\$expected = ''([a-f0-9]{128})''')
if (-not $hashMatch.Success) { throw 'The pinned SHA-512 must remain in the workflow.' }
$engineName = if ($PSVersionTable.PSEdition -eq 'Desktop') { 'powershell.exe' } elseif ($env:OS -eq 'Windows_NT') { 'pwsh.exe' } else { 'pwsh' }
$engine = Join-Path $PSHOME $engineName
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('ssb-zstd-test-' + [guid]::NewGuid().ToString('N'))
$mock = @'
$ErrorActionPreference = 'Stop'
$global:LASTEXITCODE = 0
$script:invocations = 0
$script:downloaded = $false
$env:RUNNER_TEMP = $PSScriptRoot
$env:GITHUB_PATH = Join-Path $PSScriptRoot 'job-path.txt'
[IO.File]::WriteAllText($env:GITHUB_PATH, 'preserve')
function Get-Command {
    [CmdletBinding()]param([string]$Name)
    if ($Name -ne 'zstd.exe') { throw 'Unexpected command lookup.' }
    if ($scenario -in @('existing', 'existing-failure')) {
        return [pscustomobject]@{ Source = 'Invoke-FakeZstd' }
    }
}
function Invoke-WebRequest {
    [CmdletBinding()]param([string]$Uri, [string]$OutFile, [int]$TimeoutSec, [switch]$UseBasicParsing)
    if ($scenario -eq 'download') { throw 'Simulated unavailable download.' }
    $script:downloaded = $true
}
function Get-FileHash {
    [CmdletBinding()]param([string]$LiteralPath, [string]$Algorithm)
    if (-not $script:downloaded -or $Algorithm -ne 'SHA512') { throw 'Unexpected hash operation.' }
    $value = if ($scenario -eq 'hash') { '0' * 128 } else { $fixtureHash }
    [pscustomobject]@{ Hash = $value }
}
function Expand-Archive {
    [CmdletBinding()]param([string]$LiteralPath, [string]$DestinationPath, [switch]$Force)
    if ($scenario -eq 'hash') { throw 'Unverified archive was extracted.' }
    if ($scenario -eq 'extract') { throw 'Simulated extraction failure.' }
}
function Get-ChildItem {
    [CmdletBinding()]param([string]$LiteralPath, [string]$Filter, [switch]$File, [switch]$Recurse)
    if ($Filter -eq '*.zip') {
        if ($scenario -eq 'nested') { [pscustomobject]@{ FullName = 'nested.zip' } }
    } elseif ($Filter -eq 'zstd.exe') {
        if ($scenario -ne 'missing') { [pscustomobject]@{ FullName = 'Invoke-FakeZstd' } }
        if ($scenario -eq 'ambiguous') { [pscustomobject]@{ FullName = 'Invoke-FakeZstd' } }
    } else { throw 'Unexpected archive lookup.' }
}
function Split-Path {
    [CmdletBinding()]param([switch]$Parent, [Parameter(Position=0)][string]$Path)
    if (-not $Parent -or $Path -ne 'Invoke-FakeZstd') { throw 'Unexpected compressor directory.' }
    Join-Path $env:RUNNER_TEMP 'verified-zstd'
}
function Invoke-FakeZstd {
    if ($args.Count -ne 1 -or $args[0] -ne '--version') { throw 'Unexpected compressor arguments.' }
    $script:invocations++
    $global:LASTEXITCODE = if ($scenario -in @('native', 'existing-failure')) { 29 } else { 0 }
    Write-Output 'fixture Zstandard'
}
'@
$assertions = @'
$wantCalls = if ($scenario -in @('native', 'success', 'nested', 'existing')) { 1 } else { 0 }
if ($script:invocations -ne $wantCalls) { throw 'An unverified or ambiguous compressor was executed.' }
$expectedPath = 'preserve'
if ($scenario -in @('success', 'nested')) {
    $expectedPath += (Join-Path $env:RUNNER_TEMP 'verified-zstd') + [Environment]::NewLine
}
if ([IO.File]::ReadAllText($env:GITHUB_PATH) -cne $expectedPath) { throw 'Unexpected PATH publication.' }
# This is the actual actions/runner PowerShell epilogue, not a success override.
if ((Test-Path -LiteralPath variable:\LASTEXITCODE)) { exit $LASTEXITCODE }
'@
try {
    New-Item -ItemType Directory -Path $fixture | Out-Null
    foreach ($scenario in @('download', 'hash', 'extract', 'missing', 'ambiguous', 'native', 'success', 'nested', 'existing', 'existing-failure')) {
        $directory = Join-Path $fixture $scenario
        New-Item -ItemType Directory -Path $directory | Out-Null
        $child = Join-Path $directory 'check.ps1'
        $prefix = '$scenario = ''' + $scenario + "'`n" + '$fixtureHash = ''' + $hashMatch.Groups[1].Value + "'`n"
        [IO.File]::WriteAllText($child, $prefix + $mock + "`n" + $body + "`n" + $assertions, [Text.UTF8Encoding]::new($true))
        $out = Join-Path $directory 'stdout.txt'
        $err = Join-Path $directory 'stderr.txt'
        $process = Start-Process -FilePath $engine -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $child + '"')) -PassThru -NoNewWindow -RedirectStandardOutput $out -RedirectStandardError $err
        try {
            if (-not $process.WaitForExit(15000)) { $process.Kill(); throw "Fixture timed out: $scenario" }
            $code = $process.ExitCode
        } finally { $process.Dispose() }
        $stdout = [IO.File]::ReadAllText($out)
        $stderr = [IO.File]::ReadAllText($err)
        if ($scenario -eq 'existing-failure') {
            if ($code -eq 0 -or $stderr -notmatch 'Existing zstd.exe failed its version check') {
                throw 'An existing broken compressor must remain a blocking failure.'
            }
        } elseif ($code -ne 0) {
            throw "Compressor fixture failed ($scenario, exit $code): $stdout $stderr"
        }
        if ($scenario -in @('download', 'hash', 'extract', 'missing', 'ambiguous', 'native') -and $stdout -notmatch 'Zstandard unavailable; keep gzip') {
            throw "Expected explicit fallback warning: $scenario"
        }
    }
    Write-Output 'PASS: 10 offline compressor fallback scenarios.'
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
