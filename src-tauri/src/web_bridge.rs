//! Local browser bridge for dictation started from a trusted web application.
//!
//! The bridge is intentionally loopback-only. A web page first opens a WebSocket,
//! prepares a one-time request id + nonce, then triggers `tocky://listen?...`.
//! Matching both values prevents an unrelated tab from claiming another tab's take.
//! The browser `Origin` header is also checked exactly during the WebSocket handshake.

use crate::errors::ErrorPayload;
use crate::session;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

pub const BRIDGE_ADDR: &str = "127.0.0.1:17891";
pub const PROTOCOL_VERSION: u32 = 1;
const BRIDGE_PATH: &str = "/bridge";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 4 * 1024;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const MAX_MESSAGES_PER_WINDOW: u32 = 40;
const PREPARE_TIMEOUT: Duration = Duration::from_secs(60);

/// Exact origins accepted by the first vertical slice. Keep this deliberately narrow;
/// production origins can be added through a later managed setting instead of using a
/// wildcard. PTAP local development is pinned to Vite port 5173.
const ALLOWED_ORIGINS: &[&str] = &[
    "https://staging.ptap-next-staging.pages.dev",
    "http://127.0.0.1:5173",
    "http://localhost:5173",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestPhase {
    Prepared,
    Recording,
}

struct BridgeRequest {
    client_id: Uuid,
    request_id: String,
    nonce: String,
    phase: RequestPhase,
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Default)]
pub struct WebBridge {
    request: Mutex<Option<BridgeRequest>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Prepare {
        protocol_version: u32,
        request_id: String,
        nonce: String,
        #[serde(default)]
        field_context: Option<Value>,
    },
    Stop {
        request_id: String,
    },
    Cancel {
        request_id: String,
    },
    Ping,
}

struct RateBudget {
    started: Instant,
    count: u32,
}

impl Default for RateBudget {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            count: 0,
        }
    }
}

impl RateBudget {
    fn allow(&mut self) -> bool {
        if self.started.elapsed() >= RATE_WINDOW {
            self.started = Instant::now();
            self.count = 0;
        }
        if self.count >= MAX_MESSAGES_PER_WINDOW {
            return false;
        }
        self.count += 1;
        true
    }
}

/// Starts the loopback server without blocking application startup.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_server(app).await {
            log::error!("web bridge stopped: {error:#}");
        }
    });
}

async fn run_server(app: AppHandle) -> anyhow::Result<()> {
    let listener = TcpListener::bind(BRIDGE_ADDR).await?;
    log::info!("web bridge listening on ws://{BRIDGE_ADDR}{BRIDGE_PATH}");

    loop {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            log::warn!("web bridge rejected non-loopback peer {peer}");
            continue;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = handle_connection(app, stream).await {
                log::warn!("web bridge connection ended: {error:#}");
            }
        });
    }
}

async fn handle_connection(app: AppHandle, stream: TcpStream) -> anyhow::Result<()> {
    let accepted_origin = Arc::new(Mutex::new(None::<String>));
    let origin_capture = accepted_origin.clone();

    let socket = accept_hdr_async(stream, move |request: &Request, response: Response| {
        if request.uri().path() != BRIDGE_PATH {
            return Err(reject(StatusCode::NOT_FOUND, "unknown bridge path"));
        }
        let origin = request
            .headers()
            .get("origin")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !origin_allowed(origin) {
            return Err(reject(StatusCode::FORBIDDEN, "origin not allowed"));
        }
        if let Ok(mut slot) = origin_capture.lock() {
            *slot = Some(origin.to_string());
        }
        Ok(response)
    })
    .await?;

    let origin = accepted_origin
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let client_id = Uuid::new_v4();
    let (mut writer, mut reader) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    send_json(
        &out_tx,
        json!({
            "type": "hello",
            "protocol_version": PROTOCOL_VERSION,
            "bridge": "tocky",
        }),
    );

    let writer_task = tauri::async_runtime::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if writer.send(message).await.is_err() {
                break;
            }
        }
    });

    let mut budget = RateBudget::default();
    while let Some(message) = reader.next().await {
        let message = message?;
        if message.len() > MAX_MESSAGE_BYTES {
            send_protocol_error(&out_tx, None, "PAYLOAD_TOO_LARGE");
            break;
        }
        if !budget.allow() {
            send_protocol_error(&out_tx, None, "RATE_LIMITED");
            break;
        }

        match message {
            Message::Ping(payload) => {
                let _ = out_tx.send(Message::Pong(payload));
            }
            Message::Close(_) => break,
            Message::Text(text) => {
                let command: ClientMessage = match serde_json::from_str(text.as_ref()) {
                    Ok(value) => value,
                    Err(_) => {
                        send_protocol_error(&out_tx, None, "INVALID_MESSAGE");
                        continue;
                    }
                };
                handle_command(&app, client_id, &origin, &out_tx, command);
            }
            _ => send_protocol_error(&out_tx, None, "UNSUPPORTED_MESSAGE"),
        }
    }

    let was_recording = clear_client_request(&app, client_id);
    if was_recording {
        session::cancel(&app);
    }
    writer_task.abort();
    Ok(())
}

fn handle_command(
    app: &AppHandle,
    client_id: Uuid,
    origin: &str,
    tx: &mpsc::UnboundedSender<Message>,
    command: ClientMessage,
) {
    match command {
        ClientMessage::Prepare {
            protocol_version,
            request_id,
            nonce,
            field_context,
        } => {
            if protocol_version != PROTOCOL_VERSION {
                send_protocol_error(tx, Some(&request_id), "PROTOCOL_VERSION_MISMATCH");
                return;
            }
            if !valid_request_id(&request_id) || !valid_nonce(&nonce) {
                send_protocol_error(tx, Some(&request_id), "INVALID_REQUEST");
                return;
            }
            if field_context
                .as_ref()
                .and_then(|value| serde_json::to_vec(value).ok())
                .map(|value| value.len() > MAX_CONTEXT_BYTES)
                .unwrap_or(false)
            {
                send_protocol_error(tx, Some(&request_id), "FIELD_CONTEXT_TOO_LARGE");
                return;
            }
            if app.state::<session::Recorder>().is_busy() {
                send_protocol_error(tx, Some(&request_id), "BUSY");
                return;
            }

            let bridge = app.state::<WebBridge>();
            let Ok(mut slot) = bridge.request.lock() else {
                send_protocol_error(tx, Some(&request_id), "BRIDGE_UNAVAILABLE");
                return;
            };
            // A request remains owned while STT is finalizing, even though Recorder no
            // longer reports an active capture. Never let the same tab replace that slot:
            // doing so would orphan the old result and could make it fall through to the
            // normal OS paste path.
            if slot.is_some() {
                send_protocol_error(tx, Some(&request_id), "BUSY");
                return;
            }
            *slot = Some(BridgeRequest {
                client_id,
                request_id: request_id.clone(),
                nonce,
                phase: RequestPhase::Prepared,
                tx: tx.clone(),
            });
            drop(slot);

            send_json(
                tx,
                json!({
                    "type": "prepared",
                    "protocol_version": PROTOCOL_VERSION,
                    "request_id": request_id,
                    "origin": origin,
                }),
            );

            // A browser that prepares but never follows with the deep link must not
            // reserve the single bridge slot forever. Recording/finalizing requests are
            // deliberately not expired here; their lifecycle is owned by session.rs.
            let timeout_app = app.clone();
            let timeout_request_id = request_id.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(PREPARE_TIMEOUT).await;
                expire_prepared_request(&timeout_app, client_id, &timeout_request_id);
            });
        }
        ClientMessage::Stop { request_id } => {
            if owns_recording_request(app, client_id, &request_id) {
                session::stop(app);
            } else {
                send_protocol_error(tx, Some(&request_id), "REQUEST_NOT_ACTIVE");
            }
        }
        ClientMessage::Cancel { request_id } => {
            if owns_request(app, client_id, &request_id) {
                session::cancel(app);
            } else {
                send_protocol_error(tx, Some(&request_id), "REQUEST_NOT_ACTIVE");
            }
        }
        ClientMessage::Ping => send_json(
            tx,
            json!({
                "type": "pong",
                "protocol_version": PROTOCOL_VERSION,
            }),
        ),
    }
}

/// Handles a `tocky://...` URL delivered by the deep-link plugin.
///
/// `tocky://wake` intentionally does nothing besides proving/starting the application.
/// A `listen` URL is accepted only after the same browser connection prepared the
/// matching request id + nonce.
pub fn handle_deep_link(app: &AppHandle, raw_url: &str) -> bool {
    if raw_url.eq_ignore_ascii_case("tocky://wake") || raw_url.eq_ignore_ascii_case("tocky://wake/")
    {
        return true;
    }
    let Some((request_id, nonce)) = parse_listen_url(raw_url) else {
        log::warn!("ignored malformed Tocky deep link");
        return false;
    };

    let tx = {
        let bridge = app.state::<WebBridge>();
        let Ok(mut slot) = bridge.request.lock() else {
            return false;
        };
        let Some(request) = slot.as_mut() else {
            return false;
        };
        if request.request_id != request_id
            || request.nonce != nonce
            || request.phase != RequestPhase::Prepared
        {
            return false;
        }
        request.phase = RequestPhase::Recording;
        request.tx.clone()
    };

    if session::start(app, None) {
        send_json(
            &tx,
            json!({
                "type": "recording_started",
                "protocol_version": PROTOCOL_VERSION,
                "request_id": request_id,
            }),
        );
        true
    } else {
        // If cancel/disconnect already consumed the bridge request while start was
        // pending, it owns the terminal event. Avoid a second START_FAILED message.
        if clear_matching_request(app, &request_id) {
            send_protocol_error(&tx, Some(&request_id), "START_FAILED");
        }
        false
    }
}

/// Delivers the final post-terminology/post-cleanup result to the owning browser tab.
/// Returns true when the take belonged to the web bridge, so the normal paste path must
/// not run as well.
pub fn deliver_result(app: &AppHandle, raw_text: &str, final_text: &str) -> bool {
    let Some(request) = take_recording_request(app) else {
        return false;
    };
    send_json(
        &request.tx,
        json!({
            "type": "result",
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request.request_id,
            "text": final_text,
            "raw_text": raw_text,
            "vocabulary_revision": Value::Null,
        }),
    );
    true
}

pub fn deliver_error(app: &AppHandle, payload: &ErrorPayload) -> bool {
    let Some(request) = take_recording_request(app) else {
        return false;
    };
    send_json(
        &request.tx,
        json!({
            "type": "error",
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request.request_id,
            "error": payload,
        }),
    );
    true
}

pub fn deliver_empty(app: &AppHandle) -> bool {
    let Some(request) = take_recording_request(app) else {
        return false;
    };
    send_protocol_error(&request.tx, Some(&request.request_id), "EMPTY_TRANSCRIPT");
    true
}

pub fn deliver_cancelled(app: &AppHandle) -> bool {
    let Some(request) = take_any_request(app) else {
        return false;
    };
    send_json(
        &request.tx,
        json!({
            "type": "cancelled",
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request.request_id,
        }),
    );
    true
}

fn owns_request(app: &AppHandle, client_id: Uuid, request_id: &str) -> bool {
    app.state::<WebBridge>()
        .request
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .map(|request| request.client_id == client_id && request.request_id == request_id)
        })
        .unwrap_or(false)
}

fn owns_recording_request(app: &AppHandle, client_id: Uuid, request_id: &str) -> bool {
    app.state::<WebBridge>()
        .request
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref().map(|request| {
                request.client_id == client_id
                    && request.request_id == request_id
                    && request.phase == RequestPhase::Recording
            })
        })
        .unwrap_or(false)
}

fn take_recording_request(app: &AppHandle) -> Option<BridgeRequest> {
    let bridge = app.state::<WebBridge>();
    let mut slot = bridge.request.lock().ok()?;
    if slot
        .as_ref()
        .map(|request| request.phase == RequestPhase::Recording)
        .unwrap_or(false)
    {
        slot.take()
    } else {
        None
    }
}

fn take_any_request(app: &AppHandle) -> Option<BridgeRequest> {
    app.state::<WebBridge>().request.lock().ok()?.take()
}

fn expire_prepared_request(app: &AppHandle, client_id: Uuid, request_id: &str) {
    let expired = {
        let bridge = app.state::<WebBridge>();
        let Ok(mut slot) = bridge.request.lock() else {
            return;
        };
        if slot
            .as_ref()
            .map(|request| {
                request.client_id == client_id
                    && request.request_id == request_id
                    && request.phase == RequestPhase::Prepared
            })
            .unwrap_or(false)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(request) = expired {
        send_protocol_error(&request.tx, Some(&request.request_id), "PREPARE_TIMEOUT");
    }
}

fn clear_matching_request(app: &AppHandle, request_id: &str) -> bool {
    let bridge = app.state::<WebBridge>();
    let Ok(mut slot) = bridge.request.lock() else {
        return false;
    };
    if slot
        .as_ref()
        .map(|request| request.request_id == request_id)
        .unwrap_or(false)
    {
        slot.take();
        true
    } else {
        false
    }
}

fn clear_client_request(app: &AppHandle, client_id: Uuid) -> bool {
    let bridge = app.state::<WebBridge>();
    let Ok(mut slot) = bridge.request.lock() else {
        return false;
    };
    let was_recording = slot
        .as_ref()
        .map(|request| request.client_id == client_id && request.phase == RequestPhase::Recording)
        .unwrap_or(false);
    if slot
        .as_ref()
        .map(|request| request.client_id == client_id)
        .unwrap_or(false)
    {
        slot.take();
    }
    was_recording
}

fn reject(status: StatusCode, message: &str) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(message.to_string()));
    *response.status_mut() = status;
    response
}

fn send_protocol_error(tx: &mpsc::UnboundedSender<Message>, request_id: Option<&str>, code: &str) {
    send_json(
        tx,
        json!({
            "type": "error",
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request_id,
            "error": { "kind": "bridge_protocol", "detail": code },
        }),
    );
}

fn send_json(tx: &mpsc::UnboundedSender<Message>, value: Value) {
    let _ = tx.send(Message::text(value.to_string()));
}

fn origin_allowed(origin: &str) -> bool {
    ALLOWED_ORIGINS.iter().any(|allowed| *allowed == origin)
}

fn valid_request_id(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn valid_nonce(value: &str) -> bool {
    (32..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn parse_listen_url(raw_url: &str) -> Option<(String, String)> {
    let query = raw_url.strip_prefix("tocky://listen?")?;
    let mut request_id = None;
    let mut nonce = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        let decoded = urlencoding::decode(value).ok()?.into_owned();
        match key {
            "request_id" if request_id.is_none() => request_id = Some(decoded),
            "nonce" if nonce.is_none() => nonce = Some(decoded),
            _ => {}
        }
    }
    let request_id = request_id?;
    let nonce = nonce?;
    if !valid_request_id(&request_id) || !valid_nonce(&nonce) {
        return None;
    }
    Some((request_id, nonce))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_allowlist_is_exact() {
        assert!(origin_allowed(
            "https://staging.ptap-next-staging.pages.dev"
        ));
        assert!(origin_allowed("http://127.0.0.1:5173"));
        assert!(!origin_allowed("https://evil.example"));
        assert!(!origin_allowed(
            "https://staging.ptap-next-staging.pages.dev.evil.example"
        ));
    }

    #[test]
    fn deep_link_parser_requires_valid_request_and_nonce() {
        let request_id = Uuid::new_v4().to_string();
        let nonce = "a2345678901234567890123456789012";
        let url = format!("tocky://listen?request_id={request_id}&nonce={nonce}");
        assert_eq!(
            parse_listen_url(&url),
            Some((request_id, nonce.to_string()))
        );

        assert!(parse_listen_url(
            "tocky://listen?request_id=nope&nonce=a2345678901234567890123456789012"
        )
        .is_none());
        assert!(parse_listen_url(
            "tocky://listen?request_id=00000000-0000-4000-8000-000000000001&nonce=short"
        )
        .is_none());
        assert!(parse_listen_url("https://listen?request_id=00000000-0000-4000-8000-000000000001&nonce=a2345678901234567890123456789012").is_none());
    }

    #[test]
    fn nonce_rejects_unsafe_characters() {
        assert!(valid_nonce("abcDEF0123_-abcDEF0123_-abcDEF0123_"));
        assert!(!valid_nonce("abcDEF0123+/abcDEF0123+/abcDEF0123+/"));
    }

    #[test]
    fn rate_budget_rejects_excess_messages() {
        let mut budget = RateBudget::default();
        for _ in 0..MAX_MESSAGES_PER_WINDOW {
            assert!(budget.allow());
        }
        assert!(!budget.allow());
    }
}
