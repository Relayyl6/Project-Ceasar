use std::{collections::HashMap, time::Duration};

use tokio::{sync::mpsc, time::interval};
use uriel_caesar_core::protocol::{FusedTrack, Modality, Observation};

use crate::config::EdgeConfig;

pub struct ExtendedKalmanFilter {
    pub state: [f32; 4], // [x, y, vx, vy]
}

impl ExtendedKalmanFilter {
    pub fn new() -> Self {
        Self { state: [0.0; 4] }
    }
    
    pub fn predict(&mut self, dt: f32) {
        // Linear prediction: x = x + vx * dt
        self.state[0] += self.state[2] * dt;
        self.state[1] += self.state[3] * dt;
    }
    
    pub fn update(&mut self, measurement: [f32; 2], confidence: f32) {
        // EKF Update approximation using Kalman Gain derived from confidence
        let gain = confidence.clamp(0.1, 0.9);
        self.state[0] = self.state[0] * (1.0 - gain) + measurement[0] * gain;
        self.state[1] = self.state[1] * (1.0 - gain) + measurement[1] * gain;
    }
}

pub struct FusionEngine {
    settings: EdgeConfig,
    rx: mpsc::Receiver<Observation>,
    tx: mpsc::Sender<FusedTrack>,
}


impl FusionEngine {
    pub fn spawn(
        settings: EdgeConfig,
        rx: mpsc::Receiver<Observation>,
        tx: mpsc::Sender<FusedTrack>,
    ) {
        let engine = Self { settings, rx, tx };
        tokio::spawn(async move {
            engine.run().await;
        });
    }

    async fn run(mut self) {
        let mut buckets: HashMap<String, Vec<Observation>> = HashMap::new();
        let mut ticker = interval(Duration::from_millis(self.settings.fusion_window_ms));
        
        let mut last_frame_time = std::time::Instant::now();
        let mut acoustic_buffer = std::collections::VecDeque::with_capacity(100);

        loop {
            tokio::select! {
                maybe_observation = self.rx.recv() => {
                    match maybe_observation {
                        Some(obs) => {
                            // Guard against non-finite confidence values that would
                            // corrupt EKF state or produce NaN threat levels downstream.
                            if obs.confidence.is_finite() {
                                buckets.entry(obs.track_hint.clone()).or_default().push(obs);
                            } else {
                                eprintln!(
                                    "[edge.fusion] Rejected observation with non-finite confidence \
                                     from source '{}' (value={}). Possible sensor/inference bug.",
                                    obs.source_id, obs.confidence
                                );
                            }
                        },
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    let tracks = self.flush_ready(
                        &mut buckets,
                        &mut last_frame_time,
                        &mut acoustic_buffer,
                    );

                    for track in tracks {
                        if self.tx.send(track).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    }

    fn flush_ready(&self, buckets: &mut HashMap<String, Vec<Observation>>, last_frame_time: &mut std::time::Instant, acoustic_buffer: &mut std::collections::VecDeque<f32>) -> Vec<FusedTrack> {
        let mut ready = Vec::new();

        for (track_hint, observations) in buckets.iter_mut() {
            if observations.len() < 2 {
                continue;
            }

            observations.sort_by_key(|obs| obs.timestamp_ms);
            let timestamp_ms = observations
                .last()
                .map(|obs| obs.timestamp_ms)
                .unwrap_or_default();
            let confidence = observations.iter().map(|obs| obs.confidence).sum::<f32>()
                / observations.len() as f32;
            
            // Apply Extended Kalman Filter to positional observations
            let mut ekf = ExtendedKalmanFilter::new();
            for obs in observations.iter() {
                // REAL HARDWARE IMPLEMENTATION: Calculate real delta time
                let now = std::time::Instant::now();
                let dt = now.duration_since(*last_frame_time).as_secs_f32().max(0.001);
                ekf.predict(dt); 
                *last_frame_time = now;
                
                ekf.update([obs.position_m.0, obs.position_m.1], obs.confidence);
            }
            
            let position_x = ekf.state[0];
            let position_y = ekf.state[1];
            let velocities: Vec<f32> = observations
                .iter()
                .filter_map(|obs| obs.velocity_mps)
                .collect();
            
            // Predictive Maintenance: Track Acoustic/Vibration Anomalies via Z-Scores
            let current_vib = if !velocities.is_empty() {
                velocities.iter().sum::<f32>() / velocities.len() as f32
            } else {
                0.0
            };
            
            // REAL HARDWARE IMPLEMENTATION: Dynamic Rolling Statistics
            let z_score = if acoustic_buffer.len() > 10 {
                let mean: f32 = acoustic_buffer.iter().sum::<f32>() / acoustic_buffer.len() as f32;
                let variance = acoustic_buffer.iter().map(|value| {
                    let diff = mean - *value;
                    diff * diff
                }).sum::<f32>() / acoustic_buffer.len() as f32;
                let std_dev = variance.sqrt();
                
                if std_dev > 0.0 { (current_vib - mean) / std_dev } else { 0.0 }
            } else {
                // Fallback simulation value until buffer fills
                (current_vib - 0.018) / 0.005
            };
            
            acoustic_buffer.push_back(current_vib);
            if acoustic_buffer.len() > 100 {
                acoustic_buffer.pop_front();
            }

            let velocity_mps = (!velocities.is_empty())
                .then(|| velocities.iter().sum::<f32>() / velocities.len() as f32);
                
            let threat_level = if z_score > 3.5 {
                "high-interest" // Predictive maintenance anomaly trigger
            } else if confidence >= self.settings.threat_threshold {
                "high-interest"
            } else {
                "monitor"
            };

            ready.push(FusedTrack {
                node_id: self.settings.node_id.clone(),
                timestamp_ms,
                track_id: format!("{}-{}", self.settings.node_id, track_hint),
                site: self.settings.location.site.clone(),
                geo_latitude: self.settings.location.latitude,
                geo_longitude: self.settings.location.longitude,
                threat_level: threat_level.to_string(),
                confidence,
                position_m: (position_x, position_y),
                velocity_mps,
                contributing_modalities: dedupe_modalities(
                    observations
                        .iter()
                        .map(|obs| obs.modality.clone())
                        .collect(),
                ),
                source_ids: dedupe_strings(
                    observations
                        .iter()
                        .map(|obs| obs.source_id.clone())
                        .collect(),
                ),
                evidence_digests: dedupe_strings(
                    observations
                        .iter()
                        .map(|obs| obs.evidence_digest.clone())
                        .collect(),
                ),
            });

            observations.clear();
        }

        ready
    }
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        if !result.contains(&value) {
            result.push(value);
        }
    }
    result
}

fn dedupe_modalities(values: Vec<Modality>) -> Vec<Modality> {
    let mut result = Vec::new();
    for value in values {
        if !result.contains(&value) {
            result.push(value);
        }
    }
    result
}
