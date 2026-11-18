use crate::client::Client;
use crate::clients::ipc::ipc_socket::Socket;
use crate::{DaemonRequest, DaemonResponse, DaemonStatus, GoXLRCommand, HttpSettings};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;

#[derive(Debug)]
pub struct IPCClient {
    socket: Socket<DaemonResponse, DaemonRequest>,
    status: DaemonStatus,
    http_settings: HttpSettings,
}

impl IPCClient {
    pub fn new(socket: Socket<DaemonResponse, DaemonRequest>) -> Self {
        Self {
            socket,
            status: DaemonStatus::default(),
            http_settings: Default::default(),
        }
    }

    pub async fn read_mic_eq_parameter(
        &mut self,
        serial: &str,
        band: u8,
        field: goxlr_types::MicEqField,
    ) -> Result<i32> {
        match self
            .request(DaemonRequest::ReadMicEqParameter(
                serial.into(),
                band,
                field,
            ))
            .await?
        {
            DaemonResponse::MicEqParameter(raw) => Ok(raw),
            DaemonResponse::Error(error) => Err(anyhow!(error)),
            _ => bail!("Unexpected DSP EQ readback response"),
        }
    }

    async fn request(&mut self, request: DaemonRequest) -> Result<DaemonResponse> {
        self.socket
            .send(request)
            .await
            .context("Failed to send a command to the GoXLR daemon process")?;
        self.socket
            .read()
            .await
            .context("Failed to retrieve the command result from the GoXLR daemon process")?
            .context("Failed to parse the command result from the GoXLR daemon process")
    }
}

#[async_trait]
impl Client for IPCClient {
    async fn send(&mut self, request: DaemonRequest) -> Result<()> {
        let result = self.request(request).await?;
        match result {
            DaemonResponse::Status(status) => {
                self.status = status.clone();
                self.http_settings = status.config.http_settings;
                Ok(())
            }
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error(error) => Err(anyhow!("{}", error)),
            DaemonResponse::MicLevel(_level) => {
                bail!("Received Mic Level as Response, shouldn't happen!");
            }
            DaemonResponse::MicEqParameter(_) => bail!("Unexpected DSP EQ readback response"),
            DaemonResponse::Patch(_patch) => {
                Err(anyhow!("Received Patch as response, shouldn't happen!"))
            }
        }
    }

    async fn poll_status(&mut self) -> Result<()> {
        self.send(DaemonRequest::GetStatus).await
    }

    async fn command(&mut self, serial: &str, command: GoXLRCommand) -> Result<()> {
        self.send(DaemonRequest::Command(serial.to_string(), command))
            .await
    }

    async fn daemon_command(&mut self, command: DaemonRequest) -> Result<()> {
        self.send(command).await
    }

    fn status(&self) -> &DaemonStatus {
        &self.status
    }

    fn http_status(&self) -> &HttpSettings {
        &self.http_settings
    }
}
