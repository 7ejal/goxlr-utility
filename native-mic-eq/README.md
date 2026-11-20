# GoXLR Native Audio Processor

Native Rust desktop controls for the full-size GoXLR. The executable uses eframe
and connects to the running GoXLR daemon by local IPC. The daemon owns the USB
connection and sends the supported stock EQ commands through the existing
driver. Run the original Utility for routing, mixing, effects and profile
saves.

Build on Windows with:

```powershell
cargo build --locked --release -p goxlr-native-mic-eq
cargo test --locked --release -p goxlr-native-mic-eq
```

With one full-size device reporting 1.5.1.907 and the 1 kHz band at +2 dB,
run the opt-in native IPC gain round trip:

```powershell
cargo test --locked --release -p goxlr-native-mic-eq native_ipc_gain_round_trip -- --ignored
```

Physical audio measurements use a fixed headphone cup next to the microphone,
the GoXLR Game playback endpoint and the Chat Mic capture endpoint. The fixture
requires PowerShell 7 and FFmpeg. With the headphone positioned and the
original Utility running, the following tests one stock band and verifies
restoration of the original EQ values in the daemon:

```powershell
pwsh -File tools/Test-GoXLRMicAudio.ps1 -Client <path-to-goxlr-client.exe> -Band 1000 -ToneAmplitude 0.06
```

All ten stock bands passed the reversible frequency and gain audio checks on the
tested device reporting firmware 1.5.1.907. An explicit apply button sends each
validated frequency or gain command. The app
checks the active mic profile and original EQ settings again before applying.
Raw width is the profile code in the 0–32 range. Effective Q and files on disk
are separate from the daemon's active settings.

Experimental same-size firmware 908 adds the ARM USB-to-DSP readback bridge.
The physical device has returned matching repeated values for all 36 raw
frequency, gain and width parameters across 12 bands. The native application
reads these through the research daemon and displays them from DSP state.
Frequency is decoded from its logarithmic selector. Width is a raw selector,
not a calibrated numerical Q value.

Native writes use expected-value checks and DSP readback confirmation. The
12-band controls and automatic profile replay are enabled only on 908, after
all twelve bands passed physical audio checks. Microphone profiles retain all
36 optional native overrides. The original 907 frequency/gain behavior remains
available. On 908, Save and Save As capture the complete live 36-field DSP
snapshot, including untouched fields. See [the measured results](FIRMWARE-908.md) for exact tested changes
and the remaining experimental limits.

Older profile width attributes remain profile-only. They were never proof of
active DSP width and are not replayed automatically. An explicit native edit
creates the override that enables its subsequent profile replay.

See [ARM placement and hardware evidence](ARM-PLACEMENT.md) for the exact image,
recovery results and outstanding gates. Use `tools/Read-GoXLRRecovery.ps1` with
`-ValidationScript tools/Test-GoXLRNativeEq.ps1` to preserve profiles and the
original daemon during the extended physical checks. `-ValidationArguments`
accepts `ReadOnly`, `Bands` and `ContinueAfterAudioFailure`. The helper stops
before writes when the signal is too quiet or the baseline is unstable, and
requires all 36 original values after every tested band.

## Headphone and dynamics research

See [the 2026-11-20 installation, validation and feature status](DEVELOPMENT-STATUS.md)
and [the stock noise-gate audit](NOISE-GATE.md). Firmware 909 remains active;
the [910 diagnostics candidate](FIRMWARE-910.md) is not authorized to flash.

See [the current evidence and blocked integration gates](HEADPHONE-DYNAMICS.md)
for headphone-only EQ and the offline stock compressor/de-esser control lab.

The [909 dynamics bridge](FIRMWARE-909.md) extends this same native app and
transport. All 47 fields passed physical live write/read/restoration checks.
Calibrated compressor/de-esser audio response is still pending, so dynamics
edits remain disabled in the GUI. The capability panel distinguishes verified
readback from unmeasured processing, routing and performance features. It never
displays estimated DSP load or fabricated gain-reduction meters.

The parameter reference lists all 47 DSP keys, raw domains, current DSP values
and all 47 recovered descriptor defaults. Generic original EQ object descriptor
values are explicitly distinguished from unknown effective boot settings; they
are not copied from the user's profile and do not authorize reset.
Selector labels retain the different native timing definitions: de-esser times
are nominal 99% settling times; compressor times are nominal time constants.
Frequency/bandwidth controls are not inferred from de-esser smoothing/weighting.
See [the parameter reference](PARAMETERS.md) for units, setter addresses and gates.

The compressor response view is a read-only manual-mode coefficient model from
the original P:0343DD / P:00441D code, checked against serialized kernel PCM
vectors. It uses internal normalized input/output dB, including the original
threshold-0 ratio bypass. It does not measure microphone response or assume the
private automatic-makeup flag. Attack/release, saturation and physical calibration
are excluded; no knee or automatic-makeup control is enabled by the graph.

Saved microphone profiles use the existing daemon's Save, Save As and Load
operations. Saving captures all 36 native EQ values plus stock microphone
settings; the app verifies that saving did not change the live DSP snapshot or
stock microphone settings. Load choices come from the daemon's actual profile
list and loading is an explicit action. Profile operations are disabled on
unvalidated firmware. This does not make the stock profile format persist all
11 advanced raw dynamics fields; portable 47-field exports remain separate.

The optional Windows spectrum view records the exact Chat Mic endpoint, without
playing audio or changing its configuration. It shows a 2048-sample Hann FFT and
recorded peak/RMS levels from the first channel. These include the Windows
recording level and are not internal DSP levels, gain reduction or physical
performance measurements. Recording is off by default, stale/error data is
unavailable, and no audio file is saved. Host capture drops are labeled separately
from device underruns.

## Portable 47-field presets

Open **Portable presets** in the same native app. Refresh the device, enter a
display name and JSON file path, then choose **Export new file**. This saves the
last complete 909 DSP snapshot as exact raw integers, including all 36 EQ and 11
dynamics fields. It never overwrites an existing file. Device serial numbers and
profile names are excluded.

**Import preview** reads and validates the file, then compares every imported
value with the last live snapshot. Import works without a connected device;
comparison requires a complete 909 snapshot. Import never writes hardware.
**Apply preset** remains disabled pending complete processing validation.

Files use UTF-8 JSON format `goxlr-native-raw47`, version 1, with exact units per
key. Missing/duplicate/unknown keys, wrong units, out-of-range values, unsupported
versions, extra JSON members and invalid names are rejected. Import is bounded
to 64 KiB. This format preserves raw width without pretending it is numerical Q;
it does not replace the existing GoXLR mixer/microphone profile formats.

An ignored read-only integration test exercises the same preset codec against
all 47 actual device reads:

```powershell
cargo test --locked -p goxlr-native-mic-eq native_live_preset_snapshot -- --ignored --nocapture
```

Set `GOXLR_PRESET_EXPORT_PATH` to an explicit new file path to retain that live
snapshot; existing files are refused. The test performs no hardware writes.

## Device mic activity

The optional **Device mic activity** bar uses the stock microphone-level command
through the existing daemon. It polls at most ten times per second, allows one
activity request in flight and pauses during other device operations. Failed,
profile-mismatched or older-than-500-ms samples are unavailable, not frozen.
Its scale is the daemon's `20 log10(raw) − 72.2` transformation, clamped to
−72.2…0. It is uncalibrated device activity, not proven peak/RMS/dBFS, gain
reduction or DSP load. It starts disabled and adds no firmware command.

## Recovered detector response graph

The dynamics section includes an interactive native graph of the recovered
fixed 4 kHz third-order Butterworth high-pass de-esser detector. Hover to read
model magnitude. Selecting the 44100/48000 Hz coefficient bank changes only the
graph; it neither measures nor changes the hardware clock. The formula is
checked against the actual serialized detector kernel in
`tools/dynamics_kernel.rs`. This is a detector model, not an acoustic measurement,
output EQ or spectrum. Original processing applies its gain to unfiltered audio
(wideband de-essing); no frequency or bandwidth control is invented.

De-esser intensity is shown as a raw 0–100 scale, not a reduction percentage:
serialized full-kernel tests produced identical output at 0, 1 and 2, including
measurable reduction in the 8 kHz fixture. Intensity zero is not bypass.
Disabling gain reduction targets unity through a 64-sample transition while
the detector continues running. It is not an instant bit-exact dry or workload
bypass. These are native-code test results; acoustic calibration remains pending.
