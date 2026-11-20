# Noise gate: stock controls and remaining evidence

This is a source audit of the existing Utility, not a new physical gate
calibration or an extension of the verified 47-field bridge. Preserve profile 4
and its −52 dB threshold. No gate commands were sent for this audit.

| Stock control | Effect key | Host/profile range and representation |
| --- | --- | --- |
| Threshold | `0x11` | Integer −59..0, labeled dB; acoustic reference uncalibrated |
| Attack | `0x16` | Table index 0..45; nominal 10..2000 ms |
| Release | `0x17` | Same 46-entry nominal time table |
| Attenuation | `0x15` | User percentage 0..100 converted to signed attenuation table |
| Enabled | `0x14` | Stored bool; current effect serializer always sends 1 (see below) |
| Gate mode | `0x10` | Profile byte, parser fallback 2; no validated public mode semantics |

The time table in `types/src/lib.rs::GateTimes` is 10..200 ms in 10 ms
steps, 250..1000 ms in 50 ms steps, then 1100..2000 ms in 100 ms steps.
These are existing host labels, not measured opening/closing time constants.
For example index 0 is 10 ms, 9 is 100 ms and 19 is 200 ms.

`daemon/src/mic_profile.rs::gate_attenuation_from_percent` uses index
`floor(percent * 0.24)` for 0..99, and index 25 for 100. The exact signed
table is −6..−28 in integer steps, then −30, −32, −61. Thus 0% sends −6,
99% sends −30, and 100% sends −61. Percentage is not linear gain or a
percentage of measured attenuation. The −32 entry is unreachable from an
integer percentage in this mapping. Its exact acoustic interpretation and
closed-gate floor remain uncalibrated.

## Existing command paths and caveats

`daemon/src/device.rs` routes threshold, attack, release and attenuation through
both `apply_mic_params` and `apply_effects`. The corresponding microphone
parameter keys are `0x30200`, `0x30400`, `0x30600`, `0x30900`; the profile
adapter encodes their values as floats. These source paths establish host
serialization, not actual DSP readback.

`SetGateActive` changes the profile bool then applies `GateEnabled`, but
`get_effect_value(GateEnabled)` currently returns constant 1. A native bypass
must not claim hardware bypass from that saved bool or command acknowledgment.
This existing behavior is documented here and was not changed without physical
validation. `GateMode` is serialized from the profile and has no independently
validated meanings or native controls.

The 909/910 bridge allowlist and `DynamicsControl` currently contain 36 EQ,
six de-esser and five compressor fields; gate fields are excluded. The stock
status gate values are profile state, not additional members of the 47 proven
DSP readbacks. The existing dynamics ROM inspector does not establish gate
descriptor limits, callback addresses or internal state ownership.

## Features still blocked

No independently adjustable hysteresis or hold parameter has been established.
`hold_time` in `daemon/src/device.rs` is a button long-press duration and is not
an audio gate hold. No gate gain reduction meter, limiter or processing-order
control is proven. Do not infer such controls from profile fields, approximate
curves or a successful USB acknowledgment.

Next work is the original gate descriptor/callback and state-machine trace,
followed by an image-specific read-only bridge extension only when ownership
is proven. With a quiet fixed acoustic fixture, measure open/close thresholds,
attack/release response and closed-gate attenuation separately, test bypass,
and restore/compare all stock settings and the existing 47 values. Physical
testing remains deferred while the fixture is unavailable.

Evidence: `profile/src/microphone/gate.rs`, `profile/src/mic_profile.rs`,
`daemon/src/mic_profile.rs`, `daemon/src/device.rs`, `types/src/lib.rs`, and
`tools/goxlr_dynamics_map.go`. No firmware, native executable or hardware state
was changed by this audit.
