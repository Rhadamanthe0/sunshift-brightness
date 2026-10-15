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
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('ssb zstd test ' + [guid]::NewGuid().ToString('N'))
# Start-Process can lose ExitCode with redirected streams on Windows PowerShell.
# Starting the .NET Process directly retains the handle, even for an instant exit.
function Invoke-FixtureProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [ValidateRange(1, 60000)][int]$TimeoutMilliseconds = 15000
    )
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $engine
    $info.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $ScriptPath + '"'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $started = $false
    try {
        $started = $process.Start()
        if (-not $started) { throw 'Unable to start the fixture process.' }
        # Drain both pipes concurrently; waiting first deadlocks on full buffers.
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutMilliseconds)) {
            throw [TimeoutException]::new("Fixture timed out: $ScriptPath")
        }
        $code = $process.ExitCode
        if ($null -eq $code) { throw 'Fixture process returned no exit code.' }
        $streams = [Threading.Tasks.Task[]]@($stdoutTask, $stderrTask)
        if (-not [Threading.Tasks.Task]::WaitAll($streams, 5000)) {
            throw 'Fixture output streams did not close.'
        }
        [pscustomobject]@{
            ExitCode = $code
            Stdout = $stdoutTask.GetAwaiter().GetResult()
            Stderr = $stderrTask.GetAwaiter().GetResult()
        }
    } finally {
        try {
            if ($started -and -not $process.HasExited) {
                try { $process.Kill() }
                catch {
                    # A natural exit can race the termination request.
                    if (-not $process.HasExited) { throw }
                }
                if (-not $process.WaitForExit(5000)) {
                    throw 'Fixture process did not stop after termination.'
                }
            }
        } finally { $process.Dispose() }
    }
}
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
    # Validate the harness itself: real exit codes, saturated pipes and timeout.
    # The fixture root deliberately contains spaces to exercise argument quoting.
    foreach ($expectedExit in @(0, 29)) {
        $child = Join-Path $fixture "exit-$expectedExit.ps1"
        [IO.File]::WriteAllText($child, "exit $expectedExit", [Text.UTF8Encoding]::new($true))
        $result = Invoke-FixtureProcess -ScriptPath $child
        if ($result.ExitCode -ne $expectedExit -or $result.Stdout -ne '' -or $result.Stderr -ne '') {
            throw "Fixture harness lost exit code $expectedExit or returned unexpected output."
        }
    }
    $child = Join-Path $fixture 'streams.ps1'
    [IO.File]::WriteAllText($child, "[Console]::Out.Write(('O' * 131072)); [Console]::Error.Write(('E' * 131072)); exit 0", [Text.UTF8Encoding]::new($true))
    $result = Invoke-FixtureProcess -ScriptPath $child
    if ($result.ExitCode -ne 0 -or $result.Stdout -cne ('O' * 131072) -or $result.Stderr -cne ('E' * 131072)) {
        throw 'Fixture harness failed to drain both output streams.'
    }
    $child = Join-Path $fixture 'timeout.ps1'
    [IO.File]::WriteAllText($child, 'Start-Sleep -Seconds 60', [Text.UTF8Encoding]::new($true))
    $timedOut = $false
    try { $null = Invoke-FixtureProcess -ScriptPath $child -TimeoutMilliseconds 250 }
    catch [TimeoutException] { $timedOut = $true }
    if (-not $timedOut) { throw 'Fixture harness accepted a timed-out process.' }
    foreach ($scenario in @('download', 'hash', 'extract', 'missing', 'ambiguous', 'native', 'success', 'nested', 'existing', 'existing-failure')) {
        $directory = Join-Path $fixture $scenario
        New-Item -ItemType Directory -Path $directory | Out-Null
        $child = Join-Path $directory 'check.ps1'
        $prefix = '$scenario = ''' + $scenario + "'`n" + '$fixtureHash = ''' + $hashMatch.Groups[1].Value + "'`n"
        [IO.File]::WriteAllText($child, $prefix + $mock + "`n" + $body + "`n" + $assertions, [Text.UTF8Encoding]::new($true))
        $result = Invoke-FixtureProcess -ScriptPath $child
        $code = $result.ExitCode
        $stdout = $result.Stdout
        $stderr = $result.Stderr
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
    Write-Output 'PASS: 4 process harness checks and 10 offline compressor fallback scenarios.'
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
