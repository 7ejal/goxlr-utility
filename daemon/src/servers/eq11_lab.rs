//! Host-only test transport and block scheduler. No USB access, no firmware
//! command, no audio-device access. The candidate four-lane block is synthetic.
const Q23: i64 = 1 << 23;
const WORDS: usize = 120;
pub(crate) const PACKET_SIZE: usize = 376;

#[derive(Clone, Copy)]
struct Bank {
    enabled: bool,
    preamp: u32,
    words: [i32; WORDS],
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn decode(bytes: &[u8]) -> Result<Bank, &'static str> {
    if bytes.len() != PACKET_SIZE || &bytes[..4] != b"E11L" {
        return Err("Invalid EQ11 lab packet size or magic");
    }
    if bytes[4] != 1 || bytes[5] > 1 || bytes[6..8] != [0, 0] {
        return Err("Unsupported version, flags or reserved bits");
    }
    if crc32(&bytes[..372]) != u32::from_be_bytes(bytes[372..376].try_into().unwrap()) {
        return Err("Invalid CRC32");
    }
    let preamp = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
    if preamp > Q23 as u32 {
        return Err("Preamp must be <= unity");
    }
    let mut words = [0; WORDS];
    for (word, data) in words.iter_mut().zip(bytes[12..372].as_chunks::<3>().0) {
        let raw = i32::from_be_bytes([0, data[0], data[1], data[2]]);
        *word = if raw & 0x800000 != 0 {
            raw - 0x1000000
        } else {
            raw
        };
    }
    for channel in words.as_chunks::<60>().0 {
        if channel[55..60] != [0, 0, 0x080000, 0, 0] {
            return Err("Twelfth section must be the exact stock identity");
        }
        for section in channel.as_chunks::<5>().0 {
            let d = i64::from(section[3]);
            let e = i64::from(section[4]);
            // Stock layout: f1=-2e/Q23; f2=-d/Q23. Jury stability.
            if d.abs() >= Q23 || Q23 + 2 * e + d <= 0 || Q23 - 2 * e + d <= 0 {
                return Err("Unstable stock-layout filter");
            }
        }
    }
    Ok(Bank {
        enabled: bytes[5] == 1,
        preamp,
        words,
    })
}

#[derive(Default)]
pub(crate) struct Eq11Lab {
    pending: Option<Bank>,
    active: Option<Bank>,
    generation: u64,
    history: [[[f64; 4]; 12]; 2],
}

pub(crate) struct LabResult {
    pub(crate) generation: u64,
    pub(crate) changed_samples: [usize; 2],
    pub(crate) other_lanes_unchanged: bool,
}

impl Eq11Lab {
    fn stage(&mut self, packet: &[u8]) -> Result<(), &'static str> {
        let validated = decode(packet)?;
        self.pending = Some(validated);
        Ok(())
    }

    fn process(&mut self, block: &mut [[i32; 4]; 64]) {
        // One complete stereo bank is activated only at a block boundary.
        // No partially updated coefficient bank can become active.
        if let Some(bank) = self.pending.take() {
            self.active = Some(bank);
            self.history = [[[0.0; 4]; 12]; 2];
            self.generation = self.generation.saturating_add(1);
        }
        let Some(bank) = self.active else { return };
        if !bank.enabled {
            return;
        }
        // Float reference model. History is continuous within a generation.
        // Reset on coefficient changes; NOT a click-free implementation.
        for (ch, history) in self.history.iter_mut().enumerate() {
            for frame in block.iter_mut() {
                let mut input = f64::from(frame[ch]) * f64::from(bank.preamp) / Q23 as f64;
                for (index, h) in history.iter_mut().enumerate() {
                    let w = &bank.words[ch * 60 + index * 5..][..5];
                    let s = [0.5, 1.0, 0.25][index % 3];
                    let k = Q23 as f64;
                    let y = input * f64::from(w[2]) * 4.0 / (k * s)
                        + h[0] * f64::from(w[1]) * 4.0 / (k * s)
                        + h[1] * f64::from(w[0]) * 2.0 / (k * s)
                        - h[2] * f64::from(w[4]) * 2.0 / k
                        - h[3] * f64::from(w[3]) / k;
                    *h = [input, h[0], y, h[2]];
                    input = y;
                }
                frame[ch] = input.round().clamp(-8388608.0, 8388607.0) as i32;
            }
        }
    }

    pub(crate) fn run_update(&mut self, packet: &[u8]) -> Result<LabResult, &'static str> {
        self.stage(packet)?;
        let original = std::array::from_fn(|i| {
            let t = i as i32;
            [
                0x080000 + t * 256,
                0x040000 - t * 128,
                0x100000 + t,
                -0x080000 - t,
            ]
        });
        let mut block = original;
        self.process(&mut block);
        Ok(LabResult {
            generation: self.generation,
            changed_samples: std::array::from_fn(|ch| {
                block
                    .iter()
                    .zip(&original)
                    .filter(|(a, b)| a[ch] != b[ch])
                    .count()
            }),
            other_lanes_unchanged: block.iter().zip(&original).all(|(a, b)| a[2..] == b[2..]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(enabled: bool, preamp: u32) -> [u8; PACKET_SIZE] {
        let mut p = [0; PACKET_SIZE];
        p[..4].copy_from_slice(b"E11L");
        p[4] = 1;
        p[5] = u8::from(enabled);
        p[8..12].copy_from_slice(&preamp.to_be_bytes());
        for i in 0..24 {
            let word = [0x100000u32, 0x200000, 0x080000][i % 3].to_be_bytes();
            p[12 + i * 15 + 6..12 + i * 15 + 9].copy_from_slice(&word[1..]);
        }
        fix_crc(&mut p);
        p
    }
    fn fix_crc(p: &mut [u8; PACKET_SIZE]) {
        let crc = crc32(&p[..372]);
        p[372..].copy_from_slice(&crc.to_be_bytes());
    }
    #[test]
    fn block_commit_is_atomic_and_bypass_preserves_every_lane() {
        let mut lab = Eq11Lab::default();
        lab.stage(&packet(true, 1 << 22)).unwrap();
        assert!(lab.active.is_none());
        assert_eq!(lab.generation, 0);
        let mut block = [[1000, -1000, 3000, -4000]; 64];
        lab.process(&mut block);
        assert_eq!(lab.generation, 1);
        assert_eq!(block, [[500, -500, 3000, -4000]; 64]);
        let r = lab.run_update(&packet(false, 1 << 22)).unwrap();
        assert_eq!(r.generation, 2);
        assert_eq!(r.changed_samples, [0, 0]);
        assert!(r.other_lanes_unchanged);
    }
    #[test]
    fn bad_updates_leave_active_and_pending_untouched() {
        let mut lab = Eq11Lab::default();
        let good = packet(true, 1 << 22);
        lab.stage(&good).unwrap();
        for index in [0, 4, 5, 6, 12, 372] {
            let mut bad = good;
            bad[index] ^= 0xff;
            assert!(lab.stage(&bad).is_err());
        }
        assert!(lab.stage(&good[..375]).is_err());
        let mut bad = good;
        bad[8..12].copy_from_slice(&0xffffffffu32.to_be_bytes());
        fix_crc(&mut bad);
        assert!(lab.stage(&bad).is_err());
        let mut bad = good;
        bad[21..24].copy_from_slice(&[0x80, 0, 0]);
        fix_crc(&mut bad);
        assert!(lab.stage(&bad).is_err());
        let mut bad = good;
        bad[12 + 11 * 15 + 6] ^= 1;
        fix_crc(&mut bad);
        assert!(lab.stage(&bad).is_err());
        let mut block = [[1000, -1000, 3, 4]; 64];
        lab.process(&mut block);
        assert_eq!(lab.generation, 1);
        assert_eq!(block[0], [500, -500, 3, 4]);
    }
    #[test]
    fn browser_generated_packet_passes_receiver_and_changes_both_lanes() {
        let packet = include_bytes!("../../../headphone-eq-tests/eq11-update-fixture.bin");
        let mut lab = Eq11Lab::default();
        let r = lab.run_update(packet).unwrap();
        assert_eq!(r.generation, 1);
        assert!(r.changed_samples.iter().all(|&n| n > 0));
        assert!(r.other_lanes_unchanged);
    }
    #[test]
    fn repeated_updates_and_corruption_stress() {
        let fixture = include_bytes!("../../../headphone-eq-tests/eq11-update-fixture.bin");
        let mut lab = Eq11Lab::default();
        for i in 0..1000u64 {
            let valid = if i % 2 == 0 {
                *fixture
            } else {
                packet(false, 1 << 23)
            };
            let r = lab.run_update(&valid).unwrap();
            assert_eq!(r.generation, i + 1);
            assert!(r.other_lanes_unchanged);
            let mut damaged = valid;
            let index = (i as usize * 17) % 372;
            damaged[index] ^= 1;
            assert!(lab.stage(&damaged).is_err());
            assert!(lab.pending.is_none());
            assert_eq!(lab.generation, i + 1);
            let mut block = [[120000, -90000, 12345, -23456]; 64];
            lab.process(&mut block);
            assert_eq!(lab.generation, i + 1);
            assert!(block.iter().all(|f| f[2..] == [12345, -23456]));
            if i % 2 == 1 {
                assert_eq!(block, [[120000, -90000, 12345, -23456]; 64]);
            }
        }
    }
    #[test]
    fn filter_history_continues_between_blocks_without_update() {
        let mut lab = Eq11Lab::default();
        lab.stage(include_bytes!(
            "../../../headphone-eq-tests/eq11-update-fixture.bin"
        ))
        .unwrap();
        let mut impulse = [[0; 4]; 64];
        impulse[63] = [1000000, -1000000, 0, 0];
        lab.process(&mut impulse);
        let mut tail = [[0; 4]; 64];
        lab.process(&mut tail);
        assert!(tail.iter().any(|f| f[0] != 0));
        assert!(tail.iter().any(|f| f[1] != 0));
        assert!(tail.iter().all(|f| f[2..] == [0, 0]));
        assert_eq!(lab.generation, 1);
    }
    #[test]
    fn crc_known_vector_and_successive_stereo_updates() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        let mut lab = Eq11Lab::default();
        let r = lab.run_update(&packet(true, 1 << 22)).unwrap();
        assert_eq!(r.changed_samples, [64, 64]);
        assert!(r.other_lanes_unchanged);
        let r = lab.run_update(&packet(true, 1 << 23)).unwrap();
        assert_eq!(r.generation, 2);
        assert_eq!(r.changed_samples, [0, 0]);
    }
}
