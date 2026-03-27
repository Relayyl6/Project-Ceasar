use std::sync::Arc;

use anyhow::{bail, Context, Result};
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
        "Caesar hub listening on {} and writing high-interest stream to {}",
        config.listen_addr,
        store.high_interest_path().display()
    );

    loop {
        let (socket, peer) = listener.accept().await?;
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
    let mut lines = BufReader::new(socket).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let envelope: SignedEnvelope =
            serde_json::from_str(&line).context("failed to parse signed envelope JSON")?;
        verify_envelope(&envelope).context("signature verification failed")?;
        if !trusted_keys.is_empty() && !trusted_keys.iter().any(|key| key == &envelope.public_key) {
            bail!(
                "public key {} is not in the configured trusted_public_keys allowlist",
                envelope.public_key
            );
        }
        store.persist(envelope).await?;
    }
    Ok(())
}
