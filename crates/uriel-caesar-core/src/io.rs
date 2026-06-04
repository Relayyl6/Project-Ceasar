use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Read and deserialise a TOML file at `path` into `T`.
///
/// Returns a descriptive error whether the file is missing, unreadable, or
/// contains invalid TOML, so callers always know which file caused the problem.
pub fn read_toml<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse config file {}", path.display()))
}

/// Returns the current wall-clock time as milliseconds since the Unix epoch.
///
/// # Panics
/// Panics if the system clock is set to a time before the Unix epoch
/// (1970-01-01 00:00:00 UTC).  This is considered an unrecoverable
/// misconfiguration on the target platform.
pub fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is set to a time before the Unix epoch — check RTC/NTP")
        .as_millis() as u64
}

/// Serialise `value` to a compact JSON string with a trailing newline (`\n`),
/// suitable for appending to a JSONL file.
pub fn to_json_line<T: Serialize>(value: &T) -> Result<String> {
    let mut payload = serde_json::to_string(value)?;
    payload.push('\n');
    Ok(payload)
}
