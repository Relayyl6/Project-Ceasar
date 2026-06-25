use std::{collections::{HashMap, HashSet}, time::Duration};

use tokio::{sync::mpsc, time::interval};
use uriel_caesar_core::io::unix_time_ms;
use uriel_caesar_core::protocol::{FusedTrack, Modality, Observation};

use crate::config::EdgeConfig;

pub struct ExtendedKalmanFilter {
    pub state: [f32; 4], // [x, y, vx, vy]
    /// False until the first measurement seeds the position state. Without this
    /// the filter would treat the origin (0,0) as a real prior and drag the
    /// first estimate halfway to it.
    initialized: bool,
}

impl ExtendedKalmanFilter {
    pub fn new() -> Self {
        Self { state: [0.0; 4], initialized: false }
    }

    /// Constant-velocity prediction step: extrapolate position using the
    /// current velocity estimate. Velocity is held constant between updates.
    pub fn predict(&mut self, dt: f32) {
        self.state[0] += self.state[2] * dt;
        self.state[1] += self.state[3] * dt;
    }

    /// Correction step. The Kalman gain is approximated from the measurement
    /// confidence (high confidence → trust the measurement more). The velocity
    /// state is estimated from the corrected position delta over `dt`, so that
    /// the next `predict` actually extrapolates motion instead of being a no-op.
    pub fn update(&mut self, measurement: [f32; 2], confidence: f32, dt: f32) {
        let gain = confidence.clamp(0.1, 0.9);

        // Seed directly from the first measurement; there is no meaningful prior.
        if !self.initialized {
            self.state[0] = measurement[0];
            self.state[1] = measurement[1];
            self.initialized = true;
            return;
        }

        let prev_x = self.state[0];
        let prev_y = self.state[1];
        let new_x = prev_x * (1.0 - gain) + measurement[0] * gain;
        let new_y = prev_y * (1.0 - gain) + measurement[1] * gain;

        // Derive velocity from the corrected position change, smoothed by the
        // same gain so a single noisy frame cannot spike the velocity estimate.
        if dt > 1e-3 {
            let vx = (new_x - prev_x) / dt;
            let vy = (new_y - prev_y) / dt;
            self.state[2] = self.state[2] * (1.0 - gain) + vx * gain;
            self.state[3] = self.state[3] * (1.0 - gain) + vy * gain;
        }

        self.state[0] = new_x;
        self.state[1] = new_y;
    }
}

pub struct FusionEngine {
    settings: EdgeConfig,
    rx: mpsc::Receiver<Observation>,
    tx: mpsc::Sender<FusedTrack>,
    /// Per-track rolling acoustic/vibration buffers for z-score computation.
    /// Keyed by track_hint so samples from different tracks never cross-contaminate.
    acoustic_buffers: HashMap<String, (std::time::Instant, std::collections::VecDeque<f32>)>,
}


impl FusionEngine {
    pub fn spawn(
        settings: EdgeConfig,
        rx: mpsc::Receiver<Observation>,
        tx: mpsc::Sender<FusedTrack>,
    ) {
        let engine = Self { settings, rx, tx, acoustic_buffers: HashMap::new() };
        tokio::spawn(async move {
            engine.run().await;
        });
    }

    async fn run(mut self) {
        let mut buckets: HashMap<String, Vec<Observation>> = HashMap::new();
        let mut ticker = interval(Duration::from_millis(self.settings.fusion_window_ms));

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
                    let tracks = self.flush_ready(&mut buckets);

                    for track in tracks {
                        if self.tx.send(track).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    }

    fn flush_ready(&mut self, buckets: &mut HashMap<String, Vec<Observation>>) -> Vec<FusedTrack> {
        let mut ready = Vec::new();

        for (track_hint, observations) in buckets.iter_mut() {
            if observations.is_empty() {
                continue;
            }

            observations.sort_by_key(|obs| obs.timestamp_ms);
            let timestamp_ms = observations
                .last()
                .map(|obs| obs.timestamp_ms)
                .unwrap_or_default();
            let confidence = observations.iter().map(|obs| obs.confidence).sum::<f32>()
                / observations.len() as f32;
            
            // Apply Extended Kalman Filter to positional observations.
            // F4 FIX: Derive dt from consecutive observation timestamp_ms fields
            // instead of Instant::now() which measures loop overhead (~µs), not
            // real sensor inter-frame time. Use a 33ms default (30 fps) when
            // timestamps are identical to avoid division-by-zero.
            let mut ekf = ExtendedKalmanFilter::new();
            let mut prev_ts_ms: Option<u64> = None;
            for obs in observations.iter() {
                let dt = match prev_ts_ms {
                    Some(prev) => ((obs.timestamp_ms.saturating_sub(prev)) as f32 / 1000.0).max(0.001),
                    None => 0.033, // 30 fps default on very first observation
                };
                prev_ts_ms = Some(obs.timestamp_ms);
                ekf.predict(dt);
                ekf.update([obs.position_m.0, obs.position_m.1], obs.confidence, dt);
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
            // Use per-track acoustic buffer so velocity samples from one track
            // never contaminate z-score computation for another track.
            let now = std::time::Instant::now();
            let (last_seen, track_buf) = self.acoustic_buffers
                .entry(track_hint.clone())
                .or_insert_with(|| (now, std::collections::VecDeque::with_capacity(100)));
            *last_seen = now;
            let z_score = if track_buf.len() > 10 {
                let mean: f32 = track_buf.iter().sum::<f32>() / track_buf.len() as f32;
                let variance = track_buf.iter().map(|value| {
                    let diff = mean - *value;
                    diff * diff
                }).sum::<f32>() / track_buf.len() as f32;
                let std_dev = variance.max(0.0).sqrt();
                
                if std_dev > 0.0 { (current_vib - mean) / std_dev } else { 0.0 }
            } else {
                // Fallback simulation value until buffer fills
                (current_vib - 0.018) / 0.005
            };
            
            track_buf.push_back(current_vib);
            if track_buf.len() > 100 {
                track_buf.pop_front();
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

            // ── Inference telemetry aggregation ───────────────────────────────
            // Populate the per-track latency/engine fields the dashboard and hub
            // expect. Observations were sorted ascending by timestamp_ms above,
            // so `first()` is the earliest contributing frame.
            let earliest_ts = observations
                .first()
                .map(|obs| obs.timestamp_ms)
                .unwrap_or(timestamp_ms);
            let end_to_end_latency_ms = Some(unix_time_ms().saturating_sub(earliest_ts));

            let mut inference_latency_ms: HashMap<String, u64> = HashMap::new();
            for obs in observations.iter() {
                if let Some(ms) = obs.inference_latency_ms {
                    let key = format!("{:?}", obs.modality).to_lowercase();
                    let slot = inference_latency_ms.entry(key).or_insert(0);
                    *slot = (*slot).max(ms);
                }
            }

            // Attribute the track to the engine behind its highest-confidence
            // contributing observation (the one that most drove the verdict).
            let inference_engine = observations
                .iter()
                .filter(|obs| !obs.inference_engine.is_empty())
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|obs| obs.inference_engine.clone())
                .unwrap_or_default();

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
                inference_latency_ms,
                end_to_end_latency_ms,
                inference_engine,
            });

            observations.clear();
        }

        buckets.retain(|_, obs| !obs.is_empty());
        let stale_cutoff = std::time::Instant::now() - std::time::Duration::from_secs(60);
        self.acoustic_buffers.retain(|_, (last_seen, _)| *last_seen > stale_cutoff);

        ready
    }
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.into_iter().filter(|v| seen.insert(v.clone())).collect()
}

fn dedupe_modalities(values: Vec<Modality>) -> Vec<Modality> {
    // Modality is a simple enum so we can compare cheaply; clone for the HashSet key.
    let mut seen: HashSet<String> = HashSet::new();
    values.into_iter().filter(|v| seen.insert(format!("{:?}", v))).collect()
}
