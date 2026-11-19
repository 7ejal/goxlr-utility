# Preserves the 908 installation and unsaved live settings before the approved 909 test.
param(
    [Parameter(Mandatory)][string]$BackupDirectory,
    [Parameter(Mandatory)][string]$Firmware908,
    [Parameter(Mandatory)][string]$Firmware909,
    [string]$ApplicationDirectory = 'C:/Users/real5/Documents/ChatGPT/Goxlr/GoXLR-Native-Mic-EQ',
    [string]$InstalledDaemon = 'C:/Program Files/GoXLR Utility/goxlr-daemon.exe',
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
function Request($body) {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (
        ConvertTo-Json -InputObject $body -Depth 30 -Compress
    ) -TimeoutSec 10
    if ($reply.Error) { throw $reply.Error }
    $reply
}
function Snapshot($serial) {
    foreach ($band in 1..12) {
        foreach ($field in 'Frequency', 'Gain', 'Width') {
            $first = (Request @{ ReadMicEqParameter = @($serial, $band, $field) }).MicEqParameter
            $second = (Request @{ ReadMicEqParameter = @($serial, $band, $field) }).MicEqParameter
            if ($null -eq $first -or $first -ne $second) { throw 'Unstable microphone readback; stopping.' }
            [pscustomobject]@{ Band = $band; Field = $field; Raw = [int]$first }
        }
    }
}
$status = (Request 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1 -or ($devices[0].Value.hardware.versions.firmware -join '.') -ne '1.5.1.908') {
    throw 'Exactly one running 908 device is required.'
}
$serial = $devices[0].Name
$mixer = $devices[0].Value
if ((Get-FileHash $Firmware908).Hash -ne 'C8607989063552E5A412AD3B29B4E1DB355D9B737DCC59D972A5349FBFAA970F' -or
    (Get-FileHash $Firmware909).Hash -ne '74555B3667DCB7150930AE544BDFCF732FEBF7BB6A0AA9B7CFD9B513FFA3C74B') {
    throw 'The recorded firmware image hashes do not match.'
}
if (Test-Path -LiteralPath $BackupDirectory) { throw 'Choose a new backup directory.' }
$baseline = @(Snapshot $serial)
if ($DryRun) { 'Would back up original files, capture 36 fields, and save live profiles for restart.'; return }
$backup = (New-Item -ItemType Directory -Path $BackupDirectory).FullName
Copy-Item -LiteralPath $Firmware908 -Destination (Join-Path $backup 'firmware-908.bin')
Copy-Item -LiteralPath $Firmware909 -Destination (Join-Path $backup 'firmware-909.bin')
Copy-Item -LiteralPath $InstalledDaemon -Destination (Join-Path $backup 'goxlr-daemon.exe')
Copy-Item -LiteralPath (Split-Path $InstalledDaemon) -Destination (Join-Path $backup 'utility-installation') -Recurse
Copy-Item -LiteralPath $ApplicationDirectory -Destination (Join-Path $backup 'native-application') -Recurse
Copy-Item -LiteralPath $status.paths.profile_directory -Destination (Join-Path $backup 'profiles') -Recurse
Copy-Item -LiteralPath $status.paths.mic_profile_directory -Destination (Join-Path $backup 'mic-profiles') -Recurse
$config = Join-Path (Split-Path (Split-Path $status.paths.profile_directory)) 'config'
if (Test-Path -LiteralPath $config) {
    Copy-Item -LiteralPath $config -Destination (Join-Path $backup 'config') -Recurse
}
$status | ConvertTo-Json -Depth 100 | Set-Content -Encoding UTF8 (Join-Path $backup 'status-before.json')
$baseline | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $backup 'dsp-baseline-36.json')
$active = @(
    @{ Source = Join-Path $status.paths.profile_directory ($mixer.profile_name + '.goxlr'); Backup = 'original-mixer.goxlr' },
    @{ Source = Join-Path $status.paths.mic_profile_directory ($mixer.mic_profile_name + '.goxlrMicProfile'); Backup = 'original-mic.goxlrMicProfile' }
)
foreach ($file in $active) {
    $file.Copy = Join-Path $backup $file.Backup
    Copy-Item -LiteralPath $file.Source -Destination $file.Copy
    $file.Hash = (Get-FileHash $file.Copy).Hash
}
$active | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $backup 'original-active-profiles.json')
try {
    foreach ($command in 'SaveProfile', 'SaveMicProfile') {
        $operation = @{}; $operation[$command] = @()
        if ((Request @{ Command = @($serial, $operation) }) -ne 'Ok') { throw 'Saving live settings failed.' }
    }
    foreach ($file in $active) {
        Copy-Item -LiteralPath $file.Source -Destination (Join-Path $backup ('live-' + $file.Backup))
    }
    if (($baseline | ConvertTo-Json -Compress) -ne (@(Snapshot $serial) | ConvertTo-Json -Compress)) {
        throw 'Live microphone settings changed during saving.'
    }
} catch {
    foreach ($file in $active) { Copy-Item -LiteralPath $file.Copy -Destination $file.Source -Force }
    throw
}
Get-ChildItem -LiteralPath $backup -File -Recurse | ForEach-Object {
    [pscustomobject]@{ Path = $_.FullName.Substring($backup.Length + 1); SHA256 = (Get-FileHash $_.FullName).Hash }
} | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $backup 'SHA256.json')
'Backed up 908 image, installed executables, all profiles and 36 live values; live profiles saved for restart.'
