# Development status — 2026-11-20

Firmware 909 remains installed. No 910 update has been authorized or performed.
The existing native Windows app and matching 910-aware daemon were installed
with executable/profile backups, then started in the normal user session.

| Installed executable | SHA-256 |
| --- | --- |
| goxlr-daemon.exe | fba3d1dad64729b99aebce911908599e9c6a48f11ce57ddde1ff9c7b0219d6b5 |
| GoXLR-Native-Mic-EQ.exe | 80ae022c7cfacce43e43f176684d75d393862cd96efda3480e56b88a528e9dd2 |

Post-install checks verified both running executable paths and hashes, firmware
1.5.1.909, microphone profile **4**, gate **−52 dB**, every stock microphone
setting and all **47** actual DSP values against the fresh pre-install backup.
The installed daemon rejected a 910-only diagnostic on 909. Actual native IPC
readback passed after installation. The existing desktop shortcut retains its
installed application target. No DSP writes were needed for these checks.

## Completed and measured scope

- Previous 909 reversible write/read/restoration verification covers all 47
  fields. It does not establish acoustic calibration of advanced dynamics.
- The native app includes exact live readback, all 47 parameter descriptions,
  current values and recovered descriptor defaults, capability gating, portable
  preset export/import preview, existing microphone profile operations, optional
  microphone activity and real Windows Chat Mic capture/FFT/levels.
- Native unit tests: 21 passed, nine explicitly ignored hardware tests. Actual
  passive capture, current-profile Save with complete restoration checks,
  preset codec/readback and microphone activity were separately exercised.
  Physical capture produced 38 FFT windows at 48 kHz, stereo. This is host
  capture evidence, not physical DSP clock or latency measurement.
- Three Go diagnostics tests pass with the race detector. The collector refuses
  909 before sending diagnostics or creating an output. Six USB transport tests
  pass, including unsigned response handling. Host packages check and both
  Windows release executables build successfully.
- Serialized compressor and complete de-esser kernel fixtures execute original
  instructions, with explicit synthetic allocation/port/rate assumptions.
  Native manual compressor and fixed detector response graphs use that evidence.
  Their curves are models, not calibrated physical response.
- [910](FIRMWARE-910.md) preserves initialized data and the original 47-field
  bridge. Serialized reset, allocator, registration, cache-state models and
  recovered Boot2 checks pass; 54 additional serialized cases and 302 guarded
  linked-helper cases pass. A fresh guarded build is byte-identical.

## Partially completed and blocked scope

| Feature | Current limit / next evidence required |
| --- | --- |
| Advanced compressor/de-esser writes | Read/write verified; acoustic calibration pending a quiet, fixed fixture. Advanced GUI writes remain disabled. |
| Automatic makeup | Original private switch and refresh behavior recovered; no supported descriptor/readback or physical control. |
| Knee/lookahead | Original compressor has a hard knee; adjustable knee and physical lookahead are unproved. |
| De-esser frequency/bandwidth | Fixed detector; discovered smoothing/weighting fields are not frequency or bandwidth. Split-band mode unproved. |
| Gain reduction meters | Internal candidate states identified, but live ownership, pure reduction semantics and transport are unverified. No fabricated meter. |
| Numerical Q / microphone HP/LP | Raw width remains available; calibrated numerical Q and additional filter paths unverified. |
| Headphone EQ | Actual stereo DAC postmix hook and output isolation unverified. Two CPU paths proved occupied before mixing. No extra EQ inserted. |
| DSP clock/load/free cycles/memory | No supported measured physical source. Emulator timings are not hardware performance. |
| ARM diagnostics | Six read-only metrics prepared for 910; actual telemetry awaits exact-image approval and physical validation. No free heap/stack high-water claim. |
| Presets | All 47 fields persist in portable export/import preview; complete hardware apply remains disabled. Stock microphone profiles do not persist every private dynamics field. |
| Noise gate/limiter/order | Existing stock gate preserved; additional algorithms and processing-order changes unverified. |
| Full regression/recovery | Existing recovery observations retained; sustained stock-feature, cold-boot and failure-mode coverage is incomplete. |

There is no analog headphone loopback cable. Voicemeeter routing and its existing
EQ are preserved. The acoustic fixture is not ready; no additional probe audio
was played. Three-button Golden fallback previously worked on the physical unit,
but that does not guarantee recovery from every failure or failed USB state.

## Recheck an installed software update

Run the reusable read-only verifier against the backup created immediately
before that software installation:

```powershell
tools/Verify-GoXLRSoftware.ps1 -BackupDirectory <backup-directory> -DaemonSha256 FBA3D1DAD64729B99AEBCE911908599E9C6A48F11CE57DDDE1FF9C7B0219D6B5 -ApplicationSha256 80AE022C7CFACCE43E43F176684D75D393862CD96EFDA3480E56B88A528E9DD2
go test -race tools/collect_goxlr_arm_diagnostics.go tools/collect_goxlr_arm_diagnostics_test.go
```

The verifier compares JSON settings by property path, independent of map order.
It does not write parameters or flash firmware. Firmware update helpers still
do not authorize 910; image-specific approval is required first.
