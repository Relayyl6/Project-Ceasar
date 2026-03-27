use std::{collections::HashMap, time::Duration};

use tokio::{sync::mpsc, time::interval};
use uriel_caesar_core::protocol::{FusedTrack, Modality, Observation};

use crate::config::EdgeConfig;

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

        loop {
            tokio::select! {
                maybe_observation = self.rx.recv() => {
                    match maybe_observation {
                        Some(obs) => buckets.entry(obs.track_hint.clone()).or_default().push(obs),
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

    fn flush_ready(&self, buckets: &mut HashMap<String, Vec<Observation>>) -> Vec<FusedTrack> {
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
            let position_x = observations.iter().map(|obs| obs.position_m.0).sum::<f32>()
                / observations.len() as f32;
            let position_y = observations.iter().map(|obs| obs.position_m.1).sum::<f32>()
                / observations.len() as f32;
            let velocities: Vec<f32> = observations
                .iter()
                .filter_map(|obs| obs.velocity_mps)
                .collect();
            let velocity_mps = (!velocities.is_empty())
                .then(|| velocities.iter().sum::<f32>() / velocities.len() as f32);
            let threat_level = if confidence >= self.settings.threat_threshold {
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
