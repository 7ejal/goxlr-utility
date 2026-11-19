# Plays quiet tones through a fixed headphone-to-mic fixture and measures Chat Mic PCM.
# A stock band is changed only when -Band is specified and the baseline repeats.
param(
    [string]$Playback = 'Game (TC-HELICON GoXLR)',
    [string]$Capture = 'Chat Mic (TC-HELICON GoXLR)',
    [ValidateSet(1,2)][int]$CaptureChannels = 1,
    [double]$ToneAmplitude = 0.04,
    [string]$Client,
    [switch]$LibraryOnly,
    [ValidateSet("31","63","125","250","500","1000","2000","4000","8000","16000")][string]$Band
)
$ErrorActionPreference = 'Stop'
if ($ToneAmplitude -le 0 -or $ToneAmplitude -gt 0.08) { throw 'Tone amplitude must be in (0, 0.08].' }
$ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$captureDuration = if ($Band -eq "31") { "2.65" } else { "1.65" }
$sampleCount = if ($Band -eq "31") { 96000 } else { 48000 }
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class MicFixture {
    [StructLayout(LayoutKind.Sequential, Pack=2)]
    struct Format { public ushort tag, channels; public uint rate, bytes; public ushort block, bits, extra; }
    [StructLayout(LayoutKind.Sequential)]
    struct Header { public IntPtr data; public uint size, recorded; public IntPtr user; public uint flags, loops; public IntPtr next, reserved; }
    [DllImport("winmm.dll")] static extern uint waveOutGetNumDevs();
    [DllImport("winmm.dll", CharSet=CharSet.Unicode)] static extern uint waveOutGetDevCapsW(UIntPtr id, byte[] caps, uint size);
    [DllImport("winmm.dll")] static extern uint waveOutOpen(out IntPtr handle, uint id, ref Format fmt, IntPtr callback, IntPtr args, uint flags);
    [DllImport("winmm.dll")] static extern uint waveOutPrepareHeader(IntPtr h, IntPtr header, uint size);
    [DllImport("winmm.dll")] static extern uint waveOutWrite(IntPtr h, IntPtr header, uint size);
    [DllImport("winmm.dll")] static extern uint waveOutReset(IntPtr h);
    [DllImport("winmm.dll")] static extern uint waveOutUnprepareHeader(IntPtr h, IntPtr header, uint size);
    [DllImport("winmm.dll")] static extern uint waveOutClose(IntPtr h);
    static double[] tones = {700, 1000, 1500, 2300};
    public static void SetTones(double[] frequencies) { tones = frequencies; }
    static void Check(uint result) { if (result != 0) throw new Exception("waveOut error " + result); }
    public static IDisposable Start(string endpoint, double amplitude) { return Start(endpoint, amplitude, false); }
    public static IDisposable Start(string endpoint, double amplitude, bool splitStereo) {
        uint id = uint.MaxValue;
        for (uint i = 0; i < waveOutGetNumDevs(); i++) {
            var caps = new byte[84];
            if (waveOutGetDevCapsW(new UIntPtr(i), caps, (uint)caps.Length) != 0) continue;
            var name = System.Text.Encoding.Unicode.GetString(caps, 8, 64).Split('\0')[0];
            if (!name.StartsWith(endpoint, StringComparison.OrdinalIgnoreCase)) continue;
            if (id != uint.MaxValue) throw new Exception("Ambiguous playback device");
            id = i;
        }
        if (id == uint.MaxValue) throw new Exception("Playback device unavailable: " + endpoint);
        return new Tone(id, amplitude, splitStereo);
    }
    sealed class Tone : IDisposable {
        IntPtr handle, header;
        GCHandle pin;
        bool prepared;
        readonly uint size = (uint)Marshal.SizeOf(typeof(Header));
        public Tone(uint id, double amplitude, bool splitStereo) {
            if (splitStereo && tones.Length != 2) throw new Exception("Stereo test needs two tones");
            var samples = new short[48000 * 2 * 2];
            for (int i = 0; i < samples.Length / 2; i++) {
                double sum = 0;
                foreach (double hz in tones) sum += Math.Sin(2 * Math.PI * hz * i / 48000);
                samples[2*i] = samples[2*i+1] = (short)(32767 * amplitude * sum);
                if (splitStereo) {
                    samples[2*i] = (short)(32767 * amplitude * Math.Sin(2*Math.PI*tones[0]*i/48000));
                    samples[2*i+1] = (short)(32767 * amplitude * Math.Sin(2*Math.PI*tones[1]*i/48000));
                }
            }
            var format = new Format { tag=1, channels=2, rate=48000, bytes=192000, block=4, bits=16 };
            Check(waveOutOpen(out handle, id, ref format, IntPtr.Zero, IntPtr.Zero, 0));
            try {
                pin = GCHandle.Alloc(samples, GCHandleType.Pinned);
                header = Marshal.AllocHGlobal((int)size);
                Marshal.StructureToPtr(new Header { data=pin.AddrOfPinnedObject(), size=(uint)samples.Length*2, flags=12, loops=28 }, header, false);
                Check(waveOutPrepareHeader(handle, header, size));
                prepared = true;
                Check(waveOutWrite(handle, header, size));
            } catch { Dispose(); throw; }
        }
        public void Dispose() {
            waveOutReset(handle);
            if (prepared) waveOutUnprepareHeader(handle, header, size);
            if (header != IntPtr.Zero) Marshal.FreeHGlobal(header);
            if (pin.IsAllocated) pin.Free();
            waveOutClose(handle);
        }
    }
    public static double[] Levels(byte[] pcm, int count) { return Levels(pcm, count, 1, 0); }
    public static double[] Levels(byte[] pcm, int count, int channels, int channel) {
        const int start=24000;
        if (channel < 0 || channel >= channels || pcm.Length < (start+count)*4*channels) throw new Exception("Capture too short or invalid channel");
        var levels = new double[tones.Length];
        for (int j = 0; j < tones.Length; j++) {
            double omega=2*Math.PI*tones[j]/48000;
            double x=0, y=0;
            for (int i=0; i<count; i++) {
                var value=BitConverter.ToSingle(pcm, ((start+i)*channels+channel)*4);
                x += value*Math.Cos(i*omega);
                y += value*Math.Sin(i*omega);
            }
            levels[j]=Math.Round(20*Math.Log10(Math.Max(1e-10,2*Math.Sqrt(x*x+y*y)/count)), 2);
        }
        return levels;
    }
}
"@

function Measure-Tones([int]$Channel = 0) {
    $path = [IO.Path]::GetTempFileName()
    try {
        $captureProcess = [Diagnostics.ProcessStartInfo]::new($ffmpeg)
        $captureProcess.UseShellExecute = $false
        $captureProcess.CreateNoWindow = $true
        foreach ($part in @('-nostdin','-v','error','-y','-f','dshow','-i',"audio=$Capture",'-t',$captureDuration,'-ac',"$CaptureChannels",'-ar','48000','-f','f32le',$path)) {
            $captureProcess.ArgumentList.Add($part)
        }
        $process = [Diagnostics.Process]::Start($captureProcess)
        try {
            if (!$process.WaitForExit(6500)) {
                $process.Kill()
                $process.WaitForExit()
                throw 'Chat Mic capture timed out.'
            }
            if ($process.ExitCode) { throw 'Chat Mic capture failed.' }
        } finally {
            $process.Dispose()
        }
        [MicFixture]::Levels([IO.File]::ReadAllBytes($path), $sampleCount, $CaptureChannels, $Channel)
    } finally {
        Remove-Item -LiteralPath $path
    }
}

function Get-Stock {
    $status = & $Client --status-json | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Could not read the daemon status.' }
    $mixers = @($status.mixers.PSObject.Properties | Where-Object { $_.Value.hardware.device_type -eq 'Full' })
    if ($mixers.Count -ne 1 -or ($mixers[0].Value.hardware.versions.firmware -join '.') -ne '1.5.1.907') {
        throw 'This test requires exactly one full-size GoXLR reporting firmware 1.5.1.907.'
    }
    $mixer = $mixers[0].Value
    [pscustomobject]@{
        Device = $mixers[0].Name
        Profile = $mixer.mic_profile_name
        Eq = $mixer.mic_status.equaliser
        Gain = [int]$mixer.mic_status.equaliser.gain.($bandSpec.Key)
        Frequency = [double]$mixer.mic_status.equaliser.frequency.($bandSpec.Key)
    }
}

function Set-Stock($kind, $value) {
    $parameter = $kind.ToLowerInvariant()
    $arguments = @('--device', $baseline.Device, 'microphone', 'equaliser', $parameter)
    if ($value -lt 0) { $arguments += '--' }
    & $Client @arguments ($bandSpec.Cli) $value | Out-Null
    if ($LASTEXITCODE) { throw "Stock $kind command failed." }
}

function Measure-Edit($kind, $value) {
    $current = Get-Stock
    if ($current.Device -ne $baseline.Device -or $current.Profile -ne $baseline.Profile -or
        $current.Gain -ne $baseline.Gain -or $current.Frequency -ne $baseline.Frequency) {
        throw 'The active microphone settings changed before the test.'
    }
    try {
        Set-Stock $kind $value
        if ((Get-Stock).$kind -ne $value) { throw 'Daemon did not report the requested value.' }
        Start-Sleep -Milliseconds 350
        @(Measure-Tones)
    } finally {
        $current = Get-Stock
        if ($current.Device -ne $baseline.Device -or $current.Profile -ne $baseline.Profile) {
            throw 'The profile changed during the test. Check the original profile before further edits.'
        }
        if ($current.$kind -eq $value) { Set-Stock $kind $baseline.$kind }
        $after = Get-Stock
        foreach ($field in 'gain','frequency') {
            foreach ($item in $baseline.Eq.$field.PSObject.Properties) {
                if ($after.Eq.$field.($item.Name) -ne $item.Value) { throw 'Stock EQ restoration could not be confirmed.' }
            }
        }
    }
}

if ($LibraryOnly) { return }
$hz = [double[]]@(700,1000,1500,2300)
if ($Band) {
    if (!$Client -or !(Test-Path -LiteralPath $Client -PathType Leaf)) { throw 'Pass the existing GoXLR client executable.' }
    $spec = @{
        '31' = @('31Hz','31-hz',48); '63' = @('63Hz','63-hz',95); '125' = @('125Hz','125-hz',185)
        '250' = @('250Hz','250-hz',300); '500' = @('500Hz','500-hz',700); '1000' = @('1KHz','1-k-hz',1500)
        '2000' = @('2KHz','2-k-hz',1400); '4000' = @('4KHz','4-k-hz',6000)
        '8000' = @('8KHz','8-k-hz',12000); '16000' = @('16KHz','16-k-hz',12000)
    }[$Band]
    $bandSpec = @{ Key = 'Equalizer' + $spec[0]; Cli = 'equalizer' + $spec[1] }
    $baseline = Get-Stock
    $newFrequency = $spec[2]
    $newGain = if ($baseline.Gain -le 3) { $baseline.Gain + 6 } else { $baseline.Gain - 6 }
    if ($Band -ne '1000') {
        $hz = [double[]]@($baseline.Frequency, $newFrequency)
        # The 1 kHz pilot keeps the unchanged stock noise gate open for low-frequency probes.
        if ([int]$Band -lt 250) { $hz += 1000.0 }
        [MicFixture]::SetTones($hz)
    }
}
$quiet = @(Measure-Tones)
$tone = [MicFixture]::Start($Playback, $ToneAmplitude)
try {
    Start-Sleep -Milliseconds 1200
    $first = @(Measure-Tones)
    $second = @(Measure-Tones)
    $result = [ordered]@{ Band = $Band; Hz = $hz; Quiet = $quiet; First = $first; Second = $second; Status = 'Fixture only' }
    if ($Band) {
        $result.Status = 'Baseline unstable, no writes'
        $stable = $true
        for ($i = 0; $i -lt $quiet.Count; $i++) {
            if ($first[$i] - $quiet[$i] -lt 20 -or [math]::Abs($first[$i] - $second[$i]) -gt 1) { $stable = $false }
        }
        if ($stable) {
            $result.OriginalGain = $baseline.Gain
            $result.OriginalFrequency = $baseline.Frequency
            $result.TestGain = $newGain
            $result.TestFrequency = $newFrequency
            $result.GainCapture = @(Measure-Edit Gain $newGain)
            $result.AfterGainRestore = @(Measure-Tones)
            if ($baseline.Frequency -ne $newFrequency) {
                $result.FrequencyCapture = @(Measure-Edit Frequency $newFrequency)
                $result.AfterFrequencyRestore = @(Measure-Tones)
            }
            $result.Status = 'Captured; original EQ restored in daemon'
        }
    }
    [pscustomobject]$result | ConvertTo-Json -Compress
} finally { $tone.Dispose() }
