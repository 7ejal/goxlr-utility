# ARM placement audit after 4e71bd9

## Startup ownership recovered from the 907 bytes

| Range or address | Established use |
| --- | --- |
| File 0x54, VA 0x200 | ARM vectors and initialized image start |
| File 0x28 | 0xE897, equal to the ARM stored word count including its trailing checksum |
| File 0x3A2AC | ARM checksum, which maps to the first address cleared at startup |
| File 0x3A2B0 | DSP payload header |
| VA 0x3A414–0x3A458 | 17 constructor function pointers, walked by 0xF154 |
| VA 0x3A458–0x42E70 | Startup zeroing range, recovered from instructions at 0x314–0x330 |
| 0x3A4E0 | IRQ and undefined-mode stack pointer |
| 0x3A560 | Fault-handler stack pointer |
| 0x3A960 | Early supervisor stack pointer |
| 0x3AB70 | Application supervisor stack pointer, also passed into an application-object constructor |
| VA 0x42E70–0x50000 | Heap initialized by 0x127FC through 0x17B9C |

The stack pointers establish entry values. Exact stack bounds and high-water marks remain unmeasured. The constructor receiving 0x3AB70 is another reason to trace lifetime ownership before moving that boundary.

A Unicorn replay loaded the serialized 907 ARM bytes at VA 0x200 and executed 0x314 through 0x334. It cleared exactly 0x8A18 bytes, including an inserted 1,280-byte marker at the proposed append address. The guard at 0x42E70 survived. Simple ARM section expansion would therefore overlap startup clearing and existing stack/data ownership.

A second replay executed the stock heap initializer with a captured allocator call. Stock arguments were object 0x3F3F4, base 0x42E70, size 0xD190, flags 0. An in-memory trial changing the allocator ceiling to 0x4F000 reduced its size to 0xC190. This proves the allocator arguments can exclude a 4 KiB top region. It does not prove that region can be loaded, executed, or spared under live heap pressure.

## Size optimization

GCC 13.2.1, ARMv5TE, freestanding, no builtin library calls, firmware dispatcher enabled.

| Configuration | Text | Initialized data | Zeroed state |
| --- | ---: | ---: | ---: |
| Previous ARM build with hooks | 1,348 | 16 | 20 |
| Internal helpers, ARM -Os, C only | 952 | 0 | 20 |
| Internal helpers, Thumb -Oz, C only | 600 | 0 | 20 |
| Thumb -Oz, no jump tables, plus ARM hooks | 724 | 16 | 20 |

The no-jump-table build avoids the Thumb switch helper runtime dependency. The combined object has five unresolved board functions for queueing, locking, tick reading and timeout selection. Final interworking veneers, board functions and initialization will add bytes. Fitting within a zero run would not establish ownership.

## SHI ring and scheduler evidence

The effects ring is at 0x38DD4, backed by 0x3D79D–0x3DBA1, with capacity field 0x404. The DSP transmit ring is at 0x38BCC, backed by 0x3D3D2–0x3D3F6, with capacity field 0x24. The receive ring is at 0x38BBC, backed by 0x3D3F6–0x3D41A, also 0x24.

Ring capacity is computed by 0x206C. Stock enqueue 0x20BC and dequeue 0x218C update indices. The ARM task at 0xA5B8 drains effects in three-byte units and passes them to 0x7A18. The later 0x7AC4 service exchanges SHI words. The task at 0xBCF8–0xBD50 services USB, effect receive/drain and SHI in sequence. USB registration also installs a separate USB service task, so serialization across task contexts still needs verification.

The ten-byte query must be padded for the three-byte effects drain. Stock idle padding at VA 0x2F45F is FF FF FF. The DSP response emitter also pads individual one- and two-byte pieces with FF. Queueing a query through the effect ring should reserve the entire padded frame before publishing it. No hardware query has been queued.

0x10B6C increments the eCos scheduler-lock count at 0x393C0. Its balancing function is 0x10B84. The mutex wrappers 0x10F18 and 0x10F1C can schedule or block. Neither is a drop-in IRQ guard. The tick wrapper at 0x10E80 reads a 64-bit count from a runtime object rooted at 0x3FA10. Tick duration, IRQ ownership of the rings, and whether a bridge observer can safely take a particular lock remain open.

## Loading and recovery limits

The flash-bank table at 0x38BDC names Code0, Code1, Preset0, Preset1, Golden, Config and Update. Their base and capacity fields are zero in the image and are filled at runtime. The updater calls 0x7C08 to obtain Update capacity before accepting the package length. A Golden name establishes a partition label. Entry into it without the normal application and its USB interface has yet to be demonstrated.

The local image contains the ARM application, its preamble and DSP payload. No separate bootloader binary was available in the research assets inspected here. The preamble word count is a section-boundary clue. Its bootloader interpretation and maximum copy length remain unverified. Relocation into a reserved top-of-heap region requires a proven loader or an early copy path from verified storage plus instruction-cache handling.

The official quick-start guide did not provide a recovery procedure in the retrieved material. The upstream firmware updater documentation describes host-driven updating. A separate physical recovery interface remains unverified.

## Physical test preparation

Once the serialized firmware passes placement and communication tests, capture all 36 raw fields through DSP readback before any write. Compare repeated snapshots and record the active mic profile. A profile XML value cannot establish the initial DSP width.

Reuse the fixed headphone-cup fixture and its 1 dB repeatability and 20 dB signal-to-quiet checks. Test one band and one field at a time. At nonzero gain, measure the center and tones on both sides before a width edit, after it, and after restoration. Frequency and gain checks retain the existing 2 dB gain-error and 1.5 dB acoustic-restoration bounds. Width checks must show a repeatable change at the side tones while the center and neighboring bands stay consistent.

For each edit, require the requested raw value in the DSP response and the expected physical response. Restore the original raw value, reread all 36 fields, and remeasure the baseline. A timeout leaves the state unknown and requires readback resynchronization before further edits. Reconnect, profile reload and cold-start tests follow successful reversible field tests. The present firmware has no host-visible readback binding to execute this sequence.

## Reproduce

From the repository root, compile the layout audit with `rustc --edition 2024 tools/audit_goxlr_arm_layout.rs -o audit-goxlr-arm-layout.exe`, then run it with the stock or 907 image as its argument.

For the optimized object, compile the bridge using `arm-none-eabi-gcc -march=armv5te -mthumb -mthumb-interwork -Oz -fno-jump-tables -ffreestanding -fno-builtin -Wall -Wextra -Werror -DGOXLR_EQ_FIRMWARE -c tools/goxlr_eq_readback_bridge.c -o eq-bridge.o`. Assemble the existing hooks in ARM mode, link with `arm-none-eabi-ld -r`, and inspect sizes and undefined symbols.

Run the C host checks with both `GOXLR_EQ_BRIDGE_TEST` and `GOXLR_EQ_FIRMWARE` defined. Run `cargo test --locked -p goxlr-usb` for all Rust USB tests.

Firmware 908 has not been emitted. Executable loading and state ownership have not passed validation, and the daemon transport and physical tests remain pending.
