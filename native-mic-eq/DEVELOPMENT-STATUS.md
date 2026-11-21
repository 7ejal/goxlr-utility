# Development status — 2026-11-21

Firmware **1.5.1.910** is installed after one explicitly authorized update of
SHA256 `b524ac6cf1b937e9c5d224ce9794e807e3b6561652b94dc6b26111fc9ef49744`.
No subsequent firmware image or update was used. Profile **4**, gate **−52 dB**,
all 47 values and stock microphone settings match the fresh backup. Known
908/909 images, applications, profiles and recovery backups remain preserved.

| Installed file | SHA256 |
| --- | --- |
| goxlr-daemon.exe | C1F09BFCAA0B9D9ED073883423DEB6F4BBA0B064F86EA0FED0A429E3E021FEEC |
| GoXLR-Native-Mic-EQ.exe | 8D9E9E459BA96A6136DC134DBFC505F76FC88EC646E40015C936F3CCF69873DB |

Both installed paths, hashes, running processes and live settings were verified.
The existing desktop shortcut retains its native application target. No new
preview or WebView was created. Native offline tests: **25 passed, 11 ignored**.

## Physical results

- **47/47** reversible one-step write/read/raw-restoration checks passed in
  **197.66 s**. This does not prove every value or identical hidden processing
  state after every setter sequence.
- Makeup 10→12 dB measured **+2.11 dB** at 1 kHz; acoustic restoration +0.10 dB.
  This is one measured increment, not full-range gain calibration.
- De-esser intensity 0→100 measured **−9.09 dB at 8 kHz**; restoration −0.01 dB.
  With continuous playback/eight-second settling it measured **+0.01 dB at
  1 kHz** and **−1.50 dB at 2 kHz**, restoration +0.03/+0.08 dB respectively.
- These relative measurements use the user's fixture, mic EQ and Voicemeeter
  routing/EQ. Input levels differ by frequency. They are not a calibrated filter
  transfer function. Failed 4/6 kHz and compressor baselines are rejected;
  no target edit followed a failed baseline.
- Accepted measurements temporarily used gate −59 dB, with verified restoration
  to **−52 dB** in finally. No host audio configuration was changed.
- Actual native profile persistence passed in **38.66 s**: two edited
  makeup/intensity values survived Save As/load, the other 45 remained unchanged,
  then profile 4, all 47 values and stock settings were restored. The unique lab
  profile is retained as evidence.
- Native integration passed for six ARM readings, exact 47-field preset
  export/import and **38 actual Chat Mic FFT windows at 48 kHz, stereo**.
  This capture rate is not a DSP core clock measurement.
- Fresh Boot2/Golden chunks were read twice on 910 and match preserved hashes:
  Boot2 `523db594fd19665c46b80865292b122cb67ae93a3910e4799e10a08733ab3921`;
  Golden `841551db242370c0098376bea953f69249d3fa67790709d04cb9b940c054c11c`.
  Neither was written. Prior Golden fallback does not guarantee every recovery.
- A complete cold-power-cycle test has been requested; no result is claimed yet.

## Native functionality and limits

The existing 12-band EQ/raw width remains available. Only makeup and intensity
are enabled on physically checked 910, with profile/expected-value guards,
actual DSP confirmation and cache updates. Choose Save to persist these edits.
Nine other raw dynamics widgets remain disabled; full portable preset Apply is
also disabled. Import preview/export preserve all 47 exact raw fields on909/910.

The optional 1 Hz ARM panel validates metadata, clears stale/errors and pauses
during operations. Its six unsigned readings are event counters, ring address/
capacity/occupancy and native ticks. These are **not** DSP clock, utilization,
free cycles, overruns, free heap or stack high-water marks. Initial twenty
physical samples observed 1028-byte capacity and zero sampled occupancy;
this does not prove absence of peak pressure.

The optional spectrum shows recorded digital peak/RMS and a Hann FFT.
Compressor and fixed detector response graphs remain original-code models,
not acoustic transfer functions. Gain-reduction readback is unavailable.
Known firmware is detected at startup/refresh; unknown firmware is not probed.

## Remaining gates

| Feature | Missing evidence |
| --- | --- |
| Compressor threshold/ratio/attack/release | Reliable controlled static/transient measurements |
| Knee/lookahead/auto makeup | No supported independent stock control/readback; private automatic behavior is offline evidence |
| De-esser threshold/frequency/bandwidth | No independent stock controls; fixed detector/coupled reference behavior must not be relabeled |
| De-esser timing/reference controls | Independent envelope calibration |
| Numerical Q / mic HP/LP | Accurate coefficient/response calibration and supported paths |
| Headphone EQ | Physical stereo postmix sample ownership, output isolation and measured capacity |
| DSP/ARM performance | Physical clock/utilization/free-cycle/free-memory sources; computed caches are not measurements |
| Gate additions / limiter / order | Algorithm, ownership and physical behavior; stock enabled-serializer caveat remains |
| Cold boot / exhaustive regression | User power cycle, post-boot comparison and broader stock-feature/stress coverage |

## Verification

```powershell
tools/Verify-GoXLRSoftware.ps1 -ExpectedBuild 910 -BackupDirectory <fresh-backup> -DaemonSha256 C1F09BFCAA0B9D9ED073883423DEB6F4BBA0B064F86EA0FED0A429E3E021FEEC -ApplicationSha256 8D9E9E459BA96A6136DC134DBFC505F76FC88EC646E40015C936F3CCF69873DB
go test -race tools/collect_goxlr_arm_diagnostics.go tools/collect_goxlr_arm_diagnostics_test.go
```

Do not repeat write tests just to reproduce this report. Existing serialized
910/Boot2/allocator checks, 54 extra serialized and 302 guarded helper cases
remain offline engineering evidence, not physical performance measurements.
