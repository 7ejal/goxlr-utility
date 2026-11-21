# 910 bounded ARM diagnostics — authorized and physically enumerated

909 remains preserved as a rollback image. The exact authorized 910 image adds
read-only ARM telemetry. It adds no DSP processing and does not measure DSP
clock, utilization, free cycles, memory or audio latency.

| Item | Value |
| --- | --- |
| Image | GoXLR-1.5.1.910-ARM-DIAGNOSTICS-EXPERIMENTAL.bin |
| Size | 640044 bytes, unchanged |
| SHA256 | b524ac6cf1b937e9c5d224ce9794e807e3b6561652b94dc6b26111fc9ef49744 |
| ARM / outer CRC | E2DF7CCC / 9E1EFA22 |
| ARM word count / DSP offset | 0xE897 / 0x3A2B0, unchanged |
| Bridge code/data / BSS | 1224 / 24 bytes; ends 0x4F4E0 |
| Reset helper | 644 bytes at 0x4F800..0x4FA84 |
| Reservation | Existing 4096 bytes; original worker addresses 0x4FFC0/0x4FFC4 untouched |

## Initialized-data preservation

The first build was rejected before output: expanding the old 1000-byte zero
carrier would overwrite nonzero initialized ARM data. The corrected build
copies that exact 224-byte suffix from the hash-pinned 907 construction baseline
into the reset helper. Early reset relocates code, clears bridge BSS, restores
the original source bytes and clears the source carrier before stock startup.
All original data values are retained; nonzero data is never assumed disposable.
The 909 and 910 DSP sections differ only at the build-number byte 0x3A2BE.

The helper moves within the same reserved 4 KiB. Existing IRQ/FIQ and register
preservation, cache-clean/write-buffer/instruction-cache operations and stock
return path remain in place. Actual serialized execution verifies restoration
with both modeled cache-control states. Physical cache behavior and timing
remain unmeasured. No new heap or DSP allocation is introduced.

The builder rejects wrong source/helper literal addresses, incorrect helper
end/size and a mismatched preserved suffix before opening its output. A fresh
guarded build matches the recorded image byte for byte. Default bridge builds
still reproduce the tested 909 bridge and reset helper exactly.

## Bounded protocol

Existing group 0x819 submit/poll commands retain all 47 EQ/dynamics keys.
Subcommand 2 accepts exactly a metric LE16; the response is status, metric LE16
and an **unsigned** LE32 value. Unknown metrics and malformed requests fail.
The handler reads under the existing IRQ/FIQ lock; it does not enqueue DSP
messages or consume a pending EQ reply.

| ID | Metric | Proven source / meaning |
| --- | --- | --- |
| 1 | CpuAudioBit22Count | Original ARM 0x3ED64 event counter |
| 2 | CpuAudioBit23Count | Original ARM 0x3ED88 event counter |
| 3 | EffectRingCapacity | Validated ring capacity, 0x404 bytes |
| 4 | EffectRingBuffer | Validated ring buffer address, 0x3D79D |
| 5 | EffectRingUsed | Occupied bytes from validated wrapping read/write indices |
| 6 | SchedulerTickLow | Original 0x10E80 getter, wrapping native tick low word |

Ring metadata is owned at 0x38DD4 and must match its exact buffer/capacity and
bounded indices. Tick getter 0x10E80 reads the clock pointer at 0x3FA10 and the
64-bit value at pointer+8; it makes no OS call. Tick duration is uncalibrated.
The counters reset on CPU-audio reconfiguration. They are not established
overrun/underrun counters, cycle counters or DSP load indicators.

## Validation and remaining gates

- Original 25-entry USB registration, query/reply, IRQ restoration, queue
  pressure, timeout/resynchronization and all 47 allowed/313 rejected keys pass.
- Recovered Boot2 executes the serialized ARM loader, accepts size/CRC, rejects
  corrupt CRC/oversize and selects Golden for the recovered three-button mask.
- Original allocator exhaustion preserves the reservation and relocated code.
- 54 additional serialized cases cover source restoration, checksum-valid
  corruptions, pending EQ coexistence and all IRQ/FIQ mask combinations.
- 302 guarded executions of the linked helper restrict reads/writes, test
  unsigned values/wrap and reject corrupt ring metadata and unknown IDs.
- USB framing and typed host transport tests preserve full unsigned values.
  The host rejects non-910 devices before sending a diagnostic command.

The user authorized this exact SHA256; one update completed and 910 enumerated.
All 47 reversible write/read/raw-restoration checks passed in 197.66 seconds.
Profile 4, gate -52 dB, all 47 values and stock settings were preserved. Six
actual ARM metrics were collected. Fresh Boot2/Golden chunks were read twice
and match preserved hashes; neither recovery partition was written. This does
not prove every recovery failure mode or physical cache/timing margin.

The installed native app enables only physically checked makeup and de-esser
intensity on 910. Both update the profile cache after DSP confirmation; actual
Save As/load tests preserved two edited values, then restored profile 4 and all
settings. Other dynamics widgets remain disabled. See DEVELOPMENT-STATUS.md.
No further flash is needed for these software changes. A changed image still
requires its own explicit authorization.

## Reproduction

```sh
GOXLR_DYNAMICS_BRIDGE=1 GOXLR_ARM_DIAGNOSTICS=1 bash tools/build_goxlr_eq_bridge.sh <arm-toolchain-bin> <new-build-directory> 100 <preserved-907.bin>
rustc --edition=2024 tools/build_goxlr_eq_lab.rs -o <builder>
<builder> --build 910 <preserved-907.bin> <build>/eq-bridge.bin <build>/eq-same-size-stage.bin <new-910.bin>
python tools/verify_goxlr_eq_arm.py <preserved-907.bin> <build>/eq-bridge.elf --same-size <910.bin> --dynamics --arm-diagnostics --boot2 <boot2.bin> --golden <golden.bin>
python tools/verify_goxlr_arm_diagnostics.py --help
go test tools/collect_goxlr_arm_diagnostics.go tools/collect_goxlr_arm_diagnostics_test.go
```

After an approved update only:
`go run tools/collect_goxlr_arm_diagnostics.go -output arm-metrics.json`.
The report distinguishes host request duration from native ticks and does not
invent unavailable DSP performance values.
