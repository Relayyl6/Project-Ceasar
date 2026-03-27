mod camera;
mod config;
mod fusion;
mod inference;
mod sensors;
mod uplink;

use anyhow::Result;
use clap::Parser;
use config::EdgeConfig;
use fusion::FusionEngine;
use inference::{spawn_optical_worker, spawn_radar_worker, spawn_thermal_worker};
use sensors::{spawn_sources, SensorBus};
use tokio::sync::mpsc;
use uplink::Uplink;
use uriel_caesar_core::{
    crypto::EnvelopeSigner,
    io::read_toml,
    protocol::{FusedTrack, Observation},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Uriel edge node")]
struct Cli {
    #[arg(long, default_value = "configs/edge-dev.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings: EdgeConfig = read_toml(&cli.config)?;

    println!(
        "Booting Uriel edge node {} at {} with uplink {}",
        settings.node_id, settings.location.site, settings.uplink.mode
    );

    let signer = EnvelopeSigner::from_seed_hex(&settings.ed25519_seed_hex)?;
    let uplink = Uplink::from_config(&settings).await?;

    let sensor_bus = SensorBus::new();
    let (observation_tx, observation_rx) = mpsc::channel::<Observation>(128);
    let (fused_tx, mut fused_rx) = mpsc::channel::<FusedTrack>(128);

    let _sensor_tasks = spawn_sources(settings.clone(), sensor_bus.clone());
    spawn_optical_worker(
        settings.clone(),
        sensor_bus.resubscribe_optical(),
        observation_tx.clone(),
    );
    spawn_thermal_worker(sensor_bus.resubscribe_thermal(), observation_tx.clone());
    spawn_radar_worker(sensor_bus.resubscribe_radar(), observation_tx.clone());
    FusionEngine::spawn(settings.clone(), observation_rx, fused_tx);

    let mut published = 0usize;
    while let Some(track) = fused_rx.recv().await {
        let envelope = signer.sign_track(&settings.node_id, &settings.publish_topic, track)?;
        uplink.publish(&envelope).await?;
        published += 1;

        if settings.loop_count != 0 && published >= settings.loop_count {
            println!(
                "Published {} fused tracks; stopping due to loop_count.",
                published
            );
            break;
        }
    }

    Ok(())
}
