#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod activity;
mod compressor;
mod controls;
mod daemon;
mod detector;
mod diagnostics;
mod model;
mod presets;
mod profiles;
mod spectrum;

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
    preset_path: String,
    preset_name: String,
    imported_preset: Option<presets::Preset>,
    preset_status: Option<Result<String, String>>,
    mic_activity: activity::Activity,
    diagnostics: diagnostics::Diagnostics,
    detector_rate: u32,
    mic_profile_selection: String,
    mic_profile_save_as: String,
    spectrum: spectrum::Spectrum,
    profile_status: Option<String>,
    pending_profile_status: Option<&'static str>,
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
            self.mic_profile_selection = device.key.name.clone();
        }
    }

    fn microphone_profiles(
        &mut self,
        ui: &mut egui::Ui,
        device: Option<&Live>,
    ) -> Option<profiles::Action> {
        let mut action = None;
        ui.collapsing("Saved microphone profiles", |ui| {
            ui.label("Saves the current 36-field EQ snapshot and stock microphone settings through the existing Utility.");
            ui.weak("Advanced raw dynamics remain separate: export a portable preset for the complete 47-field snapshot.");
            let ready = self.pending.is_none() && device.is_some_and(|device| daemon::native_eq_writes_verified(&device.firmware));
            ui.add_enabled_ui(ready, |ui| {
                if ui.button("Save current microphone profile").clicked() { action = Some(profiles::Action::Save); }
                ui.horizontal(|ui| {
                    ui.label("New profile name");
                    ui.text_edit_singleline(&mut self.mic_profile_save_as);
                    let available = device.is_some_and(|device| !device.mic_profiles.iter().any(|name| name.eq_ignore_ascii_case(&self.mic_profile_save_as)));
                    if ui.add_enabled(profiles::valid_name(&self.mic_profile_save_as) && available, egui::Button::new("Save as new profile"))
                        .on_disabled_hover_text("Choose a valid new profile name, without a path or extension. Existing names are not overwritten.").clicked() {
                        action = Some(profiles::Action::SaveAs(self.mic_profile_save_as.clone()));
                    }
                });
                if let Some(device) = device {
                    egui::ComboBox::from_label("Existing microphone profile")
                        .selected_text(&self.mic_profile_selection).show_ui(ui, |ui| {
                            for name in &device.mic_profiles {
                                ui.selectable_value(&mut self.mic_profile_selection, name.clone(), name);
                            }
                        });
                    ui.weak("Loading replaces the current microphone settings, including microphone type and gain. Save unsaved changes first.");
                    if ui.add_enabled(device.mic_profiles.contains(&self.mic_profile_selection), egui::Button::new("Load selected microphone profile")).clicked() {
                        action = Some(profiles::Action::Load(self.mic_profile_selection.clone()));
                    }
                }
            });
            if !ready && self.pending.is_none() { ui.weak("Profile operations require the supported 908/909/910 native EQ bridge."); }
            if let Some(message) = &self.profile_status { ui.label(message); }
        });
        action
    }

    fn presets(&mut self, ui: &mut egui::Ui, device: Option<&Live>) {
        ui.collapsing("Portable presets", |ui| {
            ui.label("Export the last complete 47-field DSP snapshot; refresh first for current values. Import previews only and never writes hardware or replaces your microphone profile.");
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.preset_name);
            });
            ui.horizontal(|ui| {
                ui.label("JSON file");
                ui.text_edit_singleline(&mut self.preset_path);
            });
            let path_ready = !self.preset_path.trim().is_empty() && self.pending.is_none();
            let export_ready = device.is_some_and(|device| presets::supports_firmware(&device.firmware) && device.dynamics.is_some());
            ui.horizontal(|ui| {
                if ui.add_enabled(path_ready && export_ready, egui::Button::new("Export new file")).clicked() {
                    self.preset_status = Some(presets::Preset::from_live(self.preset_name.clone(), device.expect("Checked device"))
                        .and_then(|preset| preset.save_new(std::path::Path::new(&self.preset_path)))
                        .map(|_| "Saved all 47 fields. Existing files are never overwritten.".into())
                        .map_err(|error| format!("{error:#}")));
                }
                if ui.add_enabled(path_ready, egui::Button::new("Import preview")).clicked() {
                    self.preset_status = Some(match presets::Preset::read(std::path::Path::new(&self.preset_path)) {
                        Ok(preset) => {
                            self.preset_name = preset.name.clone();
                            self.imported_preset = Some(preset);
                            Ok("Imported 47 validated fields for preview. No hardware changes.".into())
                        }
                        Err(error) => {
                            self.imported_preset = None;
                            Err(format!("{error:#}"))
                        }
                    });
                }
            });
            if let Some(status) = &self.preset_status {
                match status {
                    Ok(message) => { ui.label(message); }
                    Err(error) => { ui.colored_label(Color32::LIGHT_RED, error); }
                }
            }
            if let Some(preset) = &self.imported_preset {
                ui.separator();
                ui.strong(format!("Preview: {}", preset.name));
                if let Some(device) = device.filter(|_| export_ready) {
                    match preset.changes(device) {
                        Ok(changes) => {
                            ui.label(format!("{} changes from the last DSP snapshot. Values below are exact raw selectors/dB.", changes.len()));
                            egui::Grid::new("preset-preview").striped(true).show(ui, |ui| {
                                for heading in ["Parameter", "DSP key", "Current", "Imported"] { ui.strong(heading); }
                                ui.end_row();
                                for change in changes {
                                    ui.label(change.name);
                                    ui.monospace(format!("0x{:03X}", change.key));
                                    ui.label(change.before.to_string());
                                    ui.label(change.after.to_string());
                                    ui.end_row();
                                }
                            });
                        }
                        Err(error) => { ui.colored_label(Color32::LIGHT_RED, format!("{error:#}")); }
                    }
                } else {
                    ui.label("47 valid fields loaded. Connect 909/910 and refresh to compare with actual DSP values.");
                }
                ui.add_enabled(false, egui::Button::new("Apply preset"))
                    .on_disabled_hover_text("Preset application requires complete processing validation. Use verified individual controls for now.");
            }
        });
    }

    fn capabilities(ui: &mut egui::Ui, device: &Live) {
        ui.collapsing("Hardware capabilities", |ui| {
            let fields = if device.dynamics.is_some() { 47 } else if daemon::has_readback(&device.firmware) { 36 } else { 0 };
            ui.label(format!("{fields} fields available through actual DSP readback."));
            ui.label(if daemon::native_eq_writes_verified(&device.firmware) {
                "12-band microphone EQ: frequency, gain and raw width physically verified on the tested bridge firmware."
            } else if daemon::has_readback(&device.firmware) {
                "DSP readback is available; native hardware writes are not validated on this firmware."
            } else if device.firmware == daemon::VERIFIED_FIRMWARE {
                "10-band microphone EQ: frequency and gain verified on 907; DSP width readback unavailable."
            } else {
                "Unrecognized firmware: native hardware writes are disabled."
            });
            ui.label(if device.dynamics.is_some() {
                "All 47 raw fields: live read/write/restoration verified on 909/910. Physical processing checks cover makeup gain and de-esser intensity on 910; remaining dynamics calibration is pending."
            } else {
                "Advanced dynamics readback requires the exact 909/910 bridge; no unsupported firmware is probed."
            });
            egui::Grid::new("hardware-capabilities").striped(true).show(ui, |ui| {
                for (feature, gate) in [
                    ("Numerical microphone Q", "Raw-width calibration pending"),
                    ("Headphone EQ / filters", "Physical DAC postmix isolation and DSP capacity unverified"),
                    ("Compressor knee / lookahead / auto makeup", "No independent stock control verified"),
                    ("De-esser frequency / bandwidth / split band", "No corresponding stock controls verified"),
                    ("Noise gate additions / limiter / processing order", "Additional processing paths unverified"),
                    ("Gain reduction", "No verified live gain-reduction readback"),
                    ("Microphone spectrum", "Optional Windows Chat Mic recording; endpoint levels, not internal DSP levels"),
                    ("DSP performance / ARM free memory", "No physical DSP clock, utilization, free cycles or heap/stack telemetry"),
                ] {
                    ui.label(feature);
                    ui.weak(gate);
                    ui.end_row();
                }
            });
        });
    }

    fn parameter_reference(ui: &mut egui::Ui, device: &Live) {
        ui.collapsing("Parameter reference — 47 fields", |ui| {
            ui.label("Descriptor defaults are serialized ROM fields, not your saved profile, effective boot settings or a reset command. EQ values are generic original three-band object defaults reused by the 12-band mapping.");
            egui::ScrollArea::horizontal().show(ui, |ui| {
                egui::Grid::new("parameter-reference").striped(true).show(ui, |ui| {
                    for label in ["DSP key", "Parameter", "Raw range", "Descriptor default", "Current DSP"] {
                        ui.strong(label);
                    }
                    ui.end_row();
                    for band in 0..12 {
                        for field in [goxlr_types::MicEqField::Frequency, goxlr_types::MicEqField::Gain, goxlr_types::MicEqField::Width] {
                            let parameter = controls::eq_parameter(band, field).expect("Known band");
                            ui.monospace(format!("0x{:03X}", parameter.key));
                            ui.label(&parameter.name).on_hover_text(parameter.meaning);
                            ui.label(format!("{}…{}", parameter.range.start(), parameter.range.end()));
                            ui.label(parameter.rom_default.map_or_else(|| "Not established".into(), |raw| match field {
                                goxlr_types::MicEqField::Frequency => format!("{raw} raw / {:.2} Hz", 20. * 2_f32.powf(raw as f32 / 24.)),
                                goxlr_types::MicEqField::Gain => format!("{raw:+} dB"),
                                goxlr_types::MicEqField::Width => format!("{raw} raw"),
                            }));
                            if daemon::has_readback(&device.firmware) {
                                let value = device.bands[band];
                                ui.label(match field {
                                    goxlr_types::MicEqField::Frequency => format!("{:.2} Hz (raw {:.0})", value.hz, 24. * (value.hz / 20.).log2()),
                                    goxlr_types::MicEqField::Gain => format!("{:+} dB", value.gain),
                                    goxlr_types::MicEqField::Width => value.raw.map_or_else(|| "Unavailable".into(), |raw| format!("{raw} raw")),
                                });
                            } else { ui.weak("Unavailable"); }
                            ui.end_row();
                        }
                    }
                    for (index, control) in goxlr_types::DynamicsControl::ALL.into_iter().enumerate() {
                        let parameter = controls::dynamics_parameter(control);
                        ui.monospace(format!("0x{:03X}", parameter.key))
                            .on_hover_text(format!("Native setter P:{:06X}", parameter.setter.expect("Known setter")));
                        ui.label(&parameter.name).on_hover_text(parameter.meaning);
                        ui.label(format!("{}…{}", parameter.range.start(), parameter.range.end()));
                        ui.label(controls::dynamics_value(control, parameter.rom_default.expect("Known ROM default")));
                        ui.label(device.dynamics.map_or_else(|| "Unavailable".into(), |values| controls::dynamics_value(control, values[index])));
                        ui.end_row();
                    }
                });
            });
        });
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
                    if let Some(message) = self.pending_profile_status.take() {
                        self.profile_status = Some(message.into());
                    }
                }
                Ok(Err(error)) => {
                    self.devices.clear();
                    self.error = Some(format!("{error:#}"));
                    self.pending = None;
                    self.pending_profile_status = None;
                    self.profile_status = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.devices.clear();
                    self.pending = None;
                    self.pending_profile_status = None;
                    self.profile_status = None;
                    self.error = Some("Daemon check did not complete".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("GoXLR Audio Processor");
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
            let preset_device = self.devices.get(self.selected).cloned();
            ui.checkbox(&mut self.mic_activity.enabled, "Device mic activity");
            if self.mic_activity.enabled {
                ui.weak("Uncalibrated device scale; not peak, RMS, gain reduction or dBFS.");
                let value = preset_device.as_ref().and_then(|device| self.mic_activity.value(&device.key, self.pending.is_some(), std::time::Instant::now()));
                if let Some(value) = value {
                    ui.add(egui::ProgressBar::new(((value + 72.2) / 72.2) as f32).text(format!("{value:.1} device dB")));
                } else {
                    ui.weak(if self.pending.is_some() { "Paused during device operation" } else { "Unavailable / waiting for a fresh device reading" })
                        .on_hover_text(self.mic_activity.error.as_deref().unwrap_or("Stale values are not displayed."));
                }
            }
            self.presets(ui, preset_device.as_ref());
            ui.collapsing("Recorded microphone spectrum", |ui| self.spectrum.show(ui, ctx));
            self.diagnostics.show(ui, ctx, preset_device.as_ref(), self.pending.is_some());
            if let Some(action) = self.microphone_profiles(ui, preset_device.as_ref()) {
                let expected = preset_device.clone().expect("Profile buttons require a device");
                self.profile_status = None;
                self.pending_profile_status = Some(match &action {
                    profiles::Action::Save => "Current microphone profile saved.",
                    profiles::Action::SaveAs(_) => "New microphone profile saved and activated.",
                    profiles::Action::Load(_) => "Selected microphone profile loaded.",
                });
                self.request(ctx, move || async move { daemon::microphone_profile(expected, action).await });
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
            Self::capabilities(ui, device);
            Self::parameter_reference(ui, device);
            let readback = daemon::has_readback(&device.firmware);
            let mut dynamics_edit = None;
            ui.collapsing("Compressor and de-esser", |ui| {
                if let Some(values) = device.dynamics {
                    ui.collapsing("Compressor static response", |ui| compressor::show(ui, &values));
                    ui.collapsing("De-esser detector response", |ui| detector::show(ui, &mut self.detector_rate));
                    ui.label("Makeup and intensity edits update the current mic profile; use Save to persist them. Other raw dynamics fields are not saved.");
                    ui.label(if device.firmware == daemon::ARM_DIAGNOSTICS_FIRMWARE {
                        "Makeup gain and de-esser intensity passed reversible physical response checks on 910. Other dynamics edits remain disabled pending calibration."
                    } else {
                        "Dynamics editing awaits physical response validation on this firmware."
                    });
                    for (index, control) in goxlr_types::DynamicsControl::ALL.into_iter().enumerate() {
                        let parameter = controls::dynamics_parameter(control);
                        let editable = daemon::dynamics_control_verified(&device.firmware, control) && self.pending.is_none();
                        ui.horizontal(|ui| {
                            ui.label(&parameter.name).on_hover_text(parameter.meaning);
                            ui.add_enabled(editable,
                                egui::DragValue::new(&mut self.dynamics[index]).range(control.range()));
                            ui.label(controls::dynamics_value(control, self.dynamics[index]));
                            if ui.add_enabled(editable
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
            let native = daemon::native_eq_writes_verified(&device.firmware);
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
        self.mic_activity.tick(
            ctx,
            self.devices.get(self.selected).map(|device| &device.key),
            self.pending.is_some(),
        );
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
        "GoXLR Audio Processor",
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
                preset_path: String::new(),
                preset_name: "My preset".into(),
                imported_preset: None,
                preset_status: None,
                mic_activity: activity::Activity::default(),
                diagnostics: diagnostics::Diagnostics::default(),
                detector_rate: 48_000,
                mic_profile_selection: String::new(),
                mic_profile_save_as: String::new(),
                spectrum: spectrum::Spectrum::default(),
                profile_status: None,
                pending_profile_status: None,
            };
            app.refresh(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
