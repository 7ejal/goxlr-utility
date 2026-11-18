//! Payloads for the experimental ARM EQ bridge. The USB command binding is pending.

use anyhow::{Result, bail, ensure};

/// Proposed command numbers for patched firmware. Never probe them on stock.
pub const SUBMIT_COMMAND: u32 = 0x0081_9000;
pub const POLL_COMMAND: u32 = 0x0081_9001;

const BAND_KEYS: [u16; 12] = [
    0x126, 0xf8, 0x113, 0x129, 0x116, 0x11d, 0x12c, 0x120, 0x109, 0x12f, 0xf5, 0x10c,
];

#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum EqField {
    Frequency,
    Gain,
    Width,
}

#[derive(Clone, Copy, Debug)]
pub struct EqParameter {
    key: u16,
    field: EqField,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EqReadback {
    Pending,
    Busy,
    NeedsResync,
    Idle,
    Value(i32),
}

impl EqParameter {
    pub fn new(band: usize, field: EqField) -> Result<Self> {
        ensure!((1..=BAND_KEYS.len()).contains(&band), "EQ band must be 1–12");
        Ok(Self {
            key: BAND_KEYS[band - 1] + field as u16,
            field,
        })
    }

    pub fn read_request(self) -> [u8; 2] {
        self.key.to_le_bytes()
    }

    pub fn effect_write(self, value: i32) -> Result<[u8; 8]> {
        // Keep writes within Utility gain limits. The DSP can report a wider range.
        self.validate(value, 9)?;
        let mut record = [0; 8];
        record[..4].copy_from_slice(&u32::from(self.key).to_le_bytes());
        record[4..].copy_from_slice(&value.to_le_bytes());
        Ok(record)
    }

    pub fn poll_reply(self, payload: &[u8]) -> Result<EqReadback> {
        match payload {
            [0, key_lo, key_hi, a, b, c, d] => {
                ensure!(
                    u16::from_le_bytes([*key_lo, *key_hi]) == self.key,
                    "EQ reply key does not match request"
                );
                let raw = i32::from_le_bytes([*a, *b, *c, *d]);
                self.validate(raw, 12)?;
                Ok(EqReadback::Value(raw))
            }
            [1] => Ok(EqReadback::Pending),
            [2] => Ok(EqReadback::Busy),
            [3] => bail!("Firmware rejected the EQ readback request"),
            [4] => Ok(EqReadback::NeedsResync),
            [5] => Ok(EqReadback::Idle),
            _ => bail!("Invalid EQ readback response"),
        }
    }

    fn validate(self, value: i32, gain_limit: i32) -> Result<()> {
        let range = match self.field {
            EqField::Frequency => 0..=240,
            EqField::Gain => -gain_limit..=gain_limit,
            EqField::Width => 0..=32,
        };
        ensure!(range.contains(&value), "Raw EQ value is out of range");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn covers_all_36_keys() -> Result<()> {
        let mut keys = HashSet::new();
        for band in 1..=12 {
            for (field, sample) in [
                (EqField::Frequency, 120),
                (EqField::Gain, -3),
                (EqField::Width, 32),
            ] {
                let parameter = EqParameter::new(band, field)?;
                let key = parameter.read_request();
                assert!(keys.insert(key));
                assert_eq!(parameter.effect_write(sample)?[..4], [key[0], key[1], 0, 0]);
                let mut reply = vec![0, key[0], key[1]];
                reply.extend_from_slice(&sample.to_le_bytes());
                assert_eq!(parameter.poll_reply(&reply)?, EqReadback::Value(sample));
            }
        }
        assert_eq!(keys.len(), 36);
        Ok(())
    }

    #[test]
    fn rejects_invalid_parameters_and_replies() -> Result<()> {
        assert!(EqParameter::new(0, EqField::Frequency).is_err());
        assert!(EqParameter::new(13, EqField::Frequency).is_err());
        let gain = EqParameter::new(11, EqField::Gain)?;
        assert_eq!(gain.read_request(), [0xf6, 0]);
        assert!(gain.effect_write(-10).is_err());
        assert_eq!(
            gain.poll_reply(&[0, 0xf6, 0, 0xf4, 0xff, 0xff, 0xff])?,
            EqReadback::Value(-12)
        );
        assert!(gain.poll_reply(&[0, 0xf7, 0, 0, 0, 0, 0]).is_err());
        assert!(gain.poll_reply(&[0, 0xf6]).is_err());
        assert_eq!(gain.poll_reply(&[4])?, EqReadback::NeedsResync);
        let width = EqParameter::new(12, EqField::Width)?;
        assert_eq!(width.read_request(), [0x0e, 1]);
        assert!(width.effect_write(33).is_err());
        Ok(())
    }
}
