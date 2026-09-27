param(
    [ValidatePattern('^[A-Za-z0-9 _-]+$')]
    [string]$BundleName = "my_spectral_resonator",
    [switch]$ArtifactOnly,
    [switch]$Offline
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$dllPath = Join-Path $repoRoot "target\release\spectral_resonator_plugin.dll"
$bundleDir = Join-Path $repoRoot "target\bundled"
$modulePath = Join-Path $bundleDir "$BundleName.clap"

Push-Location $repoRoot
try {
    $buildArgs = @('build', '-p', 'spectral-resonator-plugin', '--release', '--locked')
    if ($Offline) { $buildArgs += '--offline' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }

    $metadataArgs = @('metadata', '--no-deps', '--format-version', '1', '--locked')
    if ($Offline) { $metadataArgs += '--offline' }
    $metadataJson = & cargo @metadataArgs
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the built package version' }
    $metadata = $metadataJson | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -eq 'spectral-resonator-plugin').version
    if ($version -notmatch '^\d+\.\d+\.\d+([+-][A-Za-z0-9.-]+)?$') { throw "Invalid package version: $version" }
    $artifactDir = Join-Path $repoRoot "target\artifacts\$version"
    New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null
    $artifactPath = Join-Path $artifactDir "$BundleName.clap"
    Copy-Item -LiteralPath $dllPath -Destination $artifactPath -Force
    Write-Output "Versioned CLAP artifact created: $artifactPath"
    if ($ArtifactOnly) { return }

    # On Windows, the .clap path itself is a loadable DLL, not a directory.
    New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
    if (Test-Path -LiteralPath $modulePath -PathType Container) {
        throw "A legacy directory occupies the Windows CLAP file path: $modulePath"
    }
    # Staging first retains a usable artifact if the host has locked its copy.
    Copy-Item -LiteralPath $artifactPath -Destination $modulePath -Force
    Write-Output "CLAP plugin created: $modulePath"
}
finally {
    Pop-Location
}
