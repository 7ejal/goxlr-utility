# Measure one live dynamics edit with quiet playback and complete 47-field restoration.
#Requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][ValidateSet('DeesserEnabled','DeesserAmount','DeesserAttack','DeesserRelease','DeesserMode','DeesserScalar','CompressorRatio','CompressorThreshold','CompressorAttack','CompressorRelease','CompressorMakeup')][string]$Control,
    [Parameter(Mandatory)][int]$TestRaw,
    [ValidateRange(100,10000)][double]$Frequency = 1000,
    [ValidateRange(0.001,0.008)][double]$Amplitude = 0.008,
    [ValidateRange(2,20)][int]$SettlingSeconds = 2,
    [string]$Playback = 'Voicemeeter AUX Input',
    [string]$Capture = 'Chat Mic (TC-HELICON GoXLR)',
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$FixtureConfirmed,
    [switch]$LowerGateForMeasurement,
    [switch]$ManualMakeup,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$captures = [ordered]@{}
function Request($body) {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (ConvertTo-Json -InputObject $body -Depth 12 -Compress) -TimeoutSec 10
    if ($reply.Error) { throw $reply.Error }
    $reply
}
function Snapshot {
    $values = [ordered]@{}
    foreach ($band in 1..12) {
        foreach ($field in 'Frequency','Gain','Width') {
            $value = (Request @{ReadMicEqParameter=@($serial,$band,$field)}).MicEqParameter
            if ($null -eq $value) { throw 'Missing EQ readback.' }
            $values["$band/$field"] = [int]$value
        }
    }
    foreach ($name in 'DeesserEnabled','DeesserAmount','DeesserAttack','DeesserRelease','DeesserMode','DeesserScalar','CompressorRatio','CompressorThreshold','CompressorAttack','CompressorRelease','CompressorMakeup') {
        $value = (Request @{ReadDynamicsParameter=@($serial,$name)}).DynamicsParameter
        if ($null -eq $value) { throw 'Missing dynamics readback.' }
        $values[$name] = [int]$value
    }
    $values
}
function Check-Snapshot($actual, [int]$expectedRaw) {
    foreach ($key in $baseline.Keys) {
        $expected = if ($key -eq $Control) { $expectedRaw } else { $baseline[$key] }
        if ($actual[$key] -ne $expected) { throw "Unexpected live change: $key" }
    }
}
function Set-Value([int]$value, [int]$expected, [string]$name = $Control) {
    $current = (Request 'GetStatus').Status.mixers.$serial
    if ($current.mic_profile_name -ne $mixer.mic_profile_name -or $current.profile_name -ne $mixer.profile_name) { throw 'Profile changed; stop before writing.' }
    if ((Request @{ReadDynamicsParameter=@($serial,$name)}).DynamicsParameter -ne $expected) { throw 'Dynamics value changed before the test write.' }
    $command = if ($name -eq 'CompressorMakeup') {
        @{SetCompressorMakeupGain=$value}
    } else {
        @{SetDynamicsParameter=@{control=$name;raw=$value;expected_raw=$expected;profile_name=$mixer.mic_profile_name}}
    }
    if ((Request @{Command=@($serial,$command)}) -ne 'Ok') { throw 'Dynamics write failed.' }
}
function Capture-Level([string]$name) {
    $path = Join-Path $output "$name.f32"
    $level = @(Measure-Tones 0 $path)[0]
    $stats = [MicFixture]::SignalStats([IO.File]::ReadAllBytes($path),2,0)
    $captures[$name] = [ordered]@{ToneDb=$level;PeakDbFS=$stats[0];RmsDbFS=$stats[1];ClippedSamples=$stats[2]}
    if ($stats[2] -ne 0 -or $stats[0] -gt -3) { throw 'Insufficient recording headroom; stop calibration.' }
    [double]$level
}
$status = (Request 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1 -or $devices[0].Value.hardware.device_type -ne 'Full' -or ($devices[0].Value.hardware.versions.firmware -join '.') -notin @('1.5.1.909','1.5.1.910')) { throw 'Expected one full-size bridge device.' }
$serial = $devices[0].Name
$mixer = $devices[0].Value
if ($mixer.router.Microphone.Headphones -or $mixer.router.System.ChatMic) { throw 'Disable microphone monitoring/digital capture routing before acoustic calibration.' }
if (!$FixtureConfirmed) { throw 'The fixed headphone-to-mic fixture must be confirmed.' }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory.' }
$baseline = Snapshot
$original = [int]$baseline[$Control]
if ($TestRaw -eq $original) { throw 'The test must change the selected value.' }
if ($DryRun) { "Would test $Control $original -> $TestRaw -> $original at $Frequency Hz; no writes or audio."; return }
$output = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
$baseline | ConvertTo-Json | Set-Content (Join-Path $output 'baseline-47.json') -Encoding UTF8
[ordered]@{OriginalGateThreshold=$mixer.mic_status.noise_gate.threshold;MeasurementGateThreshold=$(if($LowerGateForMeasurement){-59}else{$mixer.mic_status.noise_gate.threshold});FixtureConfirmed=[bool]$FixtureConfirmed} | ConvertTo-Json | Set-Content (Join-Path $output 'gate-context.json') -Encoding UTF8
. (Join-Path $PSScriptRoot 'Test-GoXLRMicAudio.ps1') -LibraryOnly -Playback $Playback -Capture $Capture -CaptureChannels 2
[MicFixture]::SetTones([double[]]@($Frequency))
[MicFixture]::LoopCount = 120
$tone = $null
$changed = $false
$gateChanged = $false
try {
    if ($LowerGateForMeasurement -and $mixer.mic_status.noise_gate.threshold -ne -59) {
        $gateChanged = $true
        if ((Request @{Command=@($serial,@{SetGateThreshold=-59})}) -ne 'Ok') { throw 'Temporary gate threshold failed.' }
        if ((Request 'GetStatus').Status.mixers.$serial.mic_status.noise_gate.threshold -ne -59) { throw 'Temporary gate setting not confirmed.' }
    }
    if ($ManualMakeup) { Set-Value $baseline['CompressorMakeup'] $baseline['CompressorMakeup'] 'CompressorMakeup' }
    $quiet = Capture-Level 'quiet'
    $tone = [MicFixture]::Start($Playback,$Amplitude)
    Start-Sleep -Seconds $SettlingSeconds
    $before = Capture-Level 'before'
    $repeat = Capture-Level 'before-repeat'
    if ($before -lt -85 -or $before-$quiet -lt 20 -or [Math]::Abs($before-$repeat) -gt 1) { throw 'Acoustic baseline is too quiet or unstable; no edit performed.' }
    Check-Snapshot (Snapshot) $original
    $changed = $true
    Set-Value $TestRaw $original
    if ($ManualMakeup) { Set-Value $baseline['CompressorMakeup'] $baseline['CompressorMakeup'] 'CompressorMakeup' }
    Check-Snapshot (Snapshot) $TestRaw
    Start-Sleep -Seconds $SettlingSeconds
    $edited = Capture-Level 'edited'
    $editedRepeat = Capture-Level 'edited-repeat'
    Set-Value $original $TestRaw
    if ($ManualMakeup) { Set-Value $baseline['CompressorMakeup'] $baseline['CompressorMakeup'] 'CompressorMakeup' }
    $changed = $false
    Check-Snapshot (Snapshot) $original
    Start-Sleep -Seconds $SettlingSeconds
    $restored = Capture-Level 'restored'
    $repeatable = [Math]::Abs($restored-$before) -le 1 -and [Math]::Abs($edited-$editedRepeat) -le 1
    [ordered]@{Control=$Control;OriginalRaw=$original;TestRaw=$TestRaw;Hz=$Frequency;Amplitude=$Amplitude;SettlingSeconds=$SettlingSeconds;ContinuousPlayback=$true;Quiet=$quiet;Before=$before;BeforeRepeat=$repeat;Edited=$edited;EditedRepeat=$editedRepeat;Restored=$restored;DeltaDb=($edited-$before);All47Restored=$true;AcousticAccepted=$repeatable} | ConvertTo-Json | Tee-Object -FilePath (Join-Path $output 'measurements.json')
    if (!$repeatable) { throw 'Acoustic edit/restoration did not repeat within 1 dB.' }
} finally {
    if ($tone) { $tone.Dispose() }
    try {
        if ($changed) {
            $current = (Request @{ReadDynamicsParameter=@($serial,$Control)}).DynamicsParameter
            if ($null -ne $current -and $current -eq $TestRaw) { Set-Value $original $TestRaw }
            elseif ($null -eq $current -or $current -ne $original) { throw 'Current value is unknown or changed externally; stop before overwriting it.' }
        }
        if ($ManualMakeup) { Set-Value $baseline['CompressorMakeup'] $baseline['CompressorMakeup'] 'CompressorMakeup' }
        Check-Snapshot (Snapshot) $original
    } finally {
        if ($gateChanged) {
            $current = (Request 'GetStatus').Status.mixers.$serial
            if ($current.mic_profile_name -ne $mixer.mic_profile_name -or $current.profile_name -ne $mixer.profile_name) { throw 'Profile changed; do not overwrite its gate setting.' }
            if ($current.mic_status.noise_gate.threshold -in @(-59,$mixer.mic_status.noise_gate.threshold)) {
                if ((Request @{Command=@($serial,@{SetGateThreshold=$mixer.mic_status.noise_gate.threshold})}) -ne 'Ok') { throw 'Gate restoration failed.' }
            }
            if ((Request 'GetStatus').Status.mixers.$serial.mic_status.noise_gate.threshold -ne $mixer.mic_status.noise_gate.threshold) { throw 'Original gate threshold was not restored.' }
            "PASS original gate threshold restored: $($mixer.mic_status.noise_gate.threshold) dB"
        }
        $captures | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output 'captures.json') -Encoding UTF8
    }
}
