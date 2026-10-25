# Recreate the existing software-rendering fixture on a disposable GitHub VM.
# No browser is installed or executed; only the six pinned files are extracted.
function Install-GitHubGraphics {
    [CmdletBinding()]
    param(
        [string] $ManifestPath = (Join-Path $PSScriptRoot 'github-graphics.json'),
        [string] $DestinationRoot = 'C:\RhadaCI\graphics'
    )
    $ErrorActionPreference = 'Stop'
    if ($env:GITHUB_ACTIONS -ne 'true' -or
        $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
        throw 'Graphics preparation requires a disposable GitHub-hosted Windows runner.'
    }
    if (-not $env:RUNNER_TEMP -or -not (Test-Path -LiteralPath $env:RUNNER_TEMP -PathType Container)) {
        throw 'RUNNER_TEMP is unavailable.'
    }
    if (Test-Path -LiteralPath $DestinationRoot) {
        throw 'Refusing to replace an existing graphics fixture.'
    }
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $expected = @{
        gles = @('libEGL.dll', 'libGLESv2.dll', 'vk_swiftshader.dll')
        vulkan = @('vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll')
    }
    if ($manifest.version -ne 1 -or @($manifest.packages).Count -ne 2) {
        throw 'Invalid graphics manifest.'
    }
    $seen = @{}
    foreach ($package in $manifest.packages) {
        $group = [string]$package.directory
        if (-not $expected.ContainsKey($group) -or $seen.ContainsKey($group)) {
            throw 'Invalid or duplicate graphics directory.'
        }
        $seen[$group] = $true
        if ($package.version -notmatch '^\d+\.\d+\.\d+\.\d+$' -or
            $package.url -cne "https://storage.googleapis.com/chrome-for-testing-public/$($package.version)/win64/chrome-win64.zip" -or
            $package.sha256 -notmatch '^[a-fA-F0-9]{64}$') {
            throw 'Invalid graphics archive source or hash.'
        }
        $names = @($package.files | ForEach-Object { $_.name } | Sort-Object)
        if (($names -join ',') -cne (($expected[$group] | Sort-Object) -join ',')) {
            throw 'Invalid graphics file list.'
        }
        foreach ($file in $package.files) {
            if ($file.sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid graphics file hash.' }
        }
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $temporary = Join-Path $env:RUNNER_TEMP ('ssb-graphics-' + [guid]::NewGuid().ToString('N'))
    $stage = Join-Path $temporary 'verified'
    try {
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        foreach ($package in $manifest.packages) {
            $archive = Join-Path $temporary ($package.directory + '.zip')
            Invoke-WebRequest -UseBasicParsing -Uri $package.url -OutFile $archive -TimeoutSec 300
            if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $package.sha256) {
                throw "Graphics archive SHA-256 mismatch: $($package.directory)"
            }
            $folder = Join-Path $stage $package.directory
            New-Item -ItemType Directory -Path $folder | Out-Null
            $zip = [IO.Compression.ZipFile]::OpenRead($archive)
            try {
                foreach ($file in $package.files) {
                    $entryName = 'chrome-win64/' + $file.name
                    # .NET Framework ZIPs can use backslashes; still require one exact entry.
                    $entries = @($zip.Entries | Where-Object { $_.FullName.Replace('\', '/') -ceq $entryName })
                    if ($entries.Count -ne 1) { throw "Missing or duplicate graphics entry: $entryName" }
                    $destination = Join-Path $folder $file.name
                    [IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $destination, $false)
                    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $file.sha256) {
                        throw "Graphics file SHA-256 mismatch: $($package.directory)/$($file.name)"
                    }
                }
            } finally {
                $zip.Dispose()
            }
        }
        # Publish only after BOTH archives and every extracted file are verified.
        $parent = Split-Path -Parent $DestinationRoot
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        Move-Item -LiteralPath $stage -Destination $DestinationRoot -ErrorAction Stop
        Write-Output 'Verified Vulkan and OpenGL software-rendering fixtures installed.'
    } finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction Stop
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') { Install-GitHubGraphics }
