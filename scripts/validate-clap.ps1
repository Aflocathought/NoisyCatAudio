param(
    [ValidatePattern('^[A-Za-z0-9 _-]+$')]
    [string]$BundleName = "my_spectral_resonator",
    [string]$PluginPath = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($PluginPath)) {
    $PluginPath = Join-Path $repoRoot "target\bundled\$BundleName.clap"
}
$PluginPath = [System.IO.Path]::GetFullPath($PluginPath)
$localValidator = Join-Path $repoRoot "target\tools\bin\clap-validator.exe"

if (-not (Test-Path -LiteralPath $pluginPath -PathType Leaf)) {
    throw "Windows CLAP file not found: $pluginPath"
}

if (Test-Path -LiteralPath $localValidator -PathType Leaf) {
    $validator = $localValidator
}
else {
    $validator = (Get-Command clap-validator -ErrorAction Stop).Source
}

Push-Location $repoRoot
try {
    & $validator validate $pluginPath --only-failed
    if ($LASTEXITCODE -ne 0) {
        throw "clap-validator failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
