use crate::{
    controls::{self, RawUnit},
    daemon::Live,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const FORMAT: &str = "goxlr-native-raw47";
const MAX_FILE_BYTES: usize = 64 * 1024;

pub fn supports_firmware(firmware: &str) -> bool {
    [
        crate::daemon::DYNAMICS_FIRMWARE,
        crate::daemon::ARM_DIAGNOSTICS_FIRMWARE,
    ]
    .contains(&firmware)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    format: String,
    version: u32,
    pub name: String,
    source_firmware: String,
    pub parameters: Vec<ParameterValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterValue {
    pub key: u16,
    pub raw: i32,
    unit: RawUnit,
}

pub struct Change {
    pub name: String,
    pub key: u16,
    pub before: i32,
    pub after: i32,
}

impl Preset {
    pub fn from_live(name: String, live: &Live) -> Result<Self> {
        ensure!(
            supports_firmware(&live.firmware),
            "A complete 909/910 DSP snapshot is required"
        );
        let dynamics = live
            .dynamics
            .context("Read all dynamics parameters before exporting")?;
        let mut values = Vec::with_capacity(47);
        for band in live.bands {
            ensure!(
                band.hz.is_finite() && band.hz > 0.,
                "Invalid live frequency"
            );
            let frequency = 24. * (band.hz / 20.).log2();
            ensure!(
                (frequency - frequency.round()).abs() < 0.001,
                "Live frequency is not a raw DSP selector"
            );
            values.extend([
                frequency.round() as i32,
                i32::from(band.gain),
                i32::from(band.raw.context("Missing live width")?),
            ]);
        }
        values.extend(dynamics);
        let preset = Self {
            format: FORMAT.into(),
            version: 1,
            name,
            source_firmware: live.firmware.clone(),
            parameters: controls::parameters()
                .into_iter()
                .zip(values)
                .map(|(spec, raw)| ParameterValue {
                    key: spec.key,
                    raw,
                    unit: spec.unit,
                })
                .collect(),
        };
        preset.validate()?;
        Ok(preset)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == FORMAT && self.version == 1,
            "Unsupported preset format or version"
        );
        ensure!(
            supports_firmware(&self.source_firmware),
            "Unsupported source firmware"
        );
        ensure!(
            !self.name.is_empty()
                && self.name.trim() == self.name
                && self.name.chars().count() <= 80
                && !matches!(self.name.as_str(), "." | "..")
                && !self
                    .name
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':')),
            "Preset name must be 1–80 characters without path separators or control characters"
        );
        let specs = controls::parameters()
            .into_iter()
            .map(|spec| (spec.key, spec))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            self.parameters.len() == specs.len(),
            "Preset must contain exactly 47 parameters"
        );
        let mut seen = HashSet::new();
        for value in &self.parameters {
            let spec = specs.get(&value.key).context("Unknown parameter key")?;
            ensure!(
                seen.insert(value.key),
                "Duplicate parameter key 0x{:03X}",
                value.key
            );
            ensure!(
                spec.range.contains(&value.raw),
                "Parameter 0x{:03X} is out of range",
                value.key
            );
            ensure!(
                spec.unit == value.unit,
                "Parameter 0x{:03X} has incorrect units",
                value.key
            );
        }
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)
            .context("Could not open preset")?
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_FILE_BYTES, "Preset exceeds 64 KiB");
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let preset: Self = serde_json::from_slice(bytes).context("Invalid preset JSON")?;
        preset.validate()?;
        Ok(preset)
    }

    pub fn save_new(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .context("Could not create preset; existing files are never overwritten")?;
        file.write_all(&bytes)
            .context("Could not finish writing preset")
    }

    pub fn changes(&self, live: &Live) -> Result<Vec<Change>> {
        self.validate()?;
        let current = Self::from_live(self.name.clone(), live)?;
        let current = current
            .parameters
            .into_iter()
            .map(|value| (value.key, value.raw))
            .collect::<BTreeMap<_, _>>();
        let specs = controls::parameters()
            .into_iter()
            .map(|spec| (spec.key, spec.name))
            .collect::<BTreeMap<_, _>>();
        Ok(self
            .parameters
            .iter()
            .filter_map(|value| {
                let before = current[&value.key];
                (before != value.raw).then(|| Change {
                    name: specs[&value.key].clone(),
                    key: value.key,
                    before,
                    after: value.raw,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires the attached 909/910 device; reads all 47 fields and verifies the preset codec without hardware writes"]
    async fn native_live_preset_snapshot() -> Result<()> {
        let devices = crate::daemon::read().await?;
        ensure!(
            devices.len() == 1 && supports_firmware(&devices[0].firmware),
            "Exactly one 909/910 device is required"
        );
        let preset = Preset::from_live(
            format!("Live {} snapshot", devices[0].firmware),
            &devices[0],
        )?;
        let bytes = serde_json::to_vec(&preset)?;
        let imported: Preset = serde_json::from_slice(&bytes)?;
        imported.validate()?;
        ensure!(
            imported == preset
                && imported.parameters.len() == 47
                && imported.changes(&devices[0])?.is_empty(),
            "Live preset round trip differs"
        );
        if let Some(path) = std::env::var_os("GOXLR_PRESET_EXPORT_PATH") {
            imported.save_new(Path::new(&path))?;
        }
        println!(
            "PASS actual {} snapshot: 47 raw fields exported/imported exactly; no hardware writes",
            devices[0].firmware
        );
        Ok(())
    }

    fn live(frequency_raw: i32) -> Live {
        Live {
            key: crate::model::ProfileKey {
                serial: "private-device-name".into(),
                name: "private-profile-name".into(),
            },
            firmware: crate::daemon::DYNAMICS_FIRMWARE.into(),
            bands: [crate::model::Band {
                hz: 20. * 2_f32.powf(frequency_raw as f32 / 24.),
                gain: -3,
                raw: Some(16),
            }; 12],
            dynamics: Some(
                goxlr_types::DynamicsControl::ALL
                    .map(|control| controls::dynamics_parameter(control).rom_default.unwrap()),
            ),
            mic_profiles: Vec::new(),
        }
    }

    fn fixture() -> Preset {
        Preset {
            format: FORMAT.into(),
            version: 1,
            name: "Studio العربية".into(),
            source_firmware: crate::daemon::DYNAMICS_FIRMWARE.into(),
            parameters: controls::parameters()
                .into_iter()
                .map(|spec| ParameterValue {
                    key: spec.key,
                    raw: spec.rom_default.unwrap_or(*spec.range.start()),
                    unit: spec.unit,
                })
                .collect(),
        }
    }

    #[test]
    fn complete_raw47_round_trip_preserves_order_independent_values() -> Result<()> {
        let mut preset = fixture();
        preset.parameters.reverse();
        let bytes = serde_json::to_vec(&preset)?;
        let restored: Preset = serde_json::from_slice(&bytes)?;
        restored.validate()?;
        assert_eq!(restored, preset);
        assert_eq!(restored.parameters.len(), 47);
        Ok(())
    }

    #[test]
    fn live_export_preserves_every_frequency_selector_and_preview_does_not_modify_it() -> Result<()>
    {
        for firmware in [
            crate::daemon::DYNAMICS_FIRMWARE,
            crate::daemon::ARM_DIAGNOSTICS_FIRMWARE,
        ] {
            for raw in 0..=240 {
                let mut live = live(raw);
                live.firmware = firmware.into();
                let mut preset = Preset::from_live("Portable".into(), &live)?;
                assert_eq!(preset.source_firmware, firmware);
                for band in 0..12 {
                    assert_eq!(preset.parameters[band * 3].raw, raw);
                }
                assert!(preset.changes(&live)?.is_empty());
                preset.parameters[1].raw += 1;
                let changes = preset.changes(&live)?;
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[0].before, -3);
                assert_eq!(changes[0].after, -2);
                assert_eq!(live.bands[0].gain, -3);
                let json = serde_json::to_string(&preset)?;
                let imported: Preset = serde_json::from_str(&json)?;
                imported.validate()?;
                assert_eq!(imported.source_firmware, firmware);
                assert_eq!(imported, preset);
                assert!(!json.contains("private-device-name"));
                assert!(!json.contains("private-profile-name"));
            }
        }
        Ok(())
    }

    #[test]
    fn rejects_partial_duplicate_unknown_out_of_range_and_wrong_units() {
        let original = fixture();
        let mut changed = original.clone();
        changed.parameters.pop();
        assert!(changed.validate().is_err());
        let mut changed = original.clone();
        changed.parameters[1] = changed.parameters[0].clone();
        assert!(changed.validate().is_err());
        let mut changed = original.clone();
        changed.parameters[0].key = 0xffff;
        assert!(changed.validate().is_err());
        for (index, spec) in controls::parameters().into_iter().enumerate() {
            let mut changed = original.clone();
            changed.parameters[index].raw = spec.range.start() - 1;
            assert!(changed.validate().is_err());
            changed.parameters[index].raw = spec.range.end() + 1;
            assert!(changed.validate().is_err());
        }
        let mut changed = original;
        changed.parameters[0].unit = RawUnit::Decibels;
        assert!(changed.validate().is_err());
    }

    #[test]
    fn rejects_untrusted_names_versions_and_json_members() -> Result<()> {
        for name in [
            "",
            "..",
            "../other",
            "C:\\other",
            "line\nname",
            " padded",
            "nul\0name",
        ] {
            let mut preset = fixture();
            preset.name = name.into();
            assert!(preset.validate().is_err(), "{name:?}");
        }
        let mut preset = fixture();
        preset.name = "x".repeat(81);
        assert!(preset.validate().is_err());
        let mut preset = fixture();
        preset.version = 2;
        assert!(preset.validate().is_err());
        let mut preset = fixture();
        preset.source_firmware = "1.5.1.911".into();
        assert!(preset.validate().is_err());
        let mut value = serde_json::to_value(fixture())?;
        value["command"] = serde_json::json!("SetGateThreshold");
        assert!(serde_json::from_value::<Preset>(value).is_err());
        let mut value = serde_json::to_value(fixture())?;
        value["parameters"][0]["command"] = serde_json::json!("SetGateThreshold");
        assert!(serde_json::from_value::<Preset>(value).is_err());
        let duplicate = serde_json::to_string(&fixture())?.replacen(
            "\"version\":1",
            "\"version\":1,\"version\":1",
            1,
        );
        assert!(
            serde_json::from_str::<Preset>(&duplicate)
                .unwrap_err()
                .to_string()
                .contains("duplicate field")
        );
        assert!(serde_json::from_str::<Preset>("{broken").is_err());
        Ok(())
    }

    #[test]
    fn file_persistence_refuses_overwrite_and_oversized_import() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "goxlr-preset-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let preset = fixture();
        let mut created = false;
        let result = (|| -> Result<()> {
            preset.save_new(&path)?;
            created = true;
            assert_eq!(Preset::read(&path)?, preset);
            assert!(preset.save_new(&path).is_err());
            assert_eq!(Preset::read(&path)?, preset);
            let mut bom = vec![0xef, 0xbb, 0xbf];
            bom.extend(serde_json::to_vec(&preset)?);
            std::fs::write(&path, bom)?;
            assert_eq!(Preset::read(&path)?, preset);
            std::fs::write(&path, vec![b' '; MAX_FILE_BYTES + 1])?;
            assert!(Preset::read(&path).is_err());
            Ok(())
        })();
        if created {
            let _ = std::fs::remove_file(path);
        }
        result
    }
}
