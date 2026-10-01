param(
    [string]$IsccPath = '',
    [switch]$SkipBuild,
    [switch]$Offline,
    # These options produce an isolated test product, never a release installer.
    [ValidatePattern('^[a-f0-9]{32}$')][string]$TestId = '',
    [string]$TestRoot = '',
    [string]$TestPluginPath = '',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$TestVersion = ''
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $repoRoot 'crates/spectral-resonator-plugin/Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version = "(\d+\.\d+\.\d+)"\s*$') { throw 'Expected a numeric plugin version' }
$version = $Matches[1]
if (($TestRoot -or $TestPluginPath -or $TestVersion) -and -not $TestId) { throw 'Test overrides require TestId' }
if ($TestId -and (-not $TestRoot -or -not $TestVersion -or -not $TestPluginPath)) { throw 'Specify all test options' }

if (-not $IsccPath) {
    $candidates = @(
        (Join-Path $repoRoot 'target/installer-tools/inno-6.7.3/ISCC.exe'),
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    )
    $IsccPath = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $IsccPath) { $IsccPath = (Get-Command ISCC.exe -ErrorAction Stop).Source }
}
$IsccPath = (Resolve-Path -LiteralPath $IsccPath).Path
# The .iss preprocessor checks the compiler version; ISCC's PE version is 0.0.0.0.

if ($TestId) {
    $version = $TestVersion
    $plugin = (Resolve-Path -LiteralPath $TestPluginPath).Path
    $TestRoot = [IO.Path]::GetFullPath($TestRoot).TrimEnd('\')
    $testParent = [IO.Path]::GetFullPath((Join-Path $repoRoot 'target/installer-tests')).TrimEnd('\') + '\'
    if (-not $TestRoot.StartsWith($testParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'TestRoot must be beneath target/installer-tests'
    }
    $outputDir = Join-Path $TestRoot "packages/$version"
} else {
    if (-not $SkipBuild) {
        & (Join-Path $PSScriptRoot 'bundle-clap.ps1') -ArtifactOnly -Offline:$Offline
    }
    $plugin = Join-Path $repoRoot "target/artifacts/$version/my_spectral_resonator.clap"
    $outputDir = Join-Path $repoRoot "target/installers/$version"
}
if (-not (Test-Path -LiteralPath $plugin -PathType Leaf)) { throw "Missing CLAP artifact: $plugin" }
$runtime = Join-Path $repoRoot 'target/installer-tools/vc_redist.x64.exe'
$runtimeHash = 'CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B'
if (-not (Test-Path -LiteralPath $runtime) -or (Get-FileHash -LiteralPath $runtime).Hash -ne $runtimeHash) {
    throw 'Verified VC++ runtime missing. Run scripts/setup-installer-tools.ps1 first.'
}
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

# ICO supports PNG-encoded images. Pack the existing authored PNGs without
# changing the icon assets or needing an image-processing runtime on builders.
$icon = Join-Path $outputDir 'spectral-resonator.ico'
$sizes = @(32, 64, 128, 256)
$images = @($sizes | ForEach-Object {
    ,([IO.File]::ReadAllBytes((Join-Path $repoRoot "assets/icon/spectral-resonator-$_.png")))
})
$stream = [IO.File]::Create($icon)
$writer = [IO.BinaryWriter]::new($stream)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
    $offset = 6 + 16 * $images.Count
    for ($i = 0; $i -lt $images.Count; $i++) {
        $dimension = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
        $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
        $writer.Write([byte]0); $writer.Write([byte]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$images[$i].Length); $writer.Write([uint32]$offset)
        $offset += $images[$i].Length
    }
    foreach ($bytes in $images) { $writer.Write([byte[]]$bytes) }
} finally { $writer.Dispose(); $stream.Dispose() }

$compileArgs = @('/Qp', "/DAppVersion=$version", "/DPluginFile=$plugin", "/DOutputDir=$outputDir", "/DIconFile=$icon", "/DVcRuntimeFile=$runtime")
if ($TestId) { $compileArgs += @("/DTestId=$TestId", "/DTestRoot=$TestRoot") }
$compileArgs += (Join-Path $repoRoot 'installer/windows/spectral-resonator.iss')
& $IsccPath @compileArgs
if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed: $LASTEXITCODE" }
$setup = Join-Path $outputDir "SpectralResonator-$version-Windows-x64-Setup.exe"
$evidence = [ordered]@{
    version = $version
    test_product = [bool]$TestId
    plugin_sha256 = (Get-FileHash -LiteralPath $plugin -Algorithm SHA256).Hash
    setup_sha256 = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash
    compiler_sha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $IsccPath) 'ISCmplr.dll')).Hash
    vc_runtime_sha256 = $runtimeHash
    built_at_utc = [DateTime]::UtcNow.ToString('o')
}
$evidence | ConvertTo-Json | Set-Content -LiteralPath "$setup.json" -Encoding utf8
Write-Output "Windows installer: $setup"
