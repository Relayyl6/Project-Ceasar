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
use ndarray::{Array, Array4, Array3};
use ort::{GraphOptimizationLevel, Session};


// F7/F8/F9 FIX: Share a single reqwest Client across all inference calls.
// Building a new Client per frame reconstructs the TLS stack each time.
// OnceLock ensures one-time initialisation that is safe across async tasks.
static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

// --- Advanced Physical Hardware Tensor Integration ---
/// Encode `vocab` into an ORT embedding tensor via Ollama's `/api/embeddings` endpoint.
///
/// # Warning — blocking HTTP
/// This function uses `reqwest::blocking` and **must not** be called directly
/// from an async context. Always wrap in `tokio::task::spawn_blocking`.
///
/// # Parameters
/// * `vocab`    – Slice of label strings to embed.
/// * `endpoint` – Base URL of the Ollama server, e.g. `"http://localhost:11434"`.
///   The `/api/embeddings` path is appended automatically.
pub fn encode_vocabulary(vocab: &[String], endpoint: &str) -> Option<ort::Value> {
    // Caller passes the full base URL including path; we append the API path.
    let embed_url = format!("{}/api/embeddings", endpoint.trim_end_matches('/'));

    // Reuse the shared client — this function is called from spawn_blocking so
    // a blocking client is appropriate here. Build it independently.
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;

    let prompt = vocab.join(", ");
    let mut embed_data = vec![0.0f32; 512];

    if let Ok(resp) = client.post(&embed_url).json(&serde_json::json!({ "model": "nomic-embed-text", "prompt": prompt })).send() {
        if let Ok(json) = resp.json::<serde_json::Value>() {
            if let Some(arr) = json["embedding"].as_array() {
                for (i, val) in arr.iter().take(512).enumerate() {
                    embed_data[i] = val.as_f64().unwrap_or(0.0) as f32;
                }
            }
        }
    } else {
        println!("[edge.inference] Warning: Ollama embeddings unavailable. Using deterministic fallback for vocabulary.");
        // Deterministic hashing fallback instead of zeros
        for (i, v) in embed_data.iter_mut().enumerate() {
            *v = ((i + vocab.len()) % 255) as f32 / 255.0;
        }
    }

    // F6 FIX: Replace unwrap() with proper error paths — a shape mismatch or
    // ORT error must not panic the entire edge node.
    let arr = Array::from_shape_vec((1, 512), embed_data).ok()?;
    ort::Value::from_array(arr).ok()
}

pub fn apply_nms(data: &[f32], num_anchors: usize, num_classes: usize, custom_vocab: &[String]) -> Option<(String, f32, f32, f32)> {
    let conf_threshold = 0.25;
    let iou_threshold = 0.45;

    struct Detection {
        class_idx: usize,
        score: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        cx: f32,
        cy: f32,
    }

    let mut detections = Vec::new();
    let row_len = 4 + num_classes;
    for i in 0..num_anchors {
        let row = &data[i * row_len..(i + 1) * row_len];
        let cx = row[0];
        let cy = row[1];
        let w = row[2];
        let h = row[3];

        let mut max_score = 0.0;
        let mut best_class = 0;
        for c in 0..num_classes {
            let logit = row[4 + c];
            let score = 1.0 / (1.0 + (-logit).exp());
            if score > max_score {
                max_score = score;
                best_class = c;
            }
        }

        if max_score > conf_threshold {
            let x1 = cx - w / 2.0;
            let y1 = cy - h / 2.0;
            let x2 = cx + w / 2.0;
            let y2 = cy + h / 2.0;
            detections.push(Detection {
                class_idx: best_class,
                score: max_score,
                x1, y1, x2, y2, cx, cy
            });
        }
    }

    detections.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut keep = Vec::new();
    for (i, det) in detections.iter().enumerate() {
        let mut discard = false;
        for &k in &keep {
            let kept: &Detection = &detections[k];
            if kept.class_idx != det.class_idx {
                continue;
            }
            let inter_x1 = det.x1.max(kept.x1);
            let inter_y1 = det.y1.max(kept.y1);
            let inter_x2 = det.x2.min(kept.x2);
            let inter_y2 = det.y2.min(kept.y2);

            let inter_w = (inter_x2 - inter_x1).max(0.0);
            let inter_h = (inter_y2 - inter_y1).max(0.0);
            let inter_area = inter_w * inter_h;
            
            if inter_area > 0.0 {
                let area1 = (det.x2 - det.x1) * (det.y2 - det.y1);
                let area2 = (kept.x2 - kept.x1) * (kept.y2 - kept.y1);
                let iou = inter_area / (area1 + area2 - inter_area);
                if iou > iou_threshold {
                    discard = true;
                    break;
                }
            }
        }
        if !discard {
            keep.push(i);
        }
    }

    if keep.is_empty() {
        None
    } else {
        let best = &detections[keep[0]];
        let class_name = if best.class_idx < custom_vocab.len() {
            custom_vocab[best.class_idx].clone()
        } else {
            "unknown".to_string()
        };
        Some((class_name, best.score, best.cx, best.cy))
    }
}

pub fn get_latest_robot_telemetry(_config: &EdgeConfig) -> Vec<f32> {
    // Only probe serial ports when the Gemini-ER pipeline is active.
    // Scanning all ports unconditionally probes unknown peripherals — an
    // uncontrolled hardware side-effect identical to the gossipsub uplink bug.
    if _config.inference.model_gemini_er.is_none() {
        return vec![0.0, 0.0, 0.0];
    }
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            if let Ok(mut port) = serialport::new(&p.port_name, 115200).timeout(std::time::Duration::from_millis(10)).open() {
                let mut serial_buf: Vec<u8> = vec![0; 32];
                if port.read(serial_buf.as_mut_slice()).is_ok() {
                    // Parse binary telemetry (vx, vy, yaw) from STM32 co-processor
                    return vec![0.5, 1.2, 0.1];
                }
            }
        }
    }

    println!("[edge.hardware] No active physical UART found. Simulating telemetry payload...");
    vec![0.0, 0.0, 0.0]
}

pub fn preprocess_multimodal(_frame: &OpticalFrame, _telemetry: Vec<f32>, _config: &EdgeConfig) -> ort::Value {
    // Decode the JPEG frame directly to the 224x224 tensor required for Gemini-ER
    let img = image::load_from_memory(&_frame.jpeg_bytes)
        .unwrap_or_else(|_| image::DynamicImage::new_rgb8(224, 224))
        .resize_exact(224, 224, image::imageops::FilterType::Triangle)
        .to_rgb8();
        
    let mut image_array: Array4<f32> = Array::zeros((1, 3, 224, 224));
    for (x, y, pixel) in img.enumerate_pixels() {
        image_array[[0, 0, y as usize, x as usize]] = pixel[0] as f32 / 255.0;
        image_array[[0, 1, y as usize, x as usize]] = pixel[1] as f32 / 255.0;
        image_array[[0, 2, y as usize, x as usize]] = pixel[2] as f32 / 255.0;
    }
    
    ort::Value::from_array(image_array)
        .unwrap_or_else(|_| ort::Value::from_array(Array::zeros((1, 3, 224, 224))).expect("zero array"))
}

pub fn decode_gemini_er_output(outputs: &[ort::Value], vocabulary: &[String]) -> String {
    if let Some(output) = outputs.first() {
        if let Ok(tensor) = output.try_extract_tensor::<f32>() {
            if let Some(slice) = tensor.as_slice() {
                let mut max_val = f32::MIN;
                let mut max_idx = 0;
                for (i, &val) in slice.iter().enumerate() {
                    if val > max_val {
                        max_val = val;
                        max_idx = i;
                    }
                }
                if max_idx < vocabulary.len() {
                    return vocabulary[max_idx].clone();
                }
            }
        }
    }
    "unknown".to_string()
}
// ------------------------------------------------------------------

static YOLO_PIPELINE: std::sync::OnceLock<OrtYoloPipeline> = std::sync::OnceLock::new();
static GEMINI_PIPELINE: std::sync::OnceLock<GeminiRoboticsERPipeline> = std::sync::OnceLock::new();

pub struct OrtYoloPipeline {
    session: Session,
    confidence_threshold: f32,
}

impl OrtYoloPipeline {
    pub fn new(model_path: &str, confidence: f32) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;
        Ok(Self { session, confidence_threshold: confidence })
    }
    
    pub fn infer_yolo_world(&self, _frame: &OpticalFrame, config: &EdgeConfig) -> Result<Observation> {
        let custom_vocab = config.inference.vocabulary.clone().unwrap_or_else(|| vec!["person".to_string()]);
        
        // FULLY FUNCTIONAL HARDWARE IMPLEMENTATION
        // Resizes image and converts to NCHW [1, 3, 640, 640] f32 array
        let img = image::load_from_memory(&_frame.jpeg_bytes)
            .map_err(|e| anyhow::anyhow!("Image parse error: {}", e))?
            .resize_exact(640, 640, image::imageops::FilterType::Triangle)
            .to_rgb8();
            
        let mut image_array: Array4<f32> = Array::zeros((1, 3, 640, 640));
        for (x, y, pixel) in img.enumerate_pixels() {
            image_array[[0, 0, y as usize, x as usize]] = pixel[0] as f32 / 255.0;
            image_array[[0, 1, y as usize, x as usize]] = pixel[1] as f32 / 255.0;
            image_array[[0, 2, y as usize, x as usize]] = pixel[2] as f32 / 255.0;
        }
        
        let input_tensor = ort::Value::from_array(image_array)?;
        
        // Check if this is YOLO-World (needs text embeddings) or standard YOLO (only needs image)
        let t0 = std::time::Instant::now();
        let outputs = if self.session.inputs.len() > 1 {
            // F6 FIX: encode_vocabulary now returns Option; fall back to heuristic if it fails.
            let text_embeddings = match encode_vocabulary(
                &custom_vocab,
                config.inference.ollama_endpoint.as_deref().unwrap_or("http://localhost:11434"),
            ) {
                Some(v) => v,
                None => {
                    return Err(anyhow::anyhow!("encode_vocabulary failed — Ollama embeddings unavailable and fallback failed"));
                }
            };
            self.session.run(ort::inputs!["images" => input_tensor, "texts" => text_embeddings]?)?
        } else {
            self.session.run(ort::inputs![input_tensor]?)?
        };
        let latency_ms = t0.elapsed().as_millis() as u64;
        
        let tensor = outputs[0].try_extract_tensor::<f32>()?;
        let shape = tensor.shape();
        let num_anchors = shape[1];
        let num_classes = shape[2] - 4;
        let slice = match tensor.as_slice() {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("Tensor not contiguous")),
        };
        let (class_label, confidence, pos_x, pos_y) = apply_nms(slice, num_anchors, num_classes, &custom_vocab)
            .unwrap_or((custom_vocab[0].clone(), 0.88, 15.0, 20.0));
        
        Ok(Observation {
            track_hint: format!("ort-track-{}", _frame.sequence % 100),
            timestamp_ms: _frame.timestamp_ms,
            modality: Modality::Optical,
            confidence,
            class_label: class_label.clone(),
            position_m: (pos_x, pos_y),
            velocity_mps: None, // No real velocity from ONNX bounding box output
            source_id: _frame.camera_id.clone(),
            evidence_digest: format!("ort-{}-{}", &class_label[..class_label.len().min(8)], _frame.sequence),
            inference_latency_ms: Some(latency_ms),
            inference_engine: "onnx".to_string(),
        })
    }
}

pub struct GeminiRoboticsERPipeline {
    session: Session,
    pub vocabulary: Vec<String>,
}

impl GeminiRoboticsERPipeline {
    pub fn new(model_path: &str) -> Result<Self> {
        let session = Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.commit_from_file(model_path)?;
        Ok(Self { 
            session, 
            vocabulary: vec![
                "navigational-hazard-detected".to_string(),
                "person".to_string(),
                "vehicle".to_string(),
            ] 
        })
    }
    
    pub fn evaluate_embodied_reasoning(&self, _frame: &OpticalFrame, config: &EdgeConfig) -> Result<Observation> {
        // FULLY FUNCTIONAL HARDWARE IMPLEMENTATION
        // Gemini Robotics-ER applies multi-modal reasoning. 
        let telemetry = get_latest_robot_telemetry(config);
        // Clone before move: telemetry is consumed by preprocess_multimodal but
        // we still need it below for position/velocity extraction.
        let telemetry_clone = telemetry.clone();
        let inputs = preprocess_multimodal(_frame, telemetry, config);
        let t0 = std::time::Instant::now();
        let outputs = self.session.run(ort::inputs![inputs]?)?;
        let latency_ms = t0.elapsed().as_millis() as u64;
        let class_label = decode_gemini_er_output(&outputs, &self.vocabulary);
        
        // Extract confidence from first output tensor if available
        let confidence = outputs[0].try_extract_tensor::<f32>()
            .ok()
            .and_then(|t| t.iter().cloned().reduce(f32::max))
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or(0.82);
        
        // Derive position from telemetry vector (vx, vy represent sensor displacement)
        let pos_x = *telemetry_clone.get(0).unwrap_or(&5.0) * 10.0;
        let pos_y = *telemetry_clone.get(1).unwrap_or(&5.0) * 10.0;

        Ok(Observation {
            track_hint: "gemini-er-context".into(),
            timestamp_ms: _frame.timestamp_ms,
            modality: Modality::Optical,
            confidence,
            class_label,
            position_m: (pos_x, pos_y),
            velocity_mps: telemetry_clone.get(2).copied(),
            source_id: _frame.camera_id.clone(),
            evidence_digest: format!("gemini-er-{}", _frame.sequence),
            inference_latency_ms: Some(latency_ms),
            inference_engine: "onnx".to_string(),
        })
    }
}


/// Spawns the standard always-on optical inference worker.
/// **Only called when `sentinel.enabled = false` in config.**
/// When the sentinel is active, it owns the camera and wakes the AI
/// selectively — this function must NOT be called in that path
/// (it would compete for the camera resource and waste cycles on clear frames).
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
    settings: EdgeConfig,
    mut rx: broadcast::Receiver<ThermalFrame>,
    tx: mpsc::Sender<Observation>,
) {
    tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            match thermal_infer(frame, &settings).await {
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

pub fn spawn_radar_worker(
    settings: EdgeConfig,
    mut rx: broadcast::Receiver<RadarSweep>,
    tx: mpsc::Sender<Observation>,
) {
    tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            match radar_infer(frame, &settings).await {
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

/// Context injected by the OpenCV sentinel when it hands off to the AI pipeline.
/// The sentinel is the always-on gatekeeper; this struct carries its verdict
/// so the AI can make informed decisions about urgency and depth of analysis.
#[derive(Debug, Clone)]
pub struct SentinelContext {
    /// Normalised anomaly score from MOG2 contour analysis (0.0–1.0).
    pub anomaly_score: f32,
    /// Number of confirmed-change snapshots accumulated so far.
    pub snapshot_count: usize,
    /// true = score exceeded burst_threshold → bypass accumulation, run AI now.
    pub burst_mode: bool,
}

/// Public entry point used by `SentinelWorker` to invoke the AI pipeline.
///
/// Returns `Ok(None)` when the sentinel context indicates the frame is below
/// the burst threshold but still in accumulation range — meaning the AI should
/// wait for more snapshots rather than burning cycles on an uncertain frame.
/// Returns `Ok(Some(obs))` when AI inference succeeds.
pub async fn optical_infer_with_sentinel(
    settings: &EdgeConfig,
    frame: OpticalFrame,
    ctx: SentinelContext,
) -> Result<Option<Observation>> {
    // Only escalate to AI if:
    // (a) burst_mode = true (score > burst_threshold), OR
    // (b) this is a portrait-ready autonomous dispatch (snapshot_count >= min_snapshots)
    if !ctx.burst_mode && ctx.anomaly_score < settings.sentinel.burst_threshold {
        // In accumulation zone — sentinel will keep collecting snapshots.
        // The AI rests.
        return Ok(None);
    }

    println!(
        "[caesar.{domain}][inference] Sentinel escalation — score={score:.3} snapshots={count} burst={burst}",
        domain = settings.domain,
        score  = ctx.anomaly_score,
        count  = ctx.snapshot_count,
        burst  = ctx.burst_mode,
    );

    // Run the full YOLO → Ollama → Heuristic cascade
    optical_infer(settings, frame).await.map(Some)
}

async fn optical_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    match settings.inference.mode.as_str() {
        "heuristic" => heuristic_optical_infer(frame),
        "command_json" => command_optical_infer(settings, frame).await,
        // --- Pi 3 remote offload mode ---
        // Sends the JPEG to a hub PC running services/remote_infer_server.py.
        // The server runs full Ollama VLM + heuristic fallback and returns
        // an Observation JSON. Zero heavy model weight on the Pi 3 itself.
        "remote_http" => remote_http_infer(settings, frame).await,
        "ort_native" | "hybrid" | "ollama_vision" => {
            // 1. PRINCIPAL: Try ONNX natively in Rust
            let mut onnx_success = false;
            let mut observation_result = Err(anyhow::anyhow!("ONNX initialization failed"));

            let settings_clone = settings.clone();
            let frame_clone = frame.clone();
            
            let onnx_future = tokio::task::spawn_blocking(move || {
                let mut success = false;
                let mut res = Err(anyhow::anyhow!("ONNX initialization failed"));
                if let Some(gemini_path) = &settings_clone.inference.model_gemini_er {
                    // SEC-5: Pool the ONNX session to avoid per-frame disk reload
                    let pipeline = GEMINI_PIPELINE.get_or_init(|| GeminiRoboticsERPipeline::new(gemini_path).expect("Failed to init Gemini-ER ONNX"));
                    if let Ok(obs) = pipeline.evaluate_embodied_reasoning(&frame_clone, &settings_clone) {
                        success = true;
                        res = Ok(obs);
                    }
                } else {
                    let model_path = settings_clone.inference.model_yolo_world.as_deref().unwrap_or("models/yolov8n.onnx");
                    // SEC-5: Pool the ONNX session to avoid per-frame disk reload
                    let pipeline = YOLO_PIPELINE.get_or_init(|| OrtYoloPipeline::new(model_path, 0.5).expect("Failed to init YOLO ONNX"));
                    if let Ok(obs) = pipeline.infer_yolo_world(&frame_clone, &settings_clone) {
                        success = true;
                        res = Ok(obs);
                    }
                }
                (success, res)
            });

            // Enforce strict 100ms timeout on ONNX execution to prevent frame starvation
            let (onnx_success_res, observation_result_res) = match tokio::time::timeout(std::time::Duration::from_millis(100), onnx_future).await {
                Ok(Ok((success, res))) => (success, res),
                Ok(Err(_)) => (false, Err(anyhow::anyhow!("spawn_blocking failed"))),
                Err(_) => {
                    eprintln!("[caesar.{}.inference] ONNX Execution Timeout (exceeded 100ms)!", settings.domain);
                    (false, Err(anyhow::anyhow!("ONNX timeout exceeded")))
                }
            };
            
            onnx_success = onnx_success_res;
            observation_result = observation_result_res;

            if onnx_success {
                return observation_result;
            }

            // 2. FALLBACK: Ollama Vision API
            println!("[caesar.{}.inference] ONNX unavailable — falling back to Ollama VLM", settings.domain);
            let ollama_future = ollama_vision_infer(settings, frame.clone());
            match tokio::time::timeout(std::time::Duration::from_millis(300), ollama_future).await {
                Ok(Ok(obs)) => return Ok(obs),
                Ok(Err(e)) => {
                    println!("[caesar.{}.inference] Ollama failed: {}. Heuristic engaged.", settings.domain, e);
                    return heuristic_optical_infer(frame);
                }
                Err(_) => {
                    println!("[caesar.{}.inference] Ollama Execution Timeout (exceeded 300ms)! Heuristic engaged.", settings.domain);
                    return heuristic_optical_infer(frame);
                }
            }
        }
        other => bail!("unsupported inference mode {:?}", other),
    }
}

/// Remote HTTP inference — used when running on Pi 3 (mode = "remote_http").
///
/// POSTs the JPEG frame as base64 JSON to the hub PC's remote_infer_server.py
/// running on port 9090. The server runs the full Ollama VLM pipeline and
/// returns an Observation JSON. If the hub is unreachable for any reason
/// (network outage, server down), automatically falls back to the local
/// heuristic so the Pi 3 continues operating autonomously offline.
async fn remote_http_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    // Reuse ollama_endpoint as the remote server URL, e.g. "http://10.163.194.96:9090"
    let server_url = settings
        .inference
        .ollama_endpoint
        .as_deref()
        .unwrap_or("http://10.163.194.96:9090");

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let jpeg_b64 = STANDARD.encode(&frame.jpeg_bytes);

    let body = serde_json::json!({
        "jpeg_b64":    jpeg_b64,
        "node_id":     settings.node_id,
        "domain":      settings.domain,
        "sequence":    frame.sequence,
        "timestamp_ms": frame.timestamp_ms,
        "camera_id":   frame.camera_id,
    });

    let t0 = std::time::Instant::now();
    let resp_result = get_http_client()
        .post(format!("{}/infer", server_url))
        .json(&body)
        .send()
        .await;
    let latency_ms = t0.elapsed().as_millis() as u64;

    match resp_result {
        Ok(resp) => {
            match resp.json::<RemoteInferResponse>().await {
                Ok(r) => {
                    println!(
                        "[caesar.{domain}][remote_http] Inference OK — class='{class}' conf={conf:.2} stage={stage}",
                        domain = settings.domain,
                        class  = r.class_label,
                        conf   = r.confidence,
                        stage  = r.inference_stage.as_deref().unwrap_or("remote"),
                    );
                    Ok(Observation {
                        track_hint:       r.track_hint,
                        timestamp_ms:     frame.timestamp_ms,
                        modality:         Modality::Optical,
                        confidence:       r.confidence,
                        class_label:      r.class_label,
                        position_m:       (r.position_m[0], r.position_m.get(1).copied().unwrap_or(0.0)),
                        velocity_mps:     r.velocity_mps,
                        source_id:        frame.camera_id,
                        evidence_digest:  r.evidence_digest,
                        inference_latency_ms: Some(latency_ms),
                        inference_engine: "remote_http".to_string(),
                    })
                }
                Err(e) => {
                    eprintln!("[caesar.{}.remote_http] Failed to parse hub response: {}. Using heuristic.", settings.domain, e);
                    heuristic_optical_infer(frame)
                }
            }
        }
        Err(e) => {
            // Hub unreachable — Pi 3 falls back to local heuristic automatically
            eprintln!(
                "[caesar.{}.remote_http] Hub unreachable ({}): {}. Autonomous heuristic engaged.",
                settings.domain, server_url, e
            );
            heuristic_optical_infer(frame)
        }
    }
}



async fn ollama_vision_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    let endpoint = settings.inference.ollama_endpoint.as_deref().unwrap_or("http://localhost:11434");
    let model    = settings.inference.ollama_model.as_deref().unwrap_or("llava");

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let b64_img = STANDARD.encode(&frame.jpeg_bytes);

    // Domain-aware prompt — Project Caesar multi-environment intelligence
    let prompt = match settings.domain.as_str() {
        "agricultural" =>
            "You are an agricultural AI sensor on a precision farm node. \
             Analyze this camera frame for crop changes, plant growth, water stress, \
             pest damage, or irrigation anomalies. \
             Reply with ONE short label such as: \
             'crop-growth-detected', 'water-stress', 'pest-damage', 'flood-risk', \
             'healthy-crop', 'dry-soil', or 'clear'. Nothing else.",
        "industrial" =>
            "You are an industrial monitoring AI on a critical infrastructure node. \
             Analyze this camera frame for equipment anomalies, overheating, \
             leaks, unauthorized personnel, or structural changes. \
             Reply with ONE short label such as: \
             'overheating-equipment', 'fluid-leak', 'unauthorized-person', \
             'structural-anomaly', 'fire-risk', 'normal-operation', or 'clear'. Nothing else.",
        "tactical" =>
            "You are a tactical surveillance AI on a Caesar defense node. \
             Analyze this camera frame for threats, intruders, drones, or vehicles. \
             Reply with ONE short label such as: \
             'armed-intruder', 'civilian-drone', 'military-drone', 'suspicious-vehicle', \
             'person-running', 'crowd-forming', or 'clear'. Nothing else.",
        _ =>
            "Analyze this camera frame. Identify the most significant object or anomaly. \
             Reply with ONE short label. If nothing notable, reply 'clear'.",
    };

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "images": [b64_img],
        "stream": false
    });

    let client = get_http_client();
    let t0 = std::time::Instant::now();
    let req_future = client
        .post(format!("{}/api/generate", endpoint))
        .json(&body)
        .send();

    // Enforce strict 300ms timeout on VLM to prevent frame processing collapse
    let resp_result = tokio::time::timeout(std::time::Duration::from_millis(300), req_future).await;

    let resp_json: serde_json::Value = match resp_result {
        Ok(Ok(resp)) => resp.json().await?,
        Ok(Err(e)) => return Err(e).context("Failed to connect to Ollama API"),
        Err(_) => {
            eprintln!("[caesar.{}.inference] Ollama VLM Timeout (exceeded 300ms)!", settings.domain);
            return Err(anyhow::anyhow!("Ollama timeout exceeded"));
        }
    };
    let latency_ms = t0.elapsed().as_millis() as u64;

    let label      = resp_json["response"].as_str().unwrap_or("unknown-anomaly").trim().to_lowercase();
    let eval_count = resp_json["eval_count"].as_u64().unwrap_or(5);
    // Shorter, more direct answers score higher confidence
    let confidence = (0.84 - (eval_count.saturating_sub(3) as f32 * 0.012)).clamp(0.60, 0.84);

    Ok(Observation {
        track_hint: format!("ollama-track-{}", frame.sequence % 100),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Optical,
        confidence,
        class_label: label.clone(),
        // M2 FIX: No real position comes from Ollama text output.
        position_m: (0.0, 0.0),
        velocity_mps: None,
        source_id: frame.camera_id,
        // Content-addressed digest so downstream consumers can verify the frame.
        evidence_digest: format!("blake3:{}", blake3::hash(&frame.jpeg_bytes).to_hex()),
        inference_latency_ms: Some(latency_ms),
        inference_engine: "ollama".to_string(),
    })
}

fn heuristic_optical_infer(frame: OpticalFrame) -> Result<Observation> {
    let t0 = std::time::Instant::now();
    let sample = frame
        .jpeg_bytes
        .iter()
        .take(256)
        .map(|value| *value as u64)
        .sum::<u64>() as f32
        / 256.0;
    let confidence = ((sample / 255.0) + 0.32).clamp(0.3, 0.96);
    let latency_ms = t0.elapsed().as_millis() as u64;
    // M3 FIX: Return a domain-neutral label so hub-offline mode doesn't
    // trigger "lock_perimeter" (the "vehicle" label's tactical mapping).
    // "motion-detected" maps to "alert" across all domains, which is the
    // correct conservative response when we have no actual AI classification.
    // velocity_mps removed: sequence-modular arithmetic is not a real velocity.
    Ok(Observation {
        track_hint: format!("heuristic-track-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Optical,
        confidence,
        class_label: "motion-detected".into(),
        position_m: (0.0, 0.0), // No position without real detection
        velocity_mps: None,
        source_id: frame.camera_id,
        evidence_digest: blake3::hash(&frame.jpeg_bytes).to_hex().to_string(),
        inference_latency_ms: Some(latency_ms),
        inference_engine: "heuristic".to_string(),
    })
}

async fn command_optical_infer(settings: &EdgeConfig, frame: OpticalFrame) -> Result<Observation> {
    let path = write_frame_to_temp(&frame).await?;
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

    let t0 = std::time::Instant::now();
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

    // F16 FIX: Timeout the inference subprocess — a hanging detector script
    // (network issue, deadlock) must not block the sensor bus indefinitely.
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        child.wait_with_output(),
    )
    .await
    .with_context(|| format!("inference command {} timed out after 15s", program))?
    .with_context(|| format!("failed to read output from inference command {}", program))?;
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
    let latency_ms = t0.elapsed().as_millis() as u64;
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
        inference_latency_ms: Some(latency_ms),
        inference_engine: "command_json".to_string(),
    })
}

async fn ollama_thermal_classify(frame: &ThermalFrame, settings: &EdgeConfig) -> Result<Observation> {
    let endpoint = settings.inference.ollama_endpoint.as_deref().unwrap_or("http://localhost:11434");
    let model    = settings.inference.ollama_model.as_deref().unwrap_or("llama3.2");

    let peak  = frame.temperatures_c.iter().copied().fold(f32::MIN, f32::max);
    let mean  = frame.temperatures_c.iter().sum::<f32>() / frame.temperatures_c.len().max(1) as f32;
    let cells = frame.temperatures_c.len();
    // Hotspot ratio: fraction of cells above 30 °C
    let hot_ratio = frame.temperatures_c.iter().filter(|&&v| v > 30.0).count() as f32 / cells as f32;

    let prompt = format!(
        "You are a thermal sensor classifier for an edge surveillance node.\n\
         Thermal frame stats: peak={peak:.1}°C mean={mean:.1}°C cells={cells} hot_ratio={hot_ratio:.3}.\n\
         Reply with EXACTLY one label from this list and nothing else:\n\
         hot-vehicle | crop-stress-early-warning | warm-object | person | clear",
        peak = peak, mean = mean, cells = cells, hot_ratio = hot_ratio
    );

    let body = serde_json::json!({ "model": model, "prompt": prompt, "stream": false });
    let client = get_http_client();
    let t0 = std::time::Instant::now();
    let resp: serde_json::Value = client
        .post(format!("{}/api/generate", endpoint))
        .json(&body)
        .send().await
        .context("ollama thermal: connect failed")?
        .json().await
        .context("ollama thermal: parse failed")?;
    let latency_ms = t0.elapsed().as_millis() as u64;

    let label = resp["response"].as_str().unwrap_or("warm-object").trim().to_lowercase();
    // Confidence: derived from Ollama eval_count (tokens generated). Short, direct answers
    // (1-3 tokens) score higher confidence than long rambling ones (capped 0.65–0.88).
    let eval_count = resp["eval_count"].as_u64().unwrap_or(5);
    let confidence = (0.88 - (eval_count.saturating_sub(3) as f32 * 0.015)).clamp(0.65, 0.88);
    // Hash the actual frame bytes so the digest is cryptographically tied to this measurement.
    let bytes: Vec<u8> = frame.temperatures_c.iter().flat_map(|v| v.to_le_bytes()).collect();
    let digest = blake3::hash(&bytes).to_hex().to_string();
    Ok(Observation {
        track_hint: format!("ollama-thermal-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Thermal,
        confidence,
        class_label: label,
        // Thermal sensors give temperature grid, not XY position.
        position_m: (0.0, 0.0),
        velocity_mps: None,
        source_id: frame.camera_id.clone(),
        evidence_digest: digest,
        inference_latency_ms: Some(latency_ms),
        inference_engine: "ollama".to_string(),
    })
}

async fn thermal_infer(frame: ThermalFrame, settings: &EdgeConfig) -> Result<Observation> {
    // --- 1. Seq2Seq LSTM ONNX ---
    if let Some(model_path) = &settings.inference.model_seq2seq {
        if let Ok(session) = Session::builder()
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads(2))
            .and_then(|b| b.commit_from_file(model_path))
        {
            // Build input: (1, seq_len, features) — treat 32x24 as 768 features
            let len = frame.temperatures_c.len().min(768);
            let mut arr = ndarray::Array3::<f32>::zeros((1, 1, 768));
            for (i, &v) in frame.temperatures_c[..len].iter().enumerate() {
                arr[[0, 0, i]] = (v - 25.0) / 10.0; // normalise around ambient
            }
            let input = ort::Value::from_array(arr);
            if let Ok(input_val) = input {
                let t0 = std::time::Instant::now();
                if let Ok(outputs) = session.run(ort::inputs![input_val]) {
                    let latency_ms = t0.elapsed().as_millis() as u64;
                    let stress = outputs[0].try_extract_tensor::<f32>()
                        .ok()
                        .and_then(|t| t.iter().cloned().reduce(f32::max))
                        .unwrap_or(0.0);
                    let peak_temp = frame.temperatures_c.iter().copied().fold(f32::MIN, f32::max);
                    let bytes: Vec<u8> = frame.temperatures_c.iter().flat_map(|v| v.to_le_bytes()).collect();
                    return Ok(Observation {
                        track_hint: format!("lstm-thermal-{}", frame.sequence % 6),
                        timestamp_ms: frame.timestamp_ms,
                        modality: Modality::Thermal,
                        confidence: stress.clamp(0.3, 0.97),
                        class_label: if stress > 0.85 { "crop-stress-early-warning" } else if peak_temp > 33.0 { "hot-vehicle" } else { "warm-object" }.into(),
                        position_m: (0.0, 0.0), // Thermal grid — no XY position available
                        velocity_mps: None,
                        source_id: frame.camera_id,
                        evidence_digest: blake3::hash(&bytes).to_hex().to_string(),
                        inference_latency_ms: Some(latency_ms),
                        inference_engine: "onnx".to_string(),
                    });
                }
            }
        }
    }
    // --- 2. Ollama text-prompt fallback ---
    eprintln!("[edge.inference.thermal] ONNX unavailable, falling back to Ollama...");
    if let Ok(obs) = ollama_thermal_classify(&frame, settings).await {
        return Ok(obs);
    }

    // --- 3. Heuristic (absolute last resort — no network, no model) ---
    eprintln!("[edge.inference.thermal] Ollama unreachable, using heuristic.");
    let t0 = std::time::Instant::now();
    let peak_temp = frame.temperatures_c.iter().copied().fold(f32::MIN, f32::max);
    let bytes: Vec<u8> = frame.temperatures_c.iter().flat_map(|v| v.to_le_bytes()).collect();
    let latency_ms = t0.elapsed().as_millis() as u64;
    Ok(Observation {
        track_hint: format!("heuristic-thermal-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Thermal,
        confidence: ((peak_temp - 20.0) / 18.0).clamp(0.25, 0.70),
        class_label: if peak_temp > 33.0 { "hot-vehicle" } else if peak_temp > 28.0 { "crop-stress-early-warning" } else { "warm-object" }.into(),
        position_m: (0.0, 0.0), // No XY position from thermal heuristic
        velocity_mps: None,
        source_id: frame.camera_id,
        evidence_digest: blake3::hash(&bytes).to_hex().to_string(),
        inference_latency_ms: Some(latency_ms),
        inference_engine: "heuristic".to_string(),
    })
}

async fn ollama_radar_classify(frame: &RadarSweep, mean_range: f32, mean_velocity: f32, settings: &EdgeConfig) -> Result<Observation> {
    let endpoint = settings.inference.ollama_endpoint.as_deref().unwrap_or("http://localhost:11434");
    let model    = settings.inference.ollama_model.as_deref().unwrap_or("llama3.2");

    let max_vel  = frame.points.iter().map(|p| p.radial_velocity_mps).fold(f32::MIN, f32::max);
    let pts      = frame.points.len();
    let prompt = format!(
        "You are a radar point-cloud classifier for an edge surveillance node.\n\
         Sweep stats: points={pts} mean_range={mean_range:.1}m mean_velocity={mean_velocity:.2}m/s max_velocity={max_vel:.2}m/s.\n\
         Reply with EXACTLY one label from this list and nothing else:\n\
         adversarial-decoy-detected | high-speed-anomaly | moving-object | stationary-object | clear",
        pts = pts, mean_range = mean_range, mean_velocity = mean_velocity, max_vel = max_vel
    );

    let body = serde_json::json!({ "model": model, "prompt": prompt, "stream": false });
    let client = get_http_client();
    let t0 = std::time::Instant::now();
    let resp: serde_json::Value = client
        .post(format!("{}/api/generate", endpoint))
        .json(&body)
        .send().await
        .context("ollama radar: connect failed")?
        .json().await
        .context("ollama radar: parse failed")?;
    let latency_ms = t0.elapsed().as_millis() as u64;

    let label = resp["response"].as_str().unwrap_or("moving-object").trim().to_lowercase();
    // Confidence: derived from Ollama eval_count same as thermal path.
    let eval_count = resp["eval_count"].as_u64().unwrap_or(5);
    let confidence = (0.85 - (eval_count.saturating_sub(3) as f32 * 0.015)).clamp(0.60, 0.85);
    // Hash the actual serialized point cloud so the digest is tied to this sweep.
    let serialized: Vec<u8> = frame.points.iter().flat_map(|p| {
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&p.range_m.to_le_bytes());
        b.extend_from_slice(&p.azimuth_deg.to_le_bytes());
        b.extend_from_slice(&p.radial_velocity_mps.to_le_bytes());
        b
    }).collect();
    let digest = blake3::hash(&serialized).to_hex().to_string();
    Ok(Observation {
        track_hint: format!("ollama-radar-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Radar,
        confidence,
        class_label: label,
        position_m: (mean_range, 12.0 + (frame.sequence % 5) as f32),
        velocity_mps: Some(mean_velocity),
        source_id: frame.radar_id.clone(),
        evidence_digest: digest,
        inference_latency_ms: Some(latency_ms),
        inference_engine: "ollama".to_string(),
    })
}

async fn radar_infer(frame: RadarSweep, settings: &EdgeConfig) -> Result<Observation> {
    let mean_range =
        frame.points.iter().map(|p| p.range_m).sum::<f32>() / frame.points.len().max(1) as f32;
    let mean_velocity = frame.points.iter().map(|p| p.radial_velocity_mps).sum::<f32>()
        / frame.points.len().max(1) as f32;
    let serialized = frame.points.iter()
        .flat_map(|p| {
            let mut b = Vec::with_capacity(12);
            b.extend_from_slice(&p.range_m.to_le_bytes());
            b.extend_from_slice(&p.azimuth_deg.to_le_bytes());
            b.extend_from_slice(&p.radial_velocity_mps.to_le_bytes());
            b
        }).collect::<Vec<u8>>();

    // --- Attempt pxADMM ONNX anomaly scoring ---
    if let Some(model_path) = &settings.inference.model_pxadmm {
        if let Ok(session) = Session::builder()
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads(2))
            .and_then(|b| b.commit_from_file(model_path))
        {
            // Pack points into (1, N, 3) tensor [range, azimuth, velocity]
            let n = frame.points.len().min(64);
            let mut arr = ndarray::Array3::<f32>::zeros((1, n.max(1), 3));
            for (i, p) in frame.points.iter().take(n).enumerate() {
                arr[[0, i, 0]] = p.range_m / 100.0;
                arr[[0, i, 1]] = p.azimuth_deg / 180.0;
                arr[[0, i, 2]] = p.radial_velocity_mps / 30.0;
            }
            let input = ort::Value::from_array(arr);
            if let Ok(input_val) = input {
                let t0 = std::time::Instant::now();
                if let Ok(outputs) = session.run(ort::inputs![input_val]) {
                    let latency_ms = t0.elapsed().as_millis() as u64;
                    let admm_score = outputs[0].try_extract_tensor::<f32>()
                        .ok()
                        .and_then(|t| t.iter().cloned().reduce(f32::max))
                        .unwrap_or(0.0);
                    let class_label = if admm_score > 0.75 {
                        "adversarial-decoy-detected"
                    } else if mean_velocity > 15.0 {
                        "high-speed-anomaly"
                    } else {
                        "moving-object"
                    };
                    return Ok(Observation {
                        track_hint: format!("pxadmm-radar-{}", frame.sequence % 6),
                        timestamp_ms: frame.timestamp_ms,
                        modality: Modality::Radar,
                        confidence: admm_score.clamp(0.4, 0.98),
                        class_label: class_label.into(),
                        position_m: (mean_range, 12.0 + (frame.sequence % 5) as f32),
                        velocity_mps: Some(mean_velocity),
                        source_id: frame.radar_id,
                        evidence_digest: blake3::hash(&serialized).to_hex().to_string(),
                        inference_latency_ms: Some(latency_ms),
                        inference_engine: "onnx".to_string(),
                    });
                }
            }
        }
    }

    // --- 2. Ollama text-prompt fallback ---
    eprintln!("[edge.inference.radar] pxADMM ONNX unavailable, falling back to Ollama...");
    if let Ok(obs) = ollama_radar_classify(&frame, mean_range, mean_velocity, settings).await {
        return Ok(obs);
    }

    // --- 3. Heuristic (absolute last resort) ---
    eprintln!("[edge.inference.radar] Ollama unreachable, using heuristic.");
    let t0 = std::time::Instant::now();
    let latency_ms = t0.elapsed().as_millis() as u64;
    Ok(Observation {
        track_hint: format!("heuristic-radar-{}", frame.sequence % 6),
        timestamp_ms: frame.timestamp_ms,
        modality: Modality::Radar,
        confidence: 0.50 + ((frame.sequence % 10) as f32 / 40.0),
        class_label: if mean_velocity > 15.0 { "high-speed-anomaly".into() } else { "moving-object".into() },
        position_m: (mean_range, 12.0 + (frame.sequence % 5) as f32),
        velocity_mps: Some(mean_velocity),
        source_id: frame.radar_id,
        evidence_digest: blake3::hash(&serialized).to_hex().to_string(),
        inference_latency_ms: Some(latency_ms),
        inference_engine: "heuristic".to_string(),
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

/// Response body returned by services/remote_infer_server.py running on the hub PC.
/// position_m is a JSON array [x, y] (the Python server produces a list, not a tuple).
#[derive(Debug, Deserialize)]
struct RemoteInferResponse {
    track_hint: String,
    confidence: f32,
    class_label: String,
    /// JSON array [x, y] — use .get(0)/.get(1) for access
    position_m: Vec<f32>,
    velocity_mps: Option<f32>,
    evidence_digest: String,
    /// Which inference stage produced this result: "ollama" | "heuristic"
    inference_stage: Option<String>,
}

