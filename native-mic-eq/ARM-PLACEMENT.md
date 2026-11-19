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

The no-jump-table build avoids the Thumb switch helper runtime dependency. The complete linked bridge now contains 960 bytes of text, 16 bytes of initialized data and 24 bytes of state, including all board functions and interworking veneers. Fitting within a zero run would not establish ownership.

## SHI ring and scheduler evidence

The effects ring is at 0x38DD4, backed by 0x3D79D–0x3DBA1, with capacity field 0x404. The DSP transmit ring is at 0x38BCC, backed by 0x3D3D2–0x3D3F6, with capacity field 0x24. The receive ring is at 0x38BBC, backed by 0x3D3F6–0x3D41A, also 0x24.

Ring capacity is computed by 0x206C. Stock enqueue 0x20BC and dequeue 0x218C update indices. The ARM task at 0xA5B8 drains effects in three-byte units and passes them to 0x7A18. The later 0x7AC4 service exchanges SHI words. The task at 0xBCF8–0xBD50 services USB, effect receive/drain and SHI in sequence. USB registration also installs a separate USB service task, so serialization across task contexts still needs verification.

The ten-byte query must be padded for the three-byte effects drain. Stock idle padding at VA 0x2F45F is FF FF FF. The DSP response emitter also pads individual one- and two-byte pieces with FF. Queueing a query through the effect ring should reserve the entire padded frame before publishing it. The physical 908 readback test now queues these queries successfully; see the results below.

0x10B6C increments the eCos scheduler-lock count at 0x393C0. Its balancing function is 0x10B84. The mutex wrappers 0x10F18 and 0x10F1C can schedule or block. The bridge instead masks IRQ/FIQ around its short state and ring operations and restores the caller's CPSR. It calls no scheduler or blocking function while masked. The tick wrapper at 0x10E80 reads a 64-bit count from a runtime object rooted at 0x3FA10. The bridge reads its low word while masked and uses unsigned subtraction for timeout wrap. The linked experiment uses 100 native ticks. Their physical duration and maximum IRQ-off latency remain unmeasured.

## Physical Boot2 loading and recovery

Boot2 and Golden were retrieved through the original read-only DCP command `0x004005`, twice per 1,024-byte chunk. Host requests allow only Boot2 (partition 0, 65,536 bytes) and Golden (partition 2, 851,968 bytes). No flash erase/write command is used by the reader.

Boot2 file offset 0x10 loads at VA 0x4F000. The ARM loader at 0x4F428 checks magic `20120710`, rejects ARM counts >=0x10000 words, copies from bank+0x30 into VA 0x200 in chunks <=0x800, and requires reflected CRC residue zero over the code including its appended checksum. It writes bank metadata at 0x4FFC0/0x4FFC4 and enters VA 0x200. Code1, Code0 and Golden bases are 0x1C0000, 0xF0000 and 0x20000. No CP15 instructions were found in the Boot2 executable span.

The recovered button predicate is `(bitmap & 0x00900001) == 0x00900001`, matching FX preset 1, Megaphone and Mic Mute. Holding these across DC reconnection physically booted Golden firmware **1.3.31.83**. A subsequent normal DC power cycle physically returned to **1.5.1.907**, verified again immediately before the 908 update. The ROM UART fallback at 0x20000900 is code evidence only; physical UART recovery remains untested and no external programmer is available.

## Same-size 908 bootstrap

The earlier expanded-section .gxlab remains an offline experiment. The tested 908 candidate keeps the original ARM count 0xE897, DSP header at file 0x3A2B0 and total file size **640,044 bytes**.

The entry at VA 0x244 temporarily branches into initialized data. File 0x394D0 (VA 0x3967C) holds the 976-byte bridge; file 0x39D8C (VA 0x39F38) holds the 368-byte reset helper. These are owned initialized-data spans whose original bytes are zero. The helper copies the bridge to 0x4F000 and itself to 0x4F400, clears the bridge's 24-byte BSS, then executes from the relocated copy. Before stock initialization it restores both source spans to their original zero bytes, reserves 0x4F000–0x50000 from the heap, installs the observer and registration hooks, and restores the original startup instruction at 0x244. It preserves registers/CPSR and performs data-cache line cleaning, write-buffer draining and instruction-cache invalidation around the copied code and runtime patches.

The builder checks sizes, input versions/checksums, source bytes and byte-exact reversal to the original 907 image. `.gxlab` has invalid updater magic; explicit `.bin` output preserves the GoXLR product header and uses build 908 in the outer, ARM and DSP headers.

| Candidate identity | Value |
| --- | --- |
| SHA-256 | C8607989063552E5A412AD3B29B4E1DB355D9B737DCC59D972A5349FBFAA970F |
| ARM CRC | 47E7728C |
| Outer CRC | 92C9EA43 |
| Original 907 SHA-256 | EEA92925A5D4EB0111943F72CE7BFEB0351E9390506FECA17AC6A0C3B8346C13 |

Emulation executes the actual serialized bootstrap, original allocator through 48 allocations/exhaustion, stock USB registration plus the bridge, compiled query publication and supplied DSP replies. It tests queue wrap/pressure, IRQ restoration, tick wrap, timeout/resynchronization, both modeled cache-enable states and the recovered Boot2 loader's CRC/size/Golden selection paths. It does not emulate the complete physical peripherals or prove full stock functionality.

## Physical 908 results

The authorized updater completed upload, validation, hardware validation and flash writing. The device re-enumerated on **1.5.1.908**. A fresh rebuild produced the identical candidate hash before flashing.

All **36 parameters across bands 1–12** returned identical repeated live reads through USB submit `0x00819000`, the compiled ARM bridge, DSP query 0x47, DSP reply 0x22, poll `0x00819001`, the USB-owning daemon and HTTP API. Bands are numbered 1–12; fields are Frequency, Gain and Width. This is physical read-path evidence beyond an acknowledgment or emulator result. It establishes DSP parameter-table readback, not the audible filter response or the numerical effective Q.

Boot2 and Golden were read again after flashing and match their pre-update hashes exactly:

- Boot2: `523DB594FD19665C46B80865292B122CB67AE93A3910E4799E10A08733AB3921`
- Golden: `841551DB242370C0098376BEA953F69249D3FA67790709D04CB9B940C054C11C`

Initial acoustic attempts found no captured tone and stopped before EQ writes. After establishing a measurable fixture, all twelve bands passed reversible gain, frequency and width audio checks. Gain error was at most 0.24 dB for the requested +/-6 dB changes; maximum restoration deviation was 0.98 dB. See [the per-band physical results](FIRMWARE-908.md), including excluded failed attempts and the exact tested widths. Firmware 908 remains experimental. Booting and repeated reads exercise placement/cache handling but do not measure worst-case heap use, IRQ latency, physical timeout recovery or long-run stability.

## Rust transport, persistence and application

`ReadMicEqParameter` returns typed live values. `SetMicEqParameter` validates the field/range, requires the active profile name and expected DSP value, checks the current DSP value before writing and checks DSP readback afterward. Raw gain permits -12..12 for exact restoration; legacy stock setters retain their existing limits.

Mic profiles persist all 36 optional native overrides in `GOXLR_UTILITY_MIC_EQ_NATIVE`. Legacy edits clear the affected override. XML save/load tests cover all fields and invalid inputs. Native replay is enabled only on the physically tested 908 bridge; it pre-reads targets and attempts to restore all native edits on failure before verifying them. Legacy profiles flatten extra-stage gains while preserving their frequency and width. The native app reads and controls all twelve bands on 908 with expected-value guarded writes and DSP confirmation. No numerical Q claim is made from a raw Width selector.

The isolated IPC simulation covers 36 replies, timeout errors and malformed replies. It is transport testing, distinct from the physical device reads above.

On 908, Save and Save As read all 36 fields before updating the microphone profile. A read failure aborts before changing either the in-memory profile or its file. The physical reload test verified the complete saved snapshot, including a previously untouched band changed after saving.

## Reproduce and complete validation

Build with `bash tools/build_goxlr_eq_bridge.sh <ARM-bin-directory> <build-directory> 100`. Compile the builder with `rustc --edition 2024 tools/build_goxlr_eq_lab.rs -o <builder.exe>`. Pass `<907.bin> <eq-bridge.bin> <eq-same-size-stage.bin> <new-output.bin>` to reproduce the candidate; existing output files are refused.

Run `python tools/verify_goxlr_eq_arm.py <907.bin> <eq-bridge.elf> --same-size <908.bin> --boot2 <boot2.bin> --golden <golden.bin>`. This requires the existing Unicorn binding and performs no hardware I/O. Run `cargo test --locked -p goxlr-usb -p goxlr-profile-loader -p goxlr-native-mic-eq` for Rust checks; the legacy live 907 test remains ignored unless explicitly selected.

`tools/Read-GoXLRRecovery.ps1` backs up saved profiles, saves live settings, temporarily starts the research daemon and always restores the original daemon and saved files. Select recovery reads, `-Firmware`, or `-ValidationScript tools/Test-GoXLRNativeEq.ps1`. Validation records repeated DSP reads and applies no test changes unless the fixed headphone-cup fixture passes 1 dB repeatability and 20 dB signal-to-quiet checks. It records each measurement, restores changed fields and rechecks all 36 parameters after each band. Acoustic restoration must be within 1.5 dB; gain response within 2 dB of the requested change. Frequency and width require observable differences at center/side tones.

All twelve bands completed the reversible physical field tests. Normal cold start preserved the full DSP baseline; native profile reload and application IPC round trips passed. The corrected installed daemon and native app were verified with zero final differences in all 36 DSP fields and unchanged saved-profile hashes. Remaining experimental limits are numerical effective-Q calibration, measured live memory/timing limits, induced physical timeout recovery and sustained use. Preserve 907 and Golden throughout.
