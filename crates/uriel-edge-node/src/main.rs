mod actuator;
mod camera;
mod config;
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
    crypto::EnvelopeSigner,
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

    println!(
        "[caesar] Booting Uriel edge node '{}' | site='{}' | domain='{}' | uplink='{}'",
        settings.node_id, settings.location.site, settings.domain, settings.uplink.mode
    );

    let signer = EnvelopeSigner::from_seed_hex(&settings.ed25519_seed_hex)?;
    let uplink = Uplink::from_config(&settings).await?;

    let sensor_bus = SensorBus::new();
    let (observation_tx, observation_rx) = mpsc::channel::<Observation>(128);
    let (fused_tx, mut fused_rx)         = mpsc::channel::<FusedTrack>(128);

    // Sentinel feed channel — dashboard subscribes to receive live frames,
    // change portraits, and actuator dispatch logs.
    let (sentinel_feed_tx, mut sentinel_feed_rx) = broadcast::channel::<SentinelFeedEvent>(64);

    // Build the ActuatorBus from config — always constructed so LogActuator
    // provides an audit trail even when sentinel is disabled.
    let actuator_bus = Arc::new(ActuatorBus::from_config(
        &settings.actuators,
        settings.sentinel.autonomous_confidence,
        settings.sentinel.min_snapshots,
    ));

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
    while let Some(track) = fused_rx.recv().await {
        let envelope = signer.sign_track(&settings.node_id, &settings.publish_topic, track)?;
        uplink.publish(&envelope).await?;
        published += 1;

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
