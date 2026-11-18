# GoXLR 907 microphone EQ research

## Firmware and protocol

The local 907 image (SHA-256 `EEA92925A5D4EB0111943F72CE7BFEB0351E9390506FECA17AC6A0C3B8346C13`) differs from the stock 901 backup in 13 bytes. Seven are in checksum or version fields. The other six change DSP program words at P:404C70, P:404C76, P:404CEF and P:404CF1. The ARM payload is identical. The device reports 907 through the daemon, and its active image hash was unavailable.

The utility sends stock EQ writes through SetEffectParameters (`0x00801000`), using little-endian 32-bit effect-key and value pairs. The 907 ARM dispatch table at file offset `0x38C80` maps group `0x801` to ARM `0xAE30`. That routine enqueues SysEx command `0x35` on the effects ring, packing the key into two and the value into four seven-bit fields. The three-byte drain at ARM `0xA5B8` services the effect ring and forwards the words through `0x7A18`. The DSP SysEx dispatch at P:403155 selects P:40308E, then P:40306D decodes each record and routes its key through the X:41ABE9 table.

| Section | Frequency / gain / width effect keys | DSP mailbox IDs |
| --- | --- | --- |
| Band 11 | `0x00F5 / 0x00F6 / 0x00F7` | `0x44 / 0x45 / 0x46` |
| Band 12 | `0x010C / 0x010D / 0x010E` | `0x65 / 0x66 / 0x67` |

The [offline replay](../tools/trace_mic907_readback.rs) sends frames with the ARM handler's packing into the original DSP SysEx dispatcher. It verified all 36 parameter routes, the staged transforms, the selected graph fields and five coefficient words, and each readback response. It runs the DSP consumer against scratch graph objects. The 396 width conversion and isolated coefficient cases also passed again. The emulator does not establish live graph state or exercise USB or SHI peripherals.

DSP SysEx `0x47` reads X:419CBE plus the key's descriptor index at P:4030EC. It returns SysEx `0x22` through the outbound queue at X:000E16. This is DSP-owned raw parameter state. Active smoothed coefficients and effective Q in the physical signal path require separate verification. The daemon's optional width field remains a profile value.

The 907 ARM receive task at `0xA5B8` reads DSP words using `0x7A54`. It drains the `0xF0`/SysEx words through the discard path at `0xA6B0–0xA6C8`. The existing USB setter always emits `0x35`. The [ARM bridge reference](../tools/goxlr_eq_readback_bridge.c) creates allowlisted `0x47` queries and validates `0x22` replies with one outstanding read and a timeout that requires resynchronization. This source has host tests. It has no ARM firmware hook or USB command binding.

To rerun the offline work, copy `tools/trace_mic907_readback.rs` into the existing `goxlr-dsp-probe/examples` directory. From that Cargo project, run `cargo run --offline --locked --release --example trace_mic907_readback -- <907-image>` and `cargo run --offline --locked --release --example trace_mic_shape_mailbox_all -- <907-image>`. To check the bridge from this repository, compile with `gcc -std=c11 -Wall -Wextra -Werror -DGOXLR_EQ_BRIDGE_TEST tools/goxlr_eq_readback_bridge.c -o eq-readback-test.exe` and run the executable.

## Physical baseline

On the full-size unit reporting 1.5.1.907, the running daemon exposed ten stock bands and omitted width status. The first fixture-only run varied by up to 20.70 dB. After fixing the headphone cup near the mic, two captures at 700, 1000, 1500 and 2300 Hz repeated within 0.09 dB at tone amplitude 0.06. The first tone capture was at least 44.98 dB above the quiet capture.

The stock 1 kHz control test changed gain from +1 to +7 dB and produced +5.99 dB at 1 kHz. Moving the center from 1000 to 1500 Hz moved the 1500 Hz tone 1.20 dB relative to the 1 kHz tone, measured from the capture after restoring gain. Gain and frequency restored in daemon status across all ten bands. The largest acoustic restoration differences were 0.18 dB for gain and 0.29 dB for frequency, relative to each preceding capture. This run exercised stock commands only.

## Validation still required

- Bind the readback bridge to a USB handler and the ARM task's SHI transmit and receive paths. Verify ring capacity and ownership, DSP reply framing, timeout behavior, and that stock status words still reach their handler. Recovery independent of normal USB enumeration remains unverified.
- Capture each extra band's active values before a live write. Measure center, peak and both sides of the response, then restore and verify both the active values and the acoustic baseline. Keep the app controls gated until those tests pass.

### ARM integration work

- The C bridge exposes nonblocking USB submit and poll entry points for proposed commands `0x00819000` and `0x00819001`. Submit takes a two-byte little-endian key. The enqueue callback must reserve space for the complete query and use the stock ring owner. USB reply bodies start with one status byte. A successful poll appends the echoed key and the signed raw value. The Rust `experimental_eq` module validates poll payloads and creates stock effect-write records for all 36 fields. It does not dispatch USB commands.
- Registration at `0xB328` follows the stock loop at `0xB314–0xB324`, which registers 24 sixteen-byte nodes from `0x38E18` through `0x26384`. The trampoline prototype adds a writable node for group `0x819` after the stock entries. The receive trampoline targets VA `0xA624`. It forwards the three received bytes, restores registers and CPSR flags, replays the displaced `ldrb r3, [sp, #4]`, and resumes at `0xA628`. The isolated ARM emulator checks passed for both observer stack alignments and the registration return with mocked callees.
- The combined ARMv5TE object has 1,348 bytes of text, 16 bytes of initialized data and 20 bytes of zero-initialized state. The largest zero-filled run in the initialized ARM file span is 1,063 bytes at `0x394CD–0x398F4`. Stock code references VA `0x39680` inside that range. Executable placement and writable-state ownership remain unresolved, as do the board bindings for the lock, monotonic timeout and stock ring enqueue.
- Recomputed both checksums from the local 907 image. The ARM checksum is `48C3BE47` and the outer checksum is `6C63D1F0`. Build 908 is the next unused number in the local research images. No 908 image was emitted.

Run the Rust payload tests with `cargo test --locked -p goxlr-usb experimental_eq`. Define both `GOXLR_EQ_BRIDGE_TEST` and `GOXLR_EQ_FIRMWARE` when running the C host test to include the dispatcher and its mocked board bindings. For the ARM object, compile the C file with `arm-none-eabi-gcc -march=armv5te -marm -Os -ffreestanding -fno-builtin -Wall -Wextra -Werror -DGOXLR_EQ_FIRMWARE -c tools/goxlr_eq_readback_bridge.c -o eq-bridge.o`. Assemble `tools/goxlr_eq_arm_hook.S` with the same target flags, link the two objects with `arm-none-eabi-ld -r`, and check unresolved symbols with `arm-none-eabi-nm -u`.
