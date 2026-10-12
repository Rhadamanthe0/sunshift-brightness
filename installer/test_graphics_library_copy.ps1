$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'copy_graphics_library.ps1')
$fixture = Join-Path ([IO.Path]::GetTempPath()) ("ssb-graphics-copy-" + [guid]::NewGuid().ToString('N'))
try {
    $sourceDirectory = New-Item -ItemType Directory -Path (Join-Path $fixture 'source')
    $destinationDirectory = New-Item -ItemType Directory -Path (Join-Path $fixture 'destination')
    $source = Join-Path $sourceDirectory.FullName 'vk_swiftshader.dll'
    $destination = Join-Path $destinationDirectory.FullName 'vk_swiftshader.dll'
    [IO.File]::WriteAllBytes($source, [byte[]](1, 2, 3, 4))
    Copy-VerifiedGraphicsLibrary -Source $source -DestinationDirectory $destinationDirectory.FullName
    if ((Get-FileHash $source).Hash -cne (Get-FileHash $destination).Hash) { throw 'Fresh copy differs.' }
    $file = Get-Item -LiteralPath $destination
    $file.IsReadOnly = $true
    $attributes = $file.Attributes
    $modified = $file.LastWriteTimeUtc
    # A second backend reuses the same protected DLL without writing it.
    Copy-VerifiedGraphicsLibrary -Source $source -DestinationDirectory $destinationDirectory.FullName
    $file = Get-Item -LiteralPath $destination
    if ($file.Attributes -ne $attributes -or $file.LastWriteTimeUtc -ne $modified) {
        throw 'Identical protected DLL was modified.'
    }
    [IO.File]::WriteAllBytes($source, [byte[]](9, 8, 7, 6))
    $rejected = $false
    try { Copy-VerifiedGraphicsLibrary -Source $source -DestinationDirectory $destinationDirectory.FullName }
    catch {
        if ($_.Exception.Message -notlike '*SHA256 mismatch*existing file preserved*') { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw 'Different DLL was reused without verification.' }
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($destination)) -cne 'AQIDBA==') {
        throw 'Mismatch altered the existing DLL.'
    }
    if (-not (Get-Item $destination).IsReadOnly) { throw 'Mismatch removed protection.' }
    Write-Output 'Graphics fixture fresh copy, protected identical reuse, and mismatch preservation: PASS'
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
