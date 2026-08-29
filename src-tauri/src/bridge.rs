//! Loopback-only PTAP web ↔ TOCKY native bridge.
//!
//! The listener never binds a LAN interface. A session carries a random browser nonce,
//! an immutable backend snapshot, and the exact revision/fingerprint expected by PTAP.

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::ErrorResponse;
use tokio_tungstenite::tungstenite::http::{StatusCode, Uri};
use tokio_tungstenite::tungstenite::Message;

const ADDRESS: &str = "127.0.0.1:17891";
const PROTOCOL_VERSION: u64 = 1;

struct Prepared {
    request_id: String,
    nonce: String,
    dictionary: Arc<crate::terminology::CompiledVocabulary>,
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(app).await {
            log::error!("PTAP loopback bridge stopped: {error:#}");
        }
    });
}

async fn serve(app: AppHandle) -> Result<()> {
    let listener = TcpListener::bind(ADDRESS)
        .await
        .with_context(|| format!("binding loopback bridge at {ADDRESS}"))?;
    log::info!("PTAP loopback bridge listening on ws://{ADDRESS}/bridge");
    loop {
        let (stream, peer) = listener.accept().await.context("accepting bridge client")?;
        if !peer.ip().is_loopback() {
            log::warn!("rejecting non-loopback bridge peer {peer}");
            continue;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = connection(app, stream).await {
                log::warn!("PTAP bridge connection rejected: {error:#}");
            }
        });
    }
}

async fn connection(app: AppHandle, stream: TcpStream) -> Result<()> {
    let socket = tokio_tungstenite::accept_hdr_async(
        stream,
        |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
            if request.uri().path() != "/bridge" {
                let mut rejected =
                    ErrorResponse::new(Some("TOCKY bridge is available only at /bridge".into()));
                *rejected.status_mut() = StatusCode::NOT_FOUND;
                return Err(rejected);
            }
            let origin = request
                .headers()
                .get("origin")
                .and_then(|value| value.to_str().ok());
            if !origin.is_some_and(trusted_origin) {
                let mut rejected = ErrorResponse::new(Some("Untrusted PTAP web origin".into()));
                *rejected.status_mut() = StatusCode::FORBIDDEN;
                return Err(rejected);
            }
            Ok(response)
        },
    )
    .await
    .context("upgrading loopback WebSocket")?;
    let (mut writer, mut reader) = socket.split();
    writer
        .send(Message::Text(
            json!({ "type": "hello", "bridge": "tocky", "protocol_version": PROTOCOL_VERSION })
                .to_string(),
        ))
        .await?;

    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<Value>();
    let mut prepared: Option<Prepared> = None;
    let mut recording = false;

    loop {
        tokio::select! {
            outbound = output_rx.recv() => {
                let Some(outbound) = outbound else { break; };
                let final_message = outbound.get("type").and_then(Value::as_str) == Some("result")
                    || outbound.get("type").and_then(Value::as_str) == Some("error");
                writer.send(Message::Text(outbound.to_string())).await?;
                if final_message { break; }
            }
            inbound = reader.next() => {
                let Some(inbound) = inbound else { break; };
                let inbound = inbound.context("reading bridge message")?;
                let Message::Text(text) = inbound else { continue; };
                let value: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(error) => {
                        writer.send(Message::Text(bridge_error(None, "INVALID_MESSAGE", &format!("invalid bridge JSON: {error}")).to_string())).await?;
                        break;
                    }
                };
                match value.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "prepare" => {
                        match prepare(&app, &value) {
                            Ok((next, revision, fingerprint)) => {
                                writer.send(Message::Text(json!({
                                    "type": "prepared",
                                    "request_id": next.request_id,
                                    "vocabulary_revision": revision,
                                    "vocabulary_fingerprint": fingerprint,
                                }).to_string())).await?;
                                prepared = Some(next);
                            }
                            Err(error) => {
                                let request_id = value.get("request_id").and_then(Value::as_str);
                                let detail = if error.to_string().contains("mismatch") { "VOCABULARY_PIN_MISMATCH" } else { "VOCABULARY_SNAPSHOT_INVALID" };
                                writer.send(Message::Text(bridge_error(request_id, detail, &error.to_string()).to_string())).await?;
                                break;
                            }
                        }
                    }
                    "listen" => {
                        let result = (|| -> Result<()> {
                            let request_id = required_text(&value, "request_id")?;
                            let nonce = required_text(&value, "nonce")?;
                            let next = prepared.as_ref().context("listen arrived before prepare")?;
                            if request_id != next.request_id || nonce != next.nonce { bail!("bridge request or nonce mismatch"); }
                            crate::session::start_bridge(&app, crate::session::BridgeTake {
                                request_id,
                                vocabulary: next.dictionary.clone(),
                                output: output_tx.clone(),
                            })?;
                            Ok(())
                        })();
                        if let Err(error) = result {
                            writer.send(Message::Text(bridge_error(value.get("request_id").and_then(Value::as_str), "TOCKY_START_FAILED", &error.to_string()).to_string())).await?;
                            break;
                        }
                        recording = true;
                    }
                    "stop" if recording => crate::session::stop(&app),
                    "cancel" if recording => {
                        crate::session::cancel(&app);
                        writer.send(Message::Text(json!({ "type": "cancelled", "request_id": prepared.as_ref().map(|value| &value.request_id) }).to_string())).await?;
                        recording = false;
                        break;
                    }
                    _ => {
                        writer.send(Message::Text(json!({ "type": "error", "request_id": Value::Null, "error": { "detail": "INVALID_MESSAGE", "message": "Unsupported bridge message." } }).to_string())).await?;
                    }
                }
            }
        }
    }
    if recording {
        crate::session::cancel(&app);
    }
    Ok(())
}

fn trusted_origin(value: &str) -> bool {
    let Ok(uri) = value.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    let Some(host) = uri.host().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };
    match scheme {
        "http" => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]"),
        "https" => {
            host == "ptap-next-staging.pages.dev" || host.ends_with(".ptap-next-staging.pages.dev")
        }
        _ => false,
    }
}

fn prepare(app: &AppHandle, value: &Value) -> Result<(Prepared, u64, String)> {
    if value.get("protocol_version").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
        bail!("unsupported bridge protocol");
    }
    let request_id = required_text(value, "request_id")?;
    let nonce = required_text(value, "nonce")?;
    if nonce.len() < 24 {
        bail!("bridge nonce is too short");
    }
    let revision = value
        .get("vocabulary_revision")
        .and_then(Value::as_u64)
        .context("missing vocabulary revision")?;
    let fingerprint = required_text(value, "vocabulary_fingerprint")?;
    let snapshot: crate::terminology::VocabularySnapshot = serde_json::from_value(
        value
            .get("vocabulary_snapshot")
            .cloned()
            .context("missing backend vocabulary snapshot")?,
    )
    .context("invalid backend vocabulary snapshot")?;
    let dictionary = app
        .state::<crate::terminology::VocabularyManager>()
        .install(snapshot, revision, &fingerprint)?;
    if let Err(error) = crate::terminology::cache_path(app)
        .and_then(|path| crate::terminology::save_cache(&path, dictionary.snapshot()))
    {
        log::warn!("could not persist PTAP vocabulary cache: {error:#}");
    }
    Ok((
        Prepared {
            request_id,
            nonce,
            dictionary,
        },
        revision,
        fingerprint,
    ))
}

fn bridge_error(request_id: Option<&str>, detail: &str, message: &str) -> Value {
    json!({
        "type": "error",
        "request_id": request_id,
        "error": { "detail": detail, "message": message },
    })
}

fn required_text(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .with_context(|| format!("missing bridge field {field}"))
}

#[cfg(test)]
mod tests {
    use super::trusted_origin;

    #[test]
    fn bridge_accepts_only_ptap_staging_or_loopback_development_origins() {
        assert!(trusted_origin("http://localhost:5173"));
        assert!(trusted_origin("http://127.0.0.1:4173"));
        assert!(trusted_origin("http://[::1]:5173"));
        assert!(trusted_origin(
            "https://staging.ptap-next-staging.pages.dev"
        ));
        assert!(trusted_origin(
            "https://4f359888.ptap-next-staging.pages.dev"
        ));
        assert!(!trusted_origin("https://evil.example"));
        assert!(!trusted_origin("null"));
        assert!(!trusted_origin("file:///tmp/ptap.html"));
    }
}
