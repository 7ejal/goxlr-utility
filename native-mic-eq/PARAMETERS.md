# Native parameter reference

The existing native app lists all 47 fields read by the 909 bridge. These are
DSP parameter keys, not arbitrary memory addresses. Actual memory-bank pointers
depend on runtime object allocation. No generic memory reader is added.

## Microphone EQ — 36 fields

| Band | Frequency key | Gain key | Width key | Frequency descriptor default (raw) |
| --- | --- | --- | --- | --- |
| 1 | 0x126 | 0x127 | 0x128 | 129 |
| 2 | 0x0F8 | 0x0F9 | 0x0FA | 198 |
| 3 | 0x113 | 0x114 | 0x115 | 76 |
| 4 | 0x129 | 0x12A | 0x12B | 129 |
| 5 | 0x116 | 0x117 | 0x118 | 198 |
| 6 | 0x11D | 0x11E | 0x11F | 76 |
| 7 | 0x12C | 0x12D | 0x12E | 129 |
| 8 | 0x120 | 0x121 | 0x122 | 198 |
| 9 | 0x109 | 0x10A | 0x10B | 76 |
| 10 | 0x12F | 0x130 | 0x131 | 129 |
| 11 | 0x0F5 | 0x0F6 | 0x0F7 | 76 |
| 12 | 0x10C | 0x10D | 0x10E | 198 |

Frequency raw 0–240 decodes as `20 × 2^(raw/24)` Hz. Gain is −12…+12 dB.
Width raw 0–32 is physically adjustable, but numerical Q remains uncalibrated.
All 12 gain descriptor defaults are 0 dB and all width descriptor defaults are
16 raw. These are field 3 of the eight-word serialized descriptors, found via
`X:41ABE9 + key -> index`, `X:41A391 + 8*index`. All 36 records match between
the exact hash-pinned 901 and 909 images. Their common mapped callback is
**P:4003C4**, not an established leaf coefficient setter.

These generic original three-band object descriptor defaults are **not** the
effective 12-band boot settings or the familiar stock band centers. For example,
band 1's descriptor raw 129 decodes to about 830 Hz, not 31.5 Hz. Effective
boot/runtime defaults remain unestablished; profiles and constructors may
override descriptor values. No reset action is inferred.

Reproduce the independent 36-record extraction with
`go run tools/goxlr_dynamics_map.go -firmware <901-or-909.bin> -eq-descriptors`.
Without that option, the original 21-dynamics output remains byte-identical.

## Dynamics — 11 fields

Defaults below are raw descriptor defaults recovered from the pinned stock
control map. They are not the user's saved values and do not authorize reset.

| Control | Key | Raw range | Descriptor default | Native setter | Units / function |
| --- | --- | --- | --- | --- | --- |
| De-esser gain reduction | 0x00A | 0–1 | 1 | P:0334B5 | 0 targets unity through a 64-sample gain transition; detector remains active, not instant dry/workload bypass |
| De-esser intensity | 0x00B | 0–100 | 50 | P:0334B9 | Stock nonlinear intensity; raw 0–2 share the minimum curve and can still reduce audio; 0 is not bypass |
| De-esser attack | 0x00C | 0–19 | 0 | P:0334CC | Nominal 99% settling time, 1–40 ms |
| De-esser release | 0x00D | 0–19 | 9 | P:0334D6 | Nominal 99% settling time, 15–3000 ms |
| Gain-reference smoothing | 0x00E | 0–2 | 0 | P:0334E6 | Retention pole, about 20.82/208.29/2080.50 ms at 48 kHz |
| Detector reference weighting | 0x00F | 0–100 | 20 | P:0334EF | Full-band reference weight, quantized in 1/4096 steps |
| Compressor ratio | 0x13C | 0–14 | 0 | P:0344E3 | 15 stock ratios, 1:1–64:1 |
| Compressor threshold | 0x13D | −50–0 | 0 | P:0344ED | dB; original profile UI exposes a smaller domain |
| Compressor attack | 0x13E | 0–19 | 1 | P:0344FF | Nominal time constant, 0.001–40 ms |
| Compressor release | 0x13F | 0–19 | 9 | P:034509 | Nominal time constant, 15–3000 ms |
| Compressor makeup | 0x140 | 0–24 | 4 | P:034519 | dB; signed logarithmic internal coefficient |

Attack selectors in milliseconds, except compressor selector 0 is 0.001 ms:
`1,2,3,4,5,6,7,8,9,10,12,14,16,18,20,23,26,30,35,40`.

Release selectors in milliseconds:
`15,15,25,35,45,55,65,75,85,100,115,140,170,230,340,680,1000,1500,2000,3000`.

Ratio selectors:
`1,1.1,1.2,1.4,1.6,1.8,2,2.5,3.2,4,5.6,8,16,32,64`.

Complete serialized P:C91 fixtures produced bit-identical output at intensity
0, 1 and 2, including more than 0.4 dB reduction for an 8 kHz probe. This is
offline kernel evidence, not physical acoustic calibration. Gain-enable 0
targets unity using the existing 64-sample transition while the detector keeps
running; neither setting establishes an instant bit-exact dry/workload bypass.

Smoothing is not a wideband/split-band switch. Weighting is not a frequency or
bandwidth control. Exact coefficient formulas and physical validation limits
are in [FIRMWARE-909.md](FIRMWARE-909.md). The app retains the processing gates:
no unsupported knee, lookahead, automatic makeup, headphone processing, limiter,
spectrum, gain-reduction or performance meter is synthesized from these values.

The optional spectrum instead uses actual Windows Chat Mic recording data, and
its levels include the endpoint recording level. Saved microphone profiles
capture the 36 EQ fields and existing stock microphone settings through the
daemon. They do not persist every advanced raw dynamics field; a portable
47-field preset export is the complete raw snapshot. Importing that portable
format previews changes and does not automatically load a microphone profile.
