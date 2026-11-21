# Original de-esser callback validation

2026-11-20. Offline execution of original serialized 901 and 909 instructions,
not physical calibration or measured DSP capacity. No replacement processing
algorithm or host-generated lookup table is injected.

## Executed path and fixture

`tools/deesser_kernel.rs` executes the complete P:000C91..000D5D callback.
Object construction uses P:406933, LUT binding P:4069A7, input-port binding
P:4069AD, output binding P:4069C1 and rate selection P:4069CC. Each control goes
through the original SysEx 0x35 parser/conversion and P:402AA2 dispatcher. In
particular, key 0x00F must use its converted mailbox representation, not an
unconverted raw integer written directly to the setter.

The object at X:003200, coefficient bank Y:00B000, filter histories X/Y:00B100,
input Y:00B200, output Y:00B300 and long envelope states L:00B400..00B402 are
explicit synthetic allocations. Both input ports receive the same PCM buffer;
the reference-power port processes 32 samples and the processing port 64.
Their actual upstream graph identities remain unproved. Cached attack/release
float pairs are copied from original selector tables and setters then run.
The original P:407CC3..407D2A factory generates the log/antilog tables used by
the fixture. This is an original factory from another stock class with explicit
bindings; complete main-graph table producer identity is still open.

## Controls and processing

| Key / setter | Raw range; ROM default | Confirmed function |
| --- | --- | --- |
| 0x00A / P:0334B5 | 0..1; 1 | Gain-target enable bit. Zero targets unity through the existing sample ramp; filtering, envelopes and reference estimation continue. |
| 0x00B / P:0334B9 | 0..100; 50 | Nonlinear intensity and RMS-dependent reference floor. Values 0, 1 and 2 produce the same minimum curve, not bypass. |
| 0x00C / P:0334CC | 0..19; 0 | Rising full-band and high-pass envelope retention; nominal 99% settling table 1..40 ms. |
| 0x00D / P:0334D6 | 0..19; 9 | Falling envelope retention; nominal 99% settling table 15..3000 ms. Selectors 0 and 1 both map to 15 ms. |
| 0x00E / P:0334E6 | 0..2; 0 | Adaptive reference retention, approximately 20.82 / 208.29 / 2080.50 ms e-folding at 48 kHz; not wideband/split-band selection. |
| 0x00F / P:0334EF | 0..100; 20 | Full-band envelope contribution to the lower bound of the adaptive reference, with 1/4096 coefficient quantization. |

The existing detector tests establish the fixed 4 kHz third-order high-pass,
including original 44.1/48 kHz coefficient selection. These controls do not
provide adjustable frequency, bandwidth, absolute threshold or split-band mode.
Original P:000D4B..000D51 multiplies the unfiltered processing input by a single
gain. Full-callback sample comparisons now confirm this wideband operation
under non-unity gain, rather than only examining that isolated output slice.

For intensity `a=max(raw,2)/100`, the tested original coefficients are:

```text
slope = 10^(-a) - 1
floor_dB = clamp(8 + 44*(a-.5), 8, 30)
floor_coefficient = (signed_Q23(0x80346E)/32) * 10^(-floor_dB/20)
```

The 8..30 dB term controls an internal RMS-dependent reference floor. It is not
an absolute microphone threshold or a promised maximum acoustic reduction.

P:000C91..000CA2 updates reference-port power over 32 samples:
`power=.99*power+.01*sample^2`, with native fixed-point rounding. Original
log/exp instructions estimate its square root. For each processing sample,
P:000D0E..000D23 tracks the full-band envelope and adaptive reference;
P:000D24..000D36 filters the detector; P:000D37..000D41 tracks its magnitude.
The envelope recurrence uses the selected rise/fall retention and a floor near
0.001. Native 24-bit difference truncation matters when predicting long-state
updates; the harness models it explicitly.

Let `rms` be the reference-port RMS estimate, `p` the mode retention and `w` the
converted weighting coefficient. Before the high-pass clamp the reference is:

```text
target_reference = -32 * floor_coefficient * rms
difference = floor((old_reference-target_reference)*2^23)/2^23
reference = max(0.001, target_reference + p*difference)
if HP_envelope > reference:
    reference = max(reference, fullband_envelope_high_word*w)
reference = min(reference, HP_envelope_high_word)
```

The actual floor coefficients replace the displayed rounded 0.001 value.
The 48-bit/high-word distinctions and native quantization are retained in the
test. At each 64-sample boundary, the previous reference and high-pass envelope
produce `min((reference/HP_envelope)^(-slope),1)` through original log/exp
instructions. The enable bit selects unity instead. The coefficient bank then
weights old/new block targets; the tested initialization sets these weights to
0 and approximately 1. P:000CEA..000CF1 makes a 64-sample linear gain ramp.
ASR truncation can leave up to 63 Q23 gain LSBs of ramp residual.

## Enable, output and potential meter state

Nine complete disabled-callback tone tests stay within 0.001 dB of dry across
all three modes and three intensities while the detector remains active.
Disabling an already reducing fixture produces first/last output gains
0.424149 / 0.990992 in its first block, with the next coefficient near unity at
block end. The activity flag remains 1 for that transition block and clears in
the next. This is not an immediate dry bypass or DSP-workload bypass.

Y:bank+4 contains the next linear gain coefficient; Y:bank+5 contains its sample
step. The final applied sample gain is their difference after the callback.
X:object+14 is the block target. X:object+29 is only an activity flag: the last
applied gain compared with the approximately 0.999 threshold at X:object+27.
Twenty-seven full-callback cases assert both this flag and the next-coefficient
relationship. A potential reduction readout is `-20*log10(applied_gain)`, but
these internal states are not exposed by the current 47-parameter bridge and
their live allocation is unproved. Do not present the Boolean control or flag
as a quantitative meter.

## Results and reproduction

Per hash-verified image, the new harness passes 66 original LUT entries,
9 disabled cases, 3 identical minimum-intensity cases, 101 intensity setters,
40 envelope cases, 16 RMS cases, 6858 block targets (maximum linear error
0.00009081), 1728 sample reference recurrences and output multiplies (1152 at
reduced gain), 197 weight-floor and 173 HP-clamp branch observations,
27 activity/next-gain checks, and 44 one-second PCM tone cases. The tone cases
assert finite/no-amplification output, minimum-intensity equivalence,
increasing high-frequency reduction with intensity, negligible 1 kHz change,
and the tested weighting trend. These counts overlap and are not one aggregate
test count.

At 0.1-peak sine, 48 kHz, attack 0, release 9, mode 0 and weight 20, the final
half-second wet/dry RMS changes are:

| Intensity | 1 kHz | 4 kHz | 8 kHz | 12 kHz |
| --- | --- | --- | --- | --- |
| 0 / 1 / 2 | -0.000070 dB | -0.344328 dB | -0.450787 dB | -0.434475 dB |
| 50 | -0.000423 dB | -5.234245 dB | -6.854848 dB | -6.606382 dB |
| 100 | -0.000555 dB | -9.694168 dB | -12.828259 dB | -11.413198 dB |

These are simulated original PCM-kernel results for the declared equal-port
fixture. The slower reference modes are not necessarily settled after one
second. These values are not physical transfer measurements or calibration.

```sh
cargo run --release --locked --offline --manifest-path tools/dynamics-lab/Cargo.toml -- <901-or-909.bin> --kernel --deesser
```

901 SHA-256: `5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db`.
909 SHA-256: `74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b`.
Evidence logs: `work/deesser-kernel-20261120-901.log` and
`work/deesser-kernel-20261120-909.log` in the continuation workspace.

Live graph/port/LUT allocation, acoustic parameter calibration, physical gain
meter capture and sustained physical timing remain open. No firmware, USB,
profile or audio-device operation is performed by this harness.
