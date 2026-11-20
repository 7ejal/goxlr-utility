//! Payloads for the experimental 908 ARM EQ bridge; audio validation gates UI writes.

use anyhow::{Result, bail, ensure};

/// Commands registered by the patched 908 firmware. Never probe them on stock.
pub const SUBMIT_COMMAND: u32 = 0x0081_9000;
pub const POLL_COMMAND: u32 = 0x0081_9001;

pub fn arm_diagnostic_reply(metric: goxlr_types::ArmDiagnostic, payload: &[u8]) -> Result<u32> {
    match payload {
        [0, lo, hi, a, b, c, d] if u16::from_le_bytes([*lo, *hi]) == metric as u16 => {
            Ok(u32::from_le_bytes([*a, *b, *c, *d]))
        }
        [3] => bail!("Firmware rejected the ARM diagnostic"),
        _ => bail!("Invalid ARM diagnostic response"),
    }
}

const BAND_KEYS: [u16; 12] = [
    0x126, 0xf8, 0x113, 0x129, 0x116, 0x11d, 0x12c, 0x120, 0x109, 0x12f, 0xf5, 0x10c,
];

pub use goxlr_types::MicEqField as EqField;

#[derive(Clone, Copy, Debug)]
pub struct EqParameter {
    key: u16,
    minimum: i32,
    maximum: i32,
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
        ensure!(
            (1..=BAND_KEYS.len()).contains(&band),
            "EQ band must be 1–12"
        );
        let (minimum, maximum) = match field {
            EqField::Frequency => (0, 240),
            EqField::Gain => (-12, 12),
            EqField::Width => (0, 32),
        };
        Ok(Self {
            key: BAND_KEYS[band - 1] + field as u16,
            minimum,
            maximum,
        })
    }

    pub fn dynamics(control: goxlr_types::DynamicsControl) -> Self {
        let range = control.range();
        Self {
            key: control.key(),
            minimum: *range.start(),
            maximum: *range.end(),
        }
    }

    pub fn read_request(self) -> [u8; 2] {
        self.key.to_le_bytes()
    }

    pub fn effect_write(self, value: i32) -> Result<[u8; 8]> {
        self.validate(value)?;
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
                self.validate(raw)?;
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

    fn validate(self, value: i32) -> Result<()> {
        ensure!(
            (self.minimum..=self.maximum).contains(&value),
            "Raw DSP value is out of range"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn arm_diagnostics_preserve_unsigned_values_and_validate_framing() -> Result<()> {
        use goxlr_types::ArmDiagnostic::CpuAudioBit22Count;
        assert_eq!(
            arm_diagnostic_reply(CpuAudioBit22Count, &[0, 1, 0, 255, 255, 255, 255])?,
            u32::MAX
        );
        for packet in [
            &[0, 2, 0, 0, 0, 0, 0][..],
            &[0, 1, 0][..],
            &[3][..],
            &[1][..],
        ] {
            assert!(arm_diagnostic_reply(CpuAudioBit22Count, packet).is_err());
        }
        Ok(())
    }

    #[test]
    fn dynamics_packets_validate_keys_and_rom_bounds() -> Result<()> {
        for control in goxlr_types::DynamicsControl::ALL {
            let parameter = EqParameter::dynamics(control);
            assert_eq!(parameter.read_request(), control.key().to_le_bytes());
            let range = control.range();
            for value in [*range.start(), *range.end()] {
                let packet = parameter.effect_write(value)?;
                assert_eq!(&packet[4..], &value.to_le_bytes());
                let mut reply = vec![0];
                reply.extend_from_slice(&parameter.read_request());
                reply.extend_from_slice(&value.to_le_bytes());
                assert_eq!(parameter.poll_reply(&reply)?, EqReadback::Value(value));
            }
            assert!(parameter.effect_write(range.start() - 1).is_err());
            assert!(parameter.effect_write(range.end() + 1).is_err());
        }
        Ok(())
    }

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
        assert!(gain.effect_write(-13).is_err());
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

    #[test]
    fn transport_validates_replies_before_returning_values() -> Result<()> {
        use crate::commands::Command;
        use crate::device::base::{ExecutableGoXLR, GoXLRCommands, UsbData};
        use std::collections::VecDeque;

        struct Device(VecDeque<(Command, Vec<u8>, Vec<u8>)>);
        impl ExecutableGoXLR for Device {
            fn perform_request(
                &mut self,
                command: Command,
                body: &[u8],
                retry: bool,
            ) -> Result<Vec<u8>> {
                let (expected_command, expected_body, reply) =
                    self.0.pop_front().expect("Unexpected USB request");
                assert_eq!(command, expected_command);
                assert_eq!(body, expected_body);
                assert!(!retry);
                Ok(reply)
            }
            fn get_descriptor(&self) -> Result<UsbData> {
                bail!("Unused test descriptor")
            }
        }
        impl GoXLRCommands for Device {}
        let width = EqParameter::new(12, EqField::Width)?;
        let mut device = Device(VecDeque::from([
            (Command::SubmitEqRead, vec![0x0e, 1], vec![0]),
            (Command::PollEqRead, vec![], vec![1]),
            (Command::PollEqRead, vec![], vec![0, 0x0e, 1, 16, 0, 0, 0]),
            (Command::PollEqRead, vec![], vec![0, 0x0d, 1, 16, 0, 0, 0]),
            (Command::SubmitEqRead, vec![0x0e, 1], vec![0, 0]),
        ]));
        device.submit_eq_read(width)?;
        assert_eq!(device.poll_eq_read(width)?, EqReadback::Pending);
        assert_eq!(device.poll_eq_read(width)?, EqReadback::Value(16));
        assert!(device.poll_eq_read(width).is_err());
        assert!(device.submit_eq_read(width).is_err());
        assert!(device.set_eq_parameter(width, 33).is_err());
        assert!(device.0.is_empty());
        device.0.push_back((
            Command::ReadRecoveryPartition,
            vec![2, 0, 0, 0, 10, 0, 0, 0, 2, 0, 0, 0],
            vec![0x11, 0x22],
        ));
        assert_eq!(
            device.read_recovery_partition(goxlr_types::RecoveryPartition::Golden, 10, 2)?,
            vec![0x11, 0x22]
        );
        assert!(
            device
                .read_recovery_partition(goxlr_types::RecoveryPartition::Boot2, u32::MAX, 1)
                .is_err()
        );
        assert!(
            device
                .read_recovery_partition(goxlr_types::RecoveryPartition::Boot2, 0, 1025)
                .is_err()
        );
        assert!(device.0.is_empty());
        device.0.push_back((
            Command::ReadArmDiagnostic,
            vec![1, 0],
            vec![0, 1, 0, 255, 255, 255, 255],
        ));
        assert_eq!(
            device.read_arm_diagnostic(goxlr_types::ArmDiagnostic::CpuAudioBit22Count)?,
            u32::MAX
        );
        device.0.push_back((
            Command::ReadArmDiagnostic,
            vec![1, 0],
            vec![0, 2, 0, 0, 0, 0, 0],
        ));
        assert!(
            device
                .read_arm_diagnostic(goxlr_types::ArmDiagnostic::CpuAudioBit22Count)
                .is_err()
        );
        assert!(device.0.is_empty());
        Ok(())
    }
}
