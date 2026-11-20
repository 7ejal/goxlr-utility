use eframe::egui;

#[cfg(any(windows, test))]
use std::time::{Duration, Instant};

#[cfg(any(windows, test))]
const WINDOW_SIZE: usize = 2048;
#[cfg(any(windows, test))]
const MAX_AGE: Duration = Duration::from_millis(500);

#[cfg(any(windows, test))]
struct Measurement {
    bins_dbfs: Vec<f32>,
    peak_dbfs: f64,
    rms_dbfs: f64,
    clipped_samples: usize,
    rate: u32,
    captured: Instant,
}

#[cfg(any(windows, test))]
struct Analyzer {
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    input: Vec<f32>,
    output: Vec<realfft::num_complex::Complex32>,
    scratch: Vec<realfft::num_complex::Complex32>,
    window: Vec<f32>,
    window_sum: f32,
}

#[cfg(any(windows, test))]
impl Default for Analyzer {
    fn default() -> Self {
        let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW_SIZE);
        let window: Vec<_> = (0..WINDOW_SIZE)
            .map(|i| 0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / WINDOW_SIZE as f32).cos())
            .collect();
        Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            window_sum: window.iter().sum(),
            window,
            fft,
        }
    }
}

#[cfg(any(windows, test))]
impl Analyzer {
    fn analyze(
        &mut self,
        samples: &[f32],
        rate: u32,
        captured: Instant,
    ) -> anyhow::Result<Measurement> {
        anyhow::ensure!(
            samples.len() == WINDOW_SIZE && rate > 0,
            "Incomplete recording window"
        );
        anyhow::ensure!(
            samples.iter().all(|sample| sample.is_finite()),
            "Recording contains invalid samples"
        );
        let mut peak = 0_f64;
        let mut squares = 0_f64;
        let mut clipped_samples = 0;
        for (i, &sample) in samples.iter().enumerate() {
            let value = f64::from(sample);
            peak = peak.max(value.abs());
            squares += value * value;
            clipped_samples += usize::from(value.abs() >= 1.);
            self.input[i] = sample * self.window[i];
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)?;
        let bins_dbfs: Vec<_> = self
            .output
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let sided = if index == 0 || index == WINDOW_SIZE / 2 {
                    1.
                } else {
                    2.
                };
                20. * (sided * value.norm() / self.window_sum).max(1e-12).log10()
            })
            .collect();
        anyhow::ensure!(
            bins_dbfs.iter().all(|value| value.is_finite()),
            "Recording spectrum is invalid"
        );
        Ok(Measurement {
            bins_dbfs,
            peak_dbfs: 20. * peak.log10(),
            rms_dbfs: 10. * (squares / samples.len() as f64).log10(),
            clipped_samples,
            rate,
            captured,
        })
    }
}

#[cfg(any(windows, test))]
fn fresh(captured: Instant, now: Instant) -> bool {
    now.checked_duration_since(captured)
        .is_some_and(|age| age <= MAX_AGE)
}

#[cfg(windows)]
mod recording {
    use super::*;
    use anyhow::{Context, Result, ensure};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
    };

    pub const DEVICE: &str = "Chat Mic (TC-HELICON GoXLR)";
    const CAPACITY: usize = WINDOW_SIZE * 4;

    struct Buffer {
        samples: VecDeque<f32>,
        captured: Option<Instant>,
    }

    pub struct Capture {
        _stream: cpal::Stream,
        buffer: Arc<Mutex<Buffer>>,
        failed: Arc<AtomicBool>,
        dropped: Arc<AtomicU64>,
        error: Arc<Mutex<Option<String>>>,
        pub rate: u32,
        pub channels: u16,
    }

    impl Capture {
        pub fn start() -> Result<Self> {
            let devices: Vec<_> = cpal::default_host()
                .input_devices()?
                .filter(|device| device.name().is_ok_and(|name| name == DEVICE))
                .collect();
            ensure!(
                devices.len() == 1,
                "Expected one exact Chat Mic recording device; no default microphone selected"
            );
            let device = &devices[0];
            let format = device
                .default_input_config()
                .context("Recording format unavailable")?;
            ensure!(
                format.sample_format() == cpal::SampleFormat::F32,
                "Chat Mic default format is not float32"
            );
            ensure!(
                (1..=2).contains(&format.channels()),
                "Unexpected Chat Mic channel layout"
            );
            let config: cpal::StreamConfig = format.into();
            let channels = usize::from(config.channels);
            let buffer = Arc::new(Mutex::new(Buffer {
                samples: VecDeque::with_capacity(CAPACITY),
                captured: None,
            }));
            let failed = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicU64::new(0));
            let error = Arc::new(Mutex::new(None));
            let callback_buffer = buffer.clone();
            let callback_failed = failed.clone();
            let callback_dropped = dropped.clone();
            let error_failed = failed.clone();
            let error_message = error.clone();
            let stream = device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    if data.len() % channels != 0 || data.iter().any(|value| !value.is_finite()) {
                        callback_failed.store(true, Ordering::Relaxed);
                        return;
                    }
                    // The recording callback never waits for the UI or allocates sample storage.
                    let Ok(mut buffer) = callback_buffer.try_lock() else {
                        callback_dropped
                            .fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
                        return;
                    };
                    let mut dropped = 0;
                    for frame in data.chunks_exact(channels) {
                        if buffer.samples.len() == CAPACITY {
                            buffer.samples.pop_front();
                            dropped += 1;
                        }
                        // Keep the first recorded channel, avoiding accidental stereo cancellation.
                        buffer.samples.push_back(frame[0]);
                    }
                    if !data.is_empty() {
                        buffer.captured = Some(Instant::now());
                    }
                    callback_dropped.fetch_add(dropped, Ordering::Relaxed);
                },
                move |cause| {
                    error_failed.store(true, Ordering::Relaxed);
                    if let Ok(mut message) = error_message.try_lock() {
                        *message = Some(cause.to_string());
                    }
                },
                None,
            )?;
            stream.play()?;
            Ok(Self {
                _stream: stream,
                buffer,
                failed,
                dropped,
                error,
                rate: config.sample_rate.0,
                channels: config.channels,
            })
        }

        pub fn latest(&self) -> Result<Option<([f32; WINDOW_SIZE], Instant)>> {
            if self.failed.load(Ordering::Relaxed) {
                let message = self.error.try_lock().ok().and_then(|value| value.clone());
                anyhow::bail!(
                    "{}",
                    message
                        .unwrap_or_else(|| "Recording stopped or returned invalid samples".into())
                );
            }
            let Ok(mut buffer) = self.buffer.try_lock() else {
                return Ok(None);
            };
            if buffer.samples.len() < WINDOW_SIZE {
                return Ok(None);
            }
            let captured = buffer.captured.context("Missing recording timestamp")?;
            let mut samples = [0.; WINDOW_SIZE];
            let skip = buffer.samples.len() - WINDOW_SIZE;
            for (output, input) in samples.iter_mut().zip(buffer.samples.iter().skip(skip)) {
                *output = *input;
            }
            buffer.samples.clear();
            Ok(Some((samples, captured)))
        }

        pub fn dropped(&self) -> u64 {
            self.dropped.load(Ordering::Relaxed)
        }
    }
}

#[cfg(windows)]
#[derive(Default)]
pub struct Spectrum {
    capture: Option<recording::Capture>,
    analyzer: Analyzer,
    measurement: Option<Measurement>,
    error: Option<String>,
    last_analysis: Option<Instant>,
}

#[cfg(windows)]
impl Spectrum {
    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut enabled = self.capture.is_some();
        if ui
            .checkbox(&mut enabled, "Record Chat Mic spectrum")
            .changed()
        {
            self.measurement = None;
            self.last_analysis = None;
            self.error = None;
            self.capture = if enabled {
                match recording::Capture::start() {
                    Ok(capture) => Some(capture),
                    Err(error) => {
                        self.error = Some(format!("{error:#}"));
                        None
                    }
                }
            } else {
                None
            };
        }
        ui.weak("Read-only recording; no audio is saved and no device settings are changed.");
        let now = Instant::now();
        if let Some(capture) = &self.capture {
            ctx.request_repaint_after(Duration::from_millis(50));
            if self
                .last_analysis
                .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(50))
            {
                self.last_analysis = Some(now);
                let update = capture.latest().and_then(|window| {
                    window
                        .map(|(samples, at)| self.analyzer.analyze(&samples, capture.rate, at))
                        .transpose()
                });
                match update {
                    Ok(Some(measurement)) => {
                        self.measurement = Some(measurement);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.error = Some(format!("Recording stopped: {error:#}"));
                        self.measurement = None;
                        self.capture = None;
                    }
                }
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        let Some(capture) = &self.capture else { return };
        ui.label(format!(
            "{} · channel 1 of {} · {} Hz recording · {WINDOW_SIZE}-sample Hann window",
            recording::DEVICE,
            capture.channels,
            capture.rate
        ));
        ui.weak("Recorded endpoint levels include the Windows recording level; these are not internal DSP levels or gain reduction.");
        ui.label(format!(
            "Host capture samples dropped: {} (not DSP underruns)",
            capture.dropped()
        ));
        let Some(measurement) = self
            .measurement
            .as_ref()
            .filter(|value| fresh(value.captured, Instant::now()))
        else {
            ui.label("Recording data unavailable or stale.");
            return;
        };
        ui.label(format!(
            "Recorded peak {:.1} dBFS · RMS {:.1} dBFS · samples ≥ full scale: {} in this window",
            measurement.peak_dbfs, measurement.rms_dbfs, measurement.clipped_samples
        ));
        draw_spectrum(ui, measurement);
    }
}

#[cfg(windows)]
fn draw_spectrum(ui: &mut egui::Ui, measurement: &Measurement) {
    use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};
    let high = 20_000_f32.min(measurement.rate as f32 / 2.);
    if high <= 20. {
        return;
    }
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width().max(160.), 220.),
        Sense::hover(),
    );
    let plot = response.rect.shrink2(Vec2::new(38., 24.));
    let x = |hz: f32| plot.left() + (hz / 20.).log10() / (high / 20.).log10() * plot.width();
    let y = |db: f32| plot.bottom() - (db.clamp(-120., 0.) + 120.) / 120. * plot.height();
    let grid = Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color);
    let label = ui.visuals().weak_text_color();
    for db in [-120, -90, -60, -30, 0] {
        painter.line_segment(
            [
                Pos2::new(plot.left(), y(db as f32)),
                Pos2::new(plot.right(), y(db as f32)),
            ],
            grid,
        );
        painter.text(
            Pos2::new(plot.left() - 5., y(db as f32)),
            Align2::RIGHT_CENTER,
            format!("{db}"),
            FontId::proportional(10.),
            label,
        );
    }
    for (hz, text) in [
        (20., "20 Hz"),
        (100., "100"),
        (1000., "1k"),
        (10000., "10k"),
        (20000., "20k"),
    ] {
        if hz > high {
            continue;
        }
        painter.line_segment(
            [
                Pos2::new(x(hz), plot.top()),
                Pos2::new(x(hz), plot.bottom()),
            ],
            grid,
        );
        painter.text(
            Pos2::new(x(hz), plot.bottom() + 5.),
            Align2::CENTER_TOP,
            text,
            FontId::proportional(10.),
            label,
        );
    }
    let points = measurement
        .bins_dbfs
        .iter()
        .enumerate()
        .filter_map(|(index, &db)| {
            let hz = index as f32 * measurement.rate as f32 / WINDOW_SIZE as f32;
            (hz >= 20. && hz <= high).then(|| Pos2::new(x(hz), y(db)))
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.5, Color32::from_rgb(92, 210, 160)),
    ));
    if let Some(pointer) = response.hover_pos().filter(|pos| plot.contains(*pos)) {
        let hz = 20. * (high / 20.).powf((pointer.x - plot.left()) / plot.width());
        let bin = (hz * WINDOW_SIZE as f32 / measurement.rate as f32).round() as usize;
        if let Some(db) = measurement.bins_dbfs.get(bin) {
            response.on_hover_text(format!(
                "{:.1} Hz bin · {:.1} dBFS per-bin amplitude",
                bin as f32 * measurement.rate as f32 / WINDOW_SIZE as f32,
                db
            ));
        }
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct Spectrum;

#[cfg(not(windows))]
impl Spectrum {
    pub fn show(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.label("Chat Mic spectrum capture is currently available on Windows only.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_centered_sine_has_correct_amplitude_and_rms() {
        let samples: Vec<_> = (0..WINDOW_SIZE)
            .map(|i| 0.5 * (2. * std::f32::consts::PI * 43. * i as f32 / WINDOW_SIZE as f32).sin())
            .collect();
        let result = Analyzer::default()
            .analyze(&samples, 48_000, Instant::now())
            .unwrap();
        assert!((result.bins_dbfs[43] + 6.0206).abs() < 0.002);
        assert!((result.rms_dbfs + 9.0309).abs() < 0.002);
        assert_eq!(result.clipped_samples, 0);
        assert!((result.peak_dbfs + 6.0206).abs() < 0.002);
    }

    #[test]
    fn silence_dc_and_malformed_windows_are_handled() {
        let mut analyzer = Analyzer::default();
        let silence = analyzer
            .analyze(&[0.; WINDOW_SIZE], 48_000, Instant::now())
            .unwrap();
        assert!(
            silence
                .bins_dbfs
                .iter()
                .all(|db| (*db + 240.).abs() < 0.001)
        );
        assert!(silence.peak_dbfs.is_infinite() && silence.peak_dbfs.is_sign_negative());
        assert!(silence.rms_dbfs.is_infinite() && silence.rms_dbfs.is_sign_negative());
        let dc = analyzer
            .analyze(&[1.; WINDOW_SIZE], 48_000, Instant::now())
            .unwrap();
        assert!(dc.bins_dbfs[0].abs() < 0.001);
        assert_eq!(dc.clipped_samples, WINDOW_SIZE);
        assert_eq!(dc.rms_dbfs, 0.);
        assert!(analyzer.analyze(&[0.; 20], 48_000, Instant::now()).is_err());
        assert!(
            analyzer
                .analyze(&[f32::NAN; WINDOW_SIZE], 48_000, Instant::now())
                .is_err()
        );
        assert!(
            analyzer
                .analyze(&[0.; WINDOW_SIZE], 0, Instant::now())
                .is_err()
        );
    }

    #[test]
    fn stale_and_future_samples_are_unavailable() {
        let now = Instant::now();
        assert!(fresh(now, now + Duration::from_millis(100)));
        assert!(!fresh(now, now + MAX_AGE + Duration::from_millis(1)));
        assert!(!fresh(now + Duration::from_millis(1), now));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Requires the actual GoXLR Chat Mic endpoint; passive two-second recording only"]
    fn native_live_spectrum_capture() -> anyhow::Result<()> {
        let capture = recording::Capture::start()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut analyzer = Analyzer::default();
        let mut windows = 0;
        while Instant::now() < deadline {
            if let Some((samples, captured)) = capture.latest()? {
                let measurement = analyzer.analyze(&samples, capture.rate, captured)?;
                anyhow::ensure!(fresh(captured, Instant::now()), "Live recording was stale");
                anyhow::ensure!(
                    measurement.bins_dbfs.iter().all(|value| value.is_finite()),
                    "Invalid live FFT"
                );
                windows += 1;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        anyhow::ensure!(windows > 0, "No complete live recording window received");
        println!(
            "PASS {windows} actual Chat Mic FFT windows at {} Hz, {} input channels; no output or setting writes",
            capture.rate, capture.channels
        );
        Ok(())
    }
}
