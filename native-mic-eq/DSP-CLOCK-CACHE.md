# Original DSP clock cache — source evidence, not physical frequency

The existing research identified `P:400213` as a PCTL reader and
`P:402DBE` as its startup caller. New original-instruction execution closes
the cached value's representation and arithmetic. It does **not** measure
the GoXLR's clock or identify either physical DSP core.

## Exact calculation

`P:400214`, `400218` and `40021D` read supplied `X:FFFF7D` PCTL.
The extraction instructions select `F = PCTL[7:0]`, `R = PCTL[20:16]`
and `OD = PCTL[15:14]`. Original native floating-point helpers at
`P:032325`, `03227F` and `032293` execute the calculation:

```text
24576000 * (F + 1) / ((R + 1) * (OD + 1))
```

The literal at `P:400228..40022B` is mantissa `5DC000`, exponent `98`.
The original floating representation decodes it to exactly **24,576,000**;
it is a program constant, not an oscillator measurement. DF and PEN bits
do not enter this calculation. Its `OD+1` differs from the general hardware
PLL divider `2^OD` for OD values 2 and 3; tested startup alternatives use OD=1.
This does not establish a firmware bug: intended configuration limits remain
unknown.

| Supplied PCTL | Native result | Mantissa / exponent |
| --- | ---: | --- |
| `2760C3` | 301056000 | `47C700 / 9C` |
| `2760C9` | 310272000 | `49F980 / 9C` |
| `2760CD` | 316416000 | `4B7080 / 9C` |
| `2B60C2` | 199680000 | `5F3700 / 9B` |

The first three differ from earlier **conditional** 196/202/206 MHz figures
which assumed a 16 MHz external reference. Neither set is a physical reading.
Do not label these numerical results as measured operating MHz.

## One cached floating value

After the call at `P:402DBE`, original words `567000 03519F` at `402DC0`
store A1 to `X:03519F`; words `577000 03519E` at `402DC2` store B1 to
`X:03519E`. These are the mantissa and exponent of **one** native floating
value, not independent frequency/cycle counters. Decode with:

```text
signed24(X:3519F) / 2^23 * 2^(X:3519E - 127)
```

Reading this cache alone in a future bounded diagnostic would expose the
firmware's assumed-reference calculation. It would not establish EXTAL,
active/idle cycles, processing capacity or actual core frequency. No such
hardware getter is added here; firmware 910 remains unchanged.

## Actual serialized tests

`tools/clock_kernel.rs` extends the existing DSP lab through the optional
`--clock-kernel` flag. Default parser/dynamics behavior is unchanged.
The exact hash-pinned 901 and 909 images each pass:

- 45 original calculation words and six original cache-store words pinned.
- 439 executions across feedback/reference/output divider fields and supplied
  ignored DF/PEN combinations, using original floating arithmetic instructions.
- Seven original startup call/cache-store slices, with adjacent cache guards,
  software stack restoration and unchanged supplied PCTL checks.

The full scheduler, oscillator, PLL hardware and live DSP memory are not
present. PCTL is synthetic RAM; execution time is not hardware performance.
The tests do not claim exhaustive MMIO access isolation or full startup.

Verify the complete source image with the existing hash-pinned Go inspector,
then execute the serialized lab:

```sh
go run tools/goxlr_dynamics_map.go -firmware <exact-901-or-909.bin>
cargo run --locked --offline --release --jobs 2 --manifest-path tools/dynamics-lab/Cargo.toml -- <same-image.bin> --clock-kernel
```

901 SHA-256: `5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db`.
909 SHA-256: `74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b`.

Remaining physical gates: current PLL configuration, actual reference clock,
per-core active/idle cycles and deadline/memory margin. The verified ARM
communication counters do not satisfy these gates. The stereo headphone
postmix sample owner and isolated physical DAC route also remain unresolved;
no headphone processing is inserted by this work.
