param([switch] $PrepareOnly)
$ErrorActionPreference = 'Stop'
$filter = 'native_windows_gui_lifecycle'
$executable = $env:SSB_GUI_TEST_EXECUTABLE
if ($PrepareOnly -or -not $executable) {
    $artifacts = & cargo test --locked --no-run --lib --message-format=json
    if ($LASTEXITCODE -ne 0) { throw 'Unable to build the native GUI test executable.' }
    $executable = $null
    foreach ($line in $artifacts) {
        $artifact = $line | ConvertFrom-Json
        if ($artifact.reason -eq 'compiler-artifact' -and $artifact.executable -and
            $artifact.profile.test -and $artifact.target.kind -contains 'lib') {
            $executable = $artifact.executable
        }
    }
}
if (-not $executable -or -not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw 'Native GUI test executable was not found.'
}
if ($PrepareOnly) {
    "SSB_GUI_TEST_EXECUTABLE=$executable" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    return
}

$session = (Get-Process -Id $PID).SessionId
Write-Output "GUI test runner session: $session"
if ($session -ne 0) {
    & $executable $filter --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Native GUI lifecycle failed.' }
    & (Join-Path $PSScriptRoot 'diagnose_windows_monitors.ps1') -CheckBrightness:($env:SSB_TEST_MONITOR_ROUNDTRIP -eq '1')
    return
}

# Services have no interactive desktop. Run only the test executable as the
# logged-in console user, without changing the runner service or user files.
$user = (Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5).UserName
if (-not $user) { throw 'Log into the Windows desktop before running the GUI test.' }
$id = [guid]::NewGuid().ToString('N')
$task = "SSB-CI-GUI-$id"
$directory = Join-Path $env:RUNNER_TEMP "ssb-gui-$id"
$output = Join-Path $directory 'output.txt'
$errorOutput = Join-Path $directory 'stderr.txt'
$exitFile = Join-Path $directory 'exit.txt'
$exitTemporary = Join-Path $directory 'exit.tmp'
$binary = Join-Path $directory 'gui-test.exe'
$diagnostics = Join-Path $directory 'diagnose.ps1'
$diagnosticOutput = Join-Path $directory 'diagnostics.txt'
$registered = $false
try {
    New-Item -ItemType Directory -Path $directory | Out-Null
    # Restrict this new folder to the runner/admins and selected desktop user.
    $acl = Get-Acl $directory
    $acl.SetAccessRuleProtection($true, $false)
    $identities = @(
        [Security.Principal.WindowsIdentity]::GetCurrent().Name,
        'S-1-5-18',
        'S-1-5-32-544',
        $user
    )
    foreach ($identity in $identities) {
        $principal = if ($identity -like 'S-1-*') {
            [Security.Principal.SecurityIdentifier]::new($identity)
        } else {
            [Security.Principal.NTAccount]::new($identity)
        }
        $rule = [Security.AccessControl.FileSystemAccessRule]::new(
            $principal, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
        )
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $directory -AclObject $acl
    Copy-Item -LiteralPath $executable -Destination $binary
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'diagnose_windows_monitors.ps1') -Destination $diagnostics
    $gcc = Get-Command gcc.exe -ErrorAction SilentlyContinue
    $dllPath = if ($gcc) { Split-Path $gcc.Source } else { '' }
    # Values are local paths, quoted as PowerShell literals. No credentials or
    # GitHub environment are passed to the interactive process.
    $binaryLiteral = $binary.Replace("'", "''")
    $outputLiteral = $output.Replace("'", "''")
    $errorLiteral = $errorOutput.Replace("'", "''")
    $exitLiteral = $exitFile.Replace("'", "''")
    $temporaryLiteral = $exitTemporary.Replace("'", "''")
    $dllLiteral = $dllPath.Replace("'", "''")
    $diagnosticsLiteral = $diagnostics.Replace("'", "''")
    $diagnosticOutputLiteral = $diagnosticOutput.Replace("'", "''")
    $checkBrightness = if ($env:SSB_TEST_MONITOR_ROUNDTRIP -eq '1') { '-CheckBrightness' } else { '' }
    $command = @"
`$env:PATH = '$dllLiteral;' + `$env:PATH
`$process = Start-Process -FilePath '$binaryLiteral' -ArgumentList '$filter', '--ignored', '--nocapture', '--test-threads=1' -NoNewWindow -Wait -PassThru -RedirectStandardOutput '$outputLiteral' -RedirectStandardError '$errorLiteral'
`$result = `$process.ExitCode
`$process.Dispose()
try {
    & '$diagnosticsLiteral' $checkBrightness *> '$diagnosticOutputLiteral'
} catch {
    `$_ | Out-File -FilePath '$diagnosticOutputLiteral' -Encoding Unicode -Append
    `$result = 1
}
[IO.File]::WriteAllText('$temporaryLiteral', [string]`$result)
[IO.File]::Move('$temporaryLiteral', '$exitLiteral')
exit `$result
"@
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $action = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument "-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand $encoded"
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromSeconds(90)) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings | Out-Null
    $registered = $true
    Start-ScheduledTask -TaskName $task
    $deadline = [DateTime]::UtcNow.AddSeconds(100)
    while (-not (Test-Path $exitFile) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if (-not (Test-Path $exitFile)) {
        $result = (Get-ScheduledTaskInfo -TaskName $task).LastTaskResult
        throw "Interactive GUI test did not finish (task result $result)."
    }
    if (([IO.File]::ReadAllText($exitFile)).Trim() -ne '0') {
        throw 'Native Windows validation failed in the interactive desktop.'
    }
} finally {
    if ($registered) {
        Stop-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $task -Confirm:$false
    }
    if (Test-Path $output) { Get-Content -LiteralPath $output -Encoding UTF8 }
    if (Test-Path $errorOutput) { Get-Content -LiteralPath $errorOutput -Encoding UTF8 }
    if (Test-Path $diagnosticOutput) { Get-Content -LiteralPath $diagnosticOutput -Encoding Unicode }
}
