//! Offline execution of serialized stock dynamics control paths, never USB I/O.
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{
    core::{DspState, MemoryMap, MemoryRegion, RegionKind},
    jit::JitEngine,
};
use std::{env, fs};

#[path = "dynamics_kernel.rs"]
mod dynamics_kernel;

#[path = "dynamics_graph_provenance.rs"]
mod dynamics_graph_provenance;

#[path = "clock_kernel.rs"]
mod clock_kernel;

const FRAME: u32 = 0x1800;
const QUEUE: u32 = 0x2000;
const VALUES: u32 = 0x1200;
const FLAGS: u32 = 0x1400;

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

fn execute(state: &mut DspState, jit: &mut JitEngine, entry: u32) {
    state.registers[reg::R7] = 0x500;
    state.stack_push(0x1f, 0);
    state.pc = entry;
    state.run(jit, 100_000);
    assert_eq!(state.pc, 0x1f, "original DSP routine did not return");
}

fn sysex(state: &mut DspState, jit: &mut JitEngine, command: u32, data: &[u32]) {
    for (offset, value) in [0xf0, 0, 1, 0x38, 0, 0x10, command]
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

fn main() {
    // Run the hash-checking Go auditor first; this harness checks the exact code it uses.
    let image = fs::read(env::args_os().nth(1).expect("901 or 908 firmware path")).unwrap();
    assert_eq!(image.len(), 640044);
    assert_eq!(&image[0x3a2b0..0x3a2ba], b"TCHDSP_V2:");
    let build = u32::from_le_bytes(image[0x1c..0x20].try_into().unwrap());
    assert!([901, 908, 909].contains(&build));
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
        let address = word(&image, offset + 3) as usize;
        assert_eq!(word(&image, offset + 6), 0);
        offset += 9;
        for entry in &mut p[address..address + count] {
            *entry = word(&image, offset);
            offset += 3;
        }
    }
    for descriptor in p[0x40ab6d..].chunks_exact(4) {
        let [flags, start, source, count] = *descriptor else {
            unreachable!()
        };
        if flags == 0 {
            break;
        }
        let (start, source, count) = (start as usize, source as usize, count as usize);
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
        &p[0x0344de..0x0344e3],
        &[0x0344e3, 0x0344ed, 0x0344ff, 0x034509, 0x034519]
    );
    assert_eq!(
        &p[0x0334af..0x0334b5],
        &[0x0334b5, 0x0334b9, 0x0334cc, 0x0334d6, 0x0334e6, 0x0334ef]
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
    if env::args().any(|arg| arg == "--clock-kernel") {
        clock_kernel::run(&mut state, &mut jit, build);
        return;
    }
    if env::args().any(|arg| arg == "--graph-provenance") {
        dynamics_graph_provenance::run(&mut state, &mut jit, build);
        return;
    }
    // Synthetic allocations and a stopped scheduler: tests actual parser/mailbox/query code,
    // not physical object allocation, SHI transport, sample processing or audio quality.
    for (address, value) in [
        (0x41a037, 0),
        (0x41c19f, 0),
        (0x41c1a0, 0),
        (0x41c1a1, 0x10),
        (0x419e40, 0x1000),
        (0x41a024, VALUES),
        (0x41a025, FLAGS),
        (0xe16, 15),
        (0xe17, QUEUE),
        (0xe18, QUEUE + 15),
    ] {
        state.write_memory(MemSpace::X, address, value);
    }
    let keys = (0xa..=0xf).chain(0x13c..=0x145).chain(0x15a..=0x15e);
    state.registers[reg::A1] = 48000;
    execute(&mut state, &mut jit, 0x0322f8);
    let rate_pair = (state.registers[reg::B1], state.registers[reg::A1]);
    for (index, object) in [(5, 0x3200), (16, 0x3600), (17, 0x3800), (18, 0x3a00)] {
        state.write_memory(MemSpace::X, 0x419e2c + index, object);
        state.write_memory(MemSpace::X, object + 4, object + 0x80);
        state.write_memory(MemSpace::X, object + 0x85, object + 0x100);
        state.write_memory(MemSpace::X, object + 8, object + 0x100);
        state.write_memory(MemSpace::X, object + 19, 1);
        let rate_offset = if index == 5 { 42 } else { 21 };
        state.write_memory(MemSpace::X, object + rate_offset, rate_pair.0);
        state.write_memory(MemSpace::X, object + rate_offset + 1, rate_pair.1);
    }
    if env::args().any(|arg| arg == "--kernel") {
        dynamics_kernel::run(&mut state, &mut jit, build);
        return;
    }
    let mut cases = 0;
    for key in keys {
        let index = state.read_memory(MemSpace::X, 0x41abe9 + key);
        assert_ne!(index, 0xffffff);
        let descriptor = 0x41a391 + 8 * index;
        assert_eq!(state.read_memory(MemSpace::X, descriptor + 4), key);
        let route_index = state.read_memory(MemSpace::X, descriptor + 6) & 0xffff;
        let route = state.read_memory(MemSpace::X, 0x427441 + route_index);
        assert_eq!(route >> 22, 0);
        let slot = (route >> 10) & 0xfff;
        let signed = |v: u32| (v << 8) as i32 >> 8;
        let minimum = signed(state.read_memory(MemSpace::X, descriptor));
        let maximum = signed(state.read_memory(MemSpace::X, descriptor + 2));
        let default = signed(state.read_memory(MemSpace::X, descriptor + 3));
        for value in [minimum - 1, minimum, default, maximum, maximum + 1] {
            let expected = value.clamp(minimum, maximum) as u32 & 0xffffff;
            let key_bytes = [key >> 7, key & 127];
            let value_bytes = [21, 14, 7, 0].map(|shift| (value as u32 >> shift) & 127);
            state.write_memory(MemSpace::X, 0x419cbe + index, 0x555555);
            for i in 0..179 {
                state.write_memory(MemSpace::X, FLAGS + i, 0);
            }
            sysex(
                &mut state,
                &mut jit,
                0x35,
                &[key_bytes.as_slice(), &value_bytes].concat(),
            );
            assert_eq!(state.read_memory(MemSpace::X, 0x419cbe + index), expected);
            for i in 0..179 {
                assert_eq!(
                    state.read_memory(MemSpace::X, FLAGS + i),
                    u32::from(i == slot),
                    "dirty slot {i:X}, key {key:X}"
                );
            }
            if route & 1023 == 1023 {
                assert_eq!(state.read_memory(MemSpace::X, VALUES + slot), expected);
            }
            state.write_memory(MemSpace::X, 0xe17, QUEUE);
            sysex(&mut state, &mut jit, 0x47, &key_bytes);
            let end = state.read_memory(MemSpace::X, 0xe17);
            let reply: Vec<_> = (QUEUE..end)
                .map(|a| state.read_memory(MemSpace::X, a))
                .collect();
            let encoded = if expected & 0x800000 != 0 {
                expected | 0xf000000
            } else {
                expected
            };
            let bytes = [21, 14, 7, 0].map(|shift| (encoded >> shift) & 127);
            assert_eq!(
                reply,
                vec![
                    0xf00001,
                    0x380010,
                    0x22ffff,
                    key_bytes[0] << 16 | key_bytes[1] << 8 | 255,
                    bytes[0] << 16 | bytes[1] << 8 | 255,
                    bytes[2] << 16 | bytes[3] << 8 | 255,
                    0xf7ffff
                ]
            );
            let first = state.read_memory(MemSpace::X, 0x427536 + 2 * slot);
            let second = state.read_memory(MemSpace::X, 0x427537 + 2 * slot);
            let object_index = (second >> 8) & 255;
            let object = state.read_memory(MemSpace::X, 0x419e2c + object_index);
            let bank = object + 0x100;
            state.write_memory(MemSpace::Y, bank - 1, 0x555555);
            state.write_memory(MemSpace::Y, bank + 16, 0x555555);
            let staged = state.read_memory(MemSpace::X, VALUES + slot);
            state.registers[reg::A1] = slot;
            state.registers[reg::B1] = staged;
            execute(&mut state, &mut jit, 0x402aa2);
            assert_eq!(state.read_memory(MemSpace::Y, bank - 1), 0x555555);
            assert_eq!(state.read_memory(MemSpace::Y, bank + 16), 0x555555);
            let command = first & 0xffff;
            let table_pair = match command {
                0x73 => Some((0x41bd70, 9)),
                0x75 => Some((0x41bd8e, 13)),
                0x76 => Some((0x41bdb6, 15)),
                0x0e => Some((0x41b970, 36)),
                0x0f => Some((0x41b998, 38)),
                _ => None,
            };
            if let Some((table, field)) = table_pair {
                for i in 0..2 {
                    assert_eq!(
                        state.read_memory(MemSpace::X, object + field + i),
                        state.read_memory(MemSpace::X, table + 2 * expected + i)
                    );
                }
            }
            if key == 0xa {
                assert_eq!(state.read_memory(MemSpace::Y, bank + 1), expected);
            }
            if key == 0xe {
                assert_eq!(
                    state.read_memory(MemSpace::Y, bank + 10),
                    state.read_memory(MemSpace::X, 0x41b9c0 + expected)
                );
            }
            cases += 1;
        }
        println!(
            "PASS build {build} key {key:03X} -> slot {slot:02X}: bounds, dirty-flag isolation, native reply/setter, guards"
        );
    }
    assert_eq!(cases, 105);
    // Recover the time criterion from the actual coefficient setter, not UI labels.
    let attack_ms = [
        1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 12., 14., 16., 18., 20., 23., 26., 30., 35., 40.,
    ];
    let release_ms = [
        15., 15., 25., 35., 45., 55., 65., 75., 85., 100., 115., 140., 170., 230., 340., 680.,
        1000., 1500., 2000., 3000.,
    ];
    for (slot, object, coefficient, times, log_target) in [
        (0x24, 0x3200, 6, attack_ms, 0.01_f64.ln()),
        (0x25, 0x3200, 7, release_ms, 0.01_f64.ln()),
        (
            0xa1,
            0x3800,
            7,
            [
                0.001, 2., 3., 4., 5., 6., 7., 8., 9., 10., 12., 14., 16., 18., 20., 23., 26., 30.,
                35., 40.,
            ],
            -1.,
        ),
        (0xa2, 0x3800, 8, release_ms, -1.),
    ] {
        let mut maximum_error = 0_f64;
        for (raw, milliseconds) in times.into_iter().enumerate() {
            state.registers[reg::A1] = slot;
            state.registers[reg::B1] = raw as u32;
            execute(&mut state, &mut jit, 0x402aa2);
            let actual =
                state.read_memory(MemSpace::Y, object + 0x100 + coefficient) as f64 / 8388608.;
            let pole = (log_target / (48000. * milliseconds / 1000.)).exp();
            let expected = if object == 0x3200 { pole } else { 1. - pole };
            maximum_error = maximum_error.max((actual - expected).abs());
            assert!(
                (actual - expected).abs() < 0.00001,
                "slot {slot:X} raw {raw}, actual {actual}, expected {expected}"
            );
        }
        println!("PASS 20 native timing coefficients slot {slot:02X}, max error {maximum_error}");
    }
    for raw in 0..=100 {
        let before: Vec<_> = (0..14)
            .map(|i| state.read_memory(MemSpace::Y, 0x3300 + i))
            .collect();
        let pieces = [21, 14, 7, 0].map(|shift| (raw >> shift) & 127);
        sysex(
            &mut state,
            &mut jit,
            0x35,
            &[0, 0x0f, pieces[0], pieces[1], pieces[2], pieces[3]],
        );
        state.registers[reg::A1] = 0x27;
        state.registers[reg::B1] = state.read_memory(MemSpace::X, VALUES + 0x27);
        execute(&mut state, &mut jit, 0x402aa2);
        for (i, old) in before.into_iter().enumerate() {
            let actual = state.read_memory(MemSpace::Y, 0x3300 + i as u32);
            if i == 13 {
                assert!((actual as f64 / 8388608. - raw as f64 / 100.).abs() < 0.00025);
            } else {
                assert_eq!(actual, old, "Detector weight changed another coefficient");
            }
        }
    }
    println!("PASS 101 detector weights: normalized 0..1, only Y:core+13 changes");
    for raw in [0, 6, 12, 24] {
        state.registers[reg::A1] = 0xa3;
        state.registers[reg::B1] = raw;
        execute(&mut state, &mut jit, 0x402aa2);
        let word = state.read_memory(MemSpace::Y, 0x3902);
        let value = ((word << 8) as i32 >> 8) as f64 / 8388608.;
        let offset = 0x780a3e as f64 / 8388608. * 32.;
        let scale = 0x550a96 as f64 / 8388608. / 256.;
        assert!((value - (raw as f64 - offset) * scale).abs() < 0.000001);
        println!("PASS compressor makeup {raw} dB -> signed log-domain coefficient {value}");
    }
    let children = [
        0, 38, 39, 25, 68, 30, 32, 35, 26, 21, 22, 23, 24, 36, 57, 27, 13, 28, 33,
    ];
    for child in 0..=68 {
        state.write_memory(MemSpace::X, 0x7000 + child, 0x8000 + child * 64);
    }
    state.write_memory(MemSpace::X, 0x8000 + 68 * 64 + 48, 0xa000);
    state.registers[reg::R0] = 0x7000;
    execute(&mut state, &mut jit, 0x4003f4);
    for (slot, child) in children.into_iter().enumerate() {
        let expected = if slot == 0 {
            0x7000
        } else if slot == 4 {
            0xa000
        } else {
            0x8000 + child * 64
        };
        assert_eq!(
            state.read_memory(MemSpace::X, 0x419e2c + slot as u32),
            expected
        );
    }
    println!(
        "PASS original object registration: de-esser root+30; compressor families root+13/+28/+33"
    );
    println!(
        "PASS {cases} serialized control cases, 185 coefficient cases and object registration; no live DSP load or acoustic result claimed"
    );
}
