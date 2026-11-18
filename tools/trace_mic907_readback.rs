//! Offline replay of the 907 DSP effect write and parameter query.
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{
    core::{DspState, MemoryMap, MemoryRegion, RegionKind},
    jit::JitEngine,
};
use std::{env, fs};

// First frequency key and mailbox in each frequency/gain/width triplet.
const BANDS: [(u32, u32); 12] = [
    (0x126, 0x4a),
    (0xf8, 0x47),
    (0x113, 0x4e),
    (0x129, 0x54),
    (0x116, 0x51),
    (0x11d, 0x58),
    (0x12c, 0x5e),
    (0x120, 0x5b),
    (0x109, 0x62),
    (0x12f, 0x68),
    (0xf5, 0x44),
    (0x10c, 0x65),
];
const FRAME: u32 = 0x1800;
const QUEUE: u32 = 0x2000;
const PARAMETERS: u32 = 0x3000;
const COEFFICIENTS: u32 = 0x3100;

fn word(image: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([0, image[offset], image[offset + 1], image[offset + 2]])
}

fn region(buffer: &mut [u32]) -> Vec<MemoryRegion> {
    vec![MemoryRegion {
        start: 0,
        end: buffer.len() as u32,
        kind: RegionKind::Buffer {
            base: buffer.as_mut_ptr(),
            offset: 0,
        },
    }]
}

fn sysex(state: &mut DspState, jit: &mut JitEngine, command: u32, data: &[u32]) {
    let header = [0xf0, 0, 1, 0x38, 0, 0x10, command];
    for (offset, value) in header
        .into_iter()
        .chain(data.iter().copied())
        .chain([0xf7])
        .enumerate()
    {
        state.write_memory(MemSpace::X, FRAME + offset as u32, value);
    }
    state.write_memory(MemSpace::X, FRAME - 1, 0);
    state.registers[reg::R0] = FRAME - 1;
    state.registers[reg::R4] = FRAME;
    state.registers[reg::A1] = (8 + data.len()) as u32;
    execute(state, jit, 0x403155);
}

fn execute(state: &mut DspState, jit: &mut JitEngine, entry: u32) {
    state.registers[reg::R7] = 0x500;
    state.stack_push(0x1f, 0);
    state.pc = entry;
    state.run(jit, 100_000);
    assert_eq!(state.pc, 0x1f, "DSP SysEx handler did not return");
}

fn main() {
    let image = fs::read(env::args_os().nth(1).expect("907 firmware image")).unwrap();
    assert_eq!(&image[0x3a2b0..0x3a2ba], b"TCHDSP_V2:");
    assert_eq!(&image[0x38c80..0x38c88], &[1, 8, 0, 0, 0x30, 0xae, 0, 0]);
    assert_eq!(
        u32::from_le_bytes(image[0x1c..0x20].try_into().unwrap()),
        907
    );
    let mut p = vec![0; 0x420000];
    let mut x = vec![0; 0x1000000];
    let mut y = vec![0; 0x1000000];
    let boot = 0x3a2b0 + 22;
    let mut offset = boot + 6 + 3 * word(&image, boot) as usize;
    loop {
        let count = word(&image, offset) as usize;
        if count == 0 {
            break;
        }
        let addr = word(&image, offset + 3) as usize;
        assert_eq!(word(&image, offset + 6), 0);
        offset += 9;
        for entry in &mut p[addr..addr + count] {
            *entry = word(&image, offset);
            offset += 3;
        }
    }
    for descriptor in p[0x40ab6d..].chunks_exact(4) {
        let [flags, start, source, count] = descriptor else {
            unreachable!()
        };
        if *flags == 0 {
            break;
        }
        let (start, source, count) = (*start as usize, *source as usize, *count as usize);
        if flags & 1 == 0 {
            continue;
        }
        if flags & 0x400 != 0 {
            for i in 0..count {
                x[start + i] = p[source + 2 * i];
                y[start + i] = p[source + 2 * i + 1];
            }
        } else if flags & 0x100 == 0 || flags & 0x200 != 0 {
            let memory = if flags & 0x100 == 0 { &mut x } else { &mut y };
            memory[start..start + count].copy_from_slice(&p[source..source + count]);
        }
    }
    assert_eq!(
        &x[0x427110..0x427119],
        &[0x35, 0, 0x40308e, 0x22, 14, 0x403088, 0x47, 10, 0x4030ec]
    );
    let mut jit = JitEngine::new(p.len());
    p[0x1f] = 0x0c001f;
    let mut state = DspState::new(MemoryMap {
        p_regions: region(&mut p),
        x_regions: region(&mut x),
        y_regions: region(&mut y),
    });
    state.registers[reg::SR] = 0x840300;
    state.registers[reg::OMR] = 0x200000;
    // Scratch graph staging and outbound ring. The real DSP handlers still perform their own routing and enqueue.
    for (addr, value) in [
        (0x41a037, 0),
        (0x41c19f, 0),
        (0x41c1a0, 0),
        (0x41c1a1, 0x10),
        (0x419e40, 0x1000),
        (0x419e35, 0x3200),
        (0x419e36, 0x3200),
        (0x419e37, 0x3200),
        (0x419e38, 0x3200),
        (0x419e41, 0x4000),
        (0x4000 + 238, 48000),
        (0x3200 + 17, COEFFICIENTS),
        (0x3200 + 18, PARAMETERS),
        (0x41a024, 0x1200),
        (0x41a025, 0x1400),
        (0xe16, 15),
        (0xe17, QUEUE),
        (0xe18, QUEUE + 15),
    ] {
        state.write_memory(MemSpace::X, addr, value);
    }
    let cases = BANDS
        .into_iter()
        .enumerate()
        .flat_map(|(band, (key, mailbox))| {
            let values = if band % 2 == 0 {
                [174, -3, 8]
            } else {
                [222, 3, 24]
            };
            (0..3)
                .zip(values)
                .map(move |(field, value)| (key + field, mailbox + field, field, value))
        });
    for (key, mailbox, field, value) in cases {
        let index = state.read_memory(MemSpace::X, 0x41abe9 + key);
        let descriptor = 0x41a391 + 8 * index;
        assert_eq!(state.read_memory(MemSpace::X, descriptor + 4), key);
        let route = state.read_memory(MemSpace::X, descriptor + 6) & 0xffff;
        assert_eq!(
            state.read_memory(MemSpace::X, 0x427441 + route) >> 10,
            mailbox
        );
        let encoded = value as u32;
        let key_bytes = [key >> 7, key & 0x7f];
        let value_bytes = [21, 14, 7, 0].map(|shift| (encoded >> shift) & 0x7f);
        state.write_memory(MemSpace::X, 0x419cbe + index, 0xffffff);
        sysex(
            &mut state,
            &mut jit,
            0x35,
            &[key_bytes.as_slice(), &value_bytes].concat(),
        );
        assert_eq!(
            state.read_memory(MemSpace::X, 0x419cbe + index),
            encoded & 0xffffff
        );
        assert_eq!(
            state.read_memory(MemSpace::X, 0x1400 + mailbox),
            1,
            "mailbox {mailbox:X} not staged"
        );
        let first_mailbox = [0x44, 0x4e, 0x58, 0x62]
            .into_iter()
            .rfind(|first| mailbox >= *first)
            .unwrap();
        let slot = [0, 2, 1][((mailbox - first_mailbox) / 3) as usize];
        let object_field = PARAMETERS + slot * 6 + 2 + field;
        for section in 0..3 {
            let fields = [1, 1, 1000 * 256, 6, 16, [2, 1, 3][section as usize]];
            for (offset, field) in fields.into_iter().enumerate() {
                state.write_memory(MemSpace::X, PARAMETERS + section * 6 + offset as u32, field);
            }
        }
        for offset in 0..15 {
            state.write_memory(MemSpace::X, COEFFICIENTS + offset, 0x555555);
        }
        let before: Vec<_> = (PARAMETERS..PARAMETERS + 18)
            .map(|addr| state.read_memory(MemSpace::X, addr))
            .collect();
        let staged = state.read_memory(MemSpace::X, 0x1200 + mailbox);
        match field {
            0 => {
                let hz = 20.0 * 2.0_f64.powf(f64::from(value) / 24.0);
                assert!((f64::from(staged) / 256.0 - hz).abs() < 1.0);
            }
            1 => assert_eq!(staged, encoded & 0xffffff),
            _ => assert_eq!(staged, (32 - value) as u32),
        }
        state.registers[reg::A1] = mailbox;
        state.registers[reg::B1] = staged;
        execute(&mut state, &mut jit, 0x402aa2);
        for (offset, previous) in before.into_iter().enumerate() {
            let address = PARAMETERS + offset as u32;
            let expected = if address == object_field {
                staged
            } else {
                previous
            };
            assert_eq!(state.read_memory(MemSpace::X, address), expected);
        }
        for offset in 0..15 {
            assert_eq!(
                state.read_memory(MemSpace::X, COEFFICIENTS + offset) != 0x555555,
                (slot * 5..slot * 5 + 5).contains(&offset),
            );
        }
        state.write_memory(MemSpace::X, 0xe17, QUEUE);
        sysex(&mut state, &mut jit, 0x47, &key_bytes);
        let end = state.read_memory(MemSpace::X, 0xe17);
        let reply: Vec<_> = (QUEUE..end)
            .map(|addr| state.read_memory(MemSpace::X, addr))
            .collect();
        let reply_value = if value < 0 {
            (encoded & 0xffffff) | 0x0f000000
        } else {
            encoded
        };
        let reply_bytes = [21, 14, 7, 0].map(|shift| (reply_value >> shift) & 0x7f);
        assert_eq!(
            reply,
            [
                0xf00001,
                0x380010,
                0x22ffff,
                (key_bytes[0] << 16) | (key_bytes[1] << 8) | 0xff,
                (reply_bytes[0] << 16) | (reply_bytes[1] << 8) | 0xff,
                (reply_bytes[2] << 16) | (reply_bytes[3] << 8) | 0xff,
                0xf7ffff
            ]
        );
        println!(
            "PASS effect {key:04X} -> mailbox {mailbox:02X}, stored value {value}, reply {reply:06X?}"
        );
    }
}
