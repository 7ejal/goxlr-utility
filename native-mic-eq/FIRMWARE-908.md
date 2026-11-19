# Experimental 908 physical validation, 2026-11-19

The same-size image booted on the physical full-size GoXLR and all twelve bands
passed reversible microphone frequency, gain and width tests. These are recorded
audio results, separate from the 396 earlier DSP emulator cases, ARM emulation
and USB acknowledgments. Firmware 908 remains experimental.

Image: `GoXLR-1.5.1.908-EQ12-SAME-SIZE-EXPERIMENTAL.bin`, 640,044 bytes.
SHA-256: `C8607989063552E5A412AD3B29B4E1DB355D9B737DCC59D972A5349FBFAA970F`.
ARM CRC: `47E7728C`; outer CRC: `92C9EA43`. The ARM word count remains `0xE897`.

Before flashing, a normal DC power cycle was verified to return from Golden
1.3.31.83 to working 1.5.1.907. Boot2 and Golden were read twice per chunk both
before and after the update; their hashes are unchanged. See
[ARM placement](ARM-PLACEMENT.md) for loader limits, memory ownership and hashes.

All 36 live DSP fields returned matching repeated reads through the actual USB
handler, ARM bridge, SHI queue, DSP query 0x47 and reply 0x22. The native Rust
application's read-only live snapshot test also passed over its real IPC path.

## Microphone measurements

Game playback used a fixed headphone cup beside the microphone; capture was
Chat Mic PCM. Baselines required every probe to be at least 20 dB above quiet
and repeat within 1 dB. Tests changed one field at a time, checked DSP readback,
restored it and re-read all 36 fields after each band. All original raw values
were restored. Saved profile files were restored byte-for-byte and the original
daemon restarted between sessions.

Gain tests requested +6 dB, or -6 dB for the three bands initially at high gain.
Frequency contrast is the change at the shifted center minus the change at the
original center, measured with +6 dB gain. Width tests changed raw 16 to raw 0
at +6 dB, then restored 16. The width-side column uses the lower side tone.
Maximum restore deviation covers all probes after gain, frequency and width
restoration and the final original-gain capture.

| Band | Gain change, dB | Frequency contrast, dB | Width-side change, dB | Maximum restore deviation, dB |
| --- | ---: | ---: | ---: | ---: |
| 1 | 5.76 | 9.84 | -3.14 | 0.86 |
| 2 | 5.90 | 9.79 | -2.69 | 0.98 |
| 3 | 5.96 | 9.85 | -2.91 | 0.32 |
| 4 | 5.99 | 9.75 | -2.84 | 0.16 |
| 5 | 5.91 | 9.58 | -2.57 | 0.21 |
| 6 | 5.95 | 5.66 | -2.70 | 0.70 |
| 7 | 6.08 | 9.68 | -2.85 | 0.13 |
| 8 | -5.99 | 9.64 | -2.84 | 0.12 |
| 9 | -6.08 | 9.80 | -2.80 | 0.30 |
| 10 | -5.97 | 9.66 | -2.88 | 0.12 |
| 11 | 6.03 | 9.61 | -2.81 | 0.15 |
| 12 | 5.97 | 9.78 | -2.88 | 0.09 |

Band 6 used the first stable 0.04-amplitude session and a half-octave frequency
shift. Bands 9–10 used separated probes at 0.04 amplitude. The other bands used
0.06 amplitude and two-second measurement windows; their frequency shift was
one octave. Width-side changes were observable while center changes remained
within 2 dB. This proves adjustable physical width; it does not calibrate raw
0–32 into numerical effective Q or test every possible selector value.

Failed attempts remain excluded from the pass table: initial captures had no
tone, and an early band-11 frequency restoration exceeded tolerance. Those runs
stopped or restored the DSP baseline. A separate digital playback capture
received all four reference tones at -25.33 dBFS. Windows endpoint volumes were
100% and unmuted. Quiet tests at the previously established 0.06 amplitude
completed the unfinished bands without lowering acceptance limits. The exact
cause of the earlier silence was not conclusively isolated.

## Remaining experimental limits

A normal DC power cycle after flashing returned to 908 with zero changes in
the 36-field baseline. The native application's real IPC frequency, gain and
width round trips passed, restoring the complete baseline after each edit.
A native width survived profile save and was reapplied by profile reload.

The installed-state comparison also caught a migration defect: legacy width
metadata that had previously been profile-only was replayed at startup. The
original live widths were restored. Replay now uses only explicit native
overrides, with a regression test covering legacy widths and a native edit's
save/load behavior. Final installed-state verification follows administrator
approval for the corrected build.

This is one physical device and one microphone fixture. Worst-case live heap
use, IRQ-off latency, native-tick duration, deliberately induced physical
timeouts and long-run stability are unmeasured. Emulator timeout/recovery
checks do not replace those measurements. Golden recovery is physically
confirmed; the ROM UART fallback remains code evidence only. No external
programmer was used. Stock routing/playback and microphone operation were
exercised, but every sampler/effect combination was not regression-tested.

The original 907 file and recovery partitions are preserved. Firmware status,
parameter-table readback, measured filter response and numeric Q calibration
are separate evidence levels.
