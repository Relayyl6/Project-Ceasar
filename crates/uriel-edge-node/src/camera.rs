use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tokio::{fs, process::Command};
use uriel_caesar_core::io::unix_time_ms;

use crate::config::OpticalSourceConfig;

#[derive(Debug, Clone)]
pub struct OpticalFrame {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub camera_id: String,
    pub width: u32,
    pub height: u32,
    pub jpeg_bytes: Vec<u8>,
}

pub async fn capture_optical_frame(
    config: &OpticalSourceConfig,
    sequence: u64,
) -> Result<OpticalFrame> {
    let jpeg_bytes = match config.mode.as_str() {
        "synthetic" => synthetic_jpeg(sequence, config.width, config.height),
        "file" => {
            let path = config
                .file_path
                .as_ref()
                .context("optical.file_path is required when mode=file")?;
            fs::read(path)
                .await
                .with_context(|| format!("failed to read optical source file {}", path))?
        }
        "command_stdout" => run_capture_command(config, sequence).await?,
        "profile_stdout" => run_profile_capture(config, sequence).await?,
        other => bail!("unsupported optical mode {:?}", other),
    };

    Ok(OpticalFrame {
        sequence,
        timestamp_ms: unix_time_ms(),
        camera_id: config.camera_id.clone(),
        width: config.width,
        height: config.height,
        jpeg_bytes,
    })
}

async fn run_capture_command(config: &OpticalSourceConfig, sequence: u64) -> Result<Vec<u8>> {
    let program = config
        .command_program
        .as_ref()
        .context("optical.command_program is required when mode=command_stdout")?;
    let args = config.command_args.clone().unwrap_or_default();
    run_command_bytes(program, &args, sequence).await
}

async fn run_profile_capture(config: &OpticalSourceConfig, sequence: u64) -> Result<Vec<u8>> {
    let (program, args) = build_profile_command(config)?;
    run_command_bytes(&program, &args, sequence).await
}

async fn run_command_bytes(program: &str, args: &[String], sequence: u64) -> Result<Vec<u8>> {
    let mut command = Command::new(program);
    for arg in args {
        command.arg(expand_arg(arg, sequence));
    }

    // A hung camera process (e.g. rpicam-jpeg waiting for the CSI bus, or ffmpeg
    // blocked on a v4l2 device that never delivers a frame) would hold a Tokio
    // worker thread indefinitely via `.await`, starving the rest of the pipeline.
    // 10 s is generous for a single-frame capture; real captures typically finish
    // in < 2 s. On timeout we kill the child implicitly when the `Child` future is
    // dropped (Tokio kills the process on drop by default since 1.x).
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        command.output(),
    )
    .await
    .with_context(|| format!("camera command {} timed out after 10 s", program))?
    .with_context(|| format!("failed to execute camera command {}", program))?;

    if !output.status.success() {
        bail!(
            "camera command {} exited with {}: {}",
            program,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if output.stdout.is_empty() {
        bail!("camera command {} produced no stdout bytes", program);
    }

    Ok(output.stdout)
}

fn build_profile_command(config: &OpticalSourceConfig) -> Result<(String, Vec<String>)> {
    let profile = config
        .profile
        .as_ref()
        .context("optical.profile is required when mode=profile_stdout")?;

    let mut args = match profile.as_str() {
        "rpi_csi_jpeg" => vec![
            "--width".to_string(),
            config.width.to_string(),
            "--height".to_string(),
            config.height.to_string(),
            "--timeout".to_string(),
            "1".to_string(),
            "--nopreview".to_string(),
            "--output".to_string(),
            "-".to_string(),
        ],
        "arducam_v4l2_ffmpeg" | "v4l2_ffmpeg_mjpeg" => {
            let device = config
                .device
                .clone()
                .unwrap_or_else(|| "/dev/video0".to_string());
            let pixel_format = config
                .pixel_format
                .clone()
                .unwrap_or_else(|| "mjpeg".to_string());
            vec![
                "-f".to_string(),
                "v4l2".to_string(),
                "-input_format".to_string(),
                pixel_format,
                "-video_size".to_string(),
                format!("{}x{}", config.width, config.height),
                "-i".to_string(),
                device,
                "-frames:v".to_string(),
                "1".to_string(),
                "-f".to_string(),
                "image2pipe".to_string(),
                "-vcodec".to_string(),
                "mjpeg".to_string(),
                "-".to_string(),
            ]
        }
        other => bail!("unsupported optical profile {:?}", other),
    };

    if let Some(extra_args) = &config.command_args {
        args.extend(extra_args.clone());
    }

    let program = match profile.as_str() {
        "rpi_csi_jpeg" => "rpicam-jpeg".to_string(),
        "arducam_v4l2_ffmpeg" | "v4l2_ffmpeg_mjpeg" => "ffmpeg".to_string(),
        _ => unreachable!(),
    };

    Ok((program, args))
}

/// Write a captured frame to a temporary JPEG file and return its path.
///
/// **Caller is responsible for deleting the file** once it is no longer needed
/// (e.g. after the inference subprocess has consumed it). The file is named
/// `uriel_frame_{camera_id}_{sequence}.jpg` inside the OS temp directory.
///
/// This function is `async` to avoid blocking the Tokio runtime thread on the
/// `fs::write` syscall; using synchronous `std::fs::write` here would stall
/// other tasks sharing the same worker thread during the disk write.
pub async fn write_frame_to_temp(frame: &OpticalFrame) -> Result<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "uriel_frame_{}_{}.jpg",
        frame.camera_id, frame.sequence
    ));
    fs::write(&path, &frame.jpeg_bytes)
        .await
        .with_context(|| format!("failed to write frame to temp path {}", path.display()))?;
    Ok(path)
}

fn expand_arg(arg: &str, sequence: u64) -> String {
    arg.replace("{sequence}", &sequence.to_string())
}

fn synthetic_jpeg(sequence: u64, width: u32, height: u32) -> Vec<u8> {
    let len = ((width * height) / 12).max(512) as usize;
    (0..len)
        .map(|idx| ((idx as u64 + sequence * 3) % 255) as u8)
        .collect()
}
