# Verifies the real recording fixture before dynamics/EQ calibration; restores its single temporary change.
#Requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateSet('Baseline','GateBypass','DigitalControl')][string]$Mode = 'Baseline',
    [string]$Playback = 'Voicemeeter AUX Input',
    [string]$Capture = 'Chat Mic (TC-HELICON GoXLR)',
    [ValidateRange(0.001,0.008)][double]$Amplitude = 0.004,
    [ValidateRange(100,10000)][double]$Frequency = 1000,
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$FixtureConfirmed,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
function Request($body) {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (
        ConvertTo-Json -InputObject $body -Depth 15 -Compress
    ) -TimeoutSec 10
    if ($reply.Error) { throw $reply.Error }
    $reply
}
$status = (Request 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1 -or $devices[0].Value.hardware.device_type -ne 'Full') { throw 'Expected one full-size GoXLR.' }
$serial = $devices[0].Name
$mixer = $devices[0].Value
if ($mixer.router.Microphone.Headphones) { throw 'Microphone monitoring must be off for the acoustic fixture.' }
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory.' }
if ($DryRun) { "Would measure $Frequency Hz at amplitude $Amplitude; mode $Mode with restoration."; return }
if (!$FixtureConfirmed) { throw 'Confirm that the headphone earcup is fixed beside the microphone, not being worn.' }
$output = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
$status | ConvertTo-Json -Depth 100 | Set-Content (Join-Path $output 'status-before.json') -Encoding UTF8
. (Join-Path $PSScriptRoot 'Test-GoXLRMicAudio.ps1') -LibraryOnly -Playback $Playback -Capture $Capture -CaptureChannels 2 -ToneAmplitude $Amplitude
[MicFixture]::SetTones([double[]]@($Frequency))
$tone = $null
$changed = $false
try {
    $command = switch ($Mode) {
        'GateBypass' { @{SetGateActive=$false} }
        'DigitalControl' { @{SetRouter=@('System','ChatMic',$true)} }
    }
    if ($command) {
        $changed = $true
        if ((Request @{Command=@($serial,$command)}) -ne 'Ok') { throw 'Fixture setup failed.' }
    }
    $quietPath = Join-Path $output 'quiet-stereo.f32'
    $quiet = @(Measure-Tones 0 $quietPath)
    $tone = [MicFixture]::Start($Playback,$Amplitude)
    Start-Sleep -Milliseconds 600
    $firstPath = Join-Path $output 'first-stereo.f32'
    $secondPath = Join-Path $output 'second-stereo.f32'
    $first = @(Measure-Tones 0 $firstPath)
    $second = @(Measure-Tones 0 $secondPath)
    $result = [ordered]@{Mode=$Mode;Hz=$Frequency;Amplitude=$Amplitude;Playback=$Playback;Capture=$Capture;Channels=@()}
    foreach ($channel in 0..1) {
        $firstPcm = [IO.File]::ReadAllBytes($firstPath)
        $secondPcm = [IO.File]::ReadAllBytes($secondPath)
        $q = [MicFixture]::Levels([IO.File]::ReadAllBytes($quietPath),$sampleCount,2,$channel)[0]
        $a = [MicFixture]::Levels($firstPcm,$sampleCount,2,$channel)[0]
        $b = [MicFixture]::Levels($secondPcm,$sampleCount,2,$channel)[0]
        $stats = [MicFixture]::SignalStats($firstPcm,2,$channel)
        $result.Channels += [ordered]@{Channel=$channel;Quiet=$q;First=$a;Second=$b;AboveQuiet=($a-$q);
            RepeatDifference=[Math]::Abs($a-$b);PeakDbFS=$stats[0];RmsDbFS=$stats[1];ClippedSamples=$stats[2]}
    }
    $result | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output 'measurements.json') -Encoding UTF8
    $result | ConvertTo-Json -Depth 6
} finally {
    if ($tone) { $tone.Dispose() }
    if ($changed) {
        $current = (Request 'GetStatus').Status.mixers.$serial
        if ($current.mic_profile_name -ne $mixer.mic_profile_name -or $current.profile_name -ne $mixer.profile_name) {
            throw 'Active profile changed; do not overwrite its settings.'
        }
        $restore = if ($Mode -eq 'GateBypass') {
            @{SetGateActive=[bool]$mixer.mic_status.noise_gate.enabled}
        } else { @{SetRouter=@('System','ChatMic',[bool]$mixer.router.System.ChatMic)} }
        if ((Request @{Command=@($serial,$restore)}) -ne 'Ok') { throw 'Fixture restoration failed.' }
        $restored = (Request 'GetStatus').Status.mixers.$serial
        if ($restored.mic_status.noise_gate.enabled -ne $mixer.mic_status.noise_gate.enabled -or
            $restored.router.System.ChatMic -ne $mixer.router.System.ChatMic) { throw 'Fixture restoration did not match.' }
    }
}
