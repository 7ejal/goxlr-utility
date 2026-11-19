use std::collections::HashMap;
use std::os::raw::c_float;
use std::str::FromStr;

use crate::profile::Attribute;
use anyhow::{Result, anyhow, bail};

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum ParseError {
    #[error("[EQ] Expected int: {0}")]
    ExpectedInt(#[from] std::num::ParseIntError),

    #[error("[EQ] Expected float: {0}")]
    ExpectedFloat(#[from] std::num::ParseFloatError),

    #[error("[EQ] Parsing Error {0}")]
    Error(#[from] anyhow::Error),
}

// The EQ has a crap load of values (20 total), we could consider splitting
// them into Gain and Freq to keep stuff tidy?
#[derive(Debug)]
pub struct Equalizer {
    bandwidth: [u8; 10],
    native_parameters: [Option<i32>; 36],
    eq_31h_gain: i8,
    eq_63h_gain: i8,
    eq_125h_gain: i8,
    eq_250h_gain: i8,
    eq_500h_gain: i8,
    eq_1k_gain: i8,
    eq_2k_gain: i8,
    eq_4k_gain: i8,
    eq_8k_gain: i8,
    eq_16k_gain: i8,

    eq_31h_freq: f32,
    eq_63h_freq: f32,
    eq_125h_freq: f32,
    eq_250h_freq: f32,
    eq_500h_freq: f32,
    eq_1k_freq: f32,
    eq_2k_freq: f32,
    eq_4k_freq: f32,
    eq_8k_freq: f32,
    eq_16k_freq: f32,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Equalizer {
    pub fn new() -> Self {
        Self {
            bandwidth: [16; 10],
            native_parameters: [None; 36],
            eq_31h_gain: 0,
            eq_63h_gain: 0,
            eq_125h_gain: 0,
            eq_250h_gain: 0,
            eq_500h_gain: 0,
            eq_1k_gain: 0,
            eq_2k_gain: 0,
            eq_4k_gain: 0,
            eq_8k_gain: 0,
            eq_16k_gain: 0,
            eq_31h_freq: 31.5,
            eq_63h_freq: 63.0,
            eq_125h_freq: 125.0,
            eq_250h_freq: 250.0,
            eq_500h_freq: 500.0,
            eq_1k_freq: 1000.0,
            eq_2k_freq: 2000.0,
            eq_4k_freq: 4000.0,
            eq_8k_freq: 8000.0,
            eq_16k_freq: 16000.0,
        }
    }

    pub fn bandwidth(&self) -> &[u8; 10] {
        &self.bandwidth
    }

    pub fn set_bandwidth(&mut self, band: usize, raw: u8) -> Result<()> {
        let Some(value) = self.bandwidth.get_mut(band) else {
            bail!("Invalid microphone EQ band");
        };
        if raw > 32 {
            bail!("Microphone EQ bandwidth must be between 0 and 32");
        }
        *value = raw;
        self.native_parameters[band * 3 + 2] = None;
        Ok(())
    }

    pub fn native_parameters(&self) -> &[Option<i32>; 36] {
        &self.native_parameters
    }

    pub fn set_native_parameter(&mut self, band: usize, field: usize, raw: i32) -> Result<()> {
        if band >= 12 || field >= 3 {
            bail!("Invalid native microphone EQ parameter");
        }
        let range = match field {
            0 => 0..=240,
            1 => -12..=12,
            _ => 0..=32,
        };
        if !range.contains(&raw) {
            bail!("Native microphone EQ value is out of range");
        }
        if band < 10 && field == 2 {
            self.bandwidth[band] = raw as u8;
        }
        self.native_parameters[band * 3 + field] = Some(raw);
        Ok(())
    }

    pub fn parse_equaliser(&mut self, attributes: &Vec<Attribute>) -> Result<(), ParseError> {
        for attr in attributes {
            if attr.name == "GOXLR_UTILITY_MIC_EQ_BANDWIDTH" {
                let values = attr
                    .value
                    .split(',')
                    .map(str::parse::<u8>)
                    .collect::<Result<Vec<_>, _>>()?;
                if values.len() != self.bandwidth.len() || values.iter().any(|&raw| raw > 32) {
                    return Err(anyhow!("Invalid microphone EQ bandwidth profile data").into());
                }
                self.bandwidth.copy_from_slice(&values);
            }

            if attr.name == "MIC_EQ_31.5HZ_GAIN" {
                self.set_eq_31h_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_63HZ_GAIN" {
                self.set_eq_63h_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_125HZ_GAIN" {
                self.set_eq_125h_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_250HZ_GAIN" {
                self.set_eq_250h_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_500HZ_GAIN" {
                self.set_eq_500h_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_1KHZ_GAIN" {
                self.set_eq_1k_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_2KHZ_GAIN" {
                self.set_eq_2k_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_4KHZ_GAIN" {
                self.set_eq_4k_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_8KHZ_GAIN" {
                self.set_eq_8k_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_16KHZ_GAIN" {
                self.set_eq_16k_gain(attr.value.parse::<c_float>()? as i8)?;
            }

            if attr.name == "MIC_EQ_31.5HZ_F" {
                self.set_eq_31h_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_63HZ_F" {
                self.set_eq_63h_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_125HZ_F" {
                self.set_eq_125h_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_250HZ_F" {
                self.set_eq_250h_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_500HZ_F" {
                self.set_eq_500h_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_1KHZ_F" {
                self.set_eq_1k_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_2KHZ_F" {
                self.set_eq_2k_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_4KHZ_F" {
                self.set_eq_4k_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_8KHZ_F" {
                self.set_eq_8k_freq(f32::from_str(attr.value.as_str())?)?;
            }

            if attr.name == "MIC_EQ_16KHZ_F" {
                self.set_eq_16k_freq(f32::from_str(attr.value.as_str())?)?;
            }
        }

        // Parse overrides after legacy fields so XML attribute order cannot clear them.
        if let Some(attr) = attributes
            .iter()
            .find(|attr| attr.name == "GOXLR_UTILITY_MIC_EQ_NATIVE")
        {
            let values = attr.value.split(',').collect::<Vec<_>>();
            if values.len() != 36 {
                return Err(anyhow!("Invalid native microphone EQ profile length").into());
            }
            for (index, value) in values.into_iter().enumerate() {
                if value != "_" {
                    self.set_native_parameter(index / 3, index % 3, value.parse::<i32>()?)?;
                }
            }
        }
        Ok(())
    }

    pub fn write_equaliser(&self, attributes: &mut HashMap<String, String>) {
        if self.native_parameters.iter().any(Option::is_some) {
            attributes.insert(
                "GOXLR_UTILITY_MIC_EQ_NATIVE".into(),
                self.native_parameters
                    .iter()
                    .map(|value| value.map_or_else(|| "_".into(), |raw| raw.to_string()))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if self.bandwidth != [16; 10] {
            attributes.insert(
                "GOXLR_UTILITY_MIC_EQ_BANDWIDTH".to_string(),
                self.bandwidth
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        attributes.insert(
            "MIC_EQ_31.5HZ_GAIN".to_string(),
            format!("{}", self.eq_31h_gain),
        );
        attributes.insert(
            "MIC_EQ_63HZ_GAIN".to_string(),
            format!("{}", self.eq_63h_gain),
        );
        attributes.insert(
            "MIC_EQ_125HZ_GAIN".to_string(),
            format!("{}", self.eq_125h_gain),
        );
        attributes.insert(
            "MIC_EQ_250HZ_GAIN".to_string(),
            format!("{}", self.eq_250h_gain),
        );
        attributes.insert(
            "MIC_EQ_500HZ_GAIN".to_string(),
            format!("{}", self.eq_500h_gain),
        );
        attributes.insert(
            "MIC_EQ_1KHZ_GAIN".to_string(),
            format!("{}", self.eq_1k_gain),
        );
        attributes.insert(
            "MIC_EQ_2KHZ_GAIN".to_string(),
            format!("{}", self.eq_2k_gain),
        );
        attributes.insert(
            "MIC_EQ_4KHZ_GAIN".to_string(),
            format!("{}", self.eq_4k_gain),
        );
        attributes.insert(
            "MIC_EQ_8KHZ_GAIN".to_string(),
            format!("{}", self.eq_8k_gain),
        );
        attributes.insert(
            "MIC_EQ_16KHZ_GAIN".to_string(),
            format!("{}", self.eq_16k_gain),
        );

        attributes.insert(
            "MIC_EQ_31.5HZ_F".to_string(),
            format!("{}", self.eq_31h_freq),
        );
        attributes.insert("MIC_EQ_63HZ_F".to_string(), format!("{}", self.eq_63h_freq));
        attributes.insert(
            "MIC_EQ_125HZ_F".to_string(),
            format!("{}", self.eq_125h_freq),
        );
        attributes.insert(
            "MIC_EQ_250HZ_F".to_string(),
            format!("{}", self.eq_250h_freq),
        );
        attributes.insert(
            "MIC_EQ_500HZ_F".to_string(),
            format!("{}", self.eq_500h_freq),
        );
        attributes.insert("MIC_EQ_1KHZ_F".to_string(), format!("{}", self.eq_1k_freq));
        attributes.insert("MIC_EQ_2KHZ_F".to_string(), format!("{}", self.eq_2k_freq));
        attributes.insert("MIC_EQ_4KHZ_F".to_string(), format!("{}", self.eq_4k_freq));
        attributes.insert("MIC_EQ_8KHZ_F".to_string(), format!("{}", self.eq_8k_freq));
        attributes.insert(
            "MIC_EQ_16KHZ_F".to_string(),
            format!("{}", self.eq_16k_freq),
        );
    }

    pub fn eq_31h_gain(&self) -> i8 {
        self.eq_31h_gain
    }
    pub fn eq_63h_gain(&self) -> i8 {
        self.eq_63h_gain
    }
    pub fn eq_125h_gain(&self) -> i8 {
        self.eq_125h_gain
    }
    pub fn eq_250h_gain(&self) -> i8 {
        self.eq_250h_gain
    }
    pub fn eq_500h_gain(&self) -> i8 {
        self.eq_500h_gain
    }
    pub fn eq_1k_gain(&self) -> i8 {
        self.eq_1k_gain
    }
    pub fn eq_2k_gain(&self) -> i8 {
        self.eq_2k_gain
    }
    pub fn eq_4k_gain(&self) -> i8 {
        self.eq_4k_gain
    }
    pub fn eq_8k_gain(&self) -> i8 {
        self.eq_8k_gain
    }
    pub fn eq_16k_gain(&self) -> i8 {
        self.eq_16k_gain
    }
    pub fn eq_31h_freq(&self) -> f32 {
        self.eq_31h_freq
    }
    pub fn eq_63h_freq(&self) -> f32 {
        self.eq_63h_freq
    }
    pub fn eq_125h_freq(&self) -> f32 {
        self.eq_125h_freq
    }
    pub fn eq_250h_freq(&self) -> f32 {
        self.eq_250h_freq
    }
    pub fn eq_500h_freq(&self) -> f32 {
        self.eq_500h_freq
    }
    pub fn eq_1k_freq(&self) -> f32 {
        self.eq_1k_freq
    }
    pub fn eq_2k_freq(&self) -> f32 {
        self.eq_2k_freq
    }
    pub fn eq_4k_freq(&self) -> f32 {
        self.eq_4k_freq
    }
    pub fn eq_8k_freq(&self) -> f32 {
        self.eq_8k_freq
    }
    pub fn eq_16k_freq(&self) -> f32 {
        self.eq_16k_freq
    }

    pub fn eq_31h_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_31h_freq)
    }
    pub fn eq_63h_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_63h_freq)
    }
    pub fn eq_125h_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_125h_freq)
    }
    pub fn eq_250h_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_250h_freq)
    }
    pub fn eq_500h_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_500h_freq)
    }
    pub fn eq_1k_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_1k_freq)
    }
    pub fn eq_2k_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_2k_freq)
    }
    pub fn eq_4k_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_4k_freq)
    }
    pub fn eq_8k_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_8k_freq)
    }
    pub fn eq_16k_freq_as_goxlr(&self) -> i32 {
        self.freq_value(self.eq_16k_freq)
    }

    pub fn set_eq_31h_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_31h_gain = value;
        self.native_parameters[1] = None;
        Ok(())
    }
    pub fn set_eq_63h_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_63h_gain = value;
        self.native_parameters[4] = None;
        Ok(())
    }
    pub fn set_eq_125h_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_125h_gain = value;
        self.native_parameters[7] = None;
        Ok(())
    }
    pub fn set_eq_250h_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_250h_gain = value;
        self.native_parameters[10] = None;
        Ok(())
    }
    pub fn set_eq_500h_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_500h_gain = value;
        self.native_parameters[13] = None;
        Ok(())
    }
    pub fn set_eq_1k_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_1k_gain = value;
        self.native_parameters[16] = None;
        Ok(())
    }
    pub fn set_eq_2k_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_2k_gain = value;
        self.native_parameters[19] = None;
        Ok(())
    }
    pub fn set_eq_4k_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_4k_gain = value;
        self.native_parameters[22] = None;
        Ok(())
    }
    pub fn set_eq_8k_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_8k_gain = value;
        self.native_parameters[25] = None;
        Ok(())
    }
    pub fn set_eq_16k_gain(&mut self, value: i8) -> Result<()> {
        validate_gain(value)?;
        self.eq_16k_gain = value;
        self.native_parameters[28] = None;
        Ok(())
    }
    pub fn set_eq_31h_freq(&mut self, value: f32) -> Result<()> {
        if !(30.0..=300.0).contains(&value) {
            bail!("31Hz Frequency must be between 30.0 and 300.0");
        }

        if value % 0.5 != 0. {
            bail!("31Hz Frequency must be a multiple of 0.5");
        }

        self.eq_31h_freq = value;
        self.native_parameters[0] = None;
        Ok(())
    }
    pub fn set_eq_63h_freq(&mut self, value: f32) -> Result<()> {
        if !(30.0..=300.0).contains(&value) {
            bail!("63Hz Frequency must be between 30.0 and 300.0");
        }
        if value % 0.5 != 0. {
            bail!("63Hz Frequency must be a multiple of 0.5");
        }

        self.eq_63h_freq = value;
        self.native_parameters[3] = None;
        Ok(())
    }
    pub fn set_eq_125h_freq(&mut self, value: f32) -> Result<()> {
        if !(30.0..=300.0).contains(&value) {
            bail!("125Hz Frequency must be between 30.0 and 300.0");
        }
        if value % 0.5 != 0. {
            bail!("125Hz Frequency must be a multiple of 0.5");
        }

        self.eq_125h_freq = value;
        self.native_parameters[6] = None;
        Ok(())
    }
    pub fn set_eq_250h_freq(&mut self, value: f32) -> Result<()> {
        if !(30.0..=300.0).contains(&value) {
            bail!("250Hz Frequency must be between 30.0 and 300.0");
        }
        if value % 0.5 != 0. {
            bail!("250Hz Frequency must be a multiple of 0.5");
        }

        self.eq_250h_freq = value;
        self.native_parameters[9] = None;
        Ok(())
    }
    pub fn set_eq_500h_freq(&mut self, value: f32) -> Result<()> {
        if !(300.0..=2000.0).contains(&value) {
            bail!("500Hz Frequency must be between 300.0 and 2000.0");
        }
        if value % 100. != 0. {
            bail!("500Hz Frequency must be a multiple of 100");
        }

        self.eq_500h_freq = value;
        self.native_parameters[12] = None;
        Ok(())
    }
    pub fn set_eq_1k_freq(&mut self, value: f32) -> Result<()> {
        if !(300.0..=2000.0).contains(&value) {
            bail!("1KHz Frequency must be between 300.0 and 2000.0");
        }
        if value % 100. != 0. {
            bail!("1KHz Frequency must be a multiple of 100");
        }
        self.eq_1k_freq = value;
        self.native_parameters[15] = None;
        Ok(())
    }
    pub fn set_eq_2k_freq(&mut self, value: f32) -> Result<()> {
        if !(300.0..=2000.0).contains(&value) {
            bail!("2KHz Frequency must be between 300.0 and 2000.0");
        }
        if value % 100. != 0. {
            bail!("2KHz Frequency must be a multiple of 100");
        }
        self.eq_2k_freq = value;
        self.native_parameters[18] = None;
        Ok(())
    }
    pub fn set_eq_4k_freq(&mut self, value: f32) -> Result<()> {
        if !(2000.0..=18000.0).contains(&value) {
            bail!("4KHz Frequency must be between 2000.0 and 18000.0");
        }
        if value % 100. != 0. {
            bail!("4KHz Frequency must be a multiple of 100");
        }
        self.eq_4k_freq = value;
        self.native_parameters[21] = None;
        Ok(())
    }
    pub fn set_eq_8k_freq(&mut self, value: f32) -> Result<()> {
        if !(2000.0..=18000.0).contains(&value) {
            bail!("8KHz Frequency must be between 2000.0 and 18000.0");
        }
        if value % 100. != 0. {
            bail!("8KHz Frequency must be a multiple of 100");
        }
        self.eq_8k_freq = value;
        self.native_parameters[24] = None;
        Ok(())
    }
    pub fn set_eq_16k_freq(&mut self, value: f32) -> Result<()> {
        if !(2000.0..=18000.0).contains(&value) {
            bail!("16KHz Frequency must be between 2000.0 and 18000.0");
        }
        if value % 100. != 0. {
            bail!("16KHz Frequency must be a multiple of 100");
        }
        self.eq_16k_freq = value;
        self.native_parameters[27] = None;
        Ok(())
    }

    fn freq_value(&self, freq: f32) -> i32 {
        (24.0 * (freq / 20.0).log2()).round() as i32
    }
}

pub fn validate_gain(gain: i8) -> Result<()> {
    if !(-9..=9).contains(&gain) {
        return Err(anyhow!("EQ Gain should be between -9 and 9"));
    }
    Ok(())
}
