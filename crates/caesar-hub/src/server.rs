use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{TcpListener, TcpStream},
};
use uriel_caesar_core::{crypto::verify_envelope, protocol::SignedEnvelope};

use crate::{config::HubConfig, store::HubStore};

pub async fn run_server(config: HubConfig) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.listen_addr))?;
    let store = HubStore::from_config(&config).await?;
    let trusted_keys = Arc::new(config.trusted_public_keys.clone().unwrap_or_default());

    println!(
        "Caesar hub listening on {} | journal → {} | high-interest → {}",
        config.listen_addr,
        store.high_interest_path().display(),
        config.storage.journal_path,
    );

    let trusted_mode = config.trusted_public_keys.as_ref().map_or(false, |k| !k.is_empty());
    if trusted_mode {
        println!("[caesar.hub] Allowlist mode: only configured trusted_public_keys will be accepted.");
    } else {
        println!("[caesar.hub] Open mode: accepting envelopes from any node (no trusted_public_keys configured).");
    }

    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[caesar.hub] accept error: {e:#}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        let store_ref = store.clone();
        let trusted_keys_ref = trusted_keys.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_socket(socket, store_ref, trusted_keys_ref).await {
                eprintln!("[caesar.hub] peer {}: {error:#}", peer);
            }
        });
    }
}

async fn handle_socket(
    socket: TcpStream,
    store: HubStore,
    trusted_keys: Arc<Vec<String>>,
) -> Result<()> {
    let mut reader = BufReader::new(socket);
    let mut buffer = Vec::new();
    loop {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            break;
        }
        if let Some(i) = buf.iter().position(|&b| b == b'\n') {
            buffer.extend_from_slice(&buf[..=i]);
            reader.consume(i + 1);

            let line_str = match std::str::from_utf8(&buffer) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[caesar.hub] invalid utf8: {e}");
                    buffer.clear();
                    continue;
                }
            };

            if line_str.trim().is_empty() {
                buffer.clear();
                continue;
            }

            let envelope: SignedEnvelope = match serde_json::from_str(line_str) {
                Ok(env) => env,
                Err(e) => {
                    eprintln!("[caesar.hub] failed to parse signed envelope JSON: {e}");
                    buffer.clear();
                    continue;
                }
            };

            if let Err(e) = verify_envelope(&envelope) {
                eprintln!("[caesar.hub] signature verification failed: {e:#}");
                buffer.clear();
                continue;
            }

            if !trusted_keys.is_empty() && !trusted_keys.iter().any(|key| key == &envelope.public_key) {
                // Skip this message but keep the connection alive for subsequent valid messages.
                // bail! would terminate the socket which would drop all future messages from this node.
                eprintln!(
                    "[caesar.hub] Rejected envelope from unknown public key {} — not in trusted_public_keys allowlist",
                    envelope.public_key
                );
                buffer.clear();
                continue;
            }

            if let Err(e) = store.persist(envelope).await {
                eprintln!("[caesar.hub] failed to persist envelope: {e:#}");
            }
            buffer.clear();
        } else {
            buffer.extend_from_slice(buf);
            let len = buf.len();
            reader.consume(len);

            // Limit maximum line size to 8 MiB to prevent OOM
            if buffer.len() > 8 * 1024 * 1024 {
                anyhow::bail!("payload too large");
            }
        }
    }
    Ok(())
}
