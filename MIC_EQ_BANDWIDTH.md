# GoXLR microphone EQ bandwidth preview

The Microphone tab has ten raw bandwidth sliders for full-size GoXLR profiles.
Values are integers from 0 (narrower) to 32 (wider), with a stock default of 16.
Use the existing Save Mic Profile command to persist your edits. Profiles with
all ten defaults retain the original XML format.

This build stages changes in the profile. Live bandwidth capability is disabled
while USB acceptance and the microphone's physical response are unverified.
The 1 kHz slider's tooltip can show modeled effective Q at 48 kHz, exactly
1,000 Hz and +6 dB, for raw 0, 8, 16 and 24. Those are stock-DSP
half-gain estimates from emulation. Other settings display the raw value.

## Windows x64 portable build

The ZIP needs the Windows GoXLR driver and the WebView2 runtime. Extract the
entire ZIP. Exit the existing GoXLR Utility from its system tray before opening
`Launch GoXLR Bandwidth.cmd`. The preview uses the normal GoXLR Utility
profile directory. Open the Microphone tab and look below the existing EQ.

The Windows daemon and CLI are compiled from this branch. The ZIP reuses
the existing native GoXLR Utility UI 0.0.6 wrapper.

## Checks

```powershell
python headphone-eq-tests/integrate_tab.py
node --test headphone-eq-tests/eq-core.test.mjs headphone-eq-tests/mic-bandwidth.test.mjs
cargo test --release --locked -p goxlr-profile-loader -p goxlr-ipc -p goxlr-usb mic_eq_bandwidth_tests
cargo build --release --locked -p goxlr-daemon -p goxlr-client
```
