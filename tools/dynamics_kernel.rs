//! Serialized native dynamics experiments. Synthetic allocation/rings, no USB or physical timing.
use super::execute;
use dsp56300_core::{MemSpace, reg};
use dsp56300_emu::{core::DspState, jit::JitEngine};

#[path = "deesser_kernel.rs"]
mod deesser_kernel;

fn signed(word: u32) -> f64 {
    ((word << 8) as i32 >> 8) as f64 / 8388608.
}

fn set(state: &mut DspState, jit: &mut JitEngine, slot: u32, value: i32) {
    state.registers[reg::A1] = slot;
    state.registers[reg::B1] = value as u32 & 0xffffff;
    execute(state, jit, 0x402aa2);
}

fn detector_response(
    state: &mut DspState,
    jit: &mut JitEngine,
    coefficient_base: u32,
    sample_rate: f64,
    hz: f64,
) -> f64 {
    let history = 0x9000;
    let input = 0x9100;
    for i in 0..6 {
        state.write_memory(MemSpace::X, history + i, 0);
        state.write_memory(MemSpace::Y, history + i, 0);
    }
    state.registers[reg::M1] = 5;
    state.registers[reg::N1] = 0xfffffe;
    state.registers[reg::M5] = 9;
    state.registers[reg::R1] = history;
    state.registers[reg::R5] = coefficient_base;
    state.registers[reg::R0] = input;
    let mut dry = 0.;
    let mut wet = 0.;
    let window = sample_rate as usize / 10;
    for sample in 0..2 * window {
        let value = 0.5 * (std::f64::consts::TAU * hz * sample as f64 / sample_rate).sin();
        let raw = (value * 8388608.).round() as i32 as u32 & 0xffffff;
        state.write_memory(MemSpace::Y, input, raw);
        state.pc = 0x000d24;
        let mut instructions = 0;
        while state.pc != 0x000d37 {
            state.execute_one(jit);
            instructions += 1;
            assert!(instructions < 32, "Native detector slice did not terminate");
        }
        assert_eq!(state.registers[reg::R1], history);
        assert_eq!(state.registers[reg::R5], coefficient_base);
        if sample >= window {
            dry += signed(raw).powi(2);
            wet += signed(state.registers[reg::A1]).powi(2);
        }
    }
    state.registers[reg::M1] = 0xffffff;
    state.registers[reg::M5] = 0xffffff;
    10. * (wet / dry).log10()
}

fn detector_model(state: &mut DspState, base: u32, sample_rate: f64, hz: f64) -> f64 {
    let c: Vec<_> = (0..10)
        .map(|i| signed(state.read_memory(MemSpace::Y, base + i)))
        .collect();
    let phase = std::f64::consts::TAU * hz / sample_rate;
    let magnitude = |v: [f64; 3]| {
        let real = v[0] + v[1] * phase.cos() + v[2] * (2. * phase).cos();
        let imag = -v[1] * phase.sin() - v[2] * (2. * phase).sin();
        real * real + imag * imag
    };
    10. * (16. * magnitude([c[2], c[0], c[1]]) * magnitude([c[7], c[5], c[6]])
        / (magnitude([1., 2. * c[3], 2. * c[4]]) * magnitude([1., 2. * c[8], 2. * c[9]])))
    .log10()
}

fn wideband_output(state: &mut DspState, jit: &mut JitEngine) {
    for input in [-0.75, -0.1, 0., 0.1, 0.75] {
        for gain in [0.25, 0.5, 0.75] {
            state.registers[reg::R0] = 0x9100;
            state.registers[reg::R2] = 0x9200;
            state.registers[reg::R3] = 0x9300;
            state.registers[reg::R4] = 0x9400;
            state.write_memory(
                MemSpace::Y,
                0x9100,
                (input * 8388608_f64).round() as i32 as u32 & 0xffffff,
            );
            state.write_memory(MemSpace::Y, 0x9400, (gain * 8388608_f64).round() as u32);
            state.write_memory(MemSpace::Y, 0x93ff, 0);
            state.pc = 0x000d4b;
            while state.pc != 0x000d52 {
                state.execute_one(jit);
            }
            let output = signed(state.read_memory(MemSpace::Y, 0x9200));
            assert!((output - input * gain).abs() < 0.000001);
        }
    }
    println!("PASS 15 native DeEsser output slices multiply unfiltered input by one wideband gain");
}

fn native_luts(state: &mut DspState, jit: &mut JitEngine) {
    assert_eq!(state.read_memory(MemSpace::P, 0x407d29), 0x016185);
    state.registers[reg::R0] = 0xa000;
    state.registers[reg::R7] = 0x500;
    state.write_memory(MemSpace::X, 0xa019, 0xa100);
    state.write_memory(MemSpace::X, 0xa11c, 1);
    state.write_memory(MemSpace::X, 0xa008, 0xa200);
    state.write_memory(MemSpace::X, 0xa009, 0xa300);
    state.stack_push(0x1f, 0);
    state.pc = 0x407cc3;
    let mut count = 0;
    while state.pc != 0x407d2b {
        state.execute_one(jit);
        count += 1;
        assert!(count < 100000, "Native LUT generator did not terminate");
    }
    println!("native generator {count} instructions");
    for (base, name) in [(0xa200, "log"), (0xa300, "exp")] {
        println!(
            "native {name} LUT {base:X}: {:?}",
            (0..33)
                .map(|i| state.read_memory(MemSpace::X, base + i))
                .collect::<Vec<_>>()
        );
    }
    for i in 0..=32 {
        let log = state.read_memory(MemSpace::X, 0xa200 + i) as f64;
        let exp = state.read_memory(MemSpace::X, 0xa300 + i) as f64;
        assert!((log - ((i as f64 + 32.) / 32.).log2() * 131072.).abs() < 1.1);
        assert!((exp - 2_f64.powf(i as f64 / 32. - 1.) * 8388608.).abs() < 2.1);
    }
    println!("PASS 66 original-generated log/antilog LUT entries; no replacement tables injected");
}

fn initialize_compressor(state: &mut DspState, jit: &mut JitEngine) {
    let object = 0x3800;
    let core = 0x3880;
    // Reinitialize one fixture instance, not an ever-growing synthetic allocation count.
    state.write_memory(MemSpace::X, 0x41bd6f, 0);
    state.registers[reg::R0] = object;
    execute(state, jit, 0x4097da);
    state.write_memory(MemSpace::X, core + 4, 0x9500);
    state.write_memory(MemSpace::X, 0x9500, 0);
    state.write_memory(MemSpace::Y, 0x9500, 0);
    state.write_memory(MemSpace::X, core + 9, 0x7fffff);
    for (offset, value) in [(10, 8), (11, 0x9600), (12, 7), (13, 0x9700), (14, 7)] {
        state.write_memory(MemSpace::X, core + offset, value);
    }
    state.registers[reg::R0] = object;
    state.registers[reg::R4] = 0xa200;
    state.registers[reg::R1] = 0xa300;
    execute(state, jit, 0x409820);
    state.registers[reg::R0] = object;
    state.registers[reg::A1] = 0;
    execute(state, jit, 0x409830);
    for (offset, value) in [(5, 0x9800), (6, 63), (7, 0x9900), (8, 63)] {
        state.write_memory(MemSpace::X, object + offset, value);
    }
}

fn compressor_inner(state: &mut DspState, jit: &mut JitEngine, input: f64) -> [f64; 8] {
    let raw = (input * 8388608.).round() as i32 as u32 & 0xffffff;
    for i in 0..8 {
        state.write_memory(MemSpace::Y, 0x9600 + i, raw);
    }
    state.registers[reg::R0] = 0x3880;
    execute(state, jit, 0x441d);
    std::array::from_fn(|i| signed(state.read_memory(MemSpace::Y, 0x9700 + i as u32)))
}

fn compressor_callback(state: &mut DspState, jit: &mut JitEngine, input: &[f64; 64]) -> [f64; 64] {
    for (i, value) in input.iter().enumerate() {
        let raw = (value * 8388608.).round() as i32 as u32 & 0xffffff;
        state.write_memory(MemSpace::Y, 0x9800 + i as u32, raw);
    }
    state.registers[reg::R0] = 0x3800;
    execute(state, jit, 0x4473);
    std::array::from_fn(|i| signed(state.read_memory(MemSpace::Y, 0x9900 + i as u32)))
}

fn gain_ramp_residual(state: &mut DspState, jit: &mut JitEngine, input: f64) {
    let raw = (input * 8388608.).round() as i32 as u32 & 0xffffff;
    for i in 0..8 {
        state.write_memory(MemSpace::Y, 0x9600 + i, raw);
    }
    state.registers[reg::R0] = 0x3880;
    state.registers[reg::R7] = 0x500;
    state.stack_push(0x1f, 0);
    state.pc = 0x441d;
    let mut instructions = 0;
    while state.pc != 0x4452 {
        state.execute_one(jit);
        instructions += 1;
        assert!(
            instructions < 128,
            "Native gain target slice did not terminate"
        );
    }
    let target = state.registers[reg::A1] as i32;
    state.run(jit, 2000);
    assert_eq!(state.pc, 0x1f);
    let current = state.read_memory(MemSpace::Y, 0x3905) as i32;
    assert!(
        (0..8).contains(&(target - current)),
        "Native steady gain ramp residual exceeds 7LSB"
    );
}

fn compressor_output(
    state: &mut DspState,
    jit: &mut JitEngine,
    ratio: i32,
    threshold: i32,
    makeup: i32,
    input: f64,
    automatic: bool,
) -> f64 {
    initialize_compressor(state, jit);
    state.registers[reg::R0] = 0x3800;
    state.registers[reg::A1] = u32::from(automatic);
    execute(state, jit, 0x409830);
    for (slot, value) in [
        (0xa3, makeup),
        (0x9f, ratio),
        (0xa0, threshold),
        (0xa1, 0),
        (0xa2, 0),
    ] {
        set(state, jit, slot, value);
    }
    let mut output = 0.;
    for block in 0..64 {
        let samples = compressor_inner(state, jit, input);
        if block >= 48 {
            output += samples.iter().sum::<f64>();
        }
    }
    output / 128.
}

fn compressor_curves(state: &mut DspState, jit: &mut JitEngine) {
    let mut cases = 0;
    let mut maximum_error = 0_f64;
    for ratio in 0..15 {
        for level in [
            -60., -40., -30., -26.2, -26.05, -26., -25.95, -25.8, -24., -20., -16., -10., -1.,
        ] {
            let input = 10_f64.powf(level / 20.);
            let output = compressor_output(state, jit, ratio, -20, 0, input, false);
            let log = input.log2() / 64.;
            let threshold = signed(state.read_memory(MemSpace::Y, 0x3900));
            let slope = signed(state.read_memory(MemSpace::Y, 0x3901));
            let makeup = signed(state.read_memory(MemSpace::Y, 0x3902));
            let log_gain = ((log - threshold).max(0.) * slope + makeup).min(0.);
            let expected = input * 32. * 2_f64.powf(64. * log_gain);
            let error = (20. * (output / expected).abs().log10()).abs();
            maximum_error = maximum_error.max(error);
            assert!(
                error < 0.02,
                "Native compressor curve mismatch R{ratio} L{level}: {error}dB"
            );
            if [0, 9, 14].contains(&ratio) {
                println!(
                    "native compressor ratio_raw{ratio}, threshold-20, input{level}dB: output{:.6}dB gain{:.6}dB",
                    20. * output.abs().log10(),
                    20. * (output / input).abs().log10()
                );
            }
            cases += 1;
        }
    }
    for makeup in [6, 12, 24] {
        let output = compressor_output(state, jit, 0, -20, makeup, 0.001, false);
        let baseline = compressor_output(state, jit, 0, -20, 0, 0.001, false);
        assert!((20. * (output / baseline).log10() - makeup as f64).abs() < 0.01);
        cases += 1;
    }
    for level in [-40., -20., -10.] {
        let input = 10_f64.powf(level / 20.);
        let positive = compressor_output(state, jit, 9, -20, 0, input, false);
        let negative = compressor_output(state, jit, 9, -20, 0, -input, false);
        assert!((positive + negative).abs() < 0.000001);
        cases += 1;
    }
    for ratio in 0..15 {
        for threshold in [-50, -40, -10, 0] {
            for level in [-60., -50., -40., -30., -20., -10., -1.] {
                let input = 10_f64.powf(level / 20.);
                let output = compressor_output(state, jit, ratio, threshold, 0, input, false);
                let t = signed(state.read_memory(MemSpace::Y, 0x3900));
                let slope = signed(state.read_memory(MemSpace::Y, 0x3901));
                let makeup = signed(state.read_memory(MemSpace::Y, 0x3902));
                let g = ((input.log2() / 64. - t).max(0.) * slope + makeup).min(0.);
                let expected = input * 32. * 2_f64.powf(64. * g);
                let error = (20. * (output / expected).abs().log10()).abs();
                maximum_error = maximum_error.max(error);
                // ASR#3 makes the 8-sample ramp retain up to 7 Q23 gain LSBs.
                let bound = 0.005 + 20. * (1. + 8. / 8388608. / 2_f64.powf(64. * g)).log10();
                assert!(
                    error < bound,
                    "Extended curve mismatch R{ratio} T{threshold} L{level}: {error}dB >{bound}dB"
                );
                cases += 1;
            }
            let input = 0.001;
            let output = compressor_output(state, jit, ratio, threshold, 12, input, true);
            let t = signed(state.read_memory(MemSpace::Y, 0x3900));
            let slope = signed(state.read_memory(MemSpace::Y, 0x3901));
            let makeup = signed(state.read_memory(MemSpace::Y, 0x3902));
            let g = ((input.log2() / 64. - t).max(0.) * slope + makeup).min(0.);
            let expected = input * 32. * 2_f64.powf(64. * g);
            let error = (20. * (output / expected).abs().log10()).abs();
            maximum_error = maximum_error.max(error);
            let bound = 0.005 + 20. * (1. + 8. / 8388608. / 2_f64.powf(64. * g)).log10();
            assert!(
                error < bound,
                "Automatic makeup PCM mismatch R{ratio} T{threshold}: {error}dB"
            );
            cases += 1;
        }
    }
    for ratio in 0..15 {
        for threshold in [-50, -20] {
            compressor_output(state, jit, ratio, threshold, 0, 0.891, false);
            gain_ramp_residual(state, jit, 0.891);
        }
    }
    println!(
        "PASS 30 native steady gain ramp targets retain 0..7 Q23 LSBs after ASR#3 interpolation"
    );
    println!(
        "PASS {cases} native compressor PCM curve/makeup/polarity cases; maxcurveerror{maximum_error:.6}dB"
    );
}

fn compressor_timing(state: &mut DspState, jit: &mut JitEngine) {
    for attack in 0..20 {
        initialize_compressor(state, jit);
        set(state, jit, 0x9f, 9);
        set(state, jit, 0xa0, -20);
        set(state, jit, 0xa1, attack);
        state.write_memory(MemSpace::X, 0x9500, 0x200000);
        state.write_memory(MemSpace::Y, 0x9500, 0);
        let alpha = signed(state.read_memory(MemSpace::Y, 0x3903));
        let pole = signed(state.read_memory(MemSpace::Y, 0x3904));
        let threshold = signed(state.read_memory(MemSpace::Y, 0x3900));
        let slope = signed(state.read_memory(MemSpace::Y, 0x3901));
        let makeup = signed(state.read_memory(MemSpace::Y, 0x3902));
        let target = ((-2. / 64. - threshold).max(0.) * slope + makeup).min(0.);
        for _ in 0..8 {
            let old = signed(state.read_memory(MemSpace::X, 0x3887));
            compressor_inner(state, jit, 0.25);
            let actual = signed(state.read_memory(MemSpace::X, 0x3887));
            assert!(
                (actual - (old * pole + target * alpha)).abs() < 2. / 8388608.,
                "Native log-gain recurrence mismatch at attack selector{attack}"
            );
        }
    }
    println!(
        "PASS 160 native subblock log-gain attack recurrences; eight updates per 64-frame wrapper"
    );
    for control in [0xa1, 0xa2] {
        for selector in 0..20 {
            initialize_compressor(state, jit);
            set(state, jit, 0x9f, 9);
            set(state, jit, 0xa0, -20);
            set(state, jit, control, selector);
            let (input, initial, alpha) = if control == 0xa1 {
                (0.25, 0., signed(state.read_memory(MemSpace::Y, 0x3907)))
            } else {
                state.write_memory(MemSpace::X, 0x9500, 0x400000);
                state.write_memory(MemSpace::Y, 0x9500, 0);
                (0., 0.5, signed(state.read_memory(MemSpace::Y, 0x3908)))
            };
            compressor_callback(state, jit, &[input; 64]);
            let actual = signed(state.read_memory(MemSpace::X, 0x9500))
                + state.read_memory(MemSpace::Y, 0x9500) as f64 / 140737488355328.;
            let expected = input + (initial - input) * (1. - alpha).powi(64);
            assert!(
                (actual - expected).abs() < 0.000002,
                "Envelope mismatch control{control:X} selector{selector}: actual{actual} expected{expected}"
            );
        }
    }
    println!("PASS 40 native 64-frame callback envelope attack/release recurrences");
    for input in [0., 0.125, -0.25, 0.5] {
        initialize_compressor(state, jit);
        set(state, jit, 0xa1, 0);
        set(state, jit, 0xa2, 0);
        set(state, jit, 0x9f, 9);
        set(state, jit, 0xa0, -20);
        let callback = compressor_callback(state, jit, &[input; 64]);
        let envelope = state.read_memory(MemSpace::X, 0x9500);
        let gain = state.read_memory(MemSpace::X, 0x3887);
        initialize_compressor(state, jit);
        set(state, jit, 0xa1, 0);
        set(state, jit, 0xa2, 0);
        set(state, jit, 0x9f, 9);
        set(state, jit, 0xa0, -20);
        let mut separate = Vec::new();
        for _ in 0..8 {
            separate.extend(compressor_inner(state, jit, input));
        }
        assert_eq!(callback.as_slice(), separate.as_slice());
        assert_eq!(envelope, state.read_memory(MemSpace::X, 0x9500));
        assert_eq!(gain, state.read_memory(MemSpace::X, 0x3887));
    }
    println!("PASS 4 actual wrapper fixtures equal eight native 8-frame kernel calls");
    for index in [0, 7, 8, 31, 63] {
        initialize_compressor(state, jit);
        set(state, jit, 0xa1, 0);
        set(state, jit, 0xa2, 0);
        let mut input = [0.; 64];
        input[index] = 0.125;
        let output = compressor_callback(state, jit, &input);
        for (i, value) in output.into_iter().enumerate() {
            if i == index {
                assert!(value > 0.);
            } else {
                assert_eq!(value, 0.);
            }
        }
    }
    println!(
        "PASS 5 native impulse positions have no sample delay or pre-echo within this callback fixture"
    );
}

fn automatic_makeup_activation(state: &mut DspState, jit: &mut JitEngine) {
    initialize_compressor(state, jit);
    set(state, jit, 0xa3, 12);
    set(state, jit, 0x9f, 9);
    set(state, jit, 0xa0, -20);
    let manual = state.read_memory(MemSpace::Y, 0x3902);
    state.registers[reg::R0] = 0x3800;
    state.registers[reg::A1] = 1;
    execute(state, jit, 0x409830);
    assert_eq!(state.read_memory(MemSpace::Y, 0x3902), manual);
    set(state, jit, 0xa0, -20);
    let automatic = state.read_memory(MemSpace::Y, 0x3902);
    assert_ne!(automatic, manual);
    set(state, jit, 0xa3, 6);
    assert_ne!(state.read_memory(MemSpace::Y, 0x3902), automatic);
    assert_eq!(state.read_memory(MemSpace::X, 0x3814), 1);
    set(state, jit, 0x9f, 9);
    assert_eq!(state.read_memory(MemSpace::Y, 0x3902), automatic);
    state.registers[reg::R0] = 0x3800;
    state.registers[reg::A1] = 0;
    execute(state, jit, 0x409830);
    assert_eq!(state.read_memory(MemSpace::Y, 0x3902), automatic);
    println!(
        "PASS private auto flag needs threshold/ratio refresh; manual makeup overrides until refresh"
    );
}

pub fn run(state: &mut DspState, jit: &mut JitEngine, build: u32) {
    if std::env::args().any(|arg| arg == "--deesser") {
        deesser_kernel::run(state, jit, build);
        return;
    }
    assert_eq!(state.read_memory(MemSpace::P, 0x441d), 0x040046);
    assert_eq!(state.read_memory(MemSpace::P, 0x4459), 0x0c1c06);
    assert_eq!(state.read_memory(MemSpace::P, 0x4473), 0x055f3c);
    assert_eq!(state.read_memory(MemSpace::P, 0x4499), 0x060890);
    assert_eq!(state.read_memory(MemSpace::P, 0x44c4), 0x01488d);
    assert_eq!(state.read_memory(MemSpace::P, 0x409830), 0x02508c);
    assert_eq!(state.read_memory(MemSpace::P, 0x03440d), 0x02539f);
    assert_eq!(state.read_memory(MemSpace::P, 0x034412), 0x05a44d);
    let object = 0x3800;
    let bank = 0x3900;
    let scale = 0x550a96 as f64 / 8388608. / 256.;
    let manual_offset = 0x780a3e as f64 / 8388608. * 32.;
    assert_eq!(state.read_memory(MemSpace::P, 0x4069d4), 0x00ac44);
    for (rate, bank) in [(44100, 0x40), (48000, 0xd0)] {
        state.registers[reg::R0] = 0x3200;
        state.registers[reg::A1] = rate;
        execute(state, jit, 0x4069cc);
        assert_eq!(state.read_memory(MemSpace::X, 0x3214), bank);
        assert_eq!(state.read_memory(MemSpace::X, 0x3215), 9);
    }
    for address in [0x40, 0xd0] {
        println!(
            "native boot Y:{address:06X}: {:?}",
            (0..10)
                .map(|i| state.read_memory(MemSpace::Y, address + i))
                .collect::<Vec<_>>()
        );
    }
    for (base, rate) in [(0xd0, 48000.), (0x40, 44100.)] {
        for hz in [500., 1000., 2000., 4000., 8000., 12000., 18000.] {
            let db = detector_response(state, jit, base, rate, hz);
            let ratio = (std::f64::consts::PI * 4000. / rate).tan()
                / (std::f64::consts::PI * hz / rate).tan();
            let expected = -10. * (1. + ratio.powi(6)).log10();
            let model = detector_model(state, base, rate, hz);
            println!(
                "native detector bank Y:{base:04X} {rate}Hz, input {hz}Hz: {db:.6}dB, exactcoeff {model:.6}dB, Butterworth3HP {expected:.6}dB"
            );
            assert!(
                (db - expected).abs() < 0.05,
                "Detector differs from3rd-order4kHzButterworth HP"
            );
            assert!(
                (db - model).abs() < 0.001,
                "Native detector differs from exactcoefficient transfer"
            );
        }
    }
    wideband_output(state, jit);
    native_luts(state, jit);
    println!("build,ratio_raw,threshold_db,auto_flag,gain_coefficient,equivalent_manual_db");
    for ratio in 0..15 {
        let exponent = state.read_memory(MemSpace::X, 0x41bd70 + 2 * ratio);
        let mantissa = state.read_memory(MemSpace::X, 0x41bd71 + 2 * ratio);
        let decoded_ratio = signed(mantissa) * 2_f64.powi(exponent as i32 - 127);
        for threshold in -50..=0 {
            for auto in [0, 1] {
                state.registers[reg::R0] = object;
                state.registers[reg::A1] = auto;
                execute(state, jit, 0x409830);
                assert_eq!(state.read_memory(MemSpace::X, object + 20), auto);
                set(state, jit, 0xa3, 12);
                let before = state.read_memory(MemSpace::Y, bank + 2);
                set(state, jit, 0x9f, ratio as i32);
                set(state, jit, 0xa0, threshold);
                let actual = state.read_memory(MemSpace::Y, bank + 2);
                if auto == 0 {
                    assert_eq!(actual, before, "Manual makeup changed with threshold/ratio");
                } else {
                    let automatic_db = if ratio == 0 || threshold == 0 {
                        0.
                    } else {
                        (0.75 * (6. - threshold as f64) * (1. - 1. / decoded_ratio)).min(30.) - 6.
                    };
                    assert!(
                        (signed(actual) - (automatic_db - manual_offset) * scale).abs() < 0.000001,
                        "Automatic makeup mismatch ratio {decoded_ratio} threshold {threshold}"
                    );
                }
                let effective_ratio = if threshold == 0 { 1. } else { decoded_ratio };
                assert!(
                    (signed(state.read_memory(MemSpace::Y, bank))
                        - (threshold as f64 - 6.) * scale)
                        .abs()
                        < 0.000001
                );
                assert!(
                    (signed(state.read_memory(MemSpace::Y, bank + 1))
                        - (1. / effective_ratio - 1.))
                        .abs()
                        < 0.000001
                );
                let coefficient = signed(actual);
                if [0, 1, 6, 9, 14].contains(&ratio) && [0, -10, -20, -40, -50].contains(&threshold)
                {
                    println!(
                        "{build},{ratio},{threshold},{auto},{coefficient:.9},{:.5}",
                        coefficient / scale + manual_offset
                    );
                }
            }
        }
    }
    for (attack, ms) in [
        0.001, 2., 3., 4., 5., 6., 7., 8., 9., 10., 12., 14., 16., 18., 20., 23., 26., 30., 35.,
        40.,
    ]
    .into_iter()
    .enumerate()
    {
        set(state, jit, 0xa1, attack as i32);
        let pole = (-64_f64 / (48000. * ms / 1000.)).exp();
        assert!((signed(state.read_memory(MemSpace::Y, bank + 3)) - (1. - pole)).abs() < 0.000001);
        assert!((signed(state.read_memory(MemSpace::Y, bank + 4)) - pole).abs() < 0.000001);
    }
    println!("PASS 20 native block-gain smoothing coefficients use64-sample attack pole");
    compressor_curves(state, jit);
    compressor_timing(state, jit);
    automatic_makeup_activation(state, jit);
    println!(
        "PASS serialized dynamics: auto1530, LUT66, detector14, wideband15, block-poles20, PCMcurves681, ramp30, gain-updates160, envelope40, wrapper4, impulses5, activation1; no physical processing claimed"
    );
}
