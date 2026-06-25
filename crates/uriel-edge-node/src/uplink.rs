use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use futures::StreamExt as _;
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
        /// Channel to send payloads to the background Swarm task.
        tx: tokio::sync::mpsc::Sender<Vec<u8>>,
        /// Serial port fallback (RFD900x LoRa radio) if swarm publish fails.
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
                let topic = settings.uplink.gossipsub_topic.clone();
                let port = settings.uplink.gossipsub_listen_port;
                let peers = settings.uplink.bootstrap_peers.clone();
                match spawn_gossipsub_swarm(&topic, port, peers).await {
                    Ok(tx) => Ok(Self {
                        inner: UplinkKind::Gossipsub {
                            tx,
                            serial_port: settings.uplink.serial_port.clone(),
                        },
                    }),
                    Err(e) => {
                        eprintln!(
                            "[caesar.gossipsub] Failed to start swarm: {} — falling back to stdout",
                            e
                        );
                        // Graceful fallback: stdout
                        Ok(Self {
                            inner: UplinkKind::Stdout,
                        })
                    }
                }
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
            UplinkKind::Gossipsub { tx, serial_port } => {
                // Send to real libp2p Gossipsub swarm via mpsc channel
                let data = payload.as_bytes().to_vec();
                if let Err(e) = tx.try_send(data) {
                    eprintln!(
                        "[caesar.gossipsub] Channel full or closed: {} — falling back to serial/stdout",
                        e
                    );
                }

                // Optional: also send via RFD900x LoRa radio for redundancy.
                //
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
                            if let Err(e) = port.write_all(payload.as_bytes()) {
                                eprintln!(
                                    "[edge.uplink.rfd900x] Write failed on port {}: {}",
                                    port_name, e
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "[edge.uplink.rfd900x] Failed to open configured port '{}': {} — gossipsub-only",
                                port_name, e
                            );
                        }
                    }
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

/// Spawns a libp2p Gossipsub swarm as a Tokio background task.
/// Returns an mpsc Sender — callers push raw payload bytes through it.
async fn spawn_gossipsub_swarm(
    topic_str: &str,
    listen_port: u16,
    bootstrap_peers: Vec<String>,
) -> anyhow::Result<tokio::sync::mpsc::Sender<Vec<u8>>> {
    use libp2p::{
        gossipsub::{self, Config as GossipConfig, MessageAuthenticity, ValidationMode},
        mdns,
        swarm::SwarmEvent,
        SwarmBuilder,
    };
    use std::time::Duration;

    #[derive(libp2p::swarm::NetworkBehaviour)]
    struct CaesarBehaviour {
        gossipsub: gossipsub::Behaviour,
        mdns: mdns::tokio::Behaviour,
    }

    let gossip_cfg = GossipConfig::builder()
        .validation_mode(ValidationMode::Strict)
        .heartbeat_interval(Duration::from_secs(1))
        .build()
        .map_err(|e| anyhow::anyhow!("gossipsub config error: {}", e))?;

    let topic = gossipsub::IdentTopic::new(topic_str);
    let topic_str_owned = topic_str.to_string();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let gossipsub = gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(key.clone()),
                gossip_cfg,
            )
            .map_err(|e| anyhow::anyhow!("gossipsub behaviour: {}", e))?;
            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())
                    .map_err(|e| anyhow::anyhow!("mdns behaviour: {}", e))?;
            Ok(CaesarBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();

    // Subscribe to the topic
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic)
        .map_err(|e| anyhow::anyhow!("gossipsub subscribe: {:?}", e))?;

    // Listen on all interfaces
    let listen_addr: libp2p::Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", listen_port).parse()?;
    swarm.listen_on(listen_addr)?;

    // Dial bootstrap peers
    for peer_str in &bootstrap_peers {
        match peer_str.parse::<libp2p::Multiaddr>() {
            Ok(addr) => {
                if let Err(e) = swarm.dial(addr.clone()) {
                    eprintln!(
                        "[caesar.gossipsub] Failed to dial bootstrap peer {}: {}",
                        peer_str, e
                    );
                }
            }
            Err(e) => eprintln!(
                "[caesar.gossipsub] Invalid bootstrap peer addr '{}': {}",
                peer_str, e
            ),
        }
    }

    // Spawn the swarm event loop as a background Tokio task.
    // The mpsc channel bridges synchronous publish() calls into the async swarm.
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Outbound: push data received from publish() into gossipsub
                Some(payload) = rx.recv() => {
                    match swarm.behaviour_mut().gossipsub.publish(topic.clone(), payload) {
                        Ok(msg_id) => eprintln!("[caesar.gossipsub] Published message {:?}", msg_id),
                        Err(e) => eprintln!("[caesar.gossipsub] Publish error: {:?}", e),
                    }
                }
                // Inbound: process swarm events
                event = swarm.next() => match event {
                    Some(SwarmEvent::Behaviour(CaesarBehaviourEvent::Mdns(
                        mdns::Event::Discovered(peers)
                    ))) => {
                        for (peer_id, _addr) in peers {
                            eprintln!("[caesar.gossipsub] Discovered peer via mDNS: {}", peer_id);
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                        }
                    }
                    Some(SwarmEvent::Behaviour(CaesarBehaviourEvent::Mdns(
                        mdns::Event::Expired(peers)
                    ))) => {
                        for (peer_id, _addr) in peers {
                            swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                        }
                    }
                    Some(SwarmEvent::Behaviour(CaesarBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    ))) => {
                        eprintln!(
                            "[caesar.gossipsub] Received message on topic '{}'",
                            topic_str_owned
                        );
                        // In future: forward inbound messages to FusionEngine
                        let _ = message;
                    }
                    Some(SwarmEvent::NewListenAddr { address, .. }) => {
                        eprintln!("[caesar.gossipsub] Listening on {}", address);
                    }
                    Some(SwarmEvent::ConnectionEstablished { peer_id, .. }) => {
                        eprintln!("[caesar.gossipsub] Connected to peer {}", peer_id);
                    }
                    Some(SwarmEvent::ConnectionClosed { peer_id, .. }) => {
                        eprintln!("[caesar.gossipsub] Disconnected from peer {}", peer_id);
                    }
                    None => break, // Swarm closed
                    _ => {}
                }
            }
        }
        eprintln!("[caesar.gossipsub] Swarm event loop exited");
    });

    Ok(tx)
}
