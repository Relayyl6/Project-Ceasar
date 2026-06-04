use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::protocol::{FusedTrack, SignedEnvelope};
use snow::{Builder, HandshakeState};

static NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

pub struct NoiseSession {
    pub state: HandshakeState,
}

impl NoiseSession {
    pub fn new_initiator(local_private: &[u8], remote_public: &[u8]) -> Result<Self> {
        if local_private.len() != 32 || remote_public.len() != 32 {
            anyhow::bail!("Noise keys must be exactly 32 bytes");
        }
        let builder = Builder::new(NOISE_PATTERN.parse()?);
        let state = builder
            .local_private_key(local_private)
            .remote_public_key(remote_public)
            .build_initiator()?;
        Ok(Self { state })
    }
}

// ── Node Identity ─────────────────────────────────────────────────────────────
//
// The node identity is a persistent Ed25519 keypair that survives reboots and
// proves the authenticity of every SignedEnvelope emitted by this node.
//
// Key lifecycle:
//   1. First boot — generate a fresh OS-random 32-byte seed via OsRng, derive
//      the keypair, write the seed hex to `key_file` with mode 0600 (Linux:
//      owner-read-only), and log the public key fingerprint so operators can
//      add it to the hub's trusted_public_keys list.
//   2. Subsequent boots — load the existing seed from the key file; the identity
//      is stable across reboots without any config change.
//   3. Legacy migration — if the TOML still has an explicit `ed25519_seed_hex`,
//      it is used for this boot AND immediately written to the key file so that
//      future boots use the file and the config entry can be removed.
//
// The config field `ed25519_seed_hex` is now `Option<String>`.
// Omitting it entirely is the correct long-term posture.
//
pub struct NodeIdentity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    /// Hex-encoded public key — what the hub's trusted_public_keys list needs.
    pub public_key_hex: String,
    /// Where the seed file lives on disk.
    pub key_file_path: std::path::PathBuf,
}

impl NodeIdentity {
    /// Load an existing identity from `key_file`, or generate a new one and
    /// save it there.
    ///
    /// - If `key_file` exists → load it (normal boot).
    /// - If `key_file` does not exist and `legacy_seed_hex` is `Some` → write
    ///   the legacy seed to the file (one-time migration).
    /// - If `key_file` does not exist and `legacy_seed_hex` is `None` → generate
    ///   a fresh OS-random key and write it (first-ever boot).
    pub fn load_or_generate(
        key_file: impl AsRef<Path>,
        legacy_seed_hex: Option<&str>,
        node_id: &str,
    ) -> Result<Self> {
        let key_file = key_file.as_ref();

        let seed: [u8; 32] = if key_file.exists() {
            // ── Normal boot: load persisted seed ──────────────────────────────
            let raw = std::fs::read_to_string(key_file)
                .with_context(|| format!(
                    "failed to read identity key file '{}'", key_file.display()
                ))?;
            let bytes = hex::decode(raw.trim())
                .with_context(|| format!(
                    "identity key file '{}' contains invalid hex", key_file.display()
                ))?;
            bytes.try_into().map_err(|_| anyhow::anyhow!(
                "identity key file '{}' must contain exactly 64 hex chars (32 bytes)",
                key_file.display()
            ))?
        } else {
            // ── First boot ────────────────────────────────────────────────────
            let seed = if let Some(hex_str) = legacy_seed_hex.filter(|s| !s.is_empty()) {
                // Legacy migration path: the config supplies an explicit seed.
                let bytes = hex::decode(hex_str)
                    .context("ed25519_seed_hex in config is not valid hex")?;
                let arr: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!(
                    "ed25519_seed_hex must be 64 hex chars (32 bytes)"
                ))?;
                println!(
                    "[caesar.identity] Migrating legacy config seed to key file '{}'.\n\
                     └─ Remove ed25519_seed_hex from your TOML after this first run.",
                    key_file.display()
                );
                arr
            } else {
                // True first boot: generate a cryptographically random seed from the OS.
                let fresh = SigningKey::generate(&mut rand_core::OsRng);
                fresh.to_bytes()
            };

            // Persist the seed and restrict file permissions.
            Self::write_key_file(key_file, &seed)
                .with_context(|| format!(
                    "failed to write identity key file '{}'", key_file.display()
                ))?;
            seed
        };

        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let public_key_hex = hex::encode(verifying_key.to_bytes());

        println!(
            "[caesar.identity] Node '{}' identity ready\n\
             ├─ Key file   : {}\n\
             ├─ Public key : {}\n\
             └─ Add this public key to hub-dev.toml trusted_public_keys if not already present.",
            node_id,
            key_file.display(),
            public_key_hex,
        );

        Ok(Self {
            signing_key,
            verifying_key,
            public_key_hex,
            key_file_path: key_file.to_path_buf(),
        })
    }

    /// Write seed as lowercase hex to `path` and set permissions to 0600 on Unix
    /// so other OS users cannot read the private seed.
    fn write_key_file(path: &Path, seed: &[u8; 32]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create key directory '{}'", parent.display())
            })?;
        }

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options.open(path).context("failed to open key file for writing")?;
        use std::io::Write;
        writeln!(file, "{}", hex::encode(seed))
            .context("failed to write seed to key file")?;

        println!(
            "[caesar.identity] New identity key written to '{}'.\n\
             └─ Back this file up securely — losing it means the hub will reject envelopes from this node.",
            path.display()
        );
        Ok(())
    }

    /// Consume this identity and produce an `EnvelopeSigner`.
    pub fn into_signer(self) -> EnvelopeSigner {
        EnvelopeSigner {
            signing_key: self.signing_key,
            verifying_key: self.verifying_key,
        }
    }
}

// ── Envelope Signer ───────────────────────────────────────────────────────────

pub struct EnvelopeSigner {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl EnvelopeSigner {
    /// Construct from a raw hex seed.
    /// Prefer `NodeIdentity::load_or_generate` in application code;
    /// this is kept for tests and external tooling.
    pub fn from_seed_hex(seed_hex: &str) -> Result<Self> {
        let seed = hex::decode(seed_hex.trim()).context("invalid ed25519 seed hex")?;
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| anyhow::anyhow!("ed25519 seed must be 32 bytes"))?;
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    pub fn sign_track(
        &self,
        node_id: &str,
        topic: &str,
        body: FusedTrack,
    ) -> Result<SignedEnvelope> {
        if node_id != body.node_id {
            anyhow::bail!("node_id mismatch between envelope and body");
        }

        #[derive(serde::Serialize)]
        struct EnvelopePayload<'a> {
            schema_version: u8,
            node_id: &'a str,
            topic: &'a str,
            body: &'a FusedTrack,
        }

        let payload = EnvelopePayload {
            schema_version: 1,
            node_id,
            topic,
            body: &body,
        };

        let body_bytes = serde_json::to_vec(&payload)?;
        let signature = self.signing_key.sign(&body_bytes);

        Ok(SignedEnvelope {
            schema_version: 1,
            node_id: node_id.to_string(),
            topic: topic.to_string(),
            body,
            public_key: hex::encode(self.verifying_key.to_bytes()),
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        })
    }
}

pub fn verify_envelope(envelope: &SignedEnvelope) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    if envelope.body.timestamp_ms > now + 60_000 {
        anyhow::bail!("Envelope from the future");
    }
    if now.saturating_sub(envelope.body.timestamp_ms) > 300_000 {
        anyhow::bail!("Envelope too old (replay protection)");
    }
    if envelope.schema_version != 1 {
        anyhow::bail!("unsupported schema version: {}", envelope.schema_version);
    }
    if envelope.node_id != envelope.body.node_id {
        anyhow::bail!("node_id mismatch between envelope and body");
    }
    if !crate::protocol::is_valid_threat_level(&envelope.body.threat_level) {
        anyhow::bail!("invalid threat_level: {}", envelope.body.threat_level);
    }
    if envelope.body.geo_latitude < -90.0 || envelope.body.geo_latitude > 90.0 {
        anyhow::bail!("invalid geo_latitude: {}", envelope.body.geo_latitude);
    }
    if envelope.body.geo_longitude < -180.0 || envelope.body.geo_longitude > 180.0 {
        anyhow::bail!("invalid geo_longitude: {}", envelope.body.geo_longitude);
    }
    if envelope.body.confidence < 0.0 || envelope.body.confidence > 1.0 {
        anyhow::bail!("invalid confidence: {}", envelope.body.confidence);
    }

    let public_key = hex::decode(&envelope.public_key).context("invalid public key hex")?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)?;

    let signature = base64::engine::general_purpose::STANDARD
        .decode(&envelope.signature)
        .context("invalid signature base64")?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes"))?;
    let signature = Signature::from_bytes(&signature);
    
    #[derive(serde::Serialize)]
    struct EnvelopePayload<'a> {
        schema_version: u8,
        node_id: &'a str,
        topic: &'a str,
        body: &'a FusedTrack,
    }

    let payload = EnvelopePayload {
        schema_version: envelope.schema_version,
        node_id: &envelope.node_id,
        topic: &envelope.topic,
        body: &envelope.body,
    };
    
    let body_bytes = serde_json::to_vec(&payload)?;
    verifying_key.verify(&body_bytes, &signature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Modality;

    fn test_track() -> FusedTrack {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        FusedTrack {
            node_id: "node-a".into(),
            timestamp_ms: now,
            track_id: "track-1".into(),
            site: "lab".into(),
            geo_latitude: 1.0,
            geo_longitude: 2.0,
            threat_level: "monitor".into(),
            confidence: 0.9,
            position_m: (1.0, 2.0),
            velocity_mps: Some(3.0),
            contributing_modalities: vec![Modality::Optical],
            source_ids: vec!["cam".into()],
            evidence_digests: vec!["digest".into()],
        }
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let signer = EnvelopeSigner::from_seed_hex(
            "a3f1e2d4b5c67890123456789abcdef0a3f1e2d4b5c67890123456789abcdef0",
        )
        .expect("signer");
        let envelope = signer
            .sign_track("node-a", "caesar_tactical_intel", test_track())
            .expect("sign");
        verify_envelope(&envelope).expect("verify");
    }

    #[test]
    fn load_or_generate_creates_stable_key_file() {
        let dir = std::env::temp_dir().join("caesar_test_identity_stable");
        let key_path = dir.join("node.key");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir_all(&dir);

        // First call: generates and persists
        let id1 = NodeIdentity::load_or_generate(&key_path, None, "test-node")
            .expect("first load_or_generate");
        assert!(key_path.exists(), "key file must be created on first boot");

        // Second call: loads the same key — identity is stable
        let id2 = NodeIdentity::load_or_generate(&key_path, None, "test-node")
            .expect("second load_or_generate");
        assert_eq!(
            id1.public_key_hex, id2.public_key_hex,
            "public key must be identical across boots"
        );

        // Third call with a different legacy_seed_hex should be ignored
        // (key file already exists, legacy seed is not used)
        let legacy = "a3f1e2d4b5c67890123456789abcdef0a3f1e2d4b5c67890123456789abcdef0";
        let id3 = NodeIdentity::load_or_generate(&key_path, Some(legacy), "test-node")
            .expect("third call ignores legacy");
        assert_eq!(
            id1.public_key_hex, id3.public_key_hex,
            "key file must take precedence over legacy seed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_seed_migration_writes_key_file() {
        let dir = std::env::temp_dir().join("caesar_test_migration");
        let key_path = dir.join("node.key");
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir_all(&dir);

        let legacy = "a3f1e2d4b5c67890123456789abcdef0a3f1e2d4b5c67890123456789abcdef0";
        let id = NodeIdentity::load_or_generate(&key_path, Some(legacy), "test-node-migrate")
            .expect("migration");

        assert!(key_path.exists(), "key file must be written during migration");
        let stored = std::fs::read_to_string(&key_path).unwrap();
        assert_eq!(stored.trim(), legacy, "stored seed must match legacy value");

        // Public key must match what from_seed_hex produces
        let expected_signer = EnvelopeSigner::from_seed_hex(legacy).unwrap();
        let expected_pk = expected_signer
            .sign_track("node-a", "y", test_track())
            .unwrap()
            .public_key;
        assert_eq!(id.public_key_hex, expected_pk);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
