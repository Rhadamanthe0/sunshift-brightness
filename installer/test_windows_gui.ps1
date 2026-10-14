param(
    [switch] $PrepareOnly,
    [ValidateSet('Vulkan', 'OpenGL')]
    [string] $Backend = 'Vulkan'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'copy_graphics_library.ps1')
$backendName = if ($PSBoundParameters.ContainsKey('Backend')) { $Backend.ToLowerInvariant() } else { '' }
$filter = 'gui::app::native_tests::native_windows_gui_lifecycle'
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
# libtest peut terminer avec succès sans exécuter aucun test. Vérifier le nom
# complet ignoré avant de l'exécuter, sans accepter un filtre devenu obsolète.
$testList = & $executable --list --ignored
if ($LASTEXITCODE -ne 0) { throw 'Unable to list the native GUI tests.' }
$nativeTests = @($testList | Where-Object { $_ -match '(?:^|::)native_windows_gui_lifecycle: test$' })
if ($nativeTests.Count -ne 1 -or $nativeTests[0] -cne "$($filter): test") {
    throw 'The expected ignored native GUI test is absent or ambiguous.'
}
if ($PrepareOnly) {
    "SSB_GUI_TEST_EXECUTABLE=$executable" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    return
}

# wgpu-hal 0.16 loads libEGL.dll (GLES 3); opengl32.dll alone is insufficient.
# The disposable MINIPC guest supplies these verified libraries. Keep them
# app-local, including when the executable is copied to the session-0 fixture.
$glesDirectory = 'C:\RhadaCI\graphics\gles'
$vulkanDirectory = 'C:\RhadaCI\graphics\vulkan'
$vulkanIcd = Join-Path $vulkanDirectory 'vk_swiftshader_icd.json'
$graphicsLibraries = @()
# Un backend explicite est une preuve CI sur SwiftShader, sans repli système.
# Sans sélection, garder le test naturel sur les pilotes de la machine.
$required = @()
if ($backendName -eq 'opengl') {
    $required = @('libEGL.dll', 'libGLESv2.dll', 'vk_swiftshader.dll') |
        ForEach-Object { Join-Path $glesDirectory $_ }
    $required += Join-Path $vulkanDirectory 'vulkan-1.dll'
    $required += Join-Path $PSScriptRoot 'vk_swiftshader_icd.json'
} elseif ($backendName -eq 'vulkan') {
    $required = @('vk_swiftshader_icd.json', 'vulkan-1.dll', 'vk_swiftshader.dll') |
        ForEach-Object { Join-Path $vulkanDirectory $_ }
}
foreach ($file in $required) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        throw "Missing SwiftShader fixture: $file"
    }
}
if ($backendName -eq 'opengl') {
    $graphicsLibraries = @(Get-ChildItem -LiteralPath $glesDirectory -Filter '*.dll' -File)
    # ANGLE 137 charge vulkan-1.dll et cherche son ICD auprès de libEGL.
    # Conserver le pilote GLES 137 ; seul le chargeur vérifié est partagé.
    $graphicsLibraries += Get-Item -LiteralPath (Join-Path $vulkanDirectory 'vulkan-1.dll')
    $graphicsLibraries += Get-Item -LiteralPath (Join-Path $PSScriptRoot 'vk_swiftshader_icd.json')
} elseif ($backendName -eq 'vulkan') {
    # PATH ne remplace pas un loader trouvé dans System32 : utiliser celui vérifié.
    $graphicsLibraries = @(Get-ChildItem -LiteralPath $vulkanDirectory -Filter '*.dll' -File)
}
$session = (Get-Process -Id $PID).SessionId
Write-Output "GUI test runner session: $session"
if ($session -ne 0) {
    # Vulkan et OpenGL peuvent fournir deux vk_swiftshader.dll différentes.
    # Chaque invocation garde ses DLL privées, comme le chemin en session 0.
    $fixture = Join-Path ([IO.Path]::GetTempPath()) ('ssb-gui-' + [guid]::NewGuid().ToString('N'))
    $privateExecutable = Join-Path $fixture (Split-Path $executable -Leaf)
    $previousBackend = $env:SSB_TEST_GUI_BACKEND
    $previousIcd = $env:VK_ICD_FILENAMES
    $previousPath = $env:PATH
    $previousAngle = $env:ANGLE_DEFAULT_PLATFORM
    try {
        New-Item -ItemType Directory -Path $fixture -ErrorAction Stop | Out-Null
        Copy-Item -LiteralPath $executable -Destination $privateExecutable -ErrorAction Stop
        foreach ($library in $graphicsLibraries) {
            Copy-VerifiedGraphicsLibrary -Source $library.FullName -DestinationDirectory $fixture
        }
        $env:SSB_TEST_GUI_BACKEND = if ($backendName) { $backendName } else { $null }
        if ($backendName -eq 'vulkan') {
            $env:VK_ICD_FILENAMES = $vulkanIcd
            $env:PATH = "$vulkanDirectory;$env:PATH"
        } elseif ($backendName -eq 'opengl') {
            $env:ANGLE_DEFAULT_PLATFORM = 'swiftshader'
            $env:VK_ICD_FILENAMES = Join-Path $fixture 'vk_swiftshader_icd.json'
        }
        & $privateExecutable $filter --exact --ignored --nocapture --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Native GUI lifecycle failed.' }
    } finally {
        $env:SSB_TEST_GUI_BACKEND = $previousBackend
        $env:VK_ICD_FILENAMES = $previousIcd
        $env:PATH = $previousPath
        $env:ANGLE_DEFAULT_PLATFORM = $previousAngle
        if (Test-Path -LiteralPath $fixture) {
            Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction Stop
        }
    }
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
    foreach ($library in $graphicsLibraries) {
        Copy-VerifiedGraphicsLibrary -Source $library.FullName -DestinationDirectory $directory
    }
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
    $glesIcdLiteral = (Join-Path $directory 'vk_swiftshader_icd.json').Replace("'", "''")
    $checkBrightness = if ($env:SSB_TEST_MONITOR_ROUNDTRIP -eq '1') { '-CheckBrightness' } else { '' }
    $command = @"
`$env:PATH = '$dllLiteral;' + `$env:PATH
`$env:SSB_TEST_GUI_BACKEND = if ('$backendName') { '$backendName' } else { `$null }
if ('$backendName' -eq 'vulkan') {
    `$env:VK_ICD_FILENAMES = '$vulkanIcd'
    `$env:PATH = '$vulkanDirectory;' + `$env:PATH
} elseif ('$backendName' -eq 'opengl') {
    `$env:ANGLE_DEFAULT_PLATFORM = 'swiftshader'
    `$env:VK_ICD_FILENAMES = '$glesIcdLiteral'
}
`$process = Start-Process -FilePath '$binaryLiteral' -ArgumentList '$filter', '--exact', '--ignored', '--nocapture', '--test-threads=1' -NoNewWindow -Wait -PassThru -RedirectStandardOutput '$outputLiteral' -RedirectStandardError '$errorLiteral'
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
    # The service/interactive path must clean up its executable, libraries and logs.
    if (Test-Path -LiteralPath $directory) {
        Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction Stop
    }
}
