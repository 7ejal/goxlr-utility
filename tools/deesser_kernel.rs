//! Full original DeEsser callback with declared synthetic buffers and original setters/LUTs.
use super::super::{VALUES, execute, sysex};
use super::{native_luts, signed};
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{core::DspState, jit::JitEngine};

const OBJECT: u32 = 0x3200;
const BANK: u32 = 0xb000;
const INPUT: u32 = 0xb200;
const OUTPUT: u32 = 0xb300;
const ENVELOPES: u32 = 0xb400;

fn control(state: &mut DspState, jit: &mut JitEngine, key: u32, value: i32) {
    let descriptor = 0x41a391 + 8 * state.read_memory(MemSpace::X, 0x41abe9 + key);
    let route = state.read_memory(
        MemSpace::X,
        0x427441 + (state.read_memory(MemSpace::X, descriptor + 6) & 0xffff),
    );
    let slot = (route >> 10) & 0xfff;
    let bytes = [21, 14, 7, 0].map(|shift| (value as u32 >> shift) & 127);
    sysex(
        state,
        jit,
        0x35,
        &[&[key >> 7, key & 127][..], &bytes].concat(),
    );
    state.registers[reg::A1] = slot;
    state.registers[reg::B1] = state.read_memory(MemSpace::X, VALUES + slot);
    execute(state, jit, 0x402aa2);
}

fn initialize(state: &mut DspState, jit: &mut JitEngine, controls: [i32; 6], rate: u32) {
    for i in 0..44 {
        state.write_memory(MemSpace::X, OBJECT + i, 0);
    }
    for i in 0..14 {
        state.write_memory(MemSpace::Y, BANK + i, 0);
    }
    for i in 0..6 {
        state.write_memory(MemSpace::X, 0xb100 + i, 0);
        state.write_memory(MemSpace::Y, 0xb100 + i, 0);
    }
    for i in 0..3 {
        state.write_memory(MemSpace::X, ENVELOPES + i, 0);
        state.write_memory(MemSpace::Y, ENVELOPES + i, 0);
    }
    state.write_memory(MemSpace::X, OBJECT + 8, BANK);
    state.write_memory(MemSpace::X, OBJECT + 22, 0xb100);
    state.write_memory(MemSpace::X, OBJECT + 23, 5);
    state.registers[reg::R0] = OBJECT;
    execute(state, jit, 0x406933);
    state.write_memory(MemSpace::X, OBJECT + 7, ENVELOPES);
    state.registers[reg::R0] = OBJECT;
    state.registers[reg::R4] = 0xa200;
    state.registers[reg::R1] = 0xa300;
    execute(state, jit, 0x4069a7);
    for mode in [1, 2] {
        state.registers[reg::R0] = OBJECT;
        state.registers[reg::A1] = mode;
        state.registers[reg::R4] = INPUT;
        state.registers[reg::M4] = 63;
        execute(state, jit, 0x4069ad);
    }
    state.registers[reg::R0] = OBJECT;
    state.registers[reg::R4] = OUTPUT;
    state.registers[reg::M4] = 63;
    execute(state, jit, 0x4069c1);
    state.registers[reg::R0] = OBJECT;
    state.registers[reg::A1] = rate;
    execute(state, jit, 0x4069cc);
    // Cached float pairs normally supplied by the graph descriptor initialization.
    for i in 0..2 {
        state.write_memory(
            MemSpace::X,
            OBJECT + 36 + i,
            state.read_memory(MemSpace::X, 0x41b970 + i),
        );
        state.write_memory(
            MemSpace::X,
            OBJECT + 38 + i,
            state.read_memory(MemSpace::X, 0x41b998 + 18 + i),
        );
    }
    for (offset, value) in controls.into_iter().enumerate() {
        control(state, jit, 0xa + offset as u32, value);
    }
    assert_eq!(state.read_memory(MemSpace::X, OBJECT + 15), 64);
    assert_eq!(state.read_memory(MemSpace::X, OBJECT + 6), 32);
}

fn block(state: &mut DspState, jit: &mut JitEngine, input: &[f64; 64]) -> [f64; 64] {
    for (i, value) in input.iter().enumerate() {
        state.write_memory(
            MemSpace::Y,
            INPUT + i as u32,
            (value * 8388608.).round() as i32 as u32 & 0xffffff,
        );
    }
    state.registers[reg::R0] = OBJECT;
    execute(state, jit, 0xc91);
    std::array::from_fn(|i| signed(state.read_memory(MemSpace::Y, OUTPUT + i as u32)))
}

fn tone(
    state: &mut DspState,
    jit: &mut JitEngine,
    controls: [i32; 6],
    hz: f64,
    frames: usize,
) -> (f64, f64) {
    initialize(state, jit, controls, 48000);
    let mut dry = 0.;
    let mut wet = 0.;
    let mut minimum = 1_f64;
    for start in (0..frames).step_by(64) {
        let input = std::array::from_fn(|i| {
            0.1 * (std::f64::consts::TAU * hz * (start + i) as f64 / 48000.).sin()
        });
        let output = block(state, jit, &input);
        minimum = minimum.min(signed(state.read_memory(MemSpace::Y, BANK + 4)));
        if start >= frames / 2 {
            for i in 0..64 {
                dry += input[i] * input[i];
                wet += output[i] * output[i];
            }
        }
    }
    (10. * (wet / dry).log10(), minimum)
}

fn long_value(state: &mut DspState, address: u32) -> f64 {
    signed(state.read_memory(MemSpace::X, address))
        + state.read_memory(MemSpace::Y, address) as f64 / 140737488355328.
}

fn coefficient_cases(state: &mut DspState, jit: &mut JitEngine) {
    initialize(state, jit, [1, 50, 0, 9, 0, 20], 48000);
    let floor_scale = signed(0x80346e) / 32.;
    for amount in 0..=100 {
        control(state, jit, 0xb, amount);
        let normalized = amount.max(2) as f64 / 100.;
        let slope = 10_f64.powf(-normalized) - 1.;
        let floor_db = (8. + 44. * (normalized - 0.5)).clamp(8., 30.);
        let floor = floor_scale * 10_f64.powf(-floor_db / 20.);
        assert!(
            (signed(state.read_memory(MemSpace::Y, BANK)) - slope).abs() < 0.000001,
            "Native intensity slope amount{amount}: {} expected{slope}",
            signed(state.read_memory(MemSpace::Y, BANK))
        );
        assert!(
            (signed(state.read_memory(MemSpace::Y, BANK + 9)) - floor).abs() < 2. / 8388608.,
            "Native reference-floor coefficient mismatch at amount{amount}"
        );
    }
    println!(
        "PASS 101 native intensity setters: nonlinear ratio and clamped 8..30dB reference floor"
    );
}

fn enable_cases(state: &mut DspState, jit: &mut JitEngine) {
    for mode in 0..3 {
        for amount in [0, 50, 100] {
            let (db, minimum) = tone(state, jit, [0, amount, 0, 9, mode, 20], 8000., 48000);
            assert!(db.abs() < 0.001 && minimum > 0.9999);
            assert!(
                long_value(state, ENVELOPES + 1) > 0.01,
                "Detector stopped while gain reduction disabled"
            );
        }
    }
    let mut zero = None;
    for amount in 0..=2 {
        let response = tone(state, jit, [1, amount, 0, 9, 0, 20], 8000., 48000);
        if let Some(previous) = zero {
            assert_eq!(response, previous);
        } else {
            zero = Some(response);
        }
        assert!(response.0 < -0.4);
    }
    initialize(state, jit, [1, 50, 0, 9, 0, 20], 48000);
    for start in (0..12800).step_by(64) {
        let input = std::array::from_fn(|i| {
            0.1 * (std::f64::consts::TAU * 8000. * (start + i) as f64 / 48000.).cos()
        });
        block(state, jit, &input);
    }
    let old_gain = signed(state.read_memory(MemSpace::Y, BANK + 4));
    assert!(old_gain < 0.6);
    let before: Vec<_> = (0..6)
        .map(|i| state.read_memory(MemSpace::X, 0xb100 + i))
        .collect();
    control(state, jit, 0xa, 0);
    let input = std::array::from_fn(|i| {
        0.1 * (std::f64::consts::TAU * 8000. * (12800 + i) as f64 / 48000.).cos()
    });
    let output = block(state, jit, &input);
    let first = output[0] / input[0];
    let last = output[63] / input[63];
    assert!((first - old_gain).abs() < 0.00002 && last > first + 0.1);
    // Exp(0) is saturated then decremented; the ASR#6 ramp can retain 63 more LSBs.
    assert!(signed(state.read_memory(MemSpace::Y, BANK + 4)) > 1. - 66. / 8388608.);
    assert_ne!(
        before,
        (0..6)
            .map(|i| state.read_memory(MemSpace::X, 0xb100 + i))
            .collect::<Vec<_>>()
    );
    let first_flag = state.read_memory(MemSpace::X, OBJECT + 29);
    assert_eq!(first_flag, 1);
    block(state, jit, &input);
    assert_eq!(state.read_memory(MemSpace::X, OBJECT + 29), 0);
    println!(
        "PASS 9 native enable-off cases and 3 identical amount0/1/2 cases; disable gain ramp {first:.6}->{last:.6}, first-block flag{first_flag} then0; detector continues"
    );
}

fn envelope_cases(state: &mut DspState, jit: &mut JitEngine) {
    for attack in [true, false] {
        for selector in 0..20 {
            let controls = if attack {
                [1, 50, selector, 9, 0, 20]
            } else {
                [1, 50, 0, selector, 0, 20]
            };
            initialize(state, jit, controls, 48000);
            let (input, mut expected, pole) = if attack {
                (
                    0.25_f64,
                    0_f64,
                    signed(state.read_memory(MemSpace::Y, BANK + 6)),
                )
            } else {
                state.write_memory(MemSpace::X, ENVELOPES, 0x400000);
                (0., 0.5, signed(state.read_memory(MemSpace::Y, BANK + 7)))
            };
            let floor = signed(state.read_memory(MemSpace::Y, BANK + 8));
            for _ in 0..64 {
                let difference = ((expected - input) * 8388608.).floor() / 8388608.;
                expected = (input + pole * difference).max(floor);
            }
            block(state, jit, &[input; 64]);
            assert!(
                (long_value(state, ENVELOPES) - expected).abs() < 0.000002,
                "DeEsser envelope mismatch attack{attack} selector{selector}: {} expected{expected}",
                long_value(state, ENVELOPES)
            );
        }
    }
    println!(
        "PASS 40 full native DeEsser callback envelope attack/release recurrences, including floor"
    );
}

fn rms_cases(state: &mut DspState, jit: &mut JitEngine) {
    for input in [0.01, 0.1, 0.25, 0.5] {
        initialize(state, jit, [0, 50, 0, 9, 0, 20], 48000);
        let pole = signed(state.read_memory(MemSpace::X, OBJECT + 9));
        let alpha = signed(state.read_memory(MemSpace::X, OBJECT + 10));
        for _ in 0..4 {
            let mut energy = signed(state.read_memory(MemSpace::X, OBJECT + 11));
            for _ in 0..32 {
                energy = input * input * alpha + energy * pole;
            }
            block(state, jit, &[input; 64]);
            let actual = signed(state.read_memory(MemSpace::X, OBJECT + 11));
            assert!((actual - energy).abs() < 32. / 8388608.);
            let rms = signed(state.read_memory(MemSpace::X, OBJECT + 25));
            assert!(
                (rms - actual.sqrt()).abs() < 0.0002,
                "Native RMS estimate mismatch input{input}: {rms} vs{}",
                actual.sqrt()
            );
        }
    }
    println!("PASS 16 full native 32-sample energy prepass and original log/exp RMS cases");
}

fn gain_target_cases(state: &mut DspState, jit: &mut JitEngine) {
    let mut maximum_error = 0_f64;
    for amount in [2, 50, 100] {
        for mode in 0..3 {
            for weight in [0, 20, 100] {
                for hz in [1000., 8000.] {
                    initialize(state, jit, [1, amount, 0, 9, mode, weight], 48000);
                    let slope = signed(state.read_memory(MemSpace::Y, BANK));
                    let new = signed(state.read_memory(MemSpace::Y, BANK + 2));
                    let old = signed(state.read_memory(MemSpace::Y, BANK + 3));
                    for start in (0..8192).step_by(64) {
                        let hp = long_value(state, ENVELOPES + 1);
                        let reference = long_value(state, ENVELOPES + 2);
                        let old_gain = signed(state.read_memory(MemSpace::X, OBJECT + 14));
                        let input = std::array::from_fn(|i| {
                            0.1 * (std::f64::consts::TAU * hz * (start + i) as f64 / 48000.).sin()
                        });
                        block(state, jit, &input);
                        if start > 0 {
                            let expected =
                                (reference / hp).powf(-slope).min(1.) * new + old_gain * old;
                            let actual = signed(state.read_memory(MemSpace::X, OBJECT + 14));
                            let error = (actual - expected).abs();
                            maximum_error = maximum_error.max(error);
                            assert!(
                                error < 0.0002,
                                "DeEsser block gain mismatch amount{amount} mode{mode} weight{weight} Hz{hz}: {actual} vs{expected}"
                            );
                        }
                    }
                }
            }
        }
    }
    println!(
        "PASS 6858 full native block gain targets from reference/HP envelope ratio; max linear error{maximum_error:.8}"
    );
}

fn reference_cases(state: &mut DspState, jit: &mut JitEngine) {
    let mut samples = 0;
    let mut weighted = 0;
    let mut clamped = 0;
    let mut reduced = 0;
    for amount in [2, 50, 100] {
        for mode in 0..3 {
            for weight in [0, 20, 100] {
                initialize(state, jit, [1, amount, 0, 9, mode, weight], 48000);
                for start in (0..4096).step_by(64) {
                    let input = std::array::from_fn(|i| {
                        0.1 * (std::f64::consts::TAU * 8000. * (start + i) as f64 / 48000.).cos()
                    });
                    block(state, jit, &input);
                }
                let input: [f64; 64] = std::array::from_fn(|i| {
                    0.05 * (std::f64::consts::TAU * 1000. * i as f64 / 48000.).cos()
                        + 0.05 * (std::f64::consts::TAU * 8000. * i as f64 / 48000.).cos()
                });
                for (i, value) in input.iter().enumerate() {
                    state.write_memory(
                        MemSpace::Y,
                        INPUT + i as u32,
                        (value * 8388608.).round() as i32 as u32 & 0xffffff,
                    );
                }
                let retention = signed(state.read_memory(MemSpace::Y, BANK + 10));
                let reference_floor = signed(state.read_memory(MemSpace::Y, BANK + 11));
                let floor_coefficient = signed(state.read_memory(MemSpace::Y, BANK + 9));
                let weight = signed(state.read_memory(MemSpace::Y, BANK + 13));
                state.registers[reg::R0] = OBJECT;
                state.registers[reg::R7] = 0x500;
                state.stack_push(0x1f, 0);
                state.pc = 0xc91;
                let mut old_reference = 0.;
                let mut intermediate = 0.;
                let mut gains = Vec::new();
                let mut instructions = 0;
                while state.pc != 0x1f {
                    match state.pc {
                        0xd0e => old_reference = long_value(state, ENVELOPES + 2),
                        0xd24 => {
                            let rms = signed(state.read_memory(MemSpace::X, OBJECT + 25));
                            let target = -32. * floor_coefficient * rms;
                            let difference =
                                ((old_reference - target) * 8388608.).floor() / 8388608.;
                            let expected = (target + retention * difference).max(reference_floor);
                            intermediate = long_value(state, ENVELOPES + 2);
                            assert!(
                                (intermediate - expected).abs() < 0.000001,
                                "Native reference retention mismatch amount{amount} mode{mode}: {intermediate} vs{expected}"
                            );
                        }
                        0xd4b => {
                            let hp = long_value(state, ENVELOPES + 1);
                            let hp_high = signed(state.read_memory(MemSpace::X, ENVELOPES + 1));
                            let full_high = signed(state.read_memory(MemSpace::X, ENVELOPES));
                            let expected = if hp > intermediate {
                                intermediate.max(full_high * weight)
                            } else {
                                intermediate
                            };
                            weighted += usize::from(expected > intermediate);
                            clamped += usize::from(expected > hp_high);
                            let expected = expected.min(hp_high);
                            let actual = long_value(state, ENVELOPES + 2);
                            assert!(
                                (actual - expected).abs() < 0.000001,
                                "Native reference weight/clamp mismatch amount{amount} mode{mode}: {actual} vs{expected}"
                            );
                            gains.push(signed(state.read_memory(MemSpace::Y, BANK + 4)));
                            samples += 1;
                        }
                        _ => {}
                    }
                    state.execute_one(jit);
                    instructions += 1;
                    assert!(
                        instructions < 12000,
                        "Full native DeEsser observer did not terminate"
                    );
                }
                assert_eq!(gains.len(), 64);
                reduced += gains.iter().filter(|gain| **gain < 0.99999).count();
                for (i, gain) in gains.iter().enumerate() {
                    let dry = signed(state.read_memory(MemSpace::Y, INPUT + i as u32));
                    let wet = signed(state.read_memory(MemSpace::Y, OUTPUT + i as u32));
                    assert!(
                        (wet - dry * gain).abs() < 2. / 8388608.,
                        "Full callback is not wideband multiplication"
                    );
                }
                let threshold = signed(state.read_memory(MemSpace::X, OBJECT + 27));
                let flag = u32::from(gains[63] < threshold);
                assert_eq!(state.read_memory(MemSpace::X, OBJECT + 29), flag);
                let next_gain = signed(state.read_memory(MemSpace::Y, BANK + 4));
                let step = signed(state.read_memory(MemSpace::Y, BANK + 5));
                assert!((next_gain - gains[63] - step).abs() < 2. / 8388608.);
            }
        }
    }
    assert!(
        weighted > 0 && clamped > 0 && reduced > 0,
        "Reference branches or reduced output were not exercised"
    );
    println!(
        "PASS {samples} native sample reference recurrences and wideband multiplies ({reduced} reduced); {weighted} weight floors, {clamped} HP clamps, 27 active-flag checks"
    );
}

pub fn run(state: &mut DspState, jit: &mut JitEngine, build: u32) {
    assert_eq!(state.read_memory(MemSpace::P, 0xc91), 0x040046);
    assert_eq!(state.read_memory(MemSpace::P, 0xd4d), 0x2053f8);
    native_luts(state, jit);
    enable_cases(state, jit);
    coefficient_cases(state, jit);
    envelope_cases(state, jit);
    rms_cases(state, jit);
    gain_target_cases(state, jit);
    reference_cases(state, jit);
    let mut previous = [0_f64; 4];
    for amount in [0, 2, 20, 50, 100] {
        for (index, hz) in [1000., 4000., 8000., 12000.].into_iter().enumerate() {
            let (db, min) = tone(state, jit, [1, amount, 0, 9, 0, 20], hz, 48000);
            assert!(db.is_finite() && min > 0. && min <= 1. && db <= 0.001);
            if index == 0 {
                assert!(db.abs() < 0.001);
            } else {
                assert!(db < -0.3);
                if amount > 2 {
                    assert!(db < previous[index]);
                }
            }
            if amount == 2 {
                assert_eq!(db, previous[index]);
            }
            previous[index] = db;
            println!(
                "full native DeEsser build{build} amount{amount} {hz}Hz: last_half{db:.6}dB minimum_gain{min:.6} flag{}",
                state.read_memory(MemSpace::X, OBJECT + 29)
            );
        }
    }
    for enabled in [0, 1] {
        for mode in 0..3 {
            let mut previous = None;
            for weight in [0, 20, 50, 100] {
                let (db, min) = tone(state, jit, [enabled, 50, 0, 9, mode, weight], 8000., 48000);
                assert!(db.is_finite() && min > 0. && min <= 1.);
                if enabled == 0 {
                    assert!(db.abs() < 0.001);
                } else {
                    assert!(db < -0.1);
                    if let Some(previous) = previous {
                        assert!(db >= previous - 0.001);
                    }
                }
                previous = Some(db);
            }
        }
    }
    println!(
        "PASS 44 full native one-second PCM tone cases: intensity, spectral selectivity, mode/weight and disabled gain"
    );
}
