//! Original PLL-derived arithmetic with supplied MMIO values; not a hardware clock measurement.
use super::execute;
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{core::DspState, jit::JitEngine};

fn native_float(mantissa: u32, exponent: u32) -> f64 {
    ((mantissa << 8) as i32 >> 8) as f64 / 8388608. * 2_f64.powi(exponent as i32 - 127)
}

fn expected(pctl: u32) -> f64 {
    24_576_000. * ((pctl & 255) + 1) as f64
        / ((((pctl >> 16) & 31) + 1) * (((pctl >> 14) & 3) + 1)) as f64
}

fn check(state: &mut DspState, jit: &mut JitEngine, pctl: u32) -> (u32, u32) {
    state.write_memory(MemSpace::X, 0xffff7d, pctl);
    state.registers[reg::SR] = 0x840300;
    execute(state, jit, 0x400213);
    let pair = (state.registers[reg::A1], state.registers[reg::B1]);
    let result = native_float(pair.0, pair.1);
    assert!(
        (result - expected(pctl)).abs() <= expected(pctl) * 0.0000002,
        "Native PLL calculation mismatch for {pctl:06X}: {result} vs {}",
        expected(pctl)
    );
    assert_eq!(state.registers[reg::R7], 0x500);
    assert_eq!(state.read_memory(MemSpace::X, 0xffff7d), pctl);
    pair
}

pub fn run(state: &mut DspState, jit: &mut JitEngine, build: u32) {
    let words: [u32; 45] = [
        0x040727, 0x57f000, 0xffff7d, 0x0c1891, 0x002026, 0x56f000, 0xffff7d, 0x213200, 0x0c1880,
        0x005028, 0x57f000, 0xffff7d, 0x211500, 0x0c1891, 0x008018, 0x205a00, 0x205d00, 0x212e00,
        0x014180, 0x0bf080, 0x032325, 0x45f400, 0x5dc000, 0x44f400, 0x000098, 0x0bf080, 0x03227f,
        0x21c500, 0x224700, 0x22a600, 0x21e4b0, 0x200022, 0x210e00, 0x0bf080, 0x032325, 0x03ff84,
        0x03ffc5, 0x21e400, 0x21c500, 0x03ff9f, 0x03ffde, 0x0bf080, 0x032293, 0x043fe7, 0x00000c,
    ];
    for (offset, word) in words.into_iter().enumerate() {
        assert_eq!(
            state.read_memory(MemSpace::P, 0x400213 + offset as u32),
            word
        );
    }
    for (address, word) in [
        (0x402dbe, 0x0bf080),
        (0x402dbf, 0x400213),
        (0x402dc0, 0x567000),
        (0x402dc1, 0x03519f),
        (0x402dc2, 0x577000),
        (0x402dc3, 0x03519e),
    ] {
        assert_eq!(state.read_memory(MemSpace::P, address), word);
    }
    println!(
        "LIMITATION build{build}: PCTL is synthetic MMIO; this executes original arithmetic, not a live clock read"
    );
    println!("Original native constant: {}", native_float(0x5dc000, 0x98));
    let named = [0, 1, 0xc3, 0x2760c3, 0x2760c9, 0x2760cd, 0x2b60c2];
    for pctl in named {
        let (mantissa, exponent) = check(state, jit, pctl);
        println!(
            "PCTL {pctl:06X}: A1 {:06X}, B1 {:06X}, decoded {}",
            state.registers[reg::A1],
            state.registers[reg::B1],
            native_float(state.registers[reg::A1], state.registers[reg::B1])
        );
        state.registers[reg::R7] = 0x500;
        state.write_memory(MemSpace::X, 0x3519d, 0x123456);
        state.write_memory(MemSpace::X, 0x351a0, 0x654321);
        state.pc = 0x402dbe;
        let mut instructions = 0;
        while state.pc != 0x402dc4 {
            state.execute_one(jit);
            instructions += 1;
            assert!(
                instructions < 20_000,
                "Original cache-store slice did not terminate"
            );
        }
        assert_eq!(state.read_memory(MemSpace::X, 0x3519f), mantissa);
        assert_eq!(state.read_memory(MemSpace::X, 0x3519e), exponent);
        assert_eq!(state.read_memory(MemSpace::X, 0x3519d), 0x123456);
        assert_eq!(state.read_memory(MemSpace::X, 0x351a0), 0x654321);
        assert_eq!(state.registers[reg::R7], 0x500);
        assert_eq!(state.read_memory(MemSpace::X, 0xffff7d), pctl);
    }
    let mut count = named.len();
    for feedback in [0, 1, 3, 63, 127, 195, 201, 205, 255] {
        for reference in [0, 7, 11, 31] {
            for output in 0..4 {
                for ignored in [0, 0x2000, 0x2700] {
                    check(
                        state,
                        jit,
                        feedback | (reference << 16) | (output << 14) | ignored,
                    );
                    count += 1;
                }
            }
        }
    }
    println!(
        "PASS {count} original arithmetic cases and {} original cache-store slices; 45 calculation + 6 cache words pinned",
        named.len()
    );
    println!(
        "Formula: 24576000*(F+1)/((R+1)*(OD+1)); DF and PEN not represented. Cache3519F/3519E is ONE native float."
    );
}
