use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

use uriel_caesar_core::{crypto::verify_envelope, protocol::{SignedEnvelope, ControlPayload}};
use rumqttc::{AsyncClient, MqttOptions, QoS};

use crate::{config::HubConfig, store::HubStore};

pub async fn run_server(config: HubConfig) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.listen_addr))?;
    let store = HubStore::from_config(&config).await?;
    let trusted_keys = Arc::new(config.trusted_public_keys.clone().unwrap_or_default());

    println!(
        "Caesar hub listening on {} | journal → {} | high-interest → {}",
        config.listen_addr,
        store.high_interest_path().display(),
        config.storage.journal_path,
    );

    let trusted_mode = config.trusted_public_keys.as_ref().map_or(false, |k| !k.is_empty());
    if trusted_mode {
        println!("[caesar.hub] Allowlist mode: only configured trusted_public_keys will be accepted.");
    } else {
        println!("[caesar.hub] Open mode: accepting envelopes from any node (no trusted_public_keys configured).");
    }

    let recent_tracks = Arc::new(tokio::sync::Mutex::new(std::collections::VecDeque::<String>::with_capacity(1000)));

    let recent_tracks_http = recent_tracks.clone();
    tokio::spawn(async move {
        let http_listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[caesar.hub] failed to bind HTTP replay API: {}", e);
                return;
            }
        };
        println!("[caesar.hub] HTTP replay API listening on 0.0.0.0:8080");
        loop {
            if let Ok((mut socket, _)) = http_listener.accept().await {
                let tracks = recent_tracks_http.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    if let Ok(n) = socket.read(&mut buf).await {
                        let req = String::from_utf8_lossy(&buf[..n]);
                        if req.starts_with("GET /api/recent_tracks") {
                            let locked = tracks.lock().await;
                            let items: Vec<String> = locked.iter().cloned().collect();
                            let json = format!("[{}]", items.join(","));
                            let response = format!(
                                "HTTP/1.1 200 OK\r\n\
                                Content-Type: application/json\r\n\
                                Content-Length: {}\r\n\
                                Connection: close\r\n\
                                Access-Control-Allow-Origin: *\r\n\
                                \r\n\
                                {}",
                                json.len(),
                                json
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = socket.write_all(response.as_bytes()).await;
                        }
                    }
                });
            }
        }
    });

    // ── MQTT Command Gateway ──────────────────────────────────────────────────
    // The hub now acts as a secure proxy, ingesting ControlPayloads from the Orchestrator,
    // validating their geofences, and forwarding them to Edge Nodes via MQTT.
    let mut mqttoptions = MqttOptions::new("caesar-hub-command-gateway", "127.0.0.1", 1883);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    
    tokio::spawn(async move {
        loop {
            if let Err(e) = eventloop.poll().await {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    });

    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[caesar.hub] accept error: {e:#}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        let store_ref = store.clone();
        let trusted_keys_ref = trusted_keys.clone();
        let mqtt_client_ref = mqtt_client.clone();
        let recent_tracks_ref = recent_tracks.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_socket(socket, store_ref, trusted_keys_ref, mqtt_client_ref, recent_tracks_ref).await {
                eprintln!("[caesar.hub] peer {}: {error:#}", peer);
            }
        });
    }
}

async fn handle_socket(
    socket: TcpStream,
    store: HubStore,
    trusted_keys: Arc<Vec<String>>,
    mqtt_client: AsyncClient,
    recent_tracks: Arc<tokio::sync::Mutex<std::collections::VecDeque<String>>>,
) -> Result<()> {
    let mut reader = BufReader::new(socket);
    let mut line = String::with_capacity(4096);

    loop {
        line.clear();
        // read_line correctly handles partial TCP delivery — it accumulates bytes
        // internally until it finds a '\n' or EOF, with no manual buffer management.
        let n = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await?;
        if n == 0 {
            break; // EOF — peer closed connection
        }

        // Guard against a malformed line that never contains '\n' and would grow unbounded.
        if line.len() > 8 * 1024 * 1024 {
            anyhow::bail!("payload too large (>{} bytes)", 8 * 1024 * 1024);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try parsing as SignedEnvelope (Telemetry)
        if let Ok(envelope) = serde_json::from_str::<SignedEnvelope>(trimmed) {
            if let Err(e) = verify_envelope(&envelope) {
                eprintln!("[caesar.hub] signature verification failed: {e:#}");
                continue;
            }
            if !trusted_keys.is_empty() && !trusted_keys.iter().any(|key| key == &envelope.public_key) {
                eprintln!("[caesar.hub] Rejected envelope from unknown public key {}", envelope.public_key);
                continue;
            }

            {
                let mut rt = recent_tracks.lock().await;
                if rt.len() >= 1000 {
                    rt.pop_front();
                }
                rt.push_back(trimmed.to_string());
            }

            if let Err(e) = store.persist(envelope).await {
                eprintln!("[caesar.hub] failed to persist envelope: {e:#}");
            }
            continue;
        }

        // Try parsing as ControlPayload (Autonomous Response)
        if let Ok(control) = serde_json::from_str::<ControlPayload>(trimmed) {
            // STRICT VALIDATION BOUNDARY
            if let Err(err) = control.validate_geofence() {
                eprintln!("[caesar.hub][SECURITY] Dropped unsafe control payload: {}", err);
                continue;
            }
            
            let payload_bytes = serde_json::to_vec(&control).unwrap();
            
            for ptz in &control.ptz_commands {
                let topic = format!("caesar/commands/{}", ptz.camera_id);
                if let Err(e) = mqtt_client.publish(topic, QoS::AtLeastOnce, false, payload_bytes.clone()).await {
                    eprintln!("[caesar.hub] failed to forward PTZ command: {e}");
                }
            }
            for drone in &control.drone_commands {
                let topic = format!("caesar/commands/{}", drone.drone_id);
                if let Err(e) = mqtt_client.publish(topic, QoS::AtLeastOnce, false, payload_bytes.clone()).await {
                    eprintln!("[caesar.hub] failed to forward drone command: {e}");
                }
            }
            continue;
        }

        eprintln!("[caesar.hub] failed to parse payload as SignedEnvelope or ControlPayload");
    }
    Ok(())
}
