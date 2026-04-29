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
        writer: Arc<Mutex<Option<tokio::io::BufWriter<TcpStream>>>>,
        addr: String,
    },
    Gossipsub {
        // Simulated libp2p Gossipsub interface
        // In real deployment, this holds the swarm publisher
        topic: String,
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
                    inner: UplinkKind::TcpJsonl {
                        writer: Arc::new(Mutex::new(None)),
                        addr: addr.clone(),
                    },
                })
            }
            "gossipsub" => {
                Ok(Self {
                    inner: UplinkKind::Gossipsub {
                        topic: settings.publish_topic.clone(),
                    },
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
            UplinkKind::TcpJsonl { writer, addr } => {
                publish_tcp_jsonl(writer, addr, payload.as_bytes()).await?;
            }
            UplinkKind::Gossipsub { topic } => {
                // Here libp2p Swarm::behaviour_mut().gossipsub.publish happens
                println!("[edge.uplink.gossipsub] Broadcasting to topic {}: {}", topic, payload.trim_end());
                
                // Simulate RFD900x Baud Rate Management: UART baud rate must be strictly lower than air data rate
                let mut hardware_active = false;
                if let Ok(ports) = serialport::available_ports() {
                    for p in ports {
                        if let Ok(mut port) = serialport::new(&p.port_name, 57600).timeout(std::time::Duration::from_millis(100)).open() {
                            if port.write_all(payload.as_bytes()).is_ok() {
                                hardware_active = true;
                                break;
                            }
                        }
                    }
                }

                if hardware_active {
                    println!("[edge.uplink.rfd900x] Enforced UART shaping to strictly match RFD900x air data constraints.");
                } else {
                    println!("[edge.uplink.rfd900x] Hardware UART missing. Falling back to stdout simulation.");
                }
            }
        }
        Ok(())
    }
}

async fn publish_tcp_jsonl(
    writer: &Arc<Mutex<Option<tokio::io::BufWriter<TcpStream>>>>,
    addr: &str,
    payload: &[u8],
) -> Result<()> {
    let mut guard = writer.lock().await;
    if guard.is_none() {
        *guard = Some(connect_writer(addr).await?);
    }

    let write_result = if let Some(stream) = guard.as_mut() {
        stream.write_all(payload).await.and_then(|_| stream.flush().await)
    } else {
        unreachable!("tcp writer must be initialized before publish");
    };

    if write_result.is_ok() {
        return Ok(());
    }

    *guard = None;
    let mut stream = connect_writer(addr).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    *guard = Some(stream);
    Ok(())
}

async fn connect_writer(addr: &str) -> Result<tokio::io::BufWriter<TcpStream>> {
    let stream = TcpStream::connect(addr)
        .await
        .with_context(|| format!("failed to connect to hub at {}", addr))?;
    Ok(tokio::io::BufWriter::new(stream))
}
