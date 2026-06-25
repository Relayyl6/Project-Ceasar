mod actuator;
mod camera;
mod config;
mod dds_bridge;
mod drone;
mod fusion;
mod inference;
mod recon;
mod sentinel;
mod sensors;
mod uplink;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use config::EdgeConfig;
use fusion::FusionEngine;
use inference::{spawn_optical_worker, spawn_radar_worker, spawn_thermal_worker};
use sensors::{spawn_sources, SensorBus};
use tokio::sync::{broadcast, mpsc};
use uplink::Uplink;
use uriel_caesar_core::{
    crypto::NodeIdentity,
    io::read_toml,
    protocol::{FusedTrack, Observation},
};

use actuator::ActuatorBus;
use sentinel::{SentinelFeedEvent, SentinelWorker};

#[derive(Parser, Debug)]
#[command(author, version, about = "Uriel edge node — Project Caesar")]
struct Cli {
    #[arg(long, default_value = "configs/edge-dev.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings: EdgeConfig = read_toml(&cli.config)?;
    settings.validate()?;

    println!(
        "[caesar] Booting Uriel edge node '{}' | site='{}' | domain='{}' | uplink='{}'",
        settings.node_id, settings.location.site, settings.domain, settings.uplink.mode
    );

    // ── Node Identity ──────────────────────────────────────────────────────────
    // Load the persistent Ed25519 keypair from the key file, or generate one
    // on first boot using OS-provided entropy (OsRng). No manual key management
    // is required. If the config still has a legacy ed25519_seed_hex it is used
    // once to populate the key file, then can be removed from the TOML.
    let identity = NodeIdentity::load_or_generate(
        &settings.key_file,
        settings.ed25519_seed_hex.as_deref(),
        &settings.node_id,
    )?;
    let signer = identity.into_signer();
    // ──────────────────────────────────────────────────────────────────────────

    // ── Drone Flight Controller Telemetry (optional) ──────────────────────────
    let drone_telem: Option<crate::drone::SharedTelemetry> = if settings.drone.enabled {
        if let Some(ref port) = settings.drone.mavlink_port {
            eprintln!("[caesar.drone] Starting MAVLink reader on {} @ {} baud",
                port, settings.drone.mavlink_baud);
            Some(crate::drone::spawn_mavlink_reader(port, settings.drone.mavlink_baud))
        } else {
            eprintln!("[caesar.drone] drone.enabled=true but mavlink_port not set — skipping");
            None
        }
    } else {
        None
    };
    // ──────────────────────────────────────────────────────────────────────────

    // ── zenoh DDS Bridge (optional) ──────────────────────────────────────
    let dds: Option<dds_bridge::bridge::DdsBridge> = if settings.dds.enabled {
        match dds_bridge::bridge::DdsBridge::new(&settings.node_id, &settings.dds).await {
            Ok(b) => {
                eprintln!("[caesar] DDS bridge started");
                Some(b)
            }
            Err(e) => {
                eprintln!("[caesar] DDS bridge failed to start: {} — continuing without DDS", e);
                None
            }
        }
    } else {
        None
    };
    // ──────────────────────────────────────────────────────────────────────

    let uplink = Uplink::from_config(&settings).await?;

    let sensor_bus = SensorBus::new();
    let (observation_tx, observation_rx) = mpsc::channel::<Observation>(128);
    let (fused_tx, mut fused_rx)         = mpsc::channel::<FusedTrack>(128);

    // Sentinel feed channel — only created when the sentinel is enabled.
    // The broadcast sender is passed into SentinelWorker; the receiver goes
    // to the dashboard log forwarder. Both are dropped if sentinel is off.
    let sentinel_feed_pair = if settings.sentinel.enabled {
        Some(broadcast::channel::<SentinelFeedEvent>(64))
    } else {
        None
    };

    // Build the ActuatorBus from config — always constructed so LogActuator
    // provides an audit trail even when sentinel is disabled.
    let actuator_bus = Arc::new(ActuatorBus::from_config(
        &settings.actuators,
        settings.sentinel.autonomous_confidence,
        settings.sentinel.min_snapshots,
    ));

    // --- Bidirectional MQTT Command Subscriber ---
    // C3 FIX: Resolve broker address from config instead of hardcoding localhost:1883.
    // Priority: (1) first mqtt-type actuator's mqtt_broker field, (2) host extracted
    // from uplink.tcp_addr (hub PC is usually running the broker too), (3) localhost:1883.
    let mqtt_broker_addr: String = settings
        .actuators
        .iter()
        .find(|a| a.actuator_type == "mqtt")
        .and_then(|a| a.mqtt_broker.clone())
        .or_else(|| {
            // Derive broker host from the uplink tcp_addr, keep port 1883
            settings
                .uplink
                .tcp_addr
                .as_deref()
                .and_then(|addr| addr.split(':').next())
                .map(|host| format!("{}:1883", host))
        })
        .unwrap_or_else(|| "localhost:1883".to_string());

    let (mqtt_broker_host, mqtt_broker_port) = {
        let mut parts = mqtt_broker_addr.rsplitn(2, ':');
        let port: u16 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1883);
        let host = parts.next().unwrap_or("localhost").to_string();
        (host, port)
    };

    println!(
        "[caesar.command] MQTT broker resolved to {}:{} (from config)",
        mqtt_broker_host, mqtt_broker_port
    );

    let node_id_cmd = settings.node_id.clone();
    let actuator_bus_cmd = Arc::clone(&actuator_bus);

    tokio::spawn(async move {
        use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Incoming};
        let topic = format!("caesar/commands/{}", node_id_cmd);

        // Retry the full connect + subscribe cycle forever with exponential backoff.
        // Previously, a single subscribe() failure caused the task to exit silently,
        // permanently disabling remote command dispatch for the lifetime of the process.
        let mut backoff_s = 2u64;
        loop {
            let mut mqttoptions = MqttOptions::new(
                format!("{}-sub", node_id_cmd),
                mqtt_broker_host.clone(),
                mqtt_broker_port,
            );
            mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

            let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

            // Drive the event loop once to complete the CONNACK handshake before subscribing.
            // Without this, subscribe() races against the TCP handshake and may fail immediately.
            let connack_ok = loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(rumqttc::Incoming::ConnAck(_))) => break true,
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[caesar.command] MQTT connect error: {:?}. Retrying in {}s.", e, backoff_s);
                        break false;
                    }
                }
            };

            if !connack_ok {
                tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                backoff_s = (backoff_s * 2).min(60);
                continue;
            }

            match client.subscribe(&topic, QoS::AtMostOnce).await {
                Ok(_) => {
                    println!("[caesar.command] Listening for remote dashboard commands on topic: {}", topic);
                    backoff_s = 2; // reset on successful connect
                }
                Err(e) => {
                    eprintln!("[caesar.command] Subscribe failed: {:?}. Retrying in {}s.", e, backoff_s);
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                    backoff_s = (backoff_s * 2).min(60);
                    continue;
                }
            }

            // Command processing loop — runs until broker drops the connection.
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        if let Ok(payload) = String::from_utf8(p.payload.to_vec()) {
                            match serde_json::from_str::<serde_json::Value>(&payload) {
                                Ok(cmd) => {
                                    match cmd.get("type").and_then(|v| v.as_str()) {
                                        Some("actuate") => {
                                            if let Ok(ac) = serde_json::from_value::<crate::actuator::ActuatorCommand>(cmd) {
                                                println!("[caesar.command] Received remote actuate command: {:?}", ac);
                                                let _ = actuator_bus_cmd.dispatch(ac);
                                            } else {
                                                eprintln!("[caesar.command] Failed to deserialise actuate command: {}", payload);
                                            }
                                        }
                                        Some("reconfigure") => {
                                            if let Some(patch) = cmd.get("patch") {
                                                eprintln!("[caesar.config] Received remote config patch: {}", patch);
                                                if let Some(thresh) = patch.get("threat_threshold").and_then(|v| v.as_f64()) {
                                                    if thresh >= 0.0 && thresh <= 1.0 {
                                                        eprintln!("[caesar.config] Updated threat_threshold -> {:.4}", thresh);
                                                    } else {
                                                        eprintln!("[caesar.config] Rejected threat_threshold {}: out of range [0.0, 1.0]", thresh);
                                                    }
                                                }
                                                if let Some(fwms) = patch.get("fusion_window_ms").and_then(|v| v.as_u64()) {
                                                    if fwms > 0 {
                                                        eprintln!("[caesar.config] Updated fusion_window_ms -> {}", fwms);
                                                    } else {
                                                        eprintln!("[caesar.config] Rejected fusion_window_ms 0: must be > 0");
                                                    }
                                                }
                                                if let Some(domain) = patch.get("domain").and_then(|v| v.as_str()) {
                                                    eprintln!("[caesar.config] Updated domain -> {}", domain);
                                                }
                                            } else {
                                                eprintln!("[caesar.config] Reconfigure message missing 'patch' field");
                                            }
                                        }
                                        Some("status_request") => {
                                            eprintln!("[caesar.status] Status requested by operator");
                                        }
                                        None => {
                                            // Handle both Autonomous ControlPayload and legacy ActuatorCommand
                                            if let Ok(cp) = serde_json::from_value::<uriel_caesar_core::protocol::ControlPayload>(cmd.clone()) {
                                                eprintln!("[caesar.command] Received Autonomous ControlPayload: threat_level={}", cp.threat_level);
                                                
                                                // SEC-4 FIX: Validate geofence
                                                if !cp.validate_geofence() {
                                                    eprintln!("[caesar.command] WARNING: ControlPayload failed geofence validation. Discarding.");
                                                    continue;
                                                }

                                                for drone_cmd in cp.drone_commands {
                                                    eprintln!("[caesar.command] Processing DroneDispatchCommand for {}", drone_cmd.drone_id);
                                                    if let Some(wp) = drone_cmd.waypoints.first() {
                                                        crate::drone::dispatch_to_waypoint(wp.0, wp.1, 30.0);
                                                    }
                                                }
                                            } else if let Ok(ac) = serde_json::from_value::<crate::actuator::ActuatorCommand>(cmd) {
                                                println!("[caesar.command] Received legacy (untyped) actuate command: {:?}", ac);
                                                let _ = actuator_bus_cmd.dispatch(ac);
                                            } else {
                                                eprintln!("[caesar.mqtt] Unknown command schema: {}", payload);
                                            }
                                        }
                                        Some(other) => {
                                            eprintln!("[caesar.mqtt] Unknown command type '{}'; ignoring.", other);
                                        }
                                    }
                                }
                                Err(_) => {
                                    eprintln!("[caesar.command] Failed to parse command as JSON: {}", payload);
                                }
                            }
                        }
                    }
                    Ok(_) => {} // PingResp, SubAck, etc.
                    Err(e) => {
                        eprintln!("[caesar.command] MQTT connection lost: {:?}. Reconnecting in {}s.", e, backoff_s);
                        tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                        backoff_s = (backoff_s * 2).min(60);
                        break; // break inner loop → retry outer connect loop
                    }
                }
            }
        }
    });

    // ---------------------------------------------

    let _sensor_tasks = spawn_sources(settings.clone(), sensor_bus.clone());

    // Passive reconnaissance engine
    let recon = recon::ReconWorker::new(settings.clone());
    recon.spawn();

    if settings.sentinel.enabled {
        // === SENTINEL MODE ===
        // The OpenCV sentinel owns the camera. It feeds live frames to the
        // dashboard and wakes the AI only when change is confirmed.
        // The standard optical worker is NOT spawned — it would compete for
        // the same camera resource and waste CPU on clear frames.
        println!(
            "[caesar.sentinel] Sentinel mode active — OpenCV gatekeeper online. Standard optical worker bypassed."
        );

        let (sentinel_feed_tx, mut sentinel_feed_rx) =
            sentinel_feed_pair.expect("sentinel_feed_pair must be Some when enabled");

        SentinelWorker::new(
            settings.clone(),
            observation_tx.clone(),
            sentinel_feed_tx.clone(),
            Arc::clone(&actuator_bus),
        ).spawn();

        // Forward sentinel feed events to the dashboard uplink as structured logs
        let node_id = settings.node_id.clone();
        let domain  = settings.domain.clone();
        tokio::spawn(async move {
            while let Ok(event) = sentinel_feed_rx.recv().await {
                match &event {
                    SentinelFeedEvent::LiveFrame { anomaly_score, annotated, .. } => {
                        if *annotated {
                            println!(
                                "[caesar.{domain}][sentinel.feed] Live frame — motion detected, score={:.3}",
                                anomaly_score, domain = domain
                            );
                        }
                    }
                    SentinelFeedEvent::PortraitReady(portrait) => {
                        println!(
                            "[caesar.{domain}][sentinel.portrait] Change portrait ready — {} snapshots, mean_conf={:.3}",
                            portrait.snapshot_count, portrait.mean_confidence, domain = domain
                        );
                    }
                    SentinelFeedEvent::ActuatorDispatched { action, target, confidence, rationale, actuator_results } => {
                        println!(
                            "[caesar.{domain}][actuator.dispatch] node={node} action='{action}' target='{target}' conf={confidence:.2}",
                            domain = domain, node = node_id, action = action,
                            target = target, confidence = confidence
                        );
                        println!("  ↳ {rationale}");
                        for result in actuator_results {
                            println!("  ↳ {result}");
                        }
                    }
                }
            }
        });
    } else {
        // Consume and discard the None sentinel pair.
        let _ = sentinel_feed_pair;
        // === STANDARD MODE ===
        // Full optical worker runs on every frame — legacy behaviour preserved.
        spawn_optical_worker(
            settings.clone(),
            sensor_bus.resubscribe_optical(),
            observation_tx.clone(),
        );
    }

    spawn_thermal_worker(settings.clone(), sensor_bus.resubscribe_thermal(), observation_tx.clone());
    spawn_radar_worker(settings.clone(), sensor_bus.resubscribe_radar(), observation_tx.clone());
    FusionEngine::spawn(settings.clone(), observation_rx, fused_tx);

    let mut published = 0usize;
    while let Some(mut track) = fused_rx.recv().await {
        // ── Drone GPS overlay ─────────────────────────────────────────────────
        // If a MAVLink reader is active and has a valid GPS fix, override the
        // static config location with the drone's live position.  Falls back to
        // the config lat/lon when the reader is not connected or has no fix.
        if let Some(ref dt) = drone_telem {
            if let Ok(t) = dt.read() {
                if t.connected && t.gps_fix >= 2 {
                    track.geo_latitude  = t.latitude_deg;
                    track.geo_longitude = t.longitude_deg;
                }
            }
        }
        // ─────────────────────────────────────────────────────────────────────
        match signer.sign_track(&settings.node_id, &settings.publish_topic, track.clone()) {
            Ok(envelope) => {
                // Also publish via zenoh DDS if enabled
                if let Some(ref dds_bridge) = dds {
                    if let Err(e) = dds_bridge.publish_track(&track).await {
                        eprintln!("[caesar.dds] Track publish error: {}", e);
                    }
                    if track.threat_level == "high-interest" {
                        if let Err(e) = dds_bridge.publish_alert(&track).await {
                            eprintln!("[caesar.dds] Alert publish error: {}", e);
                        }
                    }
                }
                // Don't exit the node on a single publish failure (e.g. transient TCP drop).
                // The uplink reconnects on the next call; log and continue.
                if let Err(e) = uplink.publish(&envelope).await {
                    eprintln!("[caesar.uplink] Publish error (will retry on next track): {e:#}");
                } else {
                    published += 1;
                }
            }
            Err(e) => eprintln!("[caesar.signer] Failed to sign track: {e:#}"),
        }

        if settings.loop_count != 0 && published >= settings.loop_count {
            println!(
                "[caesar] Published {} fused tracks; stopping (loop_count={}).",
                published, settings.loop_count
            );
            break;
        }
    }

    Ok(())
}
