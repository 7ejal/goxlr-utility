use crate::{daemon, model::ProfileKey};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use goxlr_ipc::{MixerStatus, client::Client};
use goxlr_types::{ArmDiagnostic, DeviceType};
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_AGE: Duration = Duration::from_secs(2);
const METRICS: [ArmDiagnostic; 6] = [
    ArmDiagnostic::CpuAudioBit22Count,
    ArmDiagnostic::CpuAudioBit23Count,
    ArmDiagnostic::EffectRingCapacity,
    ArmDiagnostic::EffectRingBuffer,
    ArmDiagnostic::EffectRingUsed,
    ArmDiagnostic::SchedulerTickLow,
];

#[derive(Clone, Debug)]
struct Snapshot {
    values: [u32; 6],
}

impl Snapshot {
    fn new(values: [u32; 6]) -> Result<Self> {
        ensure!(
            values[2] == 0x404 && values[3] == 0x3d79d && values[4] < values[2],
            "Invalid ARM communication-ring metadata"
        );
        Ok(Self { values })
    }
}

fn supported(firmware: &str) -> bool {
    firmware == daemon::ARM_DIAGNOSTICS_FIRMWARE
}

fn check_device(mixer: &MixerStatus, expected: &ProfileKey) -> Result<()> {
    ensure!(
        mixer.hardware.device_type == DeviceType::Full
            && supported(&mixer.hardware.versions.firmware.to_string())
            && mixer.mic_profile_name == expected.name,
        "ARM diagnostics requires the same full-size 910 device and microphone profile"
    );
    Ok(())
}

async fn read(expected: ProfileKey) -> Result<Snapshot> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut client = daemon::connect().await?;
        client.poll_status().await?;
        check_device(
            client
                .status()
                .mixers
                .get(&expected.serial)
                .context("GoXLR disconnected")?,
            &expected,
        )?;
        let mut values = [0; 6];
        for (index, metric) in METRICS.into_iter().enumerate() {
            values[index] = client.read_arm_diagnostic(&expected.serial, metric).await?;
        }
        client.poll_status().await?;
        check_device(
            client
                .status()
                .mixers
                .get(&expected.serial)
                .context("GoXLR disconnected")?,
            &expected,
        )?;
        Snapshot::new(values)
    })
    .await
    .context("ARM diagnostics timed out")?
}

#[derive(Default)]
pub struct Diagnostics {
    enabled: bool,
    pending: Option<(ProfileKey, Instant, Receiver<Result<Snapshot>>)>,
    last_poll: Option<Instant>,
    sample: Option<(ProfileKey, Instant, Snapshot)>,
    error: Option<String>,
}

impl Diagnostics {
    fn value(&self, key: Option<&ProfileKey>, busy: bool, now: Instant) -> Option<&Snapshot> {
        let (sample_key, at, sample) = self.sample.as_ref()?;
        (self.enabled
            && !busy
            && Some(sample_key) == key
            && now
                .checked_duration_since(*at)
                .is_some_and(|age| age <= MAX_AGE))
        .then_some(sample)
    }

    fn can_poll(&self, key: Option<&ProfileKey>, busy: bool, now: Instant) -> bool {
        self.enabled
            && !busy
            && key.is_some()
            && self.pending.is_none()
            && self
                .last_poll
                .is_none_or(|last| now.duration_since(last) >= POLL_INTERVAL)
    }

    fn tick(&mut self, ctx: &egui::Context, key: Option<&ProfileKey>, busy: bool) {
        let completed = self
            .pending
            .as_ref()
            .and_then(|(expected, started, receiver)| match receiver.try_recv() {
                Ok(result) => Some((expected.clone(), *started, result)),
                Err(TryRecvError::Disconnected) => Some((
                    expected.clone(),
                    *started,
                    Err(anyhow::anyhow!("ARM reading did not complete")),
                )),
                Err(TryRecvError::Empty) => None,
            });
        if let Some((expected, started, result)) = completed {
            self.pending = None;
            if key == Some(&expected) {
                match result {
                    Ok(sample) => {
                        self.sample = Some((expected, started, sample));
                        self.error = None;
                    }
                    Err(error) => {
                        self.sample = None;
                        self.error = Some(format!("{error:#}"));
                    }
                }
            }
        }
        if !self.enabled
            || busy
            || self
                .sample
                .as_ref()
                .is_some_and(|(sample_key, _, _)| Some(sample_key) != key)
        {
            self.sample = None;
        }
        if !self.enabled || key.is_none() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        let now = Instant::now();
        if !self.can_poll(key, busy, now) {
            return;
        }
        let expected = key.expect("Checked profile").clone();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some((expected.clone(), now, receiver));
        self.last_poll = Some(now);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|runtime| runtime.block_on(read(expected)));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        device: Option<&daemon::Live>,
        busy: bool,
    ) {
        let key = device
            .filter(|device| supported(&device.firmware))
            .map(|device| &device.key);
        self.tick(ctx, key, busy);
        ui.collapsing("ARM diagnostics", |ui| {
            ui.add_enabled_ui(key.is_some(), |ui| {
                ui.checkbox(&mut self.enabled, "Read live ARM metrics (1 Hz)");
            });
            ui.weak("Sequential read-only snapshots; not DSP load, clock, free heap, stack usage or overrun counts.");
            if key.is_none() {
                ui.label("Requires the verified firmware 910 diagnostics bridge.");
                return;
            }
            if !self.enabled {
                return;
            }
            if let Some(sample) = self.value(key, busy, Instant::now()) {
                let values = sample.values;
                egui::Grid::new("arm-diagnostics-values").striped(true).show(ui, |ui| {
                    for (label, value) in [
                        ("CPU-audio bit 22 events", values[0].to_string()),
                        ("CPU-audio bit 23 events", values[1].to_string()),
                        ("Communication-ring address", format!("0x{:X}", values[3])),
                        ("Communication-ring capacity", format!("{} bytes", values[2])),
                        ("Communication-ring used", format!("{} bytes", values[4])),
                        ("Scheduler tick low word", values[5].to_string()),
                    ] {
                        ui.label(label);
                        ui.monospace(value);
                        ui.end_row();
                    }
                });
                ui.add(egui::ProgressBar::new(values[4] as f32 / values[2] as f32).text("Communication-ring occupancy"));
                ui.weak("Counters may reset when CPU audio is reconfigured. Tick duration is uncalibrated; a zero occupancy snapshot does not prove absence of peak pressure.");
            } else {
                ui.label(if busy { "Paused during device operation" } else { "Unavailable / waiting for fresh ARM readings" });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires the authorized physical 910 device; reads six ARM metrics without writes"]
    async fn native_live_arm_diagnostics() -> Result<()> {
        let devices = daemon::read().await?;
        ensure!(
            devices.len() == 1 && supported(&devices[0].firmware),
            "Expected one 910 device"
        );
        let snapshot = read(devices[0].key.clone()).await?;
        println!(
            "PASS six live ARM metrics; ring 0x{:X}, capacity {} bytes, used {} bytes; no DSP performance claim",
            snapshot.values[3], snapshot.values[2], snapshot.values[4]
        );
        Ok(())
    }

    fn key(name: &str) -> ProfileKey {
        ProfileKey {
            serial: "device".into(),
            name: name.into(),
        }
    }

    #[test]
    fn only_910_can_be_probed_and_unsigned_values_are_preserved() {
        assert!(supported("1.5.1.910"));
        for firmware in ["1.5.1.909", "1.5.1.911", "unknown"] {
            assert!(!supported(firmware));
        }
        let sample = Snapshot::new([u32::MAX, 0x80000000, 0x404, 0x3d79d, 0, u32::MAX]).unwrap();
        assert_eq!(sample.values[0], u32::MAX);
        assert_eq!(sample.values[1], 0x80000000);
        assert_eq!(sample.values[5], u32::MAX);
        for values in [
            [0, 0, 0, 0x3d79d, 0, 0],
            [0, 0, 0x404, 0, 0, 0],
            [0, 0, 0x404, 0x3d79d, 0x404, 0],
        ] {
            assert!(Snapshot::new(values).is_err());
        }
    }

    #[test]
    fn stale_paused_wrong_profile_and_failed_samples_are_unavailable() {
        let now = Instant::now();
        let profile = key("one");
        let mut diagnostics = Diagnostics {
            enabled: true,
            sample: Some((
                profile.clone(),
                now,
                Snapshot::new([0, 1, 0x404, 0x3d79d, 0, 2]).unwrap(),
            )),
            ..Default::default()
        };
        assert!(diagnostics.value(Some(&profile), false, now).is_some());
        assert!(diagnostics.value(Some(&profile), true, now).is_none());
        assert!(diagnostics.value(Some(&key("two")), false, now).is_none());
        assert!(
            diagnostics
                .value(
                    Some(&profile),
                    false,
                    now + MAX_AGE + Duration::from_millis(1)
                )
                .is_none()
        );
        assert!(
            diagnostics
                .value(Some(&profile), false, now - Duration::from_millis(1))
                .is_none()
        );
        let (sender, receiver) = mpsc::channel();
        sender.send(Err(anyhow::anyhow!("Disconnected"))).unwrap();
        diagnostics.pending = Some((profile.clone(), now, receiver));
        diagnostics.tick(&egui::Context::default(), Some(&profile), true);
        assert!(diagnostics.sample.is_none());
        assert!(diagnostics.error.as_ref().unwrap().contains("Disconnected"));
    }

    #[test]
    fn polling_is_one_hz_single_flight_and_paused_during_operations() {
        let now = Instant::now();
        let profile = key("one");
        let mut diagnostics = Diagnostics {
            enabled: true,
            last_poll: Some(now),
            ..Default::default()
        };
        assert!(!diagnostics.can_poll(Some(&profile), false, now + Duration::from_millis(999)));
        assert!(diagnostics.can_poll(Some(&profile), false, now + POLL_INTERVAL));
        assert!(!diagnostics.can_poll(Some(&profile), true, now + POLL_INTERVAL));
        assert!(!diagnostics.can_poll(None, false, now + POLL_INTERVAL));
        let (_sender, receiver) = mpsc::channel();
        diagnostics.pending = Some((profile.clone(), now, receiver));
        assert!(!diagnostics.can_poll(Some(&profile), false, now + Duration::from_secs(5)));
    }
}
