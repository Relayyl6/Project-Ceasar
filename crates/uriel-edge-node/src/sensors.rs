use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::{sync::broadcast, task::JoinHandle, time::sleep};
use uriel_caesar_core::io::unix_time_ms;

use crate::{
    camera::{capture_optical_frame, OpticalFrame},
    config::{EdgeConfig, RadarConfig, ThermalConfig},
};

#[derive(Debug, Clone)]
pub struct ThermalFrame {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub camera_id: String,
    pub temperatures_c: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct RadarPoint {
    pub range_m: f32,
    pub azimuth_deg: f32,
    pub radial_velocity_mps: f32,
}

#[derive(Debug, Clone)]
pub struct RadarSweep {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub radar_id: String,
    pub points: Vec<RadarPoint>,
}

#[derive(Clone)]
pub struct SensorBus {
    optical: broadcast::Sender<OpticalFrame>,
    thermal: broadcast::Sender<ThermalFrame>,
    radar: broadcast::Sender<RadarSweep>,
}

impl SensorBus {
    pub fn new() -> Self {
        let (optical, _) = broadcast::channel(64);
        let (thermal, _) = broadcast::channel(64);
        let (radar, _) = broadcast::channel(64);
        Self {
            optical,
            thermal,
            radar,
        }
    }

    pub fn publish_optical(&self, frame: OpticalFrame) {
        let _ = self.optical.send(frame);
    }

    pub fn publish_thermal(&self, frame: ThermalFrame) {
        let _ = self.thermal.send(frame);
    }

    pub fn publish_radar(&self, frame: RadarSweep) {
        let _ = self.radar.send(frame);
    }

    pub fn resubscribe_optical(&self) -> broadcast::Receiver<OpticalFrame> {
        self.optical.subscribe()
    }

    pub fn resubscribe_thermal(&self) -> broadcast::Receiver<ThermalFrame> {
        self.thermal.subscribe()
    }

    pub fn resubscribe_radar(&self) -> broadcast::Receiver<RadarSweep> {
        self.radar.subscribe()
    }
}

pub fn spawn_sources(settings: EdgeConfig, bus: SensorBus) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();

    if settings.optical.enabled {
        let bus_ref = bus.clone();
        let optical = settings.optical.clone();
        handles.push(tokio::spawn(async move {
            let mut sequence = 0u64;
            loop {
                match capture_optical_frame(&optical, sequence).await {
                    Ok(frame) => bus_ref.publish_optical(frame),
                    Err(error) => eprintln!("[edge.optical] capture failed: {error:#}"),
                }
                sequence += 1;
                sleep(Duration::from_millis(optical.frame_interval_ms)).await;
            }
        }));
    }

    if settings.thermal.enabled {
        let bus_ref = bus.clone();
        let thermal = settings.thermal.clone();
        handles.push(tokio::spawn(async move {
            let mut sequence = 0u64;
            loop {
                match capture_thermal_frame(&thermal, sequence).await {
                    Ok(frame) => bus_ref.publish_thermal(frame),
                    Err(error) => eprintln!("[edge.thermal] capture failed: {error:#}"),
                }
                sequence += 1;
                sleep(Duration::from_millis(thermal.frame_interval_ms)).await;
            }
        }));
    }

    if settings.radar.enabled {
        let bus_ref = bus.clone();
        let radar = settings.radar.clone();
        handles.push(tokio::spawn(async move {
            let mut sequence = 0u64;
            loop {
                match capture_radar_sweep(&radar, sequence).await {
                    Ok(frame) => bus_ref.publish_radar(frame),
                    Err(error) => eprintln!("[edge.radar] capture failed: {error:#}"),
                }
                sequence += 1;
                sleep(Duration::from_millis(radar.frame_interval_ms)).await;
            }
        }));
    }

    handles
}

async fn capture_thermal_frame(config: &ThermalConfig, sequence: u64) -> Result<ThermalFrame> {
    match config.mode.as_str() {
        "synthetic" => {
            let cell_count = ((config.width as usize) / 16) * ((config.height as usize) / 16);
            Ok(ThermalFrame {
                sequence,
                timestamp_ms: unix_time_ms(),
                camera_id: config.camera_id.clone(),
                temperatures_c: (0..cell_count)
                    .map(|idx| 23.0 + ((idx as u64 + sequence) % 17) as f32 * 0.7)
                    .collect(),
            })
        }
        "file_json" => {
            let path = config
                .file_path
                .as_ref()
                .context("thermal.file_path is required when mode=file_json")?;
            let raw = tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("failed to read thermal input file {}", path))?;
            let response: ThermalAdapterResponse = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse thermal JSON file {}", path))?;
            Ok(ThermalFrame {
                sequence,
                timestamp_ms: response.timestamp_ms.unwrap_or_else(unix_time_ms),
                camera_id: config.camera_id.clone(),
                temperatures_c: response.temperatures_c,
            })
        }
        "command_json" => {
            let response = run_sensor_command::<ThermalAdapterResponse>(
                config.command_program.as_deref(),
                config.command_args.as_ref(),
                sequence,
            )
            .await?;
            Ok(ThermalFrame {
                sequence,
                timestamp_ms: response.timestamp_ms.unwrap_or_else(unix_time_ms),
                camera_id: config.camera_id.clone(),
                temperatures_c: response.temperatures_c,
            })
        }
        other => bail!("unsupported thermal mode {:?}", other),
    }
}

async fn capture_radar_sweep(config: &RadarConfig, sequence: u64) -> Result<RadarSweep> {
    match config.mode.as_str() {
        "synthetic" => {
            let points = (0..config.point_count)
                .map(|idx| RadarPoint {
                    range_m: 20.0 + ((sequence + idx as u64) % 28) as f32,
                    azimuth_deg: -16.0 + idx as f32 * 0.8,
                    radial_velocity_mps: 2.0 + ((idx as u64 + sequence) % 7) as f32,
                })
                .collect();
            Ok(RadarSweep {
                sequence,
                timestamp_ms: unix_time_ms(),
                radar_id: config.radar_id.clone(),
                points,
            })
        }
        "file_json" => {
            let path = config
                .file_path
                .as_ref()
                .context("radar.file_path is required when mode=file_json")?;
            let raw = tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("failed to read radar input file {}", path))?;
            let response: RadarAdapterResponse = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse radar JSON file {}", path))?;
            Ok(RadarSweep {
                sequence,
                timestamp_ms: response.timestamp_ms.unwrap_or_else(unix_time_ms),
                radar_id: config.radar_id.clone(),
                points: response
                    .points
                    .into_iter()
                    .map(|point| RadarPoint {
                        range_m: point.range_m,
                        azimuth_deg: point.azimuth_deg,
                        radial_velocity_mps: point.radial_velocity_mps,
                    })
                    .collect(),
            })
        }
        "command_json" => {
            let response = run_sensor_command::<RadarAdapterResponse>(
                config.command_program.as_deref(),
                config.command_args.as_ref(),
                sequence,
            )
            .await?;
            Ok(RadarSweep {
                sequence,
                timestamp_ms: response.timestamp_ms.unwrap_or_else(unix_time_ms),
                radar_id: config.radar_id.clone(),
                points: response
                    .points
                    .into_iter()
                    .map(|point| RadarPoint {
                        range_m: point.range_m,
                        azimuth_deg: point.azimuth_deg,
                        radial_velocity_mps: point.radial_velocity_mps,
                    })
                    .collect(),
            })
        }
        other => bail!("unsupported radar mode {:?}", other),
    }
}

async fn run_sensor_command<T>(
    program: Option<&str>,
    args: Option<&Vec<String>>,
    sequence: u64,
) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let program = program.context("sensor command program is required for command_json mode")?;
    let args = args.cloned().unwrap_or_default();
    let mut command = tokio::process::Command::new(program);
    for arg in args {
        command.arg(arg.replace("{sequence}", &sequence.to_string()));
    }
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to execute sensor command {}", program))?;

    if !output.status.success() {
        bail!(
            "sensor command {} exited with {}: {}",
            program,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("failed to parse JSON from sensor command {}", program))
}

#[derive(Debug, Deserialize)]
struct ThermalAdapterResponse {
    timestamp_ms: Option<u64>,
    temperatures_c: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct RadarAdapterResponse {
    timestamp_ms: Option<u64>,
    points: Vec<RadarPointResponse>,
}

#[derive(Debug, Deserialize)]
struct RadarPointResponse {
    range_m: f32,
    azimuth_deg: f32,
    radial_velocity_mps: f32,
}
