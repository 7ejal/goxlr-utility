# GoXLR 907 microphone EQ research

## Firmware and protocol

The local 907 image (SHA-256 `EEA92925A5D4EB0111943F72CE7BFEB0351E9390506FECA17AC6A0C3B8346C13`) differs from the stock 901 backup in 13 bytes. Seven are in checksum or version fields. The other six change DSP program words at P:404C70, P:404C76, P:404CEF and P:404CF1. The ARM payload is identical. The device reports 907 through the daemon, and its active image hash was unavailable.

The utility sends stock EQ writes through SetEffectParameters (`0x00801000`), using little-endian 32-bit effect-key and value pairs. The stock 1 kHz frequency, gain and width keys are `0x011D`, `0x011E` and `0x011F`. The 907 patch adds no ARM USB handlers. An existing firmware read path for active EQ values has yet to be identified. The daemon's profile width status, when present, reflects the profile value.

The candidate extra-band USB triplets `0x00F5–0x00F7` and `0x010C–0x010E` have no confirmed ARM-to-DSP mapping. The emulator's internal frequency/gain/width triples for the two outer slots are `0x44/0x45/0x46` and `0x65/0x66/0x67`. Running the existing `trace_mic_shape_mailbox_all` example against the local 907 image passed all 396 width conversion and isolated coefficient update cases. It injects mailbox messages directly and seeds the object modes, so it does not cover USB routing, boot-time mode selection or live readback.

## Physical baseline

On the full-size unit reporting 1.5.1.907, the running daemon exposed ten stock bands and omitted width status. The first fixture-only run varied by up to 20.70 dB. After fixing the headphone cup near the mic, two captures at 700, 1000, 1500 and 2300 Hz repeated within 0.09 dB at tone amplitude 0.06. The first tone capture was at least 44.98 dB above the quiet capture.

The stock 1 kHz control test changed gain from +1 to +7 dB and produced +5.99 dB at 1 kHz. Moving the center from 1000 to 1500 Hz moved the 1500 Hz tone 1.20 dB relative to the 1 kHz tone, measured from the capture after restoring gain. Gain and frequency restored in daemon status across all ten bands. The largest acoustic restoration differences were 0.18 dB for gain and 0.29 dB for frequency, relative to each preceding capture. This run exercised stock commands only.

## Validation still required

- Trace each candidate key through the ARM effect dispatcher to a distinct DSP mailbox and verify frequency, gain and width units before writing to the device.
- Identify a readback path for the active fields, including width. If a firmware extension is needed, verify the ARM/SHI request and reply path, bounded completion and DSP memory ownership before producing a flashable image. Recovery independent of normal USB enumeration remains unverified.
- Capture each extra band's active values before a live write. Measure center, peak and both sides of the response, then restore and verify both the active values and the acoustic baseline. Keep the app controls gated until those tests pass.
