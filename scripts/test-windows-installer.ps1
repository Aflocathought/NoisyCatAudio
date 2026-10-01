#Requires -Version 7.0
param(
    [string]$IsccPath = '',
    [string]$PreviousPluginPath = '',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$PreviousVersion = '0.14.1'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testId = [Guid]::NewGuid().ToString('N')
$runRoot = Join-Path $repoRoot "target/installer-tests/$testId"
$pluginDir = Join-Path $runRoot '自选 CLAP 插件'
$otherDir = Join-Path $runRoot 'different-location'
$supportDir = Join-Path $runRoot 'support'
$productKey = "Software\AflocatAudio\InstallerTests\$testId"
$arpKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\AflocatAudio.SR.Test.${testId}_is1"
$manifest = Get-Content (Join-Path $repoRoot 'crates/spectral-resonator-plugin/Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version = "(\d+\.\d+\.\d+)"\s*$') { throw 'No version in Cargo manifest' }
$currentVersion = $Matches[1]
if ([version]$PreviousVersion -ge [version]$currentVersion) { throw 'PreviousVersion must be older than current' }
if (-not $PreviousPluginPath) {
    $PreviousPluginPath = Join-Path $repoRoot "target/artifacts/$PreviousVersion/my_spectral_resonator.clap"
}
$currentPlugin = Join-Path $repoRoot "target/artifacts/$currentVersion/my_spectral_resonator.clap"
$currentHash = (Get-FileHash -LiteralPath $currentPlugin).Hash
$oldHash = (Get-FileHash -LiteralPath $PreviousPluginPath).Hash
if ($oldHash -eq $currentHash) { throw 'Upgrade test requires distinct old and new payloads' }
$checks = [Collections.Generic.List[object]]::new()

function Assert-Check([bool]$Condition, [string]$Name) {
    if (-not $Condition) { throw "FAIL: $Name" }
    $checks.Add($Name)
    Write-Output "PASS: $Name"
}

function Run-Installer([string]$Exe, [string]$Name, [string[]]$Extra = @(), [switch]$ExpectFailure) {
    $log = Join-Path $runRoot "$Name.log"
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + $log + '"')) + $Extra
    $process = Start-Process -FilePath $Exe -ArgumentList $arguments -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(45000)) { throw "Timed out: $Name (see $log); process left for inspection" }
    Assert-Check (($process.ExitCode -eq 0) -ne [bool]$ExpectFailure) "$Name exit code $($process.ExitCode)"
}

function Read-Product([string]$Name) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($productKey)
    if (-not $key) { return $null }
    try { return $key.GetValue($Name) } finally { $key.Dispose() }
}

# Read-only snapshot of the actual shared preferences. The test installer has a
# separate product identity and is compiled to refuse paths outside runRoot.
$sharedSettings = Join-Path $env:APPDATA 'com.aflocat.audio/settings.json'
$settingsHash = if (Test-Path -LiteralPath $sharedSettings) { (Get-FileHash -LiteralPath $sharedSettings).Hash } else { $null }
New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
$sentinel = Join-Path $pluginDir 'another-vendors-plugin.clap'
[IO.File]::WriteAllText($sentinel, 'An unrelated plugin in a shared scan directory.')
$sentinelHash = (Get-FileHash -LiteralPath $sentinel).Hash
$newInstaller = Join-Path $runRoot "packages/$currentVersion/SpectralResonator-$currentVersion-Windows-x64-Setup.exe"
$oldInstaller = Join-Path $runRoot "packages/$PreviousVersion/SpectralResonator-$PreviousVersion-Windows-x64-Setup.exe"
$installedPlugin = Join-Path $pluginDir 'my_spectral_resonator.clap'
$uninstaller = Join-Path $supportDir 'unins000.exe'

foreach ($package in @(@{ Version = $PreviousVersion; Path = $PreviousPluginPath }, @{ Version = $currentVersion; Path = $currentPlugin })) {
    & (Join-Path $PSScriptRoot 'build-windows-installer.ps1') -IsccPath $IsccPath -TestId $testId -TestRoot $runRoot -TestPluginPath $package.Path -TestVersion $package.Version
}

try {
    Run-Installer $newInstaller '00-missing-runtime-gate' @('/CURRENTUSER', '/TESTMISSINGRUNTIME=1') -ExpectFailure
    Assert-Check (-not (Test-Path -LiteralPath $installedPlugin) -and $null -eq (Read-Product 'PluginDir')) 'missing-runtime gate stops before plugin installation'
    Run-Installer $oldInstaller '01-custom-install' @('/CURRENTUSER', '/LANG=chinesesimp', ('/CLAPDIR="' + $pluginDir + '"'))
    Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $oldHash) 'old payload installed byte-for-byte in Unicode custom folder'
    Assert-Check ((Read-Product 'PluginDir') -eq $pluginDir) 'custom plugin directory registered'
    Assert-Check ((Read-Product 'Version') -eq $PreviousVersion) 'old version registered'
    $arp = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($arpKey)
    Assert-Check ($null -ne $arp) 'Windows Installed Apps entry exists'
    $arp.Dispose()

    # A loaded native DLL reproduces the Windows lock relevant to an audio host
    # without starting, stopping, or changing the user's host session.
    $module = [Runtime.InteropServices.NativeLibrary]::Load($installedPlugin)
    try {
        Run-Installer $newInstaller '02-locked-update' @('/CURRENTUSER') -ExpectFailure
        Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $oldHash) 'failed locked update preserves old bytes'
        Assert-Check ((Read-Product 'Version') -eq $PreviousVersion) 'failed locked update preserves registration'
        Run-Installer $uninstaller '03-locked-uninstall' -ExpectFailure
        Assert-Check ((Test-Path -LiteralPath $uninstaller) -and (Test-Path -LiteralPath $installedPlugin)) 'locked uninstall preserves plugin and uninstaller'
    } finally { [Runtime.InteropServices.NativeLibrary]::Free($module) }

    Run-Installer $newInstaller '04-move-blocked' @('/CURRENTUSER', ('/CLAPDIR="' + $otherDir + '"')) -ExpectFailure
    Assert-Check (-not (Test-Path -LiteralPath $otherDir)) 'update does not create a duplicate location'
    Run-Installer $newInstaller '05-upgrade-remembered-folder' @('/CURRENTUSER')
    Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $currentHash) 'upgrade replaces actual old payload with current payload'
    Assert-Check ((Read-Product 'Version') -eq $currentVersion) 'upgrade registers current version'
    Assert-Check ((Read-Product 'PluginDir') -eq $pluginDir) 'upgrade remembers original directory without CLAPDIR'
    Run-Installer $newInstaller '06-same-version-repair' @('/CURRENTUSER')
    Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $currentHash) 'same-version repair works'
    Run-Installer $oldInstaller '07-downgrade-blocked' @('/CURRENTUSER') -ExpectFailure
    Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $currentHash) 'downgrade leaves new payload unchanged'
    Run-Installer $uninstaller '08-uninstall'
    Assert-Check (-not (Test-Path -LiteralPath $installedPlugin)) 'uninstall removes owned CLAP file'
    Assert-Check ((Get-FileHash -LiteralPath $sentinel).Hash -eq $sentinelHash) 'uninstall preserves unrelated plugin and shared directory'
    Assert-Check ($null -eq (Read-Product 'PluginDir')) 'uninstall removes own product registration'
    $arp = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($arpKey)
    Assert-Check ($null -eq $arp) 'uninstall removes Windows Installed Apps entry'
    if ($arp) { $arp.Dispose() }

    # A silent install must not silently adopt an unmanaged same-name file.
    Copy-Item -LiteralPath $PreviousPluginPath -Destination $installedPlugin
    Run-Installer $newInstaller '09-unmanaged-file-blocked' @('/CURRENTUSER', ('/CLAPDIR="' + $pluginDir + '"')) -ExpectFailure
    Assert-Check ((Get-FileHash -LiteralPath $installedPlugin).Hash -eq $oldHash) 'unmanaged payload left intact'
    Remove-Item -LiteralPath $installedPlugin
    Run-Installer $newInstaller '10-reinstall' @('/CURRENTUSER', ('/CLAPDIR="' + $pluginDir + '"'))
    Run-Installer $uninstaller '11-uninstall-reinstalled'
    $afterSettingsHash = if (Test-Path -LiteralPath $sharedSettings) { (Get-FileHash -LiteralPath $sharedSettings).Hash } else { $null }
    Assert-Check ($settingsHash -eq $afterSettingsHash) 'actual AflocatAudio shared preferences preserved'
    $result = [ordered]@{ passed = $true; checks = $checks; test_id = $testId; current_version = $currentVersion; previous_version = $PreviousVersion; plugin_sha256 = $currentHash; run_root = $runRoot }
    $result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runRoot 'results.json') -Encoding utf8
    Write-Output "Installer lifecycle evidence: $runRoot"
} finally {
    # Clean registration through the product's own uninstaller, never a registry
    # wildcard. Retain package/log/sentinel files for diagnosing any failure.
    if ((Read-Product 'PluginDir') -and (Test-Path -LiteralPath $uninstaller)) {
        Run-Installer $uninstaller 'cleanup'
    }
}
