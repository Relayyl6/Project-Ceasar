use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    process::Command,
    sync::{broadcast, mpsc},
};
use uriel_caesar_core::protocol::{Modality, Observation};

use crate::{
    camera::{write_frame_to_temp, OpticalFrame},
    config::EdgeConfig,
    sensors::{RadarSweep, ThermalFrame},
};

pub fn spawn_optical_worker(
    settings: EdgeConfig,
    mut rx: broadcast::Receiver<OpticalFrame>,
    tx: mpsc::Sender<Observation>,
) {
    tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            match optical_infer(&settings, frame).await {
                Ok(observation) => {
                    if tx.send(observation).await.is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("[edge.inference.optical] {error:#}"),
            }
        }
    });
}

pub fn spawn_thermal_worker(
    mut rx: broadcast::Receiver<ThermalFrame>,
    tx: mpsc::Sender<Observation>,
) {
    tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            match thermal_infer(frame) {
                Ok(observation) => {
                    if tx.send(observation).await.is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("[edge.inference.thermal] {error:#}"),
            }
        }
    });
}

pub fn spawn_radar_worker(mut rx: broadcast::Receiver<RadarSweep>, tx: mpsc::Sender<Observation>) {
    tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            match radar_infer(frame) {
                Ok(observation) => {
                    if tx.send(observation).await.is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("[edge.inference.radar] {error:#}"),
            }
        }
    });
}

async fn optical_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    match settings.inference.mode.as_str() {
        "heuristic" => heuristic_optical_infer(frame),
        "command_json" => command_optical_infer(settings, frame).await,
        other => bail!("unsupported inference mode {:?}", other),
    }
}

fn heuristic_optical_infer(frame: OpticalFrame) -> Result<Observation> {
    let sample = frame
        .jpeg_bytes
        .iter()
        .take(256)
        .map(|value| *value as u64)
        .sum::<u64>() as f32
        / 256.0;
    let confidence = ((sample / 255.0) + 0.32).clamp(0.3, 0.96);
    Ok(Observation {
        track_hint: format!("track-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Optical,
        confidence,
        class_label: "vehicle".into(),
        position_m: (
            32.0 + (frame.sequence % 14) as f32,
            11.0 + (frame.sequence % 8) as f32,
        ),
        velocity_mps: Some(4.0 + (frame.sequence % 4) as f32),
        source_id: frame.camera_id,
        evidence_digest: blake3::hash(&frame.jpeg_bytes).to_hex().to_string(),
    })
}

async fn command_optical_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    let path = write_frame_to_temp(&frame)?;
    let program = settings
        .inference
        .command_program
        .as_ref()
        .context("inference.command_program is required when mode=command_json")?;
    let args = settings.inference.command_args.clone().unwrap_or_default();
    let request = DetectorCommandRequest {
        frame_path: path.clone(),
        camera_id: frame.camera_id.clone(),
        timestamp_ms: frame.timestamp_ms,
        width: frame.width,
        height: frame.height,
        sequence: frame.sequence,
    };

    let mut command = Command::new(program);
    for arg in args {
        command.arg(arg);
    }
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch inference command {}", program))?;

    if let Some(mut stdin) = child.stdin.take() {
        let payload = serde_json::to_vec(&request)?;
        use tokio::io::AsyncWriteExt;
        stdin.write_all(&payload).await?;
    }

    let output = child.wait_with_output().await?;
    let _ = std::fs::remove_file(&path);

    if !output.status.success() {
        bail!(
            "inference command {} exited with {}: {}",
            program,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let response: DetectorCommandResponse =
        serde_json::from_slice(&output.stdout).context("failed to parse detector JSON output")?;
    Ok(Observation {
        track_hint: response.track_hint,
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Optical,
        confidence: response.confidence,
        class_label: response.class_label,
        position_m: response.position_m,
        velocity_mps: response.velocity_mps,
        source_id: frame.camera_id,
        evidence_digest: response
            .evidence_digest
            .unwrap_or_else(|| blake3::hash(&frame.jpeg_bytes).to_hex().to_string()),
    })
}

fn thermal_infer(frame: ThermalFrame) -> Result<Observation> {
    let peak_temp = frame
        .temperatures_c
        .iter()
        .copied()
        .fold(f32::MIN, f32::max);
    let bytes: Vec<u8> = frame
        .temperatures_c
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();

    Ok(Observation {
        track_hint: format!("track-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Thermal,
        confidence: ((peak_temp - 20.0) / 18.0).clamp(0.35, 0.93),
        class_label: if peak_temp > 33.0 {
            "hot-vehicle"
        } else {
            "warm-object"
        }
        .into(),
        position_m: (
            31.0 + (frame.sequence % 15) as f32,
            10.5 + (frame.sequence % 7) as f32,
        ),
        velocity_mps: Some(3.5 + (frame.sequence % 3) as f32),
        source_id: frame.camera_id,
        evidence_digest: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn radar_infer(frame: RadarSweep) -> Result<Observation> {
    let mean_range =
        frame.points.iter().map(|point| point.range_m).sum::<f32>() / frame.points.len() as f32;
    let mean_velocity = frame
        .points
        .iter()
        .map(|point| point.radial_velocity_mps)
        .sum::<f32>()
        / frame.points.len() as f32;
    let serialized = frame
        .points
        .iter()
        .flat_map(|point| {
            let mut bytes = Vec::with_capacity(12);
            bytes.extend_from_slice(&point.range_m.to_le_bytes());
            bytes.extend_from_slice(&point.azimuth_deg.to_le_bytes());
            bytes.extend_from_slice(&point.radial_velocity_mps.to_le_bytes());
            bytes
        })
        .collect::<Vec<u8>>();

    Ok(Observation {
        track_hint: format!("track-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Radar,
        confidence: 0.66 + ((frame.sequence % 10) as f32 / 40.0),
        class_label: "moving-object".into(),
        position_m: (mean_range, 12.0 + (frame.sequence % 5) as f32),
        velocity_mps: Some(mean_velocity),
        source_id: frame.radar_id,
        evidence_digest: blake3::hash(&serialized).to_hex().to_string(),
    })
}

#[derive(Debug, Serialize)]
struct DetectorCommandRequest {
    frame_path: PathBuf,
    camera_id: String,
    timestamp_ms: u64,
    width: u32,
    height: u32,
    sequence: u64,
}

#[derive(Debug, Deserialize)]
struct DetectorCommandResponse {
    track_hint: String,
    confidence: f32,
    class_label: String,
    position_m: (f32, f32),
    velocity_mps: Option<f32>,
    evidence_digest: Option<String>,
}
