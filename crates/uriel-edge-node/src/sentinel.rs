//! Project Caesar — OpenCV-Native Sentinel Pipeline
//!
//! The sentinel is the always-on cognitive edge layer for optical monitoring.
//! It runs BackgroundSubtractorMOG2 at 24 fps in a dedicated OS thread (OpenCV
//! is synchronous), requiring zero GPU and negligible CPU on a Pi-class device.
//!
//! ## Signal flow
//! ```text
//! VideoCapture @ 24fps
//!   │
//!   ▼  MOG2 per frame
//! [Stage 1] Contour analysis → anomaly_score
//!   │  Always: live JPEG → SentinelFeedEvent::LiveFrame → dashboard
//!   │
//!   │  score > anomaly_threshold?  AND  6 consecutive frames triggered?
//!   ▼  YES → SentinelSnapshot → TemporalAccumulator
//!
//! [Stage 2] Temporal Accumulator (ring buffer, max_snapshots = 8)
//!   │  score > burst_threshold?  → immediate AI escalation (Stage 3)
//!   │  accumulator.is_ready()?   → build 4-quadrant ChangePortrait → dashboard
//!   │  mean_confidence ≥ 0.87?   → AI escalation + ActuatorBus dispatch
//!   ▼
//!
//! [Stage 3] optical_infer_with_sentinel()  [YOLO-World → Ollama → Heuristic]
//!   └─→ Observation → FusionEngine → Uplink
//!   └─→ ActuatorCommand → ActuatorBus → physical/logical action + dashboard log
//! ```

use std::{
    collections::VecDeque,
    io::Write,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use opencv::{
    core::{Mat, Rect, Scalar, Size, Vector},
    imgcodecs,
    imgproc,
    prelude::*,
    video,
    videoio,
};
use tokio::{runtime::Handle, sync::{broadcast, mpsc}};
use uriel_caesar_core::{io::unix_time_ms, protocol::{Observation, SentinelAlert}};
use serde_json;

use crate::{
    actuator::{ActuatorBus, ActuatorCommand, infer_action_from_class},
    camera::OpticalFrame,
    config::EdgeConfig,
    inference::{optical_infer_with_sentinel, SentinelContext},
};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A single bounding region of detected change (from MOG2 contour analysis).
#[derive(Debug, Clone)]
pub struct ChangeRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub area: f64,
}

/// One confirmed anomaly snapshot — 6 consecutive triggered frames.
#[derive(Debug, Clone)]
pub struct SentinelSnapshot {
    pub timestamp_ms: u64,
    pub anomaly_score: f32,
    pub regions: Vec<ChangeRegion>, // up to 4, sorted largest first
    pub jpeg_bytes: Vec<u8>,        // annotated frame JPEG
}

/// The 4-quadrant change portrait sent to the dashboard portrait panel.
#[derive(Debug, Clone)]
pub struct ChangePortrait {
    pub timestamp_ms: u64,
    pub snapshot_count: usize,
    pub mean_confidence: f32,
    /// 2×2 composite JPEG (640×480) of the 4 most significant change regions.
    pub composite_jpeg: Vec<u8>,
    /// AI-derived class label populated after Stage 3 completes.
    pub dominant_class: Option<String>,
}

/// Events forwarded to the dashboard over the broadcast feed channel.
#[derive(Debug, Clone)]
pub enum SentinelFeedEvent {
    /// Every frame, annotated with amber bounding boxes when motion is detected.
    /// This IS the dashboard camera view — continuous, never interrupted.
    LiveFrame {
        jpeg_bytes: Vec<u8>,
        anomaly_score: f32,
        /// true when motion boxes have been drawn on the frame
        annotated: bool,
    },
    /// A 4-quadrant portrait of accumulated change regions, shown in the
    /// dedicated portrait panel alongside the live feed.
    PortraitReady(ChangePortrait),
    /// Emitted after an autonomous actuator action is dispatched.
    /// The dashboard shows this as an action log entry.
    ActuatorDispatched {
        action: String,
        target: String,
        confidence: f32,
        rationale: String,
        actuator_results: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// TemporalAccumulator
// ---------------------------------------------------------------------------

/// Ring buffer that accumulates confirmed-anomaly snapshots over time.
/// Once `min_snapshots` entries exist and mean confidence passes the
/// autonomous threshold, the AI pipeline and actuator bus are triggered.
pub struct TemporalAccumulator {
    snapshots: VecDeque<SentinelSnapshot>,
    min_snapshots: usize,
}

impl TemporalAccumulator {
    pub fn new(min_snapshots: usize) -> Self {
        Self {
            snapshots: VecDeque::with_capacity(min_snapshots + 1),
            min_snapshots,
        }
    }

    pub fn push(&mut self, snap: SentinelSnapshot) {
        self.snapshots.push_back(snap);
        // Hard cap: keep only 2× the threshold to bound memory usage
        while self.snapshots.len() > self.min_snapshots * 2 {
            self.snapshots.pop_front();
        }
    }

    pub fn is_ready(&self) -> bool {
        self.snapshots.len() >= self.min_snapshots
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn mean_confidence(&self) -> f32 {
        if self.snapshots.is_empty() { return 0.0; }
        self.snapshots.iter().map(|s| s.anomaly_score).sum::<f32>() / self.snapshots.len() as f32
    }

    /// Build a 4-quadrant ChangePortrait from the most recent snapshots.
    pub fn build_portrait(&self) -> ChangePortrait {
        let crops: Vec<&[u8]> = self.snapshots.iter().rev().take(4)
            .map(|s| s.jpeg_bytes.as_slice()).collect();
        let composite = build_quadrant_composite(&crops).unwrap_or_default();
        ChangePortrait {
            timestamp_ms: unix_time_ms(),
            snapshot_count: self.snapshots.len(),
            mean_confidence: self.mean_confidence(),
            composite_jpeg: composite,
            dominant_class: None,
        }
    }

    /// Slide the window: discard the oldest half so accumulation continues
    /// from a fresh baseline without losing all context.
    pub fn clear_oldest_half(&mut self) {
        let keep = (self.min_snapshots / 2).max(1);
        while self.snapshots.len() > keep {
            self.snapshots.pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// 4-quadrant composite builder
// ---------------------------------------------------------------------------

/// Assembles up to 4 JPEG crops into a 2×2 composite grid (640×480 total).
/// Each quadrant (320×240) has a green border highlighting the change zone.
/// Empty quadrants are filled with a dark slate background.
fn build_quadrant_composite(crops: &[&[u8]]) -> Result<Vec<u8>> {
    const Q_W: i32 = 320;
    const Q_H: i32 = 240;
    let border = Scalar::new(0.0, 210.0, 90.0, 255.0); // Project Caesar green
    let bg     = Scalar::all(28.0);

    let mut quads: Vec<Mat> = Vec::with_capacity(4);
    for i in 0..4 {
        let mut q = Mat::new_rows_cols_with_default(Q_H, Q_W, opencv::core::CV_8UC3, bg)?;
        if let Some(jpeg) = crops.get(i) {
            let buf = Vector::<u8>::from_slice(jpeg);
            if let Ok(decoded) = imgcodecs::imdecode(&buf, imgcodecs::IMREAD_COLOR) {
                if !decoded.empty() {
                    let _ = imgproc::resize(&decoded, &mut q, Size::new(Q_W, Q_H), 0.0, 0.0, imgproc::INTER_LINEAR);
                }
            }
        }
        // Draw quadrant border
        imgproc::rectangle(
            &mut q,
            Rect::new(2, 2, Q_W - 4, Q_H - 4),
            border, 2, imgproc::LINE_8, 0,
        )?;
        quads.push(q);
    }

    let mut top = Mat::default();
    let mut bottom = Mat::default();
    let mut composite = Mat::default();
    opencv::core::hconcat2(&quads[0], &quads[1], &mut top)?;
    opencv::core::hconcat2(&quads[2], &quads[3], &mut bottom)?;
    opencv::core::vconcat2(&top, &bottom, &mut composite)?;

    let mut buf = Vector::<u8>::new();
    imgcodecs::imencode(".jpg", &composite, &mut buf, &Vector::new())?;
    Ok(buf.to_vec())
}

// ---------------------------------------------------------------------------
// Frame helpers
// ---------------------------------------------------------------------------

fn encode_mat_jpeg(mat: &Mat) -> Result<Vec<u8>> {
    let mut buf = Vector::<u8>::new();
    imgcodecs::imencode(".jpg", mat, &mut buf, &Vector::new())?;
    Ok(buf.to_vec())
}

/// Wrap an OpenCV Mat as the existing `OpticalFrame` type so it can pass
/// through the unmodified inference pipeline (YOLO → Ollama → Heuristic).
fn mat_to_optical_frame(mat: &Mat, config: &EdgeConfig, timestamp_ms: u64) -> Result<OpticalFrame> {
    let jpeg = encode_mat_jpeg(mat)?;
    Ok(OpticalFrame {
        sequence: timestamp_ms / 42, // approximate 24fps sequence number
        timestamp_ms,
        camera_id: config.optical.camera_id.clone(),
        width: mat.cols() as u32,
        height: mat.rows() as u32,
        jpeg_bytes: jpeg,
    })
}

// ---------------------------------------------------------------------------
// SentinelWorker
// ---------------------------------------------------------------------------

pub struct SentinelWorker {
    config: EdgeConfig,
    observation_tx: mpsc::Sender<Observation>,
    feed_tx: broadcast::Sender<SentinelFeedEvent>,
    actuator_bus: Arc<ActuatorBus>,
    /// Shared latest JPEG for the optional MJPEG server thread.
    latest_jpeg: Arc<Mutex<Vec<u8>>>,
}

impl SentinelWorker {
    pub fn new(
        config: EdgeConfig,
        observation_tx: mpsc::Sender<Observation>,
        feed_tx: broadcast::Sender<SentinelFeedEvent>,
        actuator_bus: Arc<ActuatorBus>,
    ) -> Self {
        Self {
            config,
            observation_tx,
            feed_tx,
            actuator_bus,
            latest_jpeg: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Spawn the sentinel in a dedicated OS thread.
    /// OpenCV's VideoCapture is synchronous/blocking and must not run on
    /// the Tokio executor. We capture the runtime Handle here so the loop
    /// can bridge back to async when dispatching AI inference tasks.
    pub fn spawn(self) {
        let handle = Handle::current();

        // If mjpeg_port is configured, start the MJPEG HTTP server before the capture loop.
        // The server shares `latest_jpeg` with the capture thread via Arc<Mutex<>>.
        if let Some(port) = self.config.sentinel.mjpeg_port {
            let jpeg_ref = Arc::clone(&self.latest_jpeg);
            let addr = format!("0.0.0.0:{}", port);
            std::thread::spawn(move || {
                Self::run_mjpeg_server(&addr, jpeg_ref);
            });
            println!("[caesar.sentinel.mjpeg] Live stream server bound to http://0.0.0.0:{}", port);
        }

        std::thread::spawn(move || {
            if let Err(e) = self.run_capture_loop(handle) {
                eprintln!("[caesar.sentinel] Capture loop terminated: {e:#}");
            }
        });
    }

    // -----------------------------------------------------------------------
    // MJPEG server — serves a browser-compatible multipart stream
    // -----------------------------------------------------------------------
    fn run_mjpeg_server(addr: &str, jpeg_ref: Arc<Mutex<Vec<u8>>>) {
        let listener = match TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[caesar.sentinel.mjpeg] Failed to bind {addr}: {e}");
                return;
            }
        };
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue; };
            let jpeg_ref = Arc::clone(&jpeg_ref);
            std::thread::spawn(move || {
                // Send HTTP headers
                let header = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=caesar_frame\r\nConnection: keep-alive\r\nCache-Control: no-cache\r\n\r\n";
                if stream.write_all(header.as_bytes()).is_err() { return; }
                let target_fps = Duration::from_millis(42); // ~24fps
                loop {
                    let jpeg = {
                        let guard = jpeg_ref.lock().unwrap();
                        guard.clone()
                    };
                    if jpeg.is_empty() {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    let part_header = format!(
                        "--caesar_frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                        jpeg.len()
                    );
                    if stream.write_all(part_header.as_bytes()).is_err() { break; }
                    if stream.write_all(&jpeg).is_err() { break; }
                    if stream.write_all(b"\r\n").is_err() { break; }
                    std::thread::sleep(target_fps);
                }
            });
        }
    }

    fn run_capture_loop(self, handle: Handle) -> Result<()> {
        let s = &self.config.sentinel;

        let mut cap = videoio::VideoCapture::new(s.device_id, videoio::CAP_ANY)
            .context("[sentinel] Failed to open VideoCapture")?;
        cap.set(videoio::CAP_PROP_FPS, s.fps as f64)?;

        // MOG2: history=500, varThreshold=16, detectShadows=false for speed
        let mut mog2 = video::create_background_subtractor_mog2(500, 16.0, false)
            .context("[sentinel] Failed to create MOG2 subtractor")?;

        let mut fg_mask = Mat::default();
        let mut frame   = Mat::default();
        let mut consecutive_change_frames: u8 = 0;
        let mut accumulator = TemporalAccumulator::new(s.min_snapshots);

        let frame_dur = Duration::from_micros(1_000_000 / s.fps.max(1) as u64);
        let mut last_tick = Instant::now();

        println!(
            "[caesar.sentinel] Online — device={} fps={} domain={} thresh={}/{}{}",
            s.device_id, s.fps, self.config.domain,
            s.anomaly_threshold, s.burst_threshold,
            s.mjpeg_port.map(|p| format!(" mjpeg_port={p}")).unwrap_or_default()
        );

        loop {
            // Pace to target fps
            let elapsed = last_tick.elapsed();
            if elapsed < frame_dur {
                std::thread::sleep(frame_dur - elapsed);
            }
            last_tick = Instant::now();

            if !cap.read(&mut frame).unwrap_or(false) || frame.empty() {
                eprintln!("[caesar.sentinel] Empty frame; retrying...");
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            // --- Stage 1: MOG2 background subtraction ---
            mog2.apply(&frame, &mut fg_mask, -1.0)?;

            let mut contours: Vector<Vector<opencv::core::Point>> = Vector::new();
            imgproc::find_contours(
                &fg_mask, &mut contours,
                imgproc::RETR_EXTERNAL, imgproc::CHAIN_APPROX_SIMPLE,
                opencv::core::Point::default(),
            )?;

            // Extract top-4 regions by area, filtered by minimum threshold
            let mut regions: Vec<ChangeRegion> = contours.iter()
                .filter_map(|c| {
                    let area = imgproc::contour_area(&c, false).ok()?;
                    if area < s.motion_area_threshold { return None; }
                    let r = imgproc::bounding_rect(&c).ok()?;
                    Some(ChangeRegion { x: r.x, y: r.y, width: r.width, height: r.height, area })
                })
                .collect();
            regions.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal));
            regions.truncate(4);

            let frame_area = (frame.cols() * frame.rows()).max(1) as f64;
            let anomaly_score = (regions.iter().map(|r| r.area).sum::<f64>() / frame_area)
                .clamp(0.0, 1.0) as f32;

            // Annotate frame — amber boxes on motion regions
            let mut annotated_frame = frame.clone();
            let amber = Scalar::new(0.0, 165.0, 255.0, 255.0);
            for region in &regions {
                let _ = imgproc::rectangle(
                    &mut annotated_frame,
                    Rect::new(region.x, region.y, region.width, region.height),
                    amber, 2, imgproc::LINE_8, 0,
                );
            }

            // Live frame always forwarded — this IS the dashboard camera view
            if let Ok(live_jpeg) = encode_mat_jpeg(&annotated_frame) {
                // Update the MJPEG server's shared frame
                if let Ok(mut guard) = self.latest_jpeg.lock() {
                    *guard = live_jpeg.clone();
                }

                let _ = self.feed_tx.send(SentinelFeedEvent::LiveFrame {
                    jpeg_bytes: live_jpeg.clone(),
                    anomaly_score,
                    annotated: !regions.is_empty(),
                });

                // --- 6-frame rolling gate ---
                if anomaly_score > s.anomaly_threshold {
                    consecutive_change_frames = consecutive_change_frames.saturating_add(1);
                } else {
                    consecutive_change_frames = 0;
                    continue; // Clear frame — nothing more to do
                }

                if consecutive_change_frames < 6 {
                    // Not yet confirmed — keep watching
                    continue;
                }

                // Confirmed anomaly — snapshot it
                let timestamp_ms = unix_time_ms();
                accumulator.push(SentinelSnapshot {
                    timestamp_ms,
                    anomaly_score,
                    regions: regions.clone(),
                    jpeg_bytes: live_jpeg,
                });

                println!(
                    "[caesar.sentinel][{}] Change confirmed score={:.3} accumulated={}/{}",
                    self.config.domain, anomaly_score, accumulator.len(), s.min_snapshots
                );

                // --- Burst escalation: high-score events skip the accumulation wait ---
                if anomaly_score > s.burst_threshold {
                    println!(
                        "[caesar.sentinel] Burst escalation: score={:.3} > {:.3} — waking AI now",
                        anomaly_score, s.burst_threshold
                    );
                    if let Ok(of) = mat_to_optical_frame(&frame, &self.config, timestamp_ms) {
                        let ctx = SentinelContext {
                            anomaly_score,
                            snapshot_count: accumulator.len(),
                            burst_mode: true,
                        };
                        let obs_tx  = self.observation_tx.clone();
                        let cfg     = self.config.clone();
                        handle.spawn(async move {
                            match optical_infer_with_sentinel(&cfg, of, ctx).await {
                                Ok(Some(obs)) => { let _ = obs_tx.send(obs).await; }
                                Ok(None) => {}
                                Err(e) => eprintln!("[caesar.sentinel.burst] inference error: {e:#}"),
                            }
                        });
                    }
                }

                // --- Portrait accumulation path ---
                if accumulator.is_ready() {
                    let mean_conf  = accumulator.mean_confidence();
                    let portrait   = accumulator.build_portrait();
                    let _ = self.feed_tx.send(SentinelFeedEvent::PortraitReady(portrait.clone()));

                    println!(
                        "[caesar.sentinel][{}] Portrait ready — {} snapshots, mean_conf={:.3}",
                        self.config.domain, portrait.snapshot_count, mean_conf
                    );

                    if mean_conf >= s.autonomous_confidence {
                        println!(
                            "[caesar.sentinel] Autonomous threshold met ({:.3} ≥ {:.3}) — escalating to AI + actuator",
                            mean_conf, s.autonomous_confidence
                        );

                        if let Ok(of) = mat_to_optical_frame(&frame, &self.config, timestamp_ms) {
                            let ctx = SentinelContext {
                                anomaly_score: mean_conf,
                                snapshot_count: accumulator.len(),
                                burst_mode: false,
                            };
                            let obs_tx       = self.observation_tx.clone();
                            let feed_tx      = self.feed_tx.clone();
                            let actuator_bus = Arc::clone(&self.actuator_bus);
                            let cfg          = self.config.clone();
                            let portrait_b64 = STANDARD.encode(&portrait.composite_jpeg);
                            let snap_count   = portrait.snapshot_count;

                            handle.spawn(async move {
                                let domain = &cfg.domain;
                                match optical_infer_with_sentinel(&cfg, of, ctx).await {
                                    Ok(Some(obs)) => {
                                        let class      = obs.class_label.clone();
                                        let ai_conf    = obs.confidence;
                                        // Derive the inference stage from the track_hint prefix:
                                        // "ort-*" → "onnx", "ollama-*" → "ollama",
                                        // "heuristic-*" → "heuristic", "gemini-er-*" → "onnx"
                                        let stage = if obs.track_hint.starts_with("ort-") || obs.track_hint.starts_with("pxadmm-") || obs.track_hint.starts_with("lstm-") || obs.track_hint.starts_with("gemini-er") {
                                            "onnx"
                                        } else if obs.track_hint.starts_with("ollama-") {
                                            "ollama"
                                        } else if obs.track_hint.starts_with("heuristic-") {
                                            "heuristic"
                                        } else {
                                            "unknown"
                                        }.to_string();

                                        // Forward observation into the fusion engine
                                        let _ = obs_tx.send(obs).await;

                                        // Build and dispatch actuator command
                                        let action = infer_action_from_class(&class, domain);
                                        let composite_conf = (mean_conf * ai_conf).clamp(0.0, 1.0);
                                        let rationale = format!(
                                            "[caesar.{domain}] AI detected '{class}' via {stage} \
                                             over {snap_count} snapshots. Mean anomaly confidence: \
                                             {mean_conf:.0}%, AI confidence: {ai_conf:.0}%. \
                                             Composite confidence: {composite_conf:.0}%.",
                                            domain=domain, class=class, stage=stage,
                                            snap_count=snap_count,
                                            mean_conf=mean_conf*100.0,
                                            ai_conf=ai_conf*100.0,
                                            composite_conf=composite_conf*100.0,
                                        );

                                        let cmd = ActuatorCommand {
                                            action: action.clone(),
                                            target: "auto".to_string(),
                                            intensity: composite_conf,
                                            rationale: rationale.clone(),
                                            confidence: composite_conf,
                                            domain: domain.clone(),
                                        };

                                        let results = actuator_bus.dispatch(cmd);
                                        let result_msgs: Vec<String> = results.iter()
                                            .map(|r| format!("{}: {}", r.actuator_id, r.message))
                                            .collect();

                                        let _ = feed_tx.send(SentinelFeedEvent::ActuatorDispatched {
                                            action,
                                            target: "auto".into(),
                                            confidence: composite_conf,
                                            rationale,
                                            actuator_results: result_msgs,
                                        });

                                        // Construct and emit a SentinelAlert for the hub/dashboard.
                                        // Serialised to JSON so the caesar-hub can parse it directly.
                                        let alert = SentinelAlert {
                                            node_id: cfg.node_id.clone(),
                                            timestamp_ms: unix_time_ms(),
                                            domain: domain.clone(),
                                            snapshot_count: snap_count,
                                            mean_confidence: mean_conf,
                                            class_label: Some(class.clone()),
                                            portrait_jpeg_b64: Some(portrait_b64.clone()),
                                            rationale: rationale.clone(),
                                            actuator_results: result_msgs.clone(),
                                            inference_stage: stage.clone(),
                                        };
                                        // Emit as structured JSON on stdout for hub ingestion.
                                        // In a future integration, feed_tx or an uplink channel
                                        // can carry this directly to the caesar-hub subscriber.
                                        if let Ok(alert_json) = serde_json::to_string(&alert) {
                                            println!("[caesar.sentinel.alert] {}", alert_json);
                                        }
                                    }
                                    Ok(None) => {
                                        // Below burst threshold but portrait ready — log without action
                                        println!(
                                            "[caesar.{domain}][sentinel] Portrait processed but \
                                             AI returned no classification. Continuing watch.",
                                            domain=domain
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!("[caesar.sentinel.portrait] inference error: {e:#}");
                                    }
                                }
                            });
                        }
                    }

                    // Slide the window — keep recent context, clear oldest half
                    accumulator.clear_oldest_half();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_snapshot(score: f32) -> SentinelSnapshot {
        SentinelSnapshot {
            timestamp_ms: 1000,
            anomaly_score: score,
            regions: vec![],
            jpeg_bytes: vec![],
        }
    }

    #[test]
    fn test_temporal_accumulator_logic() {
        let mut acc = TemporalAccumulator::new(6);
        assert!(!acc.is_ready());
        assert_eq!(acc.len(), 0);

        // Push 5 snapshots
        for _ in 0..5 {
            acc.push(dummy_snapshot(0.8));
        }
        assert!(!acc.is_ready());
        assert_eq!(acc.len(), 5);

        // Push 6th snapshot
        acc.push(dummy_snapshot(0.9));
        assert!(acc.is_ready());
        assert_eq!(acc.len(), 6);
        
        let mean = acc.mean_confidence();
        // (5 * 0.8 + 0.9) / 6 = 4.9 / 6 = 0.8166
        assert!((mean - 0.8166).abs() < 0.01);

        // Test capacity cap (2 * min_snapshots = 12)
        for _ in 0..10 {
            acc.push(dummy_snapshot(0.9));
        }
        assert_eq!(acc.len(), 12); // Should not exceed 12

        // Test sliding window
        acc.clear_oldest_half();
        // 6 / 2 = 3. Keep 3 snapshots
        assert_eq!(acc.len(), 3);
        assert!(!acc.is_ready());
    }
}
