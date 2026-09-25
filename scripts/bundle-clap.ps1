param(
    [ValidatePattern('^[A-Za-z0-9 _-]+$')]
    [string]$BundleName = "my_spectral_resonator"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$dllPath = Join-Path $repoRoot "target\release\spectral_resonator_plugin.dll"
$bundleDir = Join-Path $repoRoot "target\bundled"
$modulePath = Join-Path $bundleDir "$BundleName.clap"

Push-Location $repoRoot
try {
    cargo build -p spectral-resonator-plugin --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }

    # On Windows, the .clap path itself is a loadable DLL, not a directory.
    New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
    if (Test-Path -LiteralPath $modulePath -PathType Container) {
        throw "A legacy directory occupies the Windows CLAP file path: $modulePath"
    }
    Copy-Item -LiteralPath $dllPath -Destination $modulePath -Force
    Write-Output "CLAP plugin created: $modulePath"
}
finally {
    Pop-Location
}
