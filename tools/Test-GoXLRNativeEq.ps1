# Reads every DSP EQ field before considering reversible physical measurements.
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$ReadOnly,
    [string]$NativeTestRunner,
    [switch]$NativeWriteRoundTrip,
    [switch]$ProfileRoundTrip,
    [ValidateRange(0.001,0.08)][double]$ToneAmplitude = 0.06,
    [switch]$ContinueAfterAudioFailure,
    [ValidateRange(1,12)][int[]]$Bands = @(6,11,12,1,2,3,4,5,7,8,9,10)
)
$ErrorActionPreference = 'Stop'
function Send-Request($body) {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body (
        ConvertTo-Json -InputObject $body -Depth 20 -Compress
    ) -TimeoutSec 10
    if ($reply.Error) { throw $reply.Error }
    $reply
}
$status = (Send-Request 'GetStatus').Status
$devices = @($status.mixers.PSObject.Properties)
if ($devices.Count -ne 1 -or $devices[0].Value.hardware.device_type -ne 'Full' -or
    ($devices[0].Value.hardware.versions.firmware -join '.') -ne '1.5.1.908') {
    throw 'Exactly one full-size GoXLR on experimental 908 is required.'
}
$device = $devices[0].Name
$profileName = $devices[0].Value.mic_profile_name
function Read-Parameter($band, $field) {
    $reply = Send-Request @{ ReadMicEqParameter = @($device, $band, $field) }
    if ($null -eq $reply.MicEqParameter) { throw 'Missing DSP readback value.' }
    [int]$reply.MicEqParameter
}
$baseline = @()
foreach ($band in 1..12) {
    foreach ($field in 'Frequency', 'Gain', 'Width') {
        $first = Read-Parameter $band $field
        $second = Read-Parameter $band $field
        if ($first -ne $second) { throw "Unstable DSP readback for band $band $field." }
        $baseline += [pscustomobject]@{ Band = $band; Field = $field; Raw = $first }
    }
}
$baseline | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'dsp-baseline.json')
'All 36 DSP parameters returned matching repeated reads; no EQ writes performed.'
if ($NativeTestRunner) {
    & $NativeTestRunner 'daemon::tests::native_live_readback_snapshot' --exact --ignored --nocapture
    if ($LASTEXITCODE) { throw 'The native application live readback test failed.' }
    if ($NativeWriteRoundTrip) {
        & $NativeTestRunner 'daemon::tests::native_live_controls_round_trip' --exact --ignored --nocapture
        if ($LASTEXITCODE) { throw 'The native application live control test failed.' }
    }
}
if ($ReadOnly -and !$ProfileRoundTrip) { return }
$amplitude = $ToneAmplitude
. (Join-Path $PSScriptRoot 'Test-GoXLRMicAudio.ps1') -LibraryOnly
function Set-Parameter($band, $field, $raw) {
    $live = (Send-Request 'GetStatus').Status.mixers.$device
    if ($live.mic_profile_name -ne $profileName) { throw 'The active microphone profile changed.' }
    $expected = Read-Parameter $band $field
    if ($expected -eq $raw) { return }
    $reply = Send-Request @{ Command = @($device, @{ SetMicEqParameter = @{
        band = $band; field = $field; raw = $raw; expected_raw = $expected; profile_name = $profileName
    } }) }
    if ($reply -ne 'Ok') { throw 'The native EQ write failed.' }
    if ((Read-Parameter $band $field) -ne $raw) { throw 'DSP readback did not match the write.' }
    Start-Sleep -Milliseconds 350
}
function Check-Restoration($reference, $restored) {
    for ($i = 0; $i -lt $reference.Count; $i++) {
        if ([math]::Abs($reference[$i] - $restored[$i]) -gt 1.5) {
            throw 'Microphone audio did not return within 1.5 dB of its reference.'
        }
    }
}
if ($ProfileRoundTrip) {
    $width = ($baseline | Where-Object { $_.Band -eq 6 -and $_.Field -eq 'Width' }).Raw
    $testWidth = if ($width -eq 0) { 16 } else { 0 }
    try {
        Set-Parameter 6 Width $testWidth
        $reply = Send-Request @{ Command = @($device, @{ SaveMicProfile = @() }) }
        if ($reply -ne 'Ok') { throw 'Saving the native profile failed.' }
        Set-Parameter 6 Width $width
        $reply = Send-Request @{ Command = @($device, @{ LoadMicProfile = @($profileName, $false) }) }
        if ($reply -ne 'Ok' -or (Read-Parameter 6 Width) -ne $testWidth) {
            throw 'Reloading the saved profile did not apply its native width.'
        }
        'Native width survived profile save and was reapplied by profile reload.'
    } finally {
        foreach ($item in $baseline) { Set-Parameter $item.Band $item.Field $item.Raw }
        foreach ($item in $baseline) {
            if ((Read-Parameter $item.Band $item.Field) -ne $item.Raw) { throw 'Profile-test DSP restoration failed.' }
        }
        $reply = Send-Request @{ Command = @($device, @{ SaveMicProfile = @() }) }
        if ($reply -ne 'Ok') { throw 'Saving the restored live profile failed.' }
    }
}
if ($ReadOnly) { return }
$results = @()
foreach ($band in $Bands) {
    $original = @{}
    foreach ($item in $baseline | Where-Object Band -eq $band) { $original[$item.Field] = $item.Raw }
    $center = [math]::Round(40 * [math]::Pow(2, $original.Frequency / 24)) / 2
    $shift = if ($original.Frequency -gt 216) { -24 } else { 24 }
    $newFrequency = $original.Frequency + $shift
    $target = [math]::Round(40 * [math]::Pow(2, $newFrequency / 24)) / 2
    $hz = [double[]]@($center, $target, ([math]::Round($center * 0.7 * 2) / 2),
        ([math]::Round([math]::Min(20000, $center * 1.4) * 2) / 2))
    if ($center -lt 250) { $hz += 1000.0 }
    [MicFixture]::SetTones($hz)
    # Half-Hz probes need two full seconds; keep the frequency-shift probe apart from side tones.
    $captureDuration = '2.65'
    $sampleCount = 96000
    $quiet = @(Measure-Tones)
    $tone = [MicFixture]::Start('Game (TC-HELICON GoXLR)', $amplitude)
    $record = [ordered]@{ Band = $band; Hz = $hz; Quiet = $quiet; Original = $original; Passed = $false }
    try {
        Start-Sleep -Milliseconds 1200
        $first = @(Measure-Tones)
        $second = @(Measure-Tones)
        $record.First = $first
        $record.Second = $second
        for ($i = 0; $i -lt $hz.Count; $i++) {
            if ($first[$i] - $quiet[$i] -lt 20 -or [math]::Abs($first[$i] - $second[$i]) -gt 1) {
                throw "Band $band acoustic baseline is unstable or too quiet; no test writes for this band."
            }
        }
        $gainChange = if ($original.Gain -le 3) { 6 } else { -6 }
        $testGain = $original.Gain + $gainChange
        try {
            Set-Parameter $band Gain $testGain
            $record.GainCapture = @(Measure-Tones)
        } finally { Set-Parameter $band Gain $original.Gain }
        $record.AfterGainRestore = @(Measure-Tones)
        Check-Restoration $second $record.AfterGainRestore
        if ([math]::Abs(($record.GainCapture[0] - $second[0]) - $gainChange) -gt 2) {
            throw "Band $band gain response did not match the requested change."
        }
        # A nonzero boost makes frequency and width observable even on a flat extra band.
        $probeGain = 6
        try {
            Set-Parameter $band Gain $probeGain
            $record.ProbeReference = @(Measure-Tones)
            $record.ProbeRepeat = @(Measure-Tones)
            for ($i = 0; $i -lt $hz.Count; $i++) {
                if ([math]::Abs($record.ProbeReference[$i] - $record.ProbeRepeat[$i]) -gt 1) {
                    throw "Band $band probe baseline is unstable."
                }
            }
            try {
                Set-Parameter $band Frequency $newFrequency
                $record.FrequencyCapture = @(Measure-Tones)
            } finally { Set-Parameter $band Frequency $original.Frequency }
            $record.AfterFrequencyRestore = @(Measure-Tones)
            Check-Restoration $record.ProbeReference $record.AfterFrequencyRestore
            $testWidth = if ($original.Width -lt 16) { 24 } else { 0 }
            try {
                Set-Parameter $band Width $testWidth
                $record.WidthCapture = @(Measure-Tones)
            } finally { Set-Parameter $band Width $original.Width }
            $record.AfterWidthRestore = @(Measure-Tones)
            Check-Restoration $record.ProbeReference $record.AfterWidthRestore
            $record.TestFrequency = $newFrequency
            $record.TestWidth = $testWidth
            $frequencyContrast = ($record.FrequencyCapture[1] - $record.ProbeReference[1]) -
                ($record.FrequencyCapture[0] - $record.ProbeReference[0])
            $widthSideChange = [math]::Max(
                [math]::Abs($record.WidthCapture[2] - $record.ProbeReference[2]),
                [math]::Abs($record.WidthCapture[3] - $record.ProbeReference[3]))
            if ([math]::Abs($frequencyContrast) -lt 1.5 -or $widthSideChange -lt 0.75 -or
                [math]::Abs($record.WidthCapture[0] - $record.ProbeReference[0]) -gt 2) {
                throw "Band $band frequency or width response was not conclusive."
            }
        } finally { Set-Parameter $band Gain $original.Gain }
        $record.FinalCapture = @(Measure-Tones)
        Check-Restoration $second $record.FinalCapture
        $record.Passed = $true
        "Band $band gain, frequency and width audio checks passed; original fields restored."
    } catch {
        $record.Error = $_.Exception.Message
        if (!$ContinueAfterAudioFailure) { throw }
    } finally {
        $tone.Dispose()
        $results += [pscustomobject]$record
        $results | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'audio-results.json')
        foreach ($item in $baseline) {
            if ((Read-Parameter $item.Band $item.Field) -ne $item.Raw) {
                throw 'The complete DSP baseline was not restored.'
            }
        }
    }
}
if (@($results | Where-Object { !$_.Passed }).Count) { throw 'Physical audio validation is incomplete; see audio-results.json.' }
