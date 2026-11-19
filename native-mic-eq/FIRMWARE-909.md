# 909 dynamics bridge — prepared, physical validation pending

## Physical 909 update and 47-field validation — 2026-11-19

The user ran the exact-image updater and typed 909. The recorded phases reached
HardwareValidate, HardwareWrite and Complete; independent live status reports
1.5.1.909. All 36 microphone EQ values matched the pre-update snapshot after
reconnect. No cold power-cycle or induced recovery failure was tested.

The actual native `native_all_parameters_reversible_round_trip` passed all 47
individual field checks in 187.73 seconds: 36 EQ and 11 dynamics fields. Each
changed by one valid step, read back through the physical bridge and restored
the complete 47-field snapshot before proceeding. A separate final verification
read every field twice; all matched their baselines. This validates live control
and restoration, not the complete range or calibrated processing response.

Fresh post-update Boot2/Golden captures were read twice per chunk and match:

- Boot2: `523db594fd19665c46b80865292b122cb67ae93a3910e4799e10a08733ab3921`
- Golden: `841551db242370c0098376bea953f69249d3fa67790709d04cb9b940c054c11c`

All 95 pre-update backup files still verify. Private evidence is in
`physical-909-user-test-17978`: complete transcript, before/after snapshots,
independent-final-47.json, recovery captures and verification-summary.json.
Windows PowerShell initially delayed transcription of native stdout until the
test ended; all 47 PASS lines and the successful exit are now in the transcript.
Future runs additionally retain a dedicated native-all-parameters.log.

Settings exception outside these 47 fields: cached noise gate threshold was
-40 dB before the update and -52 dB afterward. Both preserved original/live
profiles specify -40. The user has been asked whether this was intentional;
no gate write was issued and cause is not established. Chat volume also differs
from the original snapshot during subsequent checks and was left untouched.
Do not claim all microphone/mixer settings unchanged until these are resolved.
GUI dynamics editing remains disabled pending processing validation. Physical
headphone DAC isolation and live DSP utilization/free-memory remain unmeasured.

## Authorized user-controlled update preparation

On 2026-11-19 the user authorized the exact SHA256 below for a user-controlled
update and reversible tests. The firmware bytes are unchanged. Both matching
Windows executables are now installed and their hashes verified. Original
Utility/native executables, all profiles, configuration, known 908 image and
Boot2/Golden are backed up: 95 files verified against a private SHA256 manifest.
The existing bridge cannot read the running application bank; the 908 backup
is the preserved exact tested image, not a fresh application-bank dump.
The 36 current microphone parameters were read twice and saved before installation.
Live profiles were saved for restart, with the original saved bytes separately
preserved. The workflow retains that live baseline on disk after testing so a
later restart does not discard unsaved pre-update microphone edits.

Automatic approval review blocked restarting the installed Utility and supplied
no reason. The daemon is currently stopped after the successful installation;
no firmware update or post-install physical tests have run. No bypass attempted.
`tools/Run-GoXLR909Test.cmd` is a user-run continuation: it starts the installed
daemon, verifies backups and the 36-field baseline, displays the exact firmware
hash and requires the user to type 909 before updating. Then it verifies the
baseline again, captures the 11 real dynamics reads, runs the actual native
47-field test and checks restoration. It stops on fallback, mismatch or timeout.
Logs remain private in the configured lab output directory. This is an update
and validation launcher, not a new audio application.

`native_all_parameters_reversible_round_trip` edits each of 36 EQ and 11 dynamics
parameters by one permitted step, verifies all 47 fields after each edit and
restores the full baseline before continuing. It attempts restoration even if
the intervening snapshot read fails. This test compiled and the isolated IPC
tests pass; it remains ignored and has not run on physical 909. GUI dynamics
editing stays disabled pending physical validation and separate processing tests.

## Earlier preparation evidence

Continuation from c62e0a8. The running physical unit remains on 908. No firmware
was flashed, no replacement daemon/application installed and no dynamics
parameters written to the physical device in this work. Native dynamics controls
remain disabled pending physical validation. This is not a headphone EQ image.

## Prepared image and preservation

Build 909 was unused and reserved as `dynamics-readback-whitelist-r1` before
serialization. `BUILD-909.json` records its identity.

| Item | Value |
| --- | --- |
| Filename | GoXLR-1.5.1.909-DYNAMICS-READBACK-EXPERIMENTAL.bin |
| Bytes | 640044 |
| SHA256 | 74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b |
| ARM CRC | D7F16D2B |
| Outer CRC | 791AE153 |
| ARM count / DSP header | 0xE897 / file 0x3A2B0, unchanged |
| Bridge load/state | 992 / 24 bytes, ending at 0x4F3F8 |
| Reset helper | 0x4F400, unchanged location; 8-byte separation |

The exact 908 bridge is reused with 16 additional load bytes for read permission
on de-esser keys 0x0A..0x0F and microphone compressor keys 0x13C..0x140.
Its original 36 EQ keys, framing, observer, lock, timeout and ring publication
remain supported. No extra state, DSP algorithm, DSP allocation, clock or audio
route is added. The linker now rejects overlap with the existing reset helper.
Default builds without `GOXLR_DYNAMICS_BRIDGE=1` reproduce the exact tested 908
bridge bytes. The 907 construction baseline and original 908 binary are preserved.

The builder still proves byte-exact reversal to 907, preserves section size and
recomputes both checksums. The independent ARM verifier executes the serialized
image's early copy, state clear, stock registration and query/reply hooks.
Original Boot2 code accepts its ARM CRC/size and selects Golden for the recovered
three-button mask in emulation. Existing physical Golden evidence is in
ARM-PLACEMENT.md; it is not a universal recovery guarantee. No new flash is
authorized merely by preparing or structurally validating this image.

## Exact newly recovered de-esser controls

All six keys dispatch through P:402A4C -> P:0334A3. Original P:4003F4 registers
global pointer slot 5 from root+30. Root+30 is the original DeEsser constructor
P:4068E4; its P:4068F2 initializer binds callback P:000C91.
`core` below is the Y coefficient bank referenced by object X:+8.

| New control | Key / raw range | Original operation |
| --- | --- | --- |
| Gain reduction enable | 0x0A / 0..1 | P:0334B5 writes Y:core+1. P:000CD8..CD9 checks bit 0 when constructing the block gain target. This does not bypass the entire callback or remove its DSP workload. |
| Attack | 0x0C / 0..19 | P:0334CC reads native float pairs at X:41B970, stores X:object+36/+37, then P:0333FE writes Y:core+6. |
| Release | 0x0D / 0..19 | P:0334D6 uses X:41B998 and X:object+38/+39; P:0333FE writes Y:core+7. |
| Gain-reference smoothing | 0x0E / 0..2 | P:0334E6 selects X:41B9C0 and writes Y:core+10. P:000D1F..D20 uses it as the retention multiplier in the gain-reference recurrence. |
| Detector reference weighting | 0x0F / 0..100 | Conversion 6 maps [0,100] to [0,4096]; P:0334EF normalizes it into Y:core+13. P:000D43..D49 weights the full-band envelope before the reference comparison/clamp. It does not change the two filter coefficient banks. |

Attack selector milliseconds:
`1,2,3,4,5,6,7,8,9,10,12,14,16,18,20,23,26,30,35,40`.
Release selector milliseconds:
`15,15,25,35,45,55,65,75,85,100,115,140,170,230,340,680,1000,1500,2000,3000`.

The native float format is signed24 mantissa / 2^23 times
2^(exponent-127). Executing the actual setters for all 40 de-esser timing
selectors at the original 48000 rate produces retention coefficients matching
`exp(ln(0.01)/(48000*t_seconds))`, maximum absolute error below 4e-7.
These are approximately 99% settling times, not one time constant. The kernel
chooses attack or release according to envelope direction in P:000D10..D15 and
P:000D38..D3D. This is native-code/emulator evidence, not measured microphone
attack/release on the physical unit.

The three gain-reference retention words are 0x7FDF3B, 0x7FFCB9 and 0x7FFFAC.
At 48 kHz their theoretical e-folding times are about 20.82, 208.29 and
2080.50 ms. Detector weighting is a normalized factor from 0 to 1, quantized
through the 4096-step conversion. All 101 values were checked: only coefficient
Y:core+13 changes. Its perceptual effect and interaction with the existing amount
control remain to be measured; it is not a frequency or bandwidth selector.

Existing amount key 0x0B remains raw 0..100. P:0334B9 clamps its internal
calculation to at least 2 before the nonlinear threshold/coefficient calculation.
The raw percentage is not a calibrated dB threshold.

## Original compressor parameters and behavior

All three five-control families use P:402A32 -> P:0344D1 and the original
SimpleComp P:004473/P:00441D processing. Original P:4003F4 maps global slots
16/17/18 to root children 13/28/33. Existing microphone controls address slot 17.
Keys 0x141..145 and 0x15A..15E address the other two objects; their physical
signal roles remain unassigned, and 909 deliberately does not grant their reads.

| Parameter | Mic key / ROM range | Native behavior |
| --- | --- | --- |
| Ratio | 0x13C / index 0..14 | X:41BD70 pairs: 1,1.1,1.2,1.4,1.6,1.8,2,2.5,3.2,4,5.6,8,16,32,64 to 1. P:0344E3 stores pair at X:+9/+10 and rebuilds slope/threshold coefficients. |
| Threshold | 0x13D / -50..0 dB | P:0344ED stores normalized float at X:+11/+12 and rebuilds coefficients through P:0343DD. Existing stock profile UI limits remain -40..0; the new live-only diagnostic range follows the ROM clamp. |
| Attack | 0x13E / index 0..19 | X:41BD8E: 0.001 ms, then the de-esser attack list from 2 ms onward. P:0344FF -> P:034441 writes Y:core+7. |
| Release | 0x13F / index 0..19 | X:41BDB6 uses the release list above. P:034509 -> P:034441 writes Y:core+8. |
| Makeup | 0x140 / 0..24 dB | P:034519 writes the signed log-domain coefficient Y:core+2. Native checks at 0/6/12/24 match `(dB - native(0x84,0x780A3E)) * native(0x77,0x550A96)`. This coefficient is not itself linear audio gain. |

The 40 native compressor timing coefficients match
`1-exp(-1/(48000*t_seconds))` within 4.1e-7. The sample recurrence therefore uses
a different time criterion from the de-esser. P:0344AE enables the three
compressors with literal 1 from graph setup at P:405145/40549D/405612; no
corresponding host enable descriptor was found in the complete mapped families.
No independent knee, hold, lookahead or extra de-essing frequency/bandwidth
parameter was established by this mapping. That does not prove no private
interface exists. Full physical transfer curves, knee shape and all module
interactions remain unmeasured.

## Native app and validation state

The existing Rust application's compressor/de-esser section reads 11 values
through the same daemon/USB bridge. New IPC requests are typed and range checked.
Writes require firmware 909, the same active mic profile and matching expected
raw value, then confirm actual DSP readback. If confirmation fails, the daemon
attempts a direct baseline write and reports restoration as unverified until a
fresh read succeeds. Dynamics edits are live-only; no automatic replay or profile
format is added. The GUI write gate stays false until physical tests pass.
908 rejects these keys before transport and retains its original mic EQ path.

The reversible native IPC test is present but ignored by default. It captures
all 47 EQ/dynamics fields, changes one dynamics value at a time, restores it,
and checks the complete snapshot. It has not run against a physical 909 unit.
Raw readback verification and audible processing verification are separate gates.

Both Windows release builds passed in a fresh build folder after the old build
cache reported corrupt metadata. Their hashes are recorded in BUILD-909.json.
The updater refuses 909 until the matching new daemon is installed: restarting
the older 908-only daemon on 909 could skip native EQ profile replay. Installation
must preserve the live 36-field EQ snapshot and original profile/executable
backups. The prepared native app replaces the existing app; it is not a new preview.

Fresh read-only Boot2/Golden captures were read twice per chunk and match the
preserved hashes. This is a pre-update integrity check, not a new recovery test.

Offline: 105 original parser/query/setter cases, 80 timing, 101 detector-weight
and four makeup checks per image, plus actual native object registration.
The serialized 909 ARM bridge passes all 47 permitted keys and rejects 313
others, with signed replies. Tests use synthetic pointers, ring drainage and
peripheral responses; they do not measure live timing or free DSP memory.

## Headphone routing and Voicemeeter correction

The user clarified that DT headphones connect directly to GoXLR and there is
**no analog Headphones-to-Line-In cable**. Two attempted return captures were
silent and excluded. They prove no physical DAC isolation. Temporary GoXLR
routes and headphone volume were restored and verified; all 36 mic EQ values
match this work's starting snapshot.

A read-only query of the running Voicemeeter Potato Remote API shows virtual
AUX strip -> A1 -> System (TC-HELICON GoXLR). A1 EQ is enabled and was not changed.
Use Voicemeeter AUX Input for further playback, not a direct Game bypass.
A subsequent digital capture contained background audio, so it is not a clean
amplitude/response measurement. The reusable inspector calls getters only;
parameter names follow the [official Remote API documentation](https://download.vb-audio.com/Download_CABLE/VoicemeeterRemoteAPI.pdf).

Logical ins2/3 is still not a physically proven final headphone DSP postmix
hook. Physical stereo DAC/Line-Out isolation needs an actual recording return
or a suitable fixture. No live DSP utilization counter or measured free cycles
has been obtained. 909 grants parameter reads only and adds no timing profiler;
do not infer DSP utilization from USB timing, Windows load or these emulator tests.

## Reproduction

```sh
GOXLR_DYNAMICS_BRIDGE=1 bash tools/build_goxlr_eq_bridge.sh <arm-toolchain-bin> <new-build-directory> 100
rustc --edition=2024 tools/build_goxlr_eq_lab.rs -o <builder>
<builder> --build 909 <preserved-907.bin> <build>/eq-bridge.bin <build>/eq-same-size-stage.bin <new-909.bin>
python tools/verify_goxlr_eq_arm.py <preserved-907.bin> <build>/eq-bridge.elf --same-size <909.bin> --dynamics --boot2 <preserved-boot2.bin> --golden <preserved-golden.bin>
go run tools/goxlr_dynamics_map.go -firmware <901-or-908-or-909.bin>
cargo run --locked --offline --manifest-path tools/dynamics-lab/Cargo.toml -- <901-or-908-or-909.bin>
cargo test --locked -p goxlr-usb -p goxlr-native-mic-eq --jobs 2
```

Hardware test command, only after the exact image is authorized and installed:
`cargo test --locked -p goxlr-native-mic-eq native_dynamics_reversible_round_trip -- --ignored --nocapture`.
This checks transport/restoration, not calibrated acoustic behavior. Preserve
908, Boot2/Golden, the working executables and complete mic/profile baseline.
