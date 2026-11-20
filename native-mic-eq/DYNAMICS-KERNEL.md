# Original dynamics kernel evidence — 2026-11-20

`tools/dynamics_kernel.rs` executes machine code loaded from the hash-verified
901/909 images. It does not replace the compressor or detector with a host
algorithm. Host formulas are independent comparisons against native results.
No hardware, profile, routing or firmware changes occur in this lab.

## Compressor transfer

The experiment executes the original log/antilog generator slice
P:407CC3..407D2A, including the original floating-point helpers, then the
original compressor table-binding function P:409820. The generated 33-entry
tables match `log2((i+32)/32)*2^17` and `2^(i/32-1)*2^23` within the tested
fixed-point precision. The signed-positive unity endpoint is saturated to
0x7FFFFF. No synthetic table contents are injected.

P:00441D implements a hard knee in this fixture. For normalized input `x`,
define:

```
s = (0x550A96 / 2^23) / 256
Tcoef = (threshold_dB - 6) * s
slope = 1/ratio - 1
Mcoef = (makeup_dB - native_float(0x84,0x780A3E)) * s
target_log_gain = min(max(log2(abs(x))/64 - Tcoef, 0) * slope + Mcoef, 0)
ideal_output = x * 32 * 2^(64 * target_log_gain)
```

Threshold raw zero forces an effective ratio of 1:1. In the native normalized
domain, the knee is approximately `threshold_dB - 6`; this is **not** calibrated
microphone input dBFS. The manual-mode baseline offset is about +0.0933 dB,
because the internal offset is approximately 30.01 dB whereas `20log10(32)` is
approximately 30.103 dB. For ratio 4, threshold -20, makeup 0, examples are:

| Normalized input dB | Native normalized output dB |
| --- | --- |
| -30 | -29.9067 |
| -26 | -25.9067 |
| -24 | -25.4060 |
| -20 | -24.4065 |
| -10 | -21.9067 |

The original kernel interpolates gain over eight samples using P:004459
`ASR #3`. Thirty actual target-slice checks establish a steady residual of
0..7 Q23 linear-gain LSBs. Consequently, the ideal graph is not bit-exact at
strong gain reduction: the 681 curve/makeup/polarity fixtures have maximum
ideal-model error 0.111360 dB. The original threshold -20 subset remains within
0.002823 dB. Extended checks bound the error using the measured gain-ramp
quantization plus the interpolation allowance, rather than hiding it with a
uniform loose tolerance.

## Timing, callback and lookahead limits

P:004473 copies and processes eight successive groups of eight samples, then
returns a 64-frame block. Four executed wrapper fixtures exactly match eight
successive P:00441D calls, including the final envelope and log-gain states.
Five impulse positions produce a nonzero output at the same sample position
and zero elsewhere. This demonstrates no inserted sample-delay or pre-echo
inside this callback fixture; it is not an end-to-end physical latency test.

The detector envelope updates once per sample with the original attack/release
coefficients `1-exp(-1/(48000*t_seconds))`. Forty executed 64-frame callback
cases verify all 20 attack and 20 release selectors. The log-gain state updates
once per eight-sample group using the separate attack coefficient
`1-exp(-64/(48000*t_seconds))`. Twenty coefficient-pair checks and 160 actual
log-gain recurrence checks verify this distinction. It must not be flattened
into a claim that the entire processor settles in the nominal selector time.
No independent host knee, hold or lookahead control is established.

## Internal automatic makeup

Original P:409830 stores an internal switch at outer object X:+20.
P:03440D..034412 conditionally recomputes the makeup coefficient when ratio or
threshold is applied. Across all 15 ratios, thresholds -50..0 and both switch
states, 1530 native coefficient cases pass. In automatic mode, the equivalent
manual-makeup value is:

```
min(0.75 * (6 - threshold_dB) * (1 - 1/ratio), 30) - 6 dB
```

The ratio-1 or threshold-zero branches instead select 0 dB. Small ratios can
therefore result in attenuation rather than positive makeup. Sixty additional
full-PCM cases exercise this automatic path.

The switch alone does not recompute gain. An executed activation sequence
proves that threshold/ratio must be refreshed; a direct manual-makeup setter
overrides the current gain even while the private switch remains enabled,
until that refresh. The current runtime switch value/default and a supported
USB descriptor for it are not established. Do not expose a working automatic
makeup control merely because this internal setter exists.

## De-esser

Fourteen native detector PCM cases execute P:000D24..000D36 using original boot
Y:00D0 (48 kHz) and Y:0040 (44.1 kHz) coefficient banks. They match the exact
signed-24 coefficient transfer within 0.001 dB and a third-order Butterworth
high-pass with a fixed 4 kHz corner within the tested tolerance. Original
P:4069CC selects the bank by sample rate. The five recovered extra controls do
not provide a tunable frequency or bandwidth.

Fifteen P:000D4B..000D51 slices multiply the original unfiltered sample by one
gain. This is wideband reduction in that callback, not split-band recombination.
The complete original callback, gain-control loop and intensity/smoothing
interactions now have additional offline checks in [DEESSER-KERNEL.md](DEESSER-KERNEL.md).
Calibrated acoustic response and the live graph/port allocation remain open.

## Meter and integration gates

Core X:+7 contains smoothed **total** log gain, including makeup. It is not a
gain-reduction-only meter. Y:bank+5 is the applied ramp coefficient, scaled by
32 in the output path. Core X:+9 tracks a maximum target under the inspected
instructions; its reset/consumer semantics remain unproved. None of these
states is readable through the current 47-field bridge.

Runtime allocations, envelope storage, input/output rings and the rate are
synthetic. The original main-graph allocations at P:40494A/P:404958 and bindings
at P:405135 connect root+243/+244 pointers to the compressor, but their complete
runtime LUT producer identity is still open. The test uses a real original LUT
generator from another stock class and an explicitly bound fixture; it is not
a complete boot-graph or physical-audio proof. The private automatic switch is
explicitly set by the lab; no constructor-default claim is made.

## Reproduction

The separate `--graph-provenance` experiment executes the original main
constructor and three original compressor binding calls on both serialized
901 and 909. It asserts that root+243/+244 reference shared resources 10/11
and that all three bindings receive those exact pointers. Their contents remain
zero at the bounded stop P:4058BD. This proves allocation and pointer binding,
not LUT generation, complete startup, upstream audio identity or physical audio.

Verify the image with the existing hash-pinned Go inspector, then run:

```sh
cargo run --release --locked --offline --manifest-path tools/dynamics-lab/Cargo.toml -- <901-or-909.bin> --kernel
```

Per-image categories: 1530 automatic coefficients, 66 generated LUT entries,
14 detector PCM responses, 15 wideband output slices, 20 block-pole pairs,
681 compressor PCM cases, 30 gain-ramp residuals, 160 log-gain recurrences,
40 attack/release callback cases, 4 wrapper comparisons, 5 impulse positions,
and one multi-step automatic activation sequence. These are offline checks,
not measured DSP utilization, free cycles, physical latency or acoustic results.
