use goxlr_types::{DynamicsControl, MicEqField};
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawUnit {
    FrequencySelector,
    Decibels,
    WidthSelector,
    Boolean,
    Intensity,
    DeesserTimingSelector,
    RetentionSelector,
    ReferenceWeight,
    RatioSelector,
    CompressorTimingSelector,
}

pub const BAND_KEYS: [u16; 12] = [
    0x126, 0xf8, 0x113, 0x129, 0x116, 0x11d, 0x12c, 0x120, 0x109, 0x12f, 0xf5, 0x10c,
];
// Serialized descriptor field 3 via X:41ABE9 -> X:41A391 + 8*index.
// Generic original three-band object defaults, not effective 12-band boot settings.
const EQ_DESCRIPTOR_FREQUENCY: [i32; 12] = [129, 198, 76, 129, 198, 76, 129, 198, 76, 129, 76, 198];
const ATTACK_MS: [f32; 20] = [
    1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 12., 14., 16., 18., 20., 23., 26., 30., 35., 40.,
];
const RELEASE_MS: [u16; 20] = [
    15, 15, 25, 35, 45, 55, 65, 75, 85, 100, 115, 140, 170, 230, 340, 680, 1000, 1500, 2000, 3000,
];
const RATIOS: [f32; 15] = [
    1., 1.1, 1.2, 1.4, 1.6, 1.8, 2., 2.5, 3.2, 4., 5.6, 8., 16., 32., 64.,
];

pub struct Parameter {
    pub name: String,
    pub key: u16,
    pub range: RangeInclusive<i32>,
    pub rom_default: Option<i32>,
    pub meaning: &'static str,
    pub setter: Option<u32>,
    pub unit: RawUnit,
}

pub fn eq_parameter(band: usize, field: MicEqField) -> Option<Parameter> {
    let key = *BAND_KEYS.get(band)? + field as u16;
    let (name, range, meaning) = match field {
        MicEqField::Frequency => ("Frequency", 0..=240, "20 × 2^(raw/24) Hz"),
        MicEqField::Gain => ("Gain", -12..=12, "Microphone EQ gain in dB"),
        MicEqField::Width => (
            "Width",
            0..=32,
            "Raw width selector; numerical Q is uncalibrated",
        ),
    };
    Some(Parameter {
        name: format!("Band {} {name}", band + 1),
        key,
        range,
        rom_default: Some(match field {
            MicEqField::Frequency => EQ_DESCRIPTOR_FREQUENCY[band],
            MicEqField::Gain => 0,
            MicEqField::Width => 16,
        }),
        meaning,
        setter: None,
        unit: match field {
            MicEqField::Frequency => RawUnit::FrequencySelector,
            MicEqField::Gain => RawUnit::Decibels,
            MicEqField::Width => RawUnit::WidthSelector,
        },
    })
}

pub fn dynamics_parameter(control: DynamicsControl) -> Parameter {
    use DynamicsControl::*;
    let (name, default, meaning, setter) = match control {
        DeesserEnabled => (
            "De-esser gain reduction",
            1,
            "0 targets unity through a 64-sample gain transition; the detector still runs. Not an instant bit-exact dry or workload bypass",
            0x334b5,
        ),
        DeesserAmount => (
            "De-esser intensity",
            50,
            "Stock nonlinear intensity; raw 0, 1 and 2 share the minimum curve and can still reduce audio. Zero is not bypass or a calibrated dB threshold",
            0x334b9,
        ),
        DeesserAttack => (
            "De-esser attack",
            0,
            "Nominal 99% settling time from the native coefficient table",
            0x334cc,
        ),
        DeesserRelease => (
            "De-esser release",
            9,
            "Nominal 99% settling time; selectors 0 and 1 both mean 15 ms",
            0x334d6,
        ),
        DeesserMode => (
            "Gain-reference smoothing",
            0,
            "Retention pole; not a frequency or wideband/split-band mode",
            0x334e6,
        ),
        DeesserScalar => (
            "Detector reference weighting",
            20,
            "Full-band reference weighting; not frequency or bandwidth",
            0x334ef,
        ),
        CompressorRatio => ("Compressor ratio", 0, "Stock ratio selector", 0x344e3),
        CompressorThreshold => (
            "Compressor threshold",
            0,
            "Native domain −50…0 dB; the legacy profile UI has a smaller domain",
            0x344ed,
        ),
        CompressorAttack => (
            "Compressor attack",
            1,
            "Nominal one-time-constant envelope timing",
            0x344ff,
        ),
        CompressorRelease => (
            "Compressor release",
            9,
            "Nominal one-time-constant envelope timing",
            0x34509,
        ),
        CompressorMakeup => (
            "Compressor makeup",
            4,
            "Makeup gain in dB, stored internally in signed logarithmic form",
            0x34519,
        ),
    };
    Parameter {
        name: name.into(),
        key: control.key(),
        range: control.range(),
        rom_default: Some(default),
        meaning,
        setter: Some(setter),
        unit: match control {
            DeesserEnabled => RawUnit::Boolean,
            DeesserAmount => RawUnit::Intensity,
            DeesserAttack | DeesserRelease => RawUnit::DeesserTimingSelector,
            DeesserMode => RawUnit::RetentionSelector,
            DeesserScalar => RawUnit::ReferenceWeight,
            CompressorRatio => RawUnit::RatioSelector,
            CompressorThreshold | CompressorMakeup => RawUnit::Decibels,
            CompressorAttack | CompressorRelease => RawUnit::CompressorTimingSelector,
        },
    }
}

pub fn parameters() -> Vec<Parameter> {
    let mut parameters = Vec::with_capacity(47);
    for band in 0..12 {
        for field in [MicEqField::Frequency, MicEqField::Gain, MicEqField::Width] {
            parameters.push(eq_parameter(band, field).expect("Known band"));
        }
    }
    parameters.extend(DynamicsControl::ALL.map(dynamics_parameter));
    parameters
}

pub fn dynamics_value(control: DynamicsControl, raw: i32) -> String {
    use DynamicsControl::*;
    if !control.range().contains(&raw) {
        return format!("Invalid raw value {raw}");
    }
    let index = raw as usize;
    match control {
        DeesserEnabled => if raw == 0 {
            "Unity target (64-sample transition)"
        } else {
            "Gain reduction enabled"
        }
        .into(),
        DeesserAmount => format!("{raw}/100 raw intensity"),
        DeesserScalar => format!("{raw}%"),
        DeesserAttack => format!("{} ms (99%)", ATTACK_MS[index]),
        DeesserRelease => format!("{} ms (99%)", RELEASE_MS[index]),
        DeesserMode => format!("~{} ms at 48 kHz", [20.82, 208.29, 2080.50][index]),
        CompressorRatio => format!("{}:1", RATIOS[index]),
        CompressorThreshold | CompressorMakeup => format!("{raw} dB"),
        CompressorAttack => format!("{} ms", if raw == 0 { 0.001 } else { ATTACK_MS[index] }),
        CompressorRelease => format!("{} ms", RELEASE_MS[index]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_has_47_distinct_keys_and_valid_rom_defaults() {
        let mut keys = HashSet::new();
        for band in 0..12 {
            for field in [MicEqField::Frequency, MicEqField::Gain, MicEqField::Width] {
                let parameter = eq_parameter(band, field).unwrap();
                assert!(keys.insert(parameter.key));
                assert!(parameter.range.contains(&parameter.rom_default.unwrap()));
            }
        }
        assert!(eq_parameter(12, MicEqField::Gain).is_none());
        assert_eq!(
            eq_parameter(0, MicEqField::Frequency).unwrap().rom_default,
            Some(129)
        );
        assert_eq!(
            eq_parameter(10, MicEqField::Frequency).unwrap().rom_default,
            Some(76)
        );
        assert_eq!(
            eq_parameter(11, MicEqField::Frequency).unwrap().rom_default,
            Some(198)
        );
        for control in DynamicsControl::ALL {
            let parameter = dynamics_parameter(control);
            assert!(keys.insert(parameter.key));
            assert!(parameter.range.contains(&parameter.rom_default.unwrap()));
        }
        assert_eq!(keys.len(), 47);
    }

    #[test]
    fn timing_units_keep_distinct_native_definitions_and_validate_selectors() {
        use DynamicsControl::*;
        assert_eq!(dynamics_value(DeesserAttack, 0), "1 ms (99%)");
        assert_eq!(dynamics_value(CompressorAttack, 0), "0.001 ms");
        assert_eq!(
            dynamics_value(DeesserRelease, 0),
            dynamics_value(DeesserRelease, 1)
        );
        assert_eq!(dynamics_value(CompressorRatio, 8), "3.2:1");
        assert_eq!(
            dynamics_value(DeesserEnabled, 0),
            "Unity target (64-sample transition)"
        );
        assert_eq!(dynamics_value(DeesserAmount, 0), "0/100 raw intensity");
        for control in DynamicsControl::ALL {
            for raw in [*control.range().start() - 1, *control.range().end() + 1] {
                assert!(dynamics_value(control, raw).starts_with("Invalid"));
            }
        }
    }
}
