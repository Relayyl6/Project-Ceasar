use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::protocol::{FusedTrack, SignedEnvelope};

pub struct EnvelopeSigner {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl EnvelopeSigner {
    pub fn from_seed_hex(seed_hex: &str) -> Result<Self> {
        let seed = hex::decode(seed_hex).context("invalid ed25519 seed hex")?;
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
        let body_bytes = serde_json::to_vec(&body)?;
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
    let body_bytes = serde_json::to_vec(&envelope.body)?;
    verifying_key.verify(&body_bytes, &signature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Modality;

    #[test]
    fn sign_and_verify_round_trip() {
        let signer = EnvelopeSigner::from_seed_hex(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .expect("signer");
        let track = FusedTrack {
            node_id: "node-a".into(),
            timestamp_ms: 1,
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
        };
        let envelope = signer
            .sign_track("node-a", "caesar_tactical_intel", track)
            .expect("sign");
        verify_envelope(&envelope).expect("verify");
    }
}
