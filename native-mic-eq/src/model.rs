use goxlr_types::EqFrequencies::{self, *};

pub const STOCK: [EqFrequencies; 10] = [
    Equalizer31Hz,
    Equalizer63Hz,
    Equalizer125Hz,
    Equalizer250Hz,
    Equalizer500Hz,
    Equalizer1KHz,
    Equalizer2KHz,
    Equalizer4KHz,
    Equalizer8KHz,
    Equalizer16KHz,
];
pub const LABELS: [&str; 12] = [
    "31 Hz", "63 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz",
    "Extra 11", "Extra 12",
];
pub type Bands = [Band; 12];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub hz: f32,
    pub gain: i8,
    pub raw: Option<u8>,
}

pub fn defaults() -> Bands {
    [
        31.5, 63., 125., 250., 500., 1000., 2000., 4000., 8000., 16000., 3000., 12000.,
    ]
    .map(|hz| Band {
        hz,
        gain: 0,
        raw: None,
    })
}

pub fn limits(index: usize) -> (f32, f32, f32, i8) {
    match index {
        0..=3 => (30., 300., 0.5, 9),
        4..=6 => (300., 2000., 100., 9),
        7..=9 => (2000., 18000., 100., 9),
        _ => (20., 20480., 1., 12),
    }
}

pub fn valid(index: usize, band: Band) -> bool {
    let (min, max, step, gain) = limits(index);
    (min..=max).contains(&band.hz) && band.hz % step == 0. && (-gain..=gain).contains(&band.gain)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileKey {
    pub serial: String,
    pub name: String,
}
