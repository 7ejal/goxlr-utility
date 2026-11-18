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

#[derive(Clone)]
pub struct Live {
    pub key: ProfileKey,
    pub firmware: String,
    pub bands: Bands,
}

#[derive(Clone, Copy)]
pub enum StockEdit {
    Frequency(f32),
    Gain(i8),
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
    tokio::time::timeout(Duration::from_secs(3), async {
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
        Ok(devices)
    })
    .await
    .context("Daemon status timed out")?
}

pub async fn apply(expected: Live, index: usize, edit: StockEdit) -> Result<Live> {
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
            StockEdit::Frequency(hz) => applied.hz == hz,
            StockEdit::Gain(gain) => applied.gain == gain,
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
    edit: StockEdit,
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
        StockEdit::Frequency(hz) => {
            band.hz = hz;
            ensure!(
                (index == 0 || hz >= current.bands[index - 1].hz)
                    && (index == 9 || hz <= current.bands[index + 1].hz),
                "Frequency must stay between the neighboring live stock bands"
            );
            GoXLRCommand::SetEqFreq(key, hz)
        }
        StockEdit::Gain(gain) => {
            band.gain = gain;
            GoXLRCommand::SetEqGain(key, gain)
        }
    };
    ensure!(valid(index, band), "Stock EQ value is out of range");
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            stock_command(&live, &live, 5, StockEdit::Frequency(1500.)),
            Ok(GoXLRCommand::SetEqFreq(_, 1500.))
        ));
        assert!(matches!(
            stock_command(&live, &live, 5, StockEdit::Gain(-9)),
            Ok(GoXLRCommand::SetEqGain(_, -9))
        ));
        assert!(stock_command(&live, &live, 10, StockEdit::Gain(1)).is_err());
        assert!(stock_command(&live, &live, 5, StockEdit::Frequency(2100.)).is_err());
        assert!(stock_command(&live, &live, 5, StockEdit::Gain(10)).is_err());
        let mut stale = live.clone();
        stale.key.name = "Other".into();
        assert!(stock_command(&stale, &live, 5, StockEdit::Gain(1)).is_err());
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

        let write = apply(baseline.clone(), index, StockEdit::Gain(3)).await;
        let current = read()
            .await?
            .into_iter()
            .find(|device| device.key == baseline.key)
            .context("Original mic profile is no longer active")?;
        let mut expected = baseline.bands;
        expected[index].gain = 3;
        if current.bands == expected {
            apply(current, index, StockEdit::Gain(2)).await?;
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
