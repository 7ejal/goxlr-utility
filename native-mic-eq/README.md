# GoXLR Native Mic EQ

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
available. See [the measured results](FIRMWARE-908.md) for exact tested changes
and the remaining experimental limits.

See [ARM placement and hardware evidence](ARM-PLACEMENT.md) for the exact image,
recovery results and outstanding gates. Use `tools/Read-GoXLRRecovery.ps1` with
`-ValidationScript tools/Test-GoXLRNativeEq.ps1` to preserve profiles and the
original daemon during the extended physical checks. `-ValidationArguments`
accepts `ReadOnly`, `Bands` and `ContinueAfterAudioFailure`. The helper stops
before writes when the signal is too quiet or the baseline is unstable, and
requires all 36 original values after every tested band.
