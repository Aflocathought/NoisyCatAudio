param()

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$toolRoot = Join-Path $repoRoot 'target/installer-tools'
$compilerDir = Join-Path $toolRoot 'inno-6.7.3'
$download = Join-Path $toolRoot 'innosetup-6.7.3.exe'
$expectedHash = '9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732'
New-Item -ItemType Directory -Force -Path $toolRoot | Out-Null
if (-not (Test-Path -LiteralPath $download) -or (Get-FileHash -LiteralPath $download).Hash -ne $expectedHash) {
    # Fixed official release asset, never "latest" or a third-party mirror.
    Invoke-WebRequest -UseBasicParsing -Uri 'https://api.github.com/repos/jrsoftware/issrc/releases/assets/430321419' -Headers @{ Accept = 'application/octet-stream' } -OutFile $download
}
if ((Get-FileHash -LiteralPath $download).Hash -ne $expectedHash) { throw 'Inno Setup SHA-256 mismatch; refusing to execute' }
$signature = Get-AuthenticodeSignature -LiteralPath $download
if ($signature.Status -ne 'Valid') { throw "Compiler signature is not valid: $($signature.Status)" }

$runtime = Join-Path $toolRoot 'vc_redist.x64.exe'
$runtimeHash = 'CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B'
if (-not (Test-Path -LiteralPath $runtime) -or (Get-FileHash -LiteralPath $runtime).Hash -ne $runtimeHash) {
    Invoke-WebRequest -UseBasicParsing -Uri 'https://download.visualstudio.microsoft.com/download/pr/bd1c8d9d-ba95-4eee-bc6e-df1fcc876373/CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe' -OutFile $runtime
}
if ((Get-FileHash -LiteralPath $runtime).Hash -ne $runtimeHash) { throw 'VC runtime SHA-256 mismatch' }
$runtimeSignature = Get-AuthenticodeSignature -LiteralPath $runtime
if ($runtimeSignature.Status -ne 'Valid' -or $runtimeSignature.SignerCertificate.Subject -notmatch 'CN=Microsoft Corporation,') {
    throw 'VC runtime does not have a valid Microsoft signature'
}

# Official portable mode creates no uninstall entry or file associations. The
# compiler stays under ignored target/, not in the user's installed programs.
$arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/PORTABLE=1', ('/DIR="' + $compilerDir + '"'), ('/LOG="' + (Join-Path $toolRoot 'compiler-extract.log') + '"'))
$process = Start-Process -FilePath $download -ArgumentList $arguments -WindowStyle Hidden -PassThru
if (-not $process.WaitForExit(60000)) { throw 'Portable compiler extraction timed out; inspect compiler-extract.log' }
if ($process.ExitCode -ne 0) { throw "Portable compiler extraction failed: $($process.ExitCode)" }
Write-Output "Installer compiler: $(Join-Path $compilerDir 'ISCC.exe')"
