#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod daemon;
mod model;

use daemon::{Edit, Live};
use eframe::egui::{self, Color32, RichText};
use model::LABELS;
use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

struct App {
    devices: Vec<Live>,
    selected: usize,
    pending: Option<Receiver<anyhow::Result<Vec<Live>>>>,
    error: Option<String>,
    frequency: f32,
    gain: i8,
    width: u8,
    band: usize,
    dynamics: [i32; 11],
}

impl App {
    fn refresh(&mut self, ctx: &egui::Context) {
        self.request(ctx, || async { daemon::read().await });
    }

    fn request<F, Fut>(&mut self, ctx: &egui::Context, task: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<Vec<Live>>>,
    {
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|runtime| runtime.block_on(task()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    fn load_controls(&mut self) {
        if let Some(device) = self.devices.get(self.selected) {
            let band = device.bands[self.band];
            self.frequency = band.hz;
            self.gain = band.gain;
            self.width = band.raw.unwrap_or(16);
            self.dynamics = device.dynamics.unwrap_or([0; 11]);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        use mpsc::TryRecvError;
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(Ok(devices)) => {
                    let serial = self.devices.get(self.selected).map(|d| &d.key.serial);
                    self.selected = devices
                        .iter()
                        .position(|d| Some(&d.key.serial) == serial)
                        .unwrap_or(0);
                    self.devices = devices;
                    self.load_controls();
                    self.error = None;
                    self.pending = None;
                }
                Ok(Err(error)) => {
                    self.devices.clear();
                    self.error = Some(format!("{error:#}"));
                    self.pending = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.devices.clear();
                    self.pending = None;
                    self.error = Some("Daemon check did not complete".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("GoXLR Microphone EQ");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.pending.is_none(),
                        egui::Button::new("Refresh from GoXLR"),
                    )
                    .clicked()
                {
                    self.refresh(ctx);
                }
                if self.pending.is_some() {
                    ui.spinner();
                }
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            });
            let previous = self.selected;
            if self.devices.len() > 1 && self.pending.is_none() {
                egui::ComboBox::from_label("GoXLR")
                    .selected_text(&self.devices[self.selected].key.serial)
                    .show_ui(ui, |ui| {
                        for (index, device) in self.devices.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, index, &device.key.serial);
                        }
                    });
            }
            if self.selected != previous {
                self.load_controls();
            }
            let Some(device) = self.devices.get(self.selected) else {
                if self.pending.is_none() && self.error.is_none() {
                    ui.label("No full-size GoXLR connected.");
                }
                return;
            };
            ui.label(format!(
                "Firmware {}  |  Mic profile {}",
                device.firmware, device.key.name
            ));
            let readback = daemon::has_readback(&device.firmware);
            let mut dynamics_edit = None;
            ui.collapsing("Compressor and de-esser", |ui| {
                if let Some(values) = device.dynamics {
                    ui.label("Live settings only; these changes are not saved to a profile.");
                    if !daemon::DYNAMICS_CONTROLS_VERIFIED {
                        ui.label("Controls are awaiting physical validation.");
                    }
                    let labels = ["De-esser enabled", "De-esser amount (%)", "De-esser attack",
                        "De-esser release", "Gain smoothing", "Detector weighting (%)",
                        "Compressor ratio", "Compressor threshold (dB)",
                        "Compressor attack", "Compressor release", "Compressor makeup (dB)"];
                    for (index, control) in goxlr_types::DynamicsControl::ALL.into_iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(labels[index]);
                            ui.add_enabled(daemon::DYNAMICS_CONTROLS_VERIFIED && self.pending.is_none(),
                                egui::DragValue::new(&mut self.dynamics[index]).range(control.range()));
                            use goxlr_types::DynamicsControl::*;
                            let raw = self.dynamics[index] as usize;
                            let attack = [1.,2.,3.,4.,5.,6.,7.,8.,9.,10.,12.,14.,16.,18.,20.,23.,26.,30.,35.,40.];
                            let release = [15,15,25,35,45,55,65,75,85,100,115,140,170,230,340,680,1000,1500,2000,3000];
                            let units = match control {
                                DeesserAttack => format!("{} ms (99%)", attack[raw]),
                                DeesserRelease => format!("{} ms (99%)", release[raw]),
                                CompressorAttack => format!("{} ms", if raw == 0 {0.001} else {attack[raw]}),
                                CompressorRelease => format!("{} ms", release[raw]),
                                CompressorRatio => format!("{}:1", [1.,1.1,1.2,1.4,1.6,1.8,2.,2.5,3.2,4.,5.6,8.,16.,32.,64.][raw]),
                                DeesserMode => format!("~{} ms at 48 kHz", [20.82,208.29,2080.50][raw]),
                                _ => String::new(),
                            };
                            ui.label(units);
                            if ui.add_enabled(daemon::DYNAMICS_CONTROLS_VERIFIED && self.pending.is_none()
                                && self.dynamics[index] != values[index], egui::Button::new("Apply")).clicked() {
                                dynamics_edit = Some((index, self.dynamics[index]));
                            }
                        });
                    }
                } else {
                    ui.label("Advanced dynamics are awaiting device validation.");
                }
            });
            ui.label(if readback {
                "All 12 bands show DSP readback. Q / width uses raw 0–32; numerical Q is not calibrated."
            } else {
                "Frequency and gain show the active daemon settings. Effective Q is unmeasured."
            });
            let native = readback && daemon::NATIVE_CONTROLS_VERIFIED;
            let verified = device.firmware == daemon::VERIFIED_FIRMWARE || native;
            let mut edit = None;
            ui.add_space(10.);
            egui::Grid::new("device-eq")
                .striped(true)
                .spacing([26., 8.])
                .show(ui, |ui| {
                    for label in [
                        "Band",
                        "Frequency",
                        "Gain",
                        "Width (raw 0–32)",
                        "Hardware write",
                    ] {
                        ui.label(RichText::new(label).strong());
                    }
                    ui.end_row();
                    for (index, label) in LABELS.iter().enumerate() {
                        let band = device.bands[index];
                        ui.label(*label);
                        if index < model::STOCK.len() || readback {
                            ui.label(format!("{:.1} Hz", band.hz));
                            ui.label(format!("{:+} dB", band.gain));
                            ui.label(
                                band.raw
                                    .map_or_else(|| "Unavailable".into(), |raw| raw.to_string()),
                            );
                            ui.label(if native {
                                "Frequency, gain and width verified"
                            } else if verified {
                                "Frequency and gain verified"
                            } else {
                                "Audio test pending"
                            });
                        } else {
                            for _ in 0..3 {
                                ui.label("Unavailable");
                            }
                            ui.label("USB mapping unverified");
                        }
                        ui.end_row();
                    }
                });
            if verified {
                ui.separator();
                ui.label(RichText::new("Live controls").strong());
                ui.label("Use the original GoXLR Utility to save the microphone profile.");
                let previous_band = self.band;
                egui::ComboBox::from_id_salt("verified-band")
                    .selected_text(LABELS[self.band])
                    .show_ui(ui, |ui| {
                        let count = if native { LABELS.len() } else { model::STOCK.len() };
                        for (index, label) in LABELS[..count].iter().enumerate() {
                            ui.selectable_value(&mut self.band, index, *label);
                        }
                    });
                let band = device.bands[self.band];
                if self.band != previous_band {
                    self.frequency = band.hz;
                    self.gain = band.gain;
                    self.width = band.raw.unwrap_or(16);
                }
                let (minimum, maximum, step, max_gain) = if native {
                    (20., 20480., 1., 12)
                } else { model::limits(self.band) };
                ui.add_enabled_ui(self.pending.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.frequency)
                                .range(minimum..=maximum)
                                .speed(step)
                                .suffix(" Hz"),
                        );
                        let frequency = (self.frequency / step).round() * step;
                        let frequency_changed = if native {
                            (24. * (frequency / 20.).log2()).round()
                                != (24. * (band.hz / 20.).log2()).round()
                        } else { frequency != band.hz };
                        if ui
                            .add_enabled(frequency_changed, egui::Button::new("Apply frequency"))
                            .clicked()
                        {
                            edit = Some(Edit::Frequency(frequency));
                        }
                        ui.add(
                            egui::DragValue::new(&mut self.gain)
                                .range(-max_gain..=max_gain)
                                .suffix(" dB"),
                        );
                        if ui
                            .add_enabled(self.gain != band.gain, egui::Button::new("Apply gain"))
                            .clicked()
                        {
                            edit = Some(Edit::Gain(self.gain));
                        }
                    });
                    if native {
                        ui.horizontal(|ui| {
                            ui.label("Q / width (raw selector)");
                            ui.add(egui::DragValue::new(&mut self.width).range(0..=32));
                            if ui.add_enabled(Some(self.width) != band.raw, egui::Button::new("Apply width")).clicked() {
                                edit = Some(Edit::Width(self.width));
                            }
                        });
                    }
                });
                if let Some((index, raw)) = dynamics_edit {
                    let baseline = device.clone();
                    self.request(ctx, move || async move { daemon::apply_dynamics(baseline, index, raw).await });
                } else if let Some(edit) = edit {
                    let serial = device.key.serial.clone();
                    let baseline = device.clone();
                    let index = self.band;
                    self.request(ctx, move || async move {
                        daemon::apply(baseline, index, edit).await?;
                        let devices = daemon::read().await?;
                        anyhow::ensure!(
                            devices.iter().any(|d| d.key.serial == serial),
                            "GoXLR disconnected"
                        );
                        Ok(devices)
                    });
                }
            }
            });
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720., 560.])
            .with_min_inner_size([670., 510.]),
        ..Default::default()
    };
    eframe::run_native(
        "GoXLR Native Mic EQ",
        options,
        Box::new(|cc| {
            let mut app = App {
                devices: vec![],
                selected: 0,
                pending: None,
                error: None,
                frequency: 1000.,
                gain: 0,
                width: 16,
                band: 5,
                dynamics: [0; 11],
            };
            app.refresh(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
