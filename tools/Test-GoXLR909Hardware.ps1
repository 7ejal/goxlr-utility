# User-run exact-image update and native 47-field test, retaining the live microphone baseline.
param(
    [Parameter(Mandatory)][string]$BackupDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$NativeTestRunner,
    [Parameter(Mandatory)][string]$RecoveryReader,
    [string]$InstalledDaemon = 'C:/Program Files/GoXLR Utility/goxlr-daemon.exe',
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
function Status {
    (Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body '"GetStatus"' -TimeoutSec 10).Status
}
function Check-Microphone($reference, $actual) {
    if ($reference.Count -ne 36 -or $actual.Count -ne 36) { throw 'Incomplete microphone snapshot.' }
    for ($i = 0; $i -lt 36; $i++) {
        if ($reference[$i].Band -ne $actual[$i].Band -or $reference[$i].Field -ne $actual[$i].Field -or
            $reference[$i].Raw -ne $actual[$i].Raw) { throw 'Microphone baseline differs; stop further edits.' }
    }
}
$BackupDirectory = (Resolve-Path -LiteralPath $BackupDirectory).Path
$NativeTestRunner = (Resolve-Path -LiteralPath $NativeTestRunner).Path
$RecoveryReader = (Resolve-Path -LiteralPath $RecoveryReader).Path
$manifest = Get-Content -Encoding UTF8 -Raw (Join-Path $PSScriptRoot '../native-mic-eq/BUILD-909.json') | ConvertFrom-Json
$firmware = Join-Path $BackupDirectory 'firmware-909.bin'
if ((Get-FileHash $firmware).Hash -ne $manifest.sha256 -or
    (Get-FileHash $InstalledDaemon).Hash -ne $manifest.windows_daemon_sha256 -or
    (Get-FileHash $NativeTestRunner).Hash -ne $manifest.native_47_field_test_sha256) {
    throw 'The authorized firmware, installed daemon or native test runner hash differs.'
}
$hashes = Get-Content -Encoding UTF8 -Raw (Join-Path $BackupDirectory 'SHA256.json') | ConvertFrom-Json
foreach ($file in $hashes) {
    if ((Get-FileHash -LiteralPath (Join-Path $BackupDirectory $file.Path)).Hash -ne $file.SHA256) {
        throw 'A backup file hash differs.'
    }
}
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory.' }
if ($DryRun) { 'Verified backups and installed daemon; would request user confirmation before updating and testing all 47 fields.'; return }
$output = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
Start-Transcript -Path (Join-Path $output 'hardware-test.log') | Out-Null
try {
    if (!(Get-Process -Name goxlr-daemon -ErrorAction SilentlyContinue)) {
        Start-Process -FilePath $InstalledDaemon -WindowStyle Hidden | Out-Null
    }
    $status = $null
    for ($i = 0; $i -lt 100; $i++) {
        try {
            $candidate = Status
            if (@($candidate.mixers.PSObject.Properties).Count -eq 1) { $status = $candidate; break }
        } catch {}
        Start-Sleep -Milliseconds 200
    }
    if (!$status) { throw 'The installed daemon did not become ready.' }
    $device = @($status.mixers.PSObject.Properties)
    $version = $device[0].Value.hardware.versions.firmware -join '.'
    if ($version -notin @('1.5.1.908', '1.5.1.909')) { throw 'Expected 908 or the approved 909; stop on fallback.' }
    $reference = Get-Content -Encoding UTF8 -Raw (Join-Path $BackupDirectory 'dsp-baseline-36.json') | ConvertFrom-Json
    $before = (New-Item -ItemType Directory -Path (Join-Path $output 'before')).FullName
    & (Join-Path $PSScriptRoot 'Test-GoXLRNativeEq.ps1') -OutputDirectory $before -ReadOnly
    Check-Microphone $reference (Get-Content -Encoding UTF8 -Raw (Join-Path $before 'dsp-baseline.json') | ConvertFrom-Json)
    if ($version -eq '1.5.1.908') {
        Write-Host "Exact authorized 909 SHA256: $($manifest.sha256)"
        if ((Read-Host 'Type 909 to start your firmware update, or anything else to cancel') -cne '909') {
            throw 'Update cancelled; no firmware written.'
        }
        & (Join-Path $PSScriptRoot 'Read-GoXLRRecovery.ps1') -OriginalDaemon $InstalledDaemon -ResearchDaemon $InstalledDaemon `
            -RecoveryReader $RecoveryReader -Firmware $firmware -OutputDirectory (Join-Path $output 'firmware-update')
    }
    $after = (New-Item -ItemType Directory -Path (Join-Path $output 'after')).FullName
    & (Join-Path $PSScriptRoot 'Test-GoXLRNativeEq.ps1') -OutputDirectory $after -ReadOnly
    Check-Microphone $reference (Get-Content -Encoding UTF8 -Raw (Join-Path $after 'dsp-baseline.json') | ConvertFrom-Json)
    & (Join-Path $PSScriptRoot 'Test-GoXLRNativeEq.ps1') -OutputDirectory $after -ReadOnly `
        -NativeTestRunner $NativeTestRunner -AllParametersRoundTrip
    $status = Status
    $status | ConvertTo-Json -Depth 100 | Set-Content (Join-Path $output 'status-after.json')
    if (($status.mixers.PSObject.Properties.Value.hardware.versions.firmware -join '.') -ne '1.5.1.909') {
        throw 'Unexpected firmware after testing.'
    }
    'PASS all 47 physical parameter write/read/restore checks. Acoustic processing and headphone DAC isolation are separate tests.'
} finally {
    $profiles = Get-Content -Encoding UTF8 -Raw (Join-Path $BackupDirectory 'original-active-profiles.json') | ConvertFrom-Json
    foreach ($file in $profiles) {
        # Keep the pre-update live settings persistent; original saved files remain in the backup.
        $liveName = 'live-' + $file.Backup
        $liveCopy = Join-Path $BackupDirectory $liveName
        $liveHash = @($hashes | Where-Object Path -eq $liveName)
        if ($liveHash.Count -ne 1 -or (Get-FileHash $liveCopy).Hash -ne $liveHash[0].SHA256) {
            throw 'Live profile backup hash differs.'
        }
        Copy-Item -LiteralPath $liveCopy -Destination $file.Source -Force
        if ((Get-FileHash $file.Source).Hash -ne $liveHash[0].SHA256) { throw 'Live saved-profile restoration failed.' }
    }
    Stop-Transcript | Out-Null
}
