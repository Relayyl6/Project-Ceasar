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
        /// F11 FIX: The specific serial port to use for radio uplink.
        /// Must be explicitly set in config (uplink.serial_port).
        /// None = no radio, stdout-only logging.
        serial_port: Option<String>,
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
                    if !parent.as_os_str().is_empty() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
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
                        serial_port: settings.uplink.serial_port.clone(),
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
            UplinkKind::Gossipsub { topic, serial_port } => {
                // Here libp2p Swarm::behaviour_mut().gossipsub.publish happens
                println!("[edge.uplink.gossipsub] Broadcasting to topic {}: {}", topic, payload.trim_end());

                // F11 FIX: Only write to the explicitly configured serial_port.
                // The previous code scanned ALL available serial ports and wrote
                // signed envelopes to whichever one accepted the write first —
                // this is uncontrolled data exfiltration to unknown peripherals.
                // Operators must set uplink.serial_port in their TOML config;
                // if it is absent, the uplink falls back to stdout only.
                if let Some(port_name) = serial_port {
                    match serialport::new(port_name, 57600)
                        .timeout(std::time::Duration::from_millis(100))
                        .open()
                    {
                        Ok(mut port) => {
                            if port.write_all(payload.as_bytes()).is_ok() {
                                println!("[edge.uplink.rfd900x] Sent {} bytes via configured port {}", payload.len(), port_name);
                            } else {
                                eprintln!("[edge.uplink.rfd900x] Write failed on port {}", port_name);
                            }
                        }
                        Err(e) => {
                            eprintln!("[edge.uplink.rfd900x] Failed to open configured port '{}': {} — logging to stdout only", port_name, e);
                            println!("[edge.uplink.gossipsub] (stdout fallback) {}", payload.trim_end());
                        }
                    }
                } else {
                    println!("[edge.uplink.rfd900x] No serial_port configured \u2014 stdout only (set uplink.serial_port in TOML to enable radio).");
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
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            async {
                stream.write_all(payload).await?;
                stream.flush().await
            }
        ).await.unwrap_or_else(|_| Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "write timeout")))
    } else {
        unreachable!("tcp writer must be initialized before publish");
    };

    if write_result.is_ok() {
        return Ok(());
    }

    *guard = None;
    let mut stream = connect_writer(addr).await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            stream.write_all(payload).await?;
            stream.flush().await
        }
    ).await.unwrap_or_else(|_| Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "write timeout")))?;
    *guard = Some(stream);
    Ok(())
}

async fn connect_writer(addr: &str) -> Result<tokio::io::BufWriter<TcpStream>> {
    // Apply a 10-second timeout so a black-hole route can't stall the uplink forever.
    // Connection refused responds immediately; this only guards against silent drops.
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        TcpStream::connect(addr),
    )
    .await
    .with_context(|| format!("connection to hub at {} timed out after 10s", addr))?
    .with_context(|| format!("failed to connect to hub at {}", addr))?;
    Ok(tokio::io::BufWriter::new(stream))
}
