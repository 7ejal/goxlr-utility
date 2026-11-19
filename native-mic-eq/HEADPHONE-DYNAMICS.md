# Headphone EQ and stock dynamics continuation — 2026-11-19

908 remains the tested microphone EQ baseline. This change adds offline research
tools and evidence only. No firmware, daemon, profile, routing or native app
controls were changed or installed. PR #3 remains draft.

## Current physical baseline

DESKTOP-MOR85DD reports full-size firmware 1.5.1.908, DICE 1.5.1.901 and FPGA 7.
All 36 microphone EQ fields returned identical repeated DSP reads and match the
last installed-state baseline. Both previously backed-up saved profiles retain
their recorded hashes. The installed daemon and native app match the hashes in
[FIRMWARE-908.md](FIRMWARE-908.md). Windows reports the GoXLR audio devices healthy.
These checks do not remeasure sound or prove long-term stability.

The preserved 640,044-byte 908 image SHA256 is
`c8607989063552e5a412ad3b29b4e1db355d9b737dcc59d972a5349fbfaa970f`.
Saved post-update Boot2 and Golden dumps still hash respectively to
`523db594fd19665c46b80865292b122cb67ae93a3910e4799e10a08733ab3921` and
`841551db242370c0098376bea953f69249d3fa67790709d04cb9b940c054c11c`.
The earlier physical FX preset 1 + Megaphone + Mic Mute DC-start test booted
Golden 1.3.31.83; normal DC-start returned to the application. That test was not
repeated here. It does not prove recovery from every flash/Boot2/Golden failure;
UART recovery and an external programmer remain untested/unavailable.

## Headphone path: logical evidence, physical gate still open

The hash-pinned inspector independently extracts the stock board route words at
ARM VA 0x36590 (file offset VA minus 0x1AC) from both 901 and 908. All 16
mixer-source entries are identical between those images:

| Output | DICE mixer channels | Destination |
| --- | --- | --- |
| Headphones | 0/1 | ins 2/3 |
| Stream Mix 1 | 2/3 | usb 0/1 |
| Chat Mic | 4/5 | usb 2/3 |
| Line Out | 8/9 | ins 0/1 |
| Stream Mix 2 | 10/11 | usb 23/24 |

This confirms separate logical destinations, not physical DAC wiring or audio
isolation under EQ. Existing canonical T81/T85/T86 evidence remains applicable:
the headphone-associated D5 scalar shares a control-dependent gain structure
with microphone and monitor controls. Its target Y bank at root+250+70 is
smoothed into the X bank at root+251+70 and bound to root+12. This is not a
verified final stereo headphone postmix hook. Do not patch it as headphone EQ.

Neither the installed API nor the 908 readback whitelist provides per-core busy
cycles, the actual core clock, DSP allocation high-water marks or ownership of
new sample buffers. The known ARM heap reservation [0x4F000,0x50000) leaves
49,552 configured heap bytes, not 49,552 measured free bytes. Existing 82/82/63
word DSP banks belong to the stock graph and cannot be treated as spare memory.
The new tests report no live DSP capacity measurement.

Native headphone frequency/gain/Q controls therefore remain gated. Before UI
integration: prove both physical headphone DAC lanes and postmix ownership;
measure timing and storage under stock worst-case load; then validate a neutral
default, actual bypass, L/R state isolation, bounded updates, impulse/response,
clipping margin and readback. Boost headroom must be validated against combined
filter response and saturation. Raw mic Width remains uncalibrated as numerical Q.
No 21-band mic expansion or new headphone processing is introduced.

## New stock control-to-setter mapping

The Go inspector reconstructs original X tables from primary boot descriptors
91 and 96 and verifies the serialized native callback calls and jump tables.
Host key map: X:41ABE9; eight-word descriptors: X:41A391; conversion/dispatch:
X:427441; mailbox pairs: X:427536; callbacks: X:427526. All 21 dynamics mappings
are identical between the exact stock 901 and tested 908 images.

| Control | Key | Raw ROM domain | Mailbox | Native destination |
| --- | --- | --- | --- | --- |
| De-esser switch candidate | 0x00A | 0..1 | 0x22 | P:0334B5, Y:core+1 |
| Existing de-esser amount | 0x00B | 0..100 | 0x23 | P:0334B9, wrapper X:+30/+31, derived core coefficients |
| De-esser timing A candidate | 0x00C | 0..19 | 0x24 | P:0334CC, table X:41B970, wrapper X:+36/+37 |
| De-esser timing B candidate | 0x00D | 0..19 | 0x25 | P:0334D6, table X:41B998, wrapper X:+38/+39 |
| De-esser three-value selector | 0x00E | 0..2 | 0x26 | P:0334E6, table X:41B9C0, Y:core+10 |
| De-esser scalar candidate | 0x00F | 0..100 | 0x27 | conversion 6, P:0334EF, Y:core+13 |
| Existing compressor ratio | 0x13C | 0..14 | 0x9F | P:0344E3, table X:41BD70, wrapper X:+9/+10 |
| Existing compressor threshold | 0x13D | -50..0 | 0xA0 | P:0344ED, wrapper X:+11/+12 |
| Existing compressor attack | 0x13E | 0..19 | 0xA1 | P:0344FF, table X:41BD8E, wrapper X:+13/+14 |
| Existing compressor release | 0x13F | 0..19 | 0xA2 | P:034509, table X:41BDB6, wrapper X:+15/+16 |
| Existing compressor makeup | 0x140 | 0..24 | 0xA3 | P:034519, wrapper X:+17/+18, derived Y coefficient |

De-esser mailboxes select P:402A4C -> P:0334A3 with object-pointer slot 5 in
X:419E2C. Compressor mailboxes select P:402A32 -> P:0344D1 with slot 17.
Two sibling compressor families, keys 0x141..145 and 0x15A..15E, use pointer
slots 18 and 16, respectively. Their physical roles are not assigned here.
These global pointer slots are not root-child offsets or physical channels.

The scalar conversion table has pairs [0,0], [100,4096]. The three selector
words are captured in `stock-control-map.json`; their user-facing meanings are
unproven. Timing A/B resemble attack/release control paths (INFERENCE); exact
units, ordering and audible behavior require validation. No independently
adjustable center-frequency, bandwidth or knee control was established by this
mapping. Do not label those unknown controls as Hz, Q or knee.

## Stock versus proposed native controls

The existing Utility already serializes compressor threshold, ratio, attack,
release and makeup plus de-esser amount. Its profile limits are threshold
-40..0 dB, ratio indices 0..14, attack/release indices 0..19, makeup -6..24 dB
and de-esser amount 0..100. The ROM clamp differs for threshold and makeup;
do not expand ranges or claim negative makeup works from host validation alone.
Ratio labels span 1:1 through 64:1; timing labels in `types/src/lib.rs` are host
labels, not newly measured timing. The native mic EQ app has no dynamics editor.

Exposing already supported stock controls would preserve the original
processing algorithm. The five additional de-esser controls are stock setter
candidates, not hardware-validated app features. Adjustable knee, new ratio or
timing curves, frequency/bandwidth de-essing and replacement detector algorithms
would require further algorithm/parameter proof and physical tests. They remain
disabled. The stock DeEsser P:000C91 two-stage recurrences and SimpleComp
P:00441D/P:004473 remain the earlier algorithm research starting points.

## Executed tests and reproduction

The Rust lab loads code and boot ROM data from the actual serialized 901/908
files. For each of 21 keys it executes native SysEx 0x35, clamping, conversion,
dirty-slot staging, SysEx 0x47/reply 0x22 and P:402AA2 callback dispatch at five
values: below minimum, minimum, default, maximum and above maximum. It checks
all 179 mailbox dirty flags, native lookup-pair stores, switch/selector stores
and two surrounding Y-bank guards. All 105 cases pass for each image: 210 total.
This executes original native setters, not a host dynamics approximation.

The harness supplies synthetic graph pointers, rate setup, memory backing,
mailboxes, outbound ring and a return sentinel. The scheduler is stopped; the
SHI peripheral, real allocations, PCM algorithm response, complete memory
canaries and physical audio are not tested. The raw DSP reply is parameter-table
state, not a measurement of effective attack, release, compression or de-essing.
908 cannot expose these dynamics replies to Windows through its EQ-only bridge.

From the repository root, with Go and Rust available:

```sh
go run tools/goxlr_dynamics_map.go -firmware <901-or-908.bin>
cargo run --locked --offline --manifest-path tools/dynamics-lab/Cargo.toml -- <901-or-908.bin>
```

The inspector rejects unrecognized image hashes. Run it before the lab harness.
Canonical `nice -n 15 bash VERIFY_ALL.sh` passed on main-server, constrained to
two CPUs. These are offline checks, not hardware feature approval.

Next: trace wrapper-to-core object construction and the de-esser selector/scalar
consumers, derive timing and transfer functions from native code, then plan
bounded physical measurements with trustworthy restoration/readback. Continue
physical DAC mapping independently. A new firmware image requires its own
specific authorization; no image was created or flashed in this continuation.
