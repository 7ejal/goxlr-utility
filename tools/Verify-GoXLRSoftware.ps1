# Read-only post-install verification against the live values saved before installation.
param(
    [Parameter(Mandatory)][string]$BackupDirectory,
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [string]$InstalledDaemon = 'C:/Program Files/GoXLR Utility/goxlr-daemon.exe',
    [string]$Application = 'C:/Users/real5/Documents/ChatGPT/Goxlr/GoXLR-Native-Mic-EQ/GoXLR-Native-Mic-EQ.exe',
    [Parameter(Mandatory)][string]$DaemonSha256,
    [Parameter(Mandatory)][string]$ApplicationSha256
)
$ErrorActionPreference = 'Stop'
$InstalledDaemon = (Resolve-Path -LiteralPath $InstalledDaemon).Path
$Application = (Resolve-Path -LiteralPath $Application).Path
function Request($body) {
    Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (
        ConvertTo-Json -InputObject $body -Depth 10 -Compress
    ) -TimeoutSec 10
}
function SettingLeaves($value, [string]$path = '') {
    if ($value -is [pscustomobject]) {
        foreach ($property in $value.PSObject.Properties) { SettingLeaves $property.Value "$path/$($property.Name)" }
    } else { "$path=$value" }
}
foreach ($item in @(@($InstalledDaemon, $DaemonSha256), @($Application, $ApplicationSha256))) {
    if ((Get-FileHash -LiteralPath $item[0]).Hash -ne $item[1]) { throw 'Installed executable hash differs.' }
    if (@(Get-Process | Where-Object Path -eq $item[0]).Count -ne 1) { throw 'Expected one running installed executable.' }
}
$before = Get-Content -LiteralPath (Join-Path $BackupDirectory 'status-before.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$status = (Request 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1) { throw 'Expected exactly one GoXLR.' }
$serial = $devices[0].Name
$mixer = $devices[0].Value
$original = $before.mixers.$serial
if (!$original -or ($mixer.hardware.versions.firmware -join '.') -ne '1.5.1.909' -or
    $mixer.mic_profile_name -ne '4' -or $mixer.mic_status.noise_gate.threshold -ne -52 -or
    (Compare-Object @(SettingLeaves $mixer.mic_status | Sort-Object) @(SettingLeaves $original.mic_status | Sort-Object))) {
    throw 'Firmware, profile or stock microphone settings differ from the preserved baseline.'
}
$count = 0
foreach ($field in (Get-Content -LiteralPath (Join-Path $BackupDirectory 'dsp-baseline-36.json') -Raw -Encoding UTF8 | ConvertFrom-Json)) {
    $reply = Request @{ ReadMicEqParameter = @($serial, $field.Band, $field.Field) }
    if ($null -eq $reply.MicEqParameter -or $reply.MicEqParameter -ne $field.Raw) { throw "EQ mismatch: band $($field.Band) $($field.Field)" }
    $count++
}
foreach ($field in (Get-Content -LiteralPath (Join-Path $BackupDirectory 'dynamics-baseline-11.json') -Raw -Encoding UTF8 | ConvertFrom-Json)) {
    $reply = Request @{ ReadDynamicsParameter = @($serial, $field.Control) }
    if ($null -eq $reply.DynamicsParameter -or $reply.DynamicsParameter -ne $field.Raw) { throw "Dynamics mismatch: $($field.Control)" }
    $count++
}
if ($count -ne 47) { throw 'Incomplete baseline.' }
$diagnostic = Request @{ ReadArmDiagnostic = @($serial, 'CpuAudioBit22Count') }
if ($diagnostic.Error -notmatch '910') { throw '909 did not reject the 910-only diagnostic.' }
'PASS installed executable hashes and running processes; firmware 909, profile 4, gate -52 dB, stock mic settings and all 47 DSP values preserved; 910-only diagnostics rejected.'
