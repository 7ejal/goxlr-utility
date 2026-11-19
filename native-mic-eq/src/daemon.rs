use crate::model::{Bands, ProfileKey, STOCK, defaults, valid};
use anyhow::{Context, Result, ensure};
use goxlr_ipc::{
    GoXLRCommand, MixerStatus,
    client::Client,
    clients::ipc::{ipc_client::IPCClient, ipc_socket::Socket},
};
use goxlr_types::DeviceType;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ToFsName, ToNsName, tokio::prelude::LocalSocketStream,
    traits::tokio::Stream,
};
use std::time::Duration;

pub const VERIFIED_FIRMWARE: &str = "1.5.1.907";
pub const READBACK_FIRMWARE: &str = "1.5.1.908";
// All 12 bands passed reversible physical frequency, gain and width tests on 908.
pub const NATIVE_CONTROLS_VERIFIED: bool = true;

#[derive(Clone)]
pub struct Live {
    pub key: ProfileKey,
    pub firmware: String,
    pub bands: Bands,
}

#[derive(Clone, Copy)]
pub enum Edit {
    Frequency(f32),
    Gain(i8),
    Width(u8),
}

async fn connect() -> Result<IPCClient> {
    let name = if cfg!(windows) {
        "@goxlr.socket".to_ns_name::<GenericNamespaced>()?
    } else {
        "/tmp/goxlr.socket".to_fs_name::<GenericFilePath>()?
    };
    let stream = LocalSocketStream::connect(name)
        .await
        .context("GoXLR daemon unavailable")?;
    Ok(IPCClient::new(Socket::new(stream)))
}

fn read_bands(serial: &str, mixer: &MixerStatus) -> Result<Live> {
    let mut bands = defaults();
    let status = &mixer.mic_status;
    for (i, key) in STOCK.iter().enumerate() {
        bands[i].hz = *status
            .equaliser
            .frequency
            .get(key)
            .context("Missing stock frequency")?;
        bands[i].gain = *status
            .equaliser
            .gain
            .get(key)
            .context("Missing stock gain")?;
        bands[i].raw = status
            .eq_bandwidth
            .as_ref()
            .and_then(|eq| eq.raw.get(key))
            .copied()
            .filter(|raw| *raw <= 32);
    }
    Ok(Live {
        key: ProfileKey {
            serial: serial.into(),
            name: mixer.mic_profile_name.clone(),
        },
        firmware: mixer.hardware.versions.firmware.to_string(),
        bands,
    })
}

pub async fn read() -> Result<Vec<Live>> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut client = connect().await?;
        client.poll_status().await?;
        let mut devices = client
            .status()
            .mixers
            .iter()
            .filter(|(_, mixer)| mixer.hardware.device_type == DeviceType::Full)
            .map(|(serial, mixer)| read_bands(serial, mixer))
            .collect::<Result<Vec<_>>>()?;
        devices.sort_by(|a, b| a.key.serial.cmp(&b.key.serial));
        for device in &mut devices {
            if device.firmware == READBACK_FIRMWARE {
                for (index, band) in device.bands.iter_mut().enumerate() {
                    use goxlr_types::MicEqField::{Frequency, Gain, Width};
                    let number = (index + 1) as u8;
                    let frequency = client
                        .read_mic_eq_parameter(&device.key.serial, number, Frequency)
                        .await?;
                    let gain = client
                        .read_mic_eq_parameter(&device.key.serial, number, Gain)
                        .await?;
                    let width = client
                        .read_mic_eq_parameter(&device.key.serial, number, Width)
                        .await?;
                    ensure!(
                        (0..=240).contains(&frequency)
                            && (-12..=12).contains(&gain)
                            && (0..=32).contains(&width),
                        "DSP EQ response is out of range"
                    );
                    band.hz = 20. * 2_f32.powf(frequency as f32 / 24.);
                    band.gain = gain as i8;
                    band.raw = Some(width as u8);
                }
            }
        }
        Ok(devices)
    })
    .await
    .context("Daemon status timed out")?
}

pub async fn apply(expected: Live, index: usize, edit: Edit) -> Result<Live> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut client = connect().await?;
        client.poll_status().await?;
        let serial = &expected.key.serial;
        let mixer = client
            .status()
            .mixers
            .get(serial)
            .filter(|mixer| mixer.hardware.device_type == DeviceType::Full)
            .context("Full-size GoXLR disconnected")?;
        let current = read_bands(serial, mixer)?;
        if current.firmware == READBACK_FIRMWARE {
            use goxlr_types::MicEqField::{Frequency, Gain, Width};
            ensure!(
                NATIVE_CONTROLS_VERIFIED,
                "Physical validation of native controls is incomplete"
            );
            ensure!(
                current.key == expected.key && current.firmware == expected.firmware,
                "The active device or microphone profile changed"
            );
            let band = expected.bands.get(index).context("Invalid EQ band")?;
            let (field, raw, expected_raw) = match edit {
                Edit::Frequency(hz) => {
                    ensure!(
                        hz.is_finite() && (20.0..=20480.0).contains(&hz),
                        "Frequency is out of range"
                    );
                    (
                        Frequency,
                        (24. * (hz / 20.).log2()).round() as i32,
                        (24. * (band.hz / 20.).log2()).round() as i32,
                    )
                }
                Edit::Gain(gain) => {
                    ensure!((-12..=12).contains(&gain), "Gain is out of range");
                    (Gain, i32::from(gain), i32::from(band.gain))
                }
                Edit::Width(width) => {
                    ensure!(width <= 32, "Width is out of range");
                    (
                        Width,
                        i32::from(width),
                        i32::from(band.raw.context("Missing DSP width")?),
                    )
                }
            };
            let number = (index + 1) as u8;
            client
                .command(
                    serial,
                    GoXLRCommand::SetMicEqParameter {
                        band: number,
                        field,
                        raw,
                        expected_raw,
                        profile_name: expected.key.name.clone(),
                    },
                )
                .await?;
            let applied = client.read_mic_eq_parameter(serial, number, field).await?;
            ensure!(
                applied == raw,
                "DSP value changed after the command; refresh before retrying"
            );
            let mut updated = expected;
            match field {
                Frequency => updated.bands[index].hz = 20. * 2_f32.powf(raw as f32 / 24.),
                Gain => updated.bands[index].gain = raw as i8,
                Width => updated.bands[index].raw = Some(raw as u8),
            }
            return Ok(updated);
        }
        let command = stock_command(&expected, &current, index, edit)?;
        client.command(serial, command).await?;
        client
            .poll_status()
            .await
            .context("Command acknowledged. Refresh to confirm daemon state")?;
        let mixer = client
            .status()
            .mixers
            .get(serial)
            .context("GoXLR disconnected after command")?;
        let updated = read_bands(serial, mixer)?;
        let applied = updated.bands[index];
        let matches_edit = match edit {
            Edit::Frequency(hz) => applied.hz == hz,
            Edit::Gain(gain) => applied.gain == gain,
            Edit::Width(_) => false,
        };
        ensure!(
            updated.key == expected.key && matches_edit,
            "Command acknowledged but daemon status changed. Refresh before retrying"
        );
        Ok(updated)
    })
    .await
    .context("Daemon timed out. Refresh before retrying to check the current live value")?
}

fn stock_command(
    expected: &Live,
    current: &Live,
    index: usize,
    edit: Edit,
) -> Result<GoXLRCommand> {
    ensure!(
        current.firmware == VERIFIED_FIRMWARE,
        "This control has not passed physical audio testing on the reported firmware"
    );
    let &key = STOCK
        .get(index)
        .context("Only the ten stock bands can be applied")?;
    ensure!(
        current.key == expected.key
            && current.firmware == expected.firmware
            && current.bands[..10] == expected.bands[..10],
        "Active profile or stock EQ changed. Refresh before applying"
    );
    let mut band = current.bands[index];
    let command = match edit {
        Edit::Frequency(hz) => {
            band.hz = hz;
            ensure!(
                (index == 0 || hz >= current.bands[index - 1].hz)
                    && (index == 9 || hz <= current.bands[index + 1].hz),
                "Frequency must stay between the neighboring live stock bands"
            );
            GoXLRCommand::SetEqFreq(key, hz)
        }
        Edit::Gain(gain) => {
            band.gain = gain;
            GoXLRCommand::SetEqGain(key, gain)
        }
        Edit::Width(_) => {
            anyhow::bail!("Stock firmware width has not passed physical validation")
        }
    };
    ensure!(valid(index, band), "Stock EQ value is out of range");
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires physically validated 908 controls and the research daemon; restores every edited field"]
    async fn native_live_controls_round_trip() -> Result<()> {
        let devices = read().await?;
        ensure!(
            devices.len() == 1 && devices[0].firmware == READBACK_FIRMWARE,
            "Expected exactly one experimental 908 device"
        );
        let initial = devices[0].clone();
        let index = 10;
        let band = initial.bands[index];
        for (edit, restore) in [
            (
                Edit::Frequency(band.hz * 2_f32.powf(1. / 24.)),
                Edit::Frequency(band.hz),
            ),
            (
                Edit::Gain(if band.gain < 10 {
                    band.gain + 2
                } else {
                    band.gain - 2
                }),
                Edit::Gain(band.gain),
            ),
            (
                Edit::Width(if band.raw == Some(0) { 16 } else { 0 }),
                Edit::Width(band.raw.context("Missing width")?),
            ),
        ] {
            let changed = apply(initial.clone(), index, edit).await;
            let current = read()
                .await?
                .into_iter()
                .find(|device| device.key == initial.key)
                .context("Device or profile changed during the test")?;
            apply(current, index, restore)
                .await
                .context("Native control restoration failed")?;
            let after = read().await?;
            ensure!(
                after.len() == 1 && after[0].key == initial.key && after[0].bands == initial.bands,
                "The complete DSP baseline was not restored"
            );
            changed?;
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "Requires the experimental 908 device and its research daemon; performs only reads"]
    async fn native_live_readback_snapshot() -> Result<()> {
        let devices = read().await?;
        ensure!(
            devices.len() == 1 && devices[0].firmware == READBACK_FIRMWARE,
            "Expected exactly one experimental 908 device"
        );
        for band in devices[0].bands {
            ensure!(
                (20.0..=20480.1).contains(&band.hz)
                    && (-12..=12).contains(&band.gain)
                    && band.raw.is_some_and(|raw| raw <= 32),
                "Incomplete live EQ snapshot"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn simulated_firmware_readback_over_isolated_ipc() -> Result<()> {
        use goxlr_ipc::{DaemonRequest, DaemonResponse};
        use goxlr_types::MicEqField;
        use interprocess::local_socket::{
            ListenerOptions, tokio::prelude::LocalSocketListener, traits::tokio::Listener,
        };

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let address = if cfg!(windows) {
            format!("@goxlr-native-eq-sim-{unique}").to_ns_name::<GenericNamespaced>()?
        } else {
            std::env::temp_dir()
                .join(format!("goxlr-native-eq-sim-{unique}.sock"))
                .to_fs_name::<GenericFilePath>()?
        };
        let listener: LocalSocketListener = ListenerOptions::new()
            .name(address.clone())
            .create_tokio()?;
        let server = tokio::spawn(async move {
            let mut socket: Socket<DaemonRequest, DaemonResponse> =
                Socket::new(listener.accept().await?);
            for band in 1..=12 {
                for field in [MicEqField::Frequency, MicEqField::Gain, MicEqField::Width] {
                    match socket.read().await.context("Missing simulated request")?? {
                        DaemonRequest::ReadMicEqParameter(
                            serial,
                            received_band,
                            received_field,
                        ) => {
                            ensure!(
                                serial == "fixture"
                                    && received_band == band
                                    && received_field == field,
                                "Unexpected simulated request"
                            );
                        }
                        _ => anyhow::bail!("Unexpected simulated command"),
                    }
                    let raw = match field {
                        MicEqField::Frequency => 174,
                        MicEqField::Gain => -3,
                        MicEqField::Width => 16,
                    };
                    socket.send(DaemonResponse::MicEqParameter(raw)).await?;
                }
            }
            socket.read().await.context("Missing timeout probe")??;
            socket
                .send(DaemonResponse::Error("Simulated DSP timeout".into()))
                .await?;
            socket
                .read()
                .await
                .context("Missing malformed reply probe")??;
            socket.send(DaemonResponse::Ok).await?;
            Ok::<_, anyhow::Error>(())
        });
        let mut client = IPCClient::new(Socket::new(LocalSocketStream::connect(address).await?));
        for band in 1..=12 {
            for (field, expected) in [
                (MicEqField::Frequency, 174),
                (MicEqField::Gain, -3),
                (MicEqField::Width, 16),
            ] {
                ensure!(
                    client.read_mic_eq_parameter("fixture", band, field).await? == expected,
                    "Wrong simulated parameter"
                );
            }
        }
        let error = client
            .read_mic_eq_parameter("fixture", 1, MicEqField::Width)
            .await
            .unwrap_err();
        ensure!(
            error.to_string().contains("Simulated DSP timeout"),
            "Lost firmware error"
        );
        ensure!(
            client
                .read_mic_eq_parameter("fixture", 1, MicEqField::Width)
                .await
                .is_err(),
            "Accepted a malformed reply"
        );
        server.await??;
        Ok(())
    }

    #[test]
    fn only_current_stock_frequency_and_gain_can_be_sent() {
        let live = Live {
            key: ProfileKey {
                serial: "fixture".into(),
                name: "Mic".into(),
            },
            firmware: "1.5.1.907".into(),
            bands: defaults(),
        };
        assert!(matches!(
            stock_command(&live, &live, 5, Edit::Frequency(1500.)),
            Ok(GoXLRCommand::SetEqFreq(_, 1500.))
        ));
        assert!(matches!(
            stock_command(&live, &live, 5, Edit::Gain(-9)),
            Ok(GoXLRCommand::SetEqGain(_, -9))
        ));
        assert!(stock_command(&live, &live, 10, Edit::Gain(1)).is_err());
        assert!(stock_command(&live, &live, 5, Edit::Frequency(2100.)).is_err());
        assert!(stock_command(&live, &live, 5, Edit::Gain(10)).is_err());
        let mut stale = live.clone();
        stale.key.name = "Other".into();
        assert!(stock_command(&stale, &live, 5, Edit::Gain(1)).is_err());
    }

    #[tokio::test]
    #[ignore = "Requires exactly one full-size GoXLR on 907 with the 1 kHz band at +2 dB"]
    async fn native_ipc_gain_round_trip() -> Result<()> {
        let devices = read().await?;
        ensure!(devices.len() == 1, "Expected exactly one full-size GoXLR");
        let baseline = &devices[0];
        let index = 5;
        ensure!(
            baseline.firmware == VERIFIED_FIRMWARE && baseline.bands[index].gain == 2,
            "Unexpected firmware or initial 1 kHz gain"
        );

        let write = apply(baseline.clone(), index, Edit::Gain(3)).await;
        let current = read()
            .await?
            .into_iter()
            .find(|device| device.key == baseline.key)
            .context("Original mic profile is no longer active")?;
        let mut expected = baseline.bands;
        expected[index].gain = 3;
        if current.bands == expected {
            apply(current, index, Edit::Gain(2)).await?;
        } else {
            ensure!(
                current.bands == baseline.bands,
                "Mic EQ changed concurrently"
            );
        }

        let restored = read()
            .await?
            .into_iter()
            .find(|device| device.key == baseline.key)
            .context("Original mic profile is no longer active")?;
        ensure!(
            restored.bands == baseline.bands,
            "Original mic EQ did not restore"
        );
        ensure!(write?.bands == expected, "Test gain did not apply");
        Ok(())
    }
}
