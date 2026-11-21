//! Reads Boot2 or Golden twice through the USB-owning daemon and saves matching bytes.
use anyhow::{Context, Result, ensure};
use goxlr_ipc::{
    client::Client,
    clients::ipc::{ipc_client::IPCClient, ipc_socket::Socket},
};
use goxlr_types::{DeviceType, RecoveryPartition};
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ToFsName, ToNsName, tokio::prelude::LocalSocketStream,
    traits::tokio::Stream,
};
use std::{env, fs, io::Write, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 2, "Pass boot2|golden and a new output file");
    let partition = match args[0].as_str() {
        "boot2" => RecoveryPartition::Boot2,
        "golden" => RecoveryPartition::Golden,
        _ => anyhow::bail!("Partition must be boot2 or golden"),
    };
    let output = PathBuf::from(&args[1]);
    ensure!(!output.exists(), "Output already exists");
    let name = if cfg!(windows) {
        "@goxlr.socket".to_ns_name::<GenericNamespaced>()?
    } else {
        "/tmp/goxlr.socket".to_fs_name::<GenericFilePath>()?
    };
    let mut client = IPCClient::new(Socket::new(LocalSocketStream::connect(name).await?));
    client.poll_status().await?;
    let devices = client
        .status()
        .mixers
        .iter()
        .filter(|(_, mixer)| mixer.hardware.device_type == DeviceType::Full)
        .map(|(serial, _)| serial.clone())
        .collect::<Vec<_>>();
    ensure!(devices.len() == 1, "Expected exactly one full-size GoXLR");
    let mut bytes = Vec::with_capacity(partition.length() as usize);
    for offset in (0..partition.length()).step_by(1024) {
        let length = 1024.min(partition.length() - offset);
        let first = client
            .read_recovery_partition(&devices[0], partition, offset, length)
            .await
            .with_context(|| format!("Partition read at {offset:#x} failed"))?;
        let second = client
            .read_recovery_partition(&devices[0], partition, offset, length)
            .await?;
        ensure!(
            first == second && first.len() == length as usize,
            "Partition read was inconsistent"
        );
        bytes.extend_from_slice(&first);
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?
        .write_all(&bytes)?;
    println!(
        "Saved {} matching bytes to {}",
        bytes.len(),
        output.display()
    );
    Ok(())
}
