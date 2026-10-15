# Reuse app-local graphics fixtures only when their bytes match the verified input.
function Copy-VerifiedGraphicsLibrary {
    param(
        [Parameter(Mandatory)] [string] $Source,
        [Parameter(Mandatory)] [string] $DestinationDirectory
    )
    $destination = Join-Path $DestinationDirectory (Split-Path $Source -Leaf)
    $expected = (Get-FileHash -LiteralPath $Source -Algorithm SHA256 -ErrorAction Stop).Hash
    if (Test-Path -LiteralPath $destination) {
        if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) {
            throw "Graphics fixture destination is not a file: $destination"
        }
        $actual = (Get-FileHash -LiteralPath $destination -Algorithm SHA256 -ErrorAction Stop).Hash
        if ($actual -cne $expected) {
            # Preserve the existing file on disagreement; never run another
            # backend using unverified bytes or overwrite a protected library.
            throw "Graphics fixture SHA256 mismatch: $destination; existing file preserved."
        }
        return
    }
    Copy-Item -LiteralPath $Source -Destination $destination -ErrorAction Stop
    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256 -ErrorAction Stop).Hash -cne $expected) {
        throw "Copied graphics fixture SHA256 mismatch: $destination"
    }
}
