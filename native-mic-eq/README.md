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

Firmware 907's four changed DSP words select two modes during construction.
The extra stages lack a verified live USB mapping and the daemon cannot read
back the active DSP bandwidth for a reversible write. The native client has no
Q or extra-stage write path.
