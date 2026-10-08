$ErrorActionPreference = 'Stop'

# Exercise the real NSIS script with isolated names and directory in RUNNER_TEMP.
# The application binaries are copied and checked, never launched. A renamed
# cmd.exe holds the CLI file open to verify that upgrades stop it before copying.
$id = [guid]::NewGuid().ToString('N')
$name = "SSB CI $id"
$guiName = "ssb-ci-gui-$id.exe"
$cliName = "ssb-ci-cli-$id.exe"
$runValue = "SsbCi$id"
$root = Join-Path $env:RUNNER_TEMP "ssb-installer-test-$id"
$install = Join-Path $root 'installed'
$binaries = Join-Path $root 'binaries'
$script = Join-Path $root 'test.nsi'
$installer = Join-Path $root 'installer.exe'
$shortcuts = @(Join-Path $install "$name.lnk"; Join-Path $install "Uninstall $name.lnk")
$registryPath = "Software\SunshiftBrightness\CI\$id"
$process = $null
$registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::CurrentUser,
    [Microsoft.Win32.RegistryView]::Registry32
)
$runKey = $registry.CreateSubKey($registryPath)

function Invoke-Installer([string] $Path, [string[]] $Arguments) {
    $p = Start-Process -FilePath $Path -ArgumentList $Arguments -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "Installer exited with code $($p.ExitCode)." }
}

try {
    New-Item -ItemType Directory -Path $install, $binaries | Out-Null
    $release = Join-Path $PSScriptRoot '..\target\release'
    Copy-Item (Join-Path $release 'ssb.exe') (Join-Path $binaries $guiName)
    Copy-Item (Join-Path $release 'ssb-cli.exe') (Join-Path $binaries $cliName)
    $source = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'windows.nsi'))
    $source = $source.Replace('Custom Solar Screen Brightness', $name)
    $source = $source.Replace('CustomSolarScreenBrightness', $runValue)
    $source = $source.Replace('Software\Microsoft\Windows\CurrentVersion\Run', $registryPath)
    $source = $source.Replace('$SMPROGRAMS', '$INSTDIR')
    $defaultDirectory = 'InstallDir "$LocalAppData\solar-screen-brightness"'
    if (-not $source.Contains($defaultDirectory)) { throw 'Unexpected installer directory definition.' }
    $source = $source.Replace($defaultDirectory, 'InstallDir "' + $install.Replace('$', '$$') + '"')
    $source = $source.Replace('solar-screen-brightness.exe', "ssb-ci-legacy-$id.exe")
    $source = $source.Replace('ssb-cli.exe', $cliName).Replace('ssb.exe', $guiName)
    [IO.File]::WriteAllText($script, $source, [Text.UTF8Encoding]::new($false))
    $nsis = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($nsis) {
        $compiler = $nsis.Source
    } elseif (Test-Path "${env:ProgramFiles(x86)}\NSIS\makensis.exe") {
        $compiler = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
    } else {
        $compiler = Join-Path $env:RUNNER_TOOL_CACHE 'ssb-nsis-3.11\nsis-3.11\makensis.exe'
    }
    $icon = Join-Path $PSScriptRoot '..\assets\icon-256.ico'
    & $compiler "/DBINDIR=$binaries" "/DICON=$icon" "/DOUTFILE=$installer" $script
    if ($LASTEXITCODE -ne 0) { throw 'Unable to compile the isolated installer.' }

    $config = Join-Path $install 'config.json'
    [IO.File]::WriteAllText($config, '{"brightness_day":75,"brightness_night":25,"transition_mins":40,"location":null}')
    $before = (Get-FileHash $config).Hash
    $cli = Join-Path $install $cliName
    Copy-Item "$env:SystemRoot\System32\cmd.exe" $cli
    $process = Start-Process -FilePath $cli -ArgumentList '/D', '/K' -WindowStyle Hidden -PassThru
    Start-Sleep -Milliseconds 500
    if ($process.HasExited) { throw 'The test process did not remain running.' }

    for ($upgrade = 0; $upgrade -lt 2; $upgrade++) {
        Invoke-Installer $installer @('/S')
        if (-not $process.WaitForExit(5000)) { throw 'Upgrade did not stop the running CLI.' }
        if ((Get-FileHash $config).Hash -ne $before) { throw 'Upgrade changed configuration.' }
        foreach ($binary in @($guiName, $cliName)) {
            if ((Get-FileHash (Join-Path $install $binary)).Hash -ne
                (Get-FileHash (Join-Path $binaries $binary)).Hash) {
                throw "Upgrade did not copy $binary correctly."
            }
        }
        if ($runKey.GetValue($runValue) -ne "`"$(Join-Path $install $guiName)`" --minimised") {
            throw 'Autostart command does not match the installed binary.'
        }
        foreach ($shortcut in $shortcuts) {
            if (-not (Test-Path $shortcut)) { throw "Missing shortcut: $shortcut" }
        }
        $shell = New-Object -ComObject WScript.Shell
        try {
            for ($index = 0; $index -lt $shortcuts.Count; $index++) {
                $link = $shell.CreateShortcut($shortcuts[$index])
                try {
                    $target = if ($index -eq 0) { $guiName } else { 'uninstall.exe' }
                    if ($link.TargetPath -ne (Join-Path $install $target)) {
                        throw 'Shortcut points to the wrong binary.'
                    }
                } finally {
                    [Runtime.InteropServices.Marshal]::ReleaseComObject($link) | Out-Null
                }
            }
        } finally {
            [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null
        }
    }

    # Hold the installed CLI file open again to exercise uninstallation too.
    $process.Dispose()
    $process = $null
    Copy-Item "$env:SystemRoot\System32\cmd.exe" $cli -Force
    $process = Start-Process -FilePath $cli -ArgumentList '/D', '/K' -WindowStyle Hidden -PassThru
    Start-Sleep -Milliseconds 500
    if ($process.HasExited) { throw 'The uninstall test process did not remain running.' }
    Invoke-Installer (Join-Path $install 'uninstall.exe') @('/S')
    if (-not $process.WaitForExit(5000)) { throw 'Uninstall did not stop the running CLI.' }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ((Test-Path $install) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (Test-Path $install) { throw 'Uninstall left application files behind.' }
    if ($null -ne $runKey.GetValue($runValue)) { throw 'Uninstall left its autostart entry.' }
    foreach ($shortcut in $shortcuts) {
        if (Test-Path $shortcut) { throw 'Uninstall left a shortcut behind.' }
    }
    Write-Output 'Native NSIS lifecycle passed: stopped CLI, preserved configuration twice, correct payload/autostart/shortcuts, clean uninstall.'
} finally {
    if ($process) {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }
    # Delete only this test's GUID-named entries, including on failure.
    $runKey.DeleteValue($runValue, $false)
    $runKey.Dispose()
    $registry.DeleteSubKeyTree($registryPath, $false)
    $registry.Dispose()
    foreach ($shortcut in $shortcuts) {
        if (Test-Path $shortcut) { Remove-Item -LiteralPath $shortcut }
    }
}
