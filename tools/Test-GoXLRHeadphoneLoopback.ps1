# Reversible stereo headphone-to-Line-In measurement. Restore only after unplugging the cable.
param(
    [Parameter(Mandatory)][ValidateSet('Isolate','Measure','Restore')][string]$Stage,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [ValidateRange(0.0005,0.01)][double]$Amplitude = 0.002,
    [switch]$CableDisconnected,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
function Send($body) {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (
        ConvertTo-Json -InputObject $body -Depth 20 -Compress) -TimeoutSec 10
    if ($reply.Error) { throw $reply.Error }
    $reply
}
function Command($body) {
    if ((Send @{Command=@($serial,$body)}) -ne 'Ok') { throw 'GoXLR command failed.' }
}
$status = (Send 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1 -or $devices[0].Value.hardware.device_type -ne 'Full') { throw 'One full-size GoXLR is required.' }
$serial = $devices[0].Name
$live = $devices[0].Value
$snapshotPath = Join-Path $OutputDirectory 'status-before.json'
if ($DryRun) { "Would perform $Stage; no routing or volume writes."; return }
if ($Stage -eq 'Isolate') {
    if (Test-Path -LiteralPath $snapshotPath) { throw 'Use a new output directory; preserve the prior snapshot.' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $status | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $snapshotPath
    # Remove the analog feedback path before changing any recording routes.
    Command @{SetRouter=@('LineIn','Headphones',$false)}
    foreach ($output in $live.router.LineIn.PSObject.Properties.Name) {
        if ($output -ne 'Headphones') { Command @{SetRouter=@('LineIn',$output,($output -eq 'Sampler'))} }
    }
    foreach ($input in $live.router.PSObject.Properties.Name) {
        if ($input -ne 'LineIn') { Command @{SetRouter=@($input,'Sampler',$false)} }
    }
    foreach ($output in 'Headphones','BroadcastMix','LineOut','StreamMix2') {
        Command @{SetRouter=@('Game',$output,$true)}
    }
    $isolated = (Send 'GetStatus').Status.mixers.$serial
    if ($isolated.router.LineIn.Headphones -or $isolated.router.Game.Sampler) { throw 'Loopback isolation failed.' }
    $isolated.router | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $OutputDirectory 'isolated-router.json')
    'Line In isolated from playback/streams; only its physical return feeds the Sample recording endpoint.'
    return
}
$before = Get-Content -Raw -LiteralPath $snapshotPath | ConvertFrom-Json
$original = $before.mixers.$serial
if (!$original -or $live.profile_name -ne $original.profile_name -or $live.mic_profile_name -ne $original.mic_profile_name) {
    throw 'The device or profile changed; do not overwrite the new state.'
}
if ($Stage -eq 'Restore') {
    if (!$CableDisconnected) { throw 'Disconnect the loopback cable before restoring monitored Line In.' }
    foreach ($input in $original.router.PSObject.Properties) {
        foreach ($output in $input.Value.PSObject.Properties) {
            if ($live.router.($input.Name).($output.Name) -ne $output.Value) {
                Command @{SetRouter=@($input.Name,$output.Name,[bool]$output.Value)}
            }
        }
    }
    Command @{SetVolume=@('Headphones',[int]$original.levels.volumes.Headphones)}
    $restored = (Send 'GetStatus').Status.mixers.$serial
    foreach ($input in $original.router.PSObject.Properties) {
        foreach ($output in $input.Value.PSObject.Properties) {
            if ($restored.router.($input.Name).($output.Name) -ne $output.Value) { throw 'Routing restoration failed.' }
        }
    }
    if ($restored.levels.volumes.Headphones -ne $original.levels.volumes.Headphones) { throw 'Volume restoration failed.' }
    'PASS original routing and headphone volume restored.'
    return
}
if ($live.router.LineIn.Headphones -or $live.router.LineIn.BroadcastMix -or $live.router.LineIn.StreamMix2 -or
    $live.router.Game.Sampler -or !$live.router.LineIn.Sampler) { throw 'Unsafe or ambiguous loopback routing; isolate first.' }
. (Join-Path $PSScriptRoot 'Test-GoXLRMicAudio.ps1') -LibraryOnly -CaptureChannels 2 -ToneAmplitude $Amplitude
[MicFixture]::SetTones(@(1000.0,1500.0))
$Capture = 'Sample (TC-HELICON GoXLR)'
$quiet = @(@(Measure-Tones 0),@(Measure-Tones 1))
$tone = [MicFixture]::Start('Game (TC-HELICON GoXLR)', $Amplitude, $true)
$result = [ordered]@{Amplitude=$Amplitude; Quiet=$quiet}
try {
    $Capture = 'Stream Mix 1 (TC-HELICON GoXLR)'
    $result.DigitalPlaybackControl = @(Measure-Tones 0)
    $Capture = 'Sample (TC-HELICON GoXLR)'
    $baseline = @(@(Measure-Tones 0),@(Measure-Tones 1))
    $repeat = @(@(Measure-Tones 0),@(Measure-Tones 1))
    $result.Baseline = $baseline; $result.Repeat = $repeat
    foreach ($channel in 0..1) {
        if ($baseline[$channel][$channel] - $quiet[$channel][$channel] -lt 20 -or
            [math]::Abs($repeat[$channel][$channel] - $baseline[$channel][$channel]) -gt 1) {
            throw 'No stable physical loopback signal; stop before changing headphone volume.'
        }
    }
    $digital = @{}
    foreach ($endpoint in 'Stream Mix 1','Stream Mix 2','Chat Mic') {
        $Capture = "$endpoint (TC-HELICON GoXLR)"
        $digital[$endpoint] = @(Measure-Tones 0)
    }
    Command @{SetVolume=@('Headphones',0)}
    Start-Sleep -Milliseconds 400
    $Capture = 'Sample (TC-HELICON GoXLR)'
    $result.HeadphoneMuted = @(@(Measure-Tones 0),@(Measure-Tones 1))
    $result.DigitalBefore = $digital
    $result.DigitalMuted = @{}
    foreach ($endpoint in 'Stream Mix 1','Stream Mix 2','Chat Mic') {
        $Capture = "$endpoint (TC-HELICON GoXLR)"
        $result.DigitalMuted[$endpoint] = @(Measure-Tones 0)
    }
    foreach ($channel in 0..1) {
        if ($baseline[$channel][$channel] - $result.HeadphoneMuted[$channel][$channel] -lt 20) { throw 'Headphone attenuation did not isolate the physical return.' }
    }
    foreach ($endpoint in 'Stream Mix 1','Stream Mix 2') {
        if ([math]::Abs($digital[$endpoint][0] - $result.DigitalMuted[$endpoint][0]) -gt 0.5) { throw 'A digital stream changed with headphone volume.' }
    }
    $result.Status = 'PASS physical headphone attenuation isolated from both digital stream mixes; Line Out not physically captured.'
} finally {
    $tone.Dispose()
    Command @{SetVolume=@('Headphones',[int]$original.levels.volumes.Headphones)}
    $result | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $OutputDirectory ("measurements-"+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'.json'))
}
$result.Status
'Disconnect the loopback cable, then run Restore -CableDisconnected.'
