# Installs an exact official Go Windows SDK into a new lab directory after SHA256 verification.
param(
    [Parameter(Mandatory)][ValidatePattern('^go1\.[0-9]+\.[0-9]+$')][string]$Version,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new SDK directory; existing toolchains are preserved.' }
$versions = Invoke-RestMethod 'https://go.dev/dl/?mode=json&include=all' -TimeoutSec 60
$release = @($versions | Where-Object version -eq $Version)
if ($release.Count -ne 1) { throw 'Exact Go version is absent from the official download manifest.' }
$file = @($release[0].files | Where-Object { $_.os -eq 'windows' -and $_.arch -eq 'amd64' -and $_.kind -eq 'archive' })
if ($file.Count -ne 1 -or $file[0].sha256 -notmatch '^[0-9a-f]{64}$' -or
    $file[0].filename -ne "$Version.windows-amd64.zip") { throw 'Unexpected Go archive metadata.' }
$archive = [IO.Path]::GetFullPath($OutputDirectory) + '.zip'
if (!(Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest ("https://go.dev/dl/" + $file[0].filename) -OutFile $archive -TimeoutSec 300
}
if ((Get-FileHash -LiteralPath $archive).Hash -ne $file[0].sha256) { throw 'Official Go archive SHA256 mismatch.' }
Expand-Archive -LiteralPath $archive -DestinationPath $OutputDirectory
$compiler = Join-Path $OutputDirectory 'go/bin/go.exe'
& $compiler version
if ($LASTEXITCODE) { throw 'Installed Go compiler did not run.' }
$compiler
