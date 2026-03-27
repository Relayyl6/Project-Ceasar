use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, net::TcpStream, sync::Mutex};
use uriel_caesar_core::{io::to_json_line, protocol::SignedEnvelope};

use crate::config::EdgeConfig;

pub struct Uplink {
    inner: UplinkKind,
}

enum UplinkKind {
    Stdout,
    File {
        writer: Arc<Mutex<tokio::fs::File>>,
        _path: PathBuf,
    },
    TcpJsonl {
        addr: String,
    },
}

impl Uplink {
    pub async fn from_config(settings: &EdgeConfig) -> Result<Self> {
        match settings.uplink.mode.as_str() {
            "stdout" => Ok(Self {
                inner: UplinkKind::Stdout,
            }),
            "file" => {
                let path = settings
                    .uplink
                    .file_path
                    .as_ref()
                    .context("uplink.file_path is required when mode=file")?;
                let path = PathBuf::from(path);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .await?;
                Ok(Self {
                    inner: UplinkKind::File {
                        writer: Arc::new(Mutex::new(file)),
                        _path: path,
                    },
                })
            }
            "tcp_jsonl" => {
                let addr = settings
                    .uplink
                    .tcp_addr
                    .as_ref()
                    .context("uplink.tcp_addr is required when mode=tcp_jsonl")?;
                Ok(Self {
                    inner: UplinkKind::TcpJsonl { addr: addr.clone() },
                })
            }
            other => bail!("unsupported uplink mode {:?}", other),
        }
    }

    pub async fn publish(&self, envelope: &SignedEnvelope) -> Result<()> {
        let payload = to_json_line(envelope)?;
        match &self.inner {
            UplinkKind::Stdout => {
                println!("[edge.uplink] {}", payload.trim_end());
            }
            UplinkKind::File { writer, _path: _ } => {
                let mut file = writer.lock().await;
                file.write_all(payload.as_bytes()).await?;
                file.flush().await?;
            }
            UplinkKind::TcpJsonl { addr } => {
                let mut stream = TcpStream::connect(addr)
                    .await
                    .with_context(|| format!("failed to connect to hub at {}", addr))?;
                stream.write_all(payload.as_bytes()).await?;
                stream.shutdown().await?;
            }
        }
        Ok(())
    }
}
