//! Google Gemini 3.5 Transcribe Live over the Gemini Live WebSocket API.
//!
//! Gemini differs from the other speech providers in two important transport details:
//! PCM is base64-encoded inside a JSON `realtimeInput` frame rather than sent as a raw
//! binary WebSocket frame, and audio must not start until the server acknowledges the
//! setup with `setupComplete`. The shared transport exposes hooks for both so capture,
//! buffering, timeouts and transcript delivery remain common to every provider.

use super::{request_with_header, SttEvent, WsProtocol};
use crate::audio::capture::TARGET_SAMPLE_RATE;
use crate::settings::SttSettings;
use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;

const ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const MODEL: &str = "gemini-3.5-transcribe-live";
const MAX_CUSTOM_VOCABULARY: usize = 100;

pub struct Gemini {
    api_key: String,
    language_codes: Vec<String>,
    terms: Vec<String>,
    last_final: Option<String>,
}

impl Gemini {
    pub fn new(settings: &SttSettings, api_key: String) -> Self {
        Self::with_terms(settings, api_key, Vec::new())
    }

    pub fn with_terms(settings: &SttSettings, api_key: String, terms: Vec<String>) -> Self {
        let selected_languages = if !settings.language_hints.is_empty() {
            settings.language_hints.clone()
        } else if settings.language.trim().is_empty() {
            Vec::new()
        } else {
            vec![settings.language.clone()]
        };
        let language_codes = selected_languages
            .iter()
            .filter_map(|code| gemini_language_code(code))
            .collect();
        Self {
            // Keychain values occasionally acquire a newline when copied from a web
            // console. Never put that whitespace into either the REST query or WS URL.
            api_key: api_key.trim().to_string(),
            language_codes,
            terms: normalise_custom_vocabulary(terms),
            last_final: None,
        }
    }

    fn with_config(api_key: String, language_codes: Vec<String>, terms: Vec<String>) -> Self {
        Self {
            api_key: api_key.trim().to_string(),
            language_codes,
            terms: normalise_custom_vocabulary(terms),
            last_final: None,
        }
    }
}

/// Maps the app's provider-neutral shortcuts to the full BCP-47 tags required by
/// Gemini Live. Existing full tags are deliberately kept as entered so advanced users
/// can select a documented regional variant without changing other STT providers.
fn gemini_language_code(code: &str) -> Option<String> {
    let code = code.trim();
    if code.is_empty() {
        return None;
    }
    let mapped = match code.to_ascii_lowercase().as_str() {
        "vi" => "vi-VN",
        "en" => "en-US",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "zh" => "cmn-Hans-CN",
        "th" => "th-TH",
        "id" => "id-ID",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "es" => "es-419",
        "pt" => "pt-BR",
        "it" => "it-IT",
        "ru" => "ru-RU",
        "hi" => "hi-IN",
        "ar" => "ar-EG",
        "nl" => "nl-NL",
        _ => return Some(code.to_string()),
    };
    Some(mapped.to_string())
}

fn normalise_custom_vocabulary(terms: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    terms
        .into_iter()
        .filter_map(|term| {
            let term = term.trim();
            (!term.is_empty() && seen.insert(term.to_lowercase())).then(|| term.to_string())
        })
        .take(MAX_CUSTOM_VOCABULARY)
        .collect()
}

/// Validates a Gemini API key before opening the Live WebSocket.
///
/// The Live endpoint can complete the WebSocket upgrade without ever sending the
/// application-level setup acknowledgement for an unusable credential. The ordinary
/// REST models endpoint gives a precise Google API error, which is much more useful in
/// Settings than a generic setup timeout.
pub async fn validate_api_key(api_key: &str) -> Result<()> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(anyhow!("Gemini API key is empty"));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let response = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .query(&[("pageSize", "1"), ("key", api_key)])
        .send()
        .await?;
    let status = response.status();
    let payload: Value = response.json().await?;
    if status.is_success() {
        return Ok(());
    }

    let message = payload
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Google rejected the API key");
    let google_status = payload
        .pointer("/error/status")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    Err(anyhow!(
        "Gemini API key check failed: {message} ({status}, {google_status})"
    ))
}

/// Checks the Gemini Live transport only as far as the application-level handshake.
///
/// The generic provider probe sends silence and waits for the provider to flush a speech
/// turn. Gemini Transcribe Live is allowed to produce no turn for silence, so that made
/// a valid key/model look broken when the outer probe timeout expired. A connection
/// check only needs to prove that the Live endpoint accepts the key, model and setup.
pub async fn probe_live_setup(
    settings: &SttSettings,
    api_key: String,
    terms: Vec<String>,
) -> Result<()> {
    let configured = Gemini::with_terms(settings, api_key.clone(), terms);
    let probes = setup_probe_stages(&configured);
    for (name, protocol) in probes {
        probe_one_live_setup(name, protocol).await?;
    }
    Ok(())
}

/// A deliberately small diagnostic independent of `run_stream`: it validates the
/// websocket upgrade and exactly one setup frame, then closes. No audio, transcript,
/// turn completion or stream-end signal is involved in a key check.
async fn probe_one_live_setup(name: &str, mut protocol: Gemini) -> Result<()> {
    let request = protocol.request()?;
    let (mut ws, _) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| anyhow!("Gemini Live connection did not open within 10s"))?
    .map_err(|e| anyhow!("connecting to Gemini Live: {e}"))?;
    log::info!("Gemini Live: websocket upgrade succeeded ({name})");

    let init = protocol
        .init_message()
        .ok_or_else(|| anyhow!("Gemini Live setup message was not created"))?;
    ws.send(init)
        .await
        .map_err(|e| anyhow!("sending Gemini Live setup: {e}"))?;
    log::info!("Gemini Live: setup sent ({name})");

    let wait = async {
        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    log::debug!("Gemini Live RX text ({name}): {text}");
                    // Parse first so Google's explicit model/quota/config errors are
                    // returned instead of being hidden behind a timeout.
                    let _ = protocol.parse(&text)?;
                    if protocol.is_setup_ack(&text) {
                        log::info!("Gemini Live: setupComplete ({name})");
                        return Ok(());
                    }
                }
                Ok(Message::Binary(bytes)) => {
                    log::debug!("Gemini Live RX binary ({name}): {} bytes", bytes.len());
                    if let Some(text) = binary_json_text(&bytes) {
                        log::debug!("Gemini Live RX binary UTF-8 ({name}): {text}");
                        let _ = protocol.parse(text)?;
                        if protocol.is_setup_ack(text) {
                            log::info!("Gemini Live: setupComplete ({name}, binary)");
                            return Ok(());
                        }
                    }
                }
                Ok(Message::Ping(payload)) => {
                    log::debug!("Gemini Live RX ping ({name}): {} bytes", payload.len());
                    ws.send(Message::Pong(payload))
                        .await
                        .map_err(|e| anyhow!("responding to Gemini Live ping: {e}"))?;
                }
                Ok(Message::Pong(payload)) => {
                    log::debug!("Gemini Live RX pong ({name}): {} bytes", payload.len());
                }
                Ok(Message::Close(frame)) => {
                    let reason = frame
                        .as_ref()
                        .map(|f| f.reason.to_string())
                        .filter(|r| !r.is_empty())
                        .unwrap_or_else(|| "no reason supplied".into());
                    log::warn!("Gemini Live: close before setup ({name}): {reason}");
                    return Err(anyhow!(
                        "Gemini Live closed before setup completed: {reason}"
                    ));
                }
                Ok(_) => log::debug!("Gemini Live RX unhandled frame ({name})"),
                Err(e) => {
                    log::warn!("Gemini Live websocket error ({name}): {e}");
                    return Err(anyhow!("Gemini Live setup error: {e}"));
                }
            }
        }
        Err(anyhow!("Gemini Live closed before setup completed"))
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(15), wait)
        .await
        .map_err(|_| anyhow!("Gemini Live setup did not complete within 15s ({name}); check Gemini Live diagnostics"))?;
    let _ = ws.close(None).await;
    result
}

fn setup_probe_stages(configured: &Gemini) -> Vec<(&'static str, Gemini)> {
    let mut stages = vec![
        (
            "minimal (automatic language detection)",
            Gemini::with_config(configured.api_key.clone(), Vec::new(), Vec::new()),
        ),
    ];
    if !configured.language_codes.is_empty() {
        stages.push((
            "language hints",
            Gemini::with_config(configured.api_key.clone(), configured.language_codes.clone(), Vec::new()),
        ));
    }
    if !configured.terms.is_empty() {
        stages.push((
            "custom vocabulary",
            Gemini::with_config(configured.api_key.clone(), Vec::new(), configured.terms.clone()),
        ));
    }
    if !configured.language_codes.is_empty() && !configured.terms.is_empty() {
        stages.push(("language hints + custom vocabulary", configured.clone_for_probe()));
    }
    stages
}

impl Gemini {
    fn clone_for_probe(&self) -> Self {
        Self::with_config(
            self.api_key.clone(),
            self.language_codes.clone(),
            self.terms.clone(),
        )
    }
}

impl WsProtocol for Gemini {
    fn request(&self) -> Result<Request<()>> {
        let url = format!("{ENDPOINT}?key={}", urlencoding::encode(&self.api_key));
        request_with_header(&url, "user-agent", "tockyvoice")
    }

    fn init_message(&self) -> Option<Message> {
        let mut transcription = json!({
            "languageCodes": self.language_codes,
        });
        if !self.terms.is_empty() {
            transcription["customVocabulary"] = json!(&self.terms);
        }

        Some(Message::Text(
            json!({
                "setup": {
                    "model": format!("models/{MODEL}"),
                    "generationConfig": {
                        "responseModalities": ["TEXT"]
                    },
                    "inputAudioTranscription": transcription
                }
            })
            .to_string(),
        ))
    }

    fn requires_setup_ack(&self) -> bool {
        true
    }

    fn is_setup_ack(&self, text: &str) -> bool {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.get("setupComplete").cloned())
            .is_some()
    }

    fn audio_message(&self, bytes: Vec<u8>) -> Message {
        Message::Text(
            json!({
                "realtimeInput": {
                    "audio": {
                        "data": base64_encode(&bytes),
                        "mimeType": format!("audio/pcm;rate={TARGET_SAMPLE_RATE}")
                    }
                }
            })
            .to_string(),
        )
    }

    fn finish_message(&self) -> Message {
        Message::Text(json!({ "realtimeInput": { "audioStreamEnd": true } }).to_string())
    }

    fn drain_complete(&self, text: &str) -> bool {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.get("serverContent")?.get("turnComplete")?.as_bool())
            .unwrap_or(false)
    }

    fn parse(&mut self, text: &str) -> Result<Vec<SttEvent>> {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Ok(Vec::new());
        };

        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            let code = error.get("code").and_then(Value::as_i64);
            let status = error.get("status").and_then(Value::as_str);
            let suffix = match (code, status) {
                (Some(code), Some(status)) => format!(" ({code}, {status})"),
                (Some(code), None) => format!(" ({code})"),
                (None, Some(status)) => format!(" ({status})"),
                (None, None) => String::new(),
            };
            return Err(anyhow!("Gemini: {message}{suffix}"));
        }

        let Some(content) = value.get("serverContent") else {
            // setupComplete / goAway / sessionResumptionUpdate carry no transcript.
            return Ok(Vec::new());
        };

        let turn_complete = content
            .get("turnComplete")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Final is authoritative. If a server frame ever carries both final and interim,
        // emitting only final avoids putting a stale partial back on screen after commit.
        let mut events = Vec::new();
        if let Some(final_text) = transcription_text(content, "inputTranscription") {
            if !final_text.trim().is_empty() {
                if self.last_final.as_deref() != Some(final_text) {
                    self.last_final = Some(final_text.to_string());
                    events.push(SttEvent::Final(final_text.to_string()));
                }
            }
        }
        if events.is_empty() {
            if let Some(interim) = transcription_text(content, "interimInputTranscription") {
                if !interim.trim().is_empty() {
                    events.push(SttEvent::Partial(interim.to_string()));
                }
            }
        }
        if turn_complete {
            self.last_final = None;
        }
        Ok(events)
    }
}

fn transcription_text<'a>(content: &'a Value, key: &str) -> Option<&'a str> {
    content.get(key)?.get("text")?.as_str()
}

/// Gemini's documented frames are text, but accept UTF-8 JSON carried in a binary
/// WebSocket frame too. Non-text binary data has no Live JSON semantics and is ignored
/// after its type and size were recorded by the caller.
fn binary_json_text(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

/// RFC 4648 standard base64, kept local so Gemini support adds no dependency to the
/// desktop binary for a single wire-format conversion.
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{SttProviderKind, SttSettings};

    fn settings() -> SttSettings {
        SttSettings {
            provider: SttProviderKind::Gemini,
            soniox_model: "stt-rt-preview".into(),
            deepgram_model: "nova-2".into(),
            language: "vi".into(),
            language_hints: vec!["vi".into(), "en".into()],
        }
    }

    fn gemini() -> Gemini {
        Gemini::with_terms(
            &settings(),
            "test-key".into(),
            vec!["7PK2604".into(), "ROTUYN".into()],
        )
    }

    #[test]
    fn setup_uses_live_transcribe_languages_and_automotive_vocabulary() {
        let Message::Text(text) = gemini().init_message().unwrap() else {
            panic!("Gemini setup must be JSON text");
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value,
            json!({
                "setup": {
                    "model": "models/gemini-3.5-transcribe-live",
                    "generationConfig": { "responseModalities": ["TEXT"] },
                    "inputAudioTranscription": {
                        "languageCodes": ["vi-VN", "en-US"],
                        "customVocabulary": ["7PK2604", "ROTUYN"],
                    },
                },
            })
        );
    }

    #[test]
    fn waits_for_setup_complete_before_audio() {
        let protocol = gemini();
        assert!(protocol.requires_setup_ack());
        assert!(protocol.is_setup_ack(r#"{"setupComplete":{}}"#));
        assert!(!protocol.is_setup_ack(r#"{"serverContent":{}}"#));
    }

    #[test]
    fn binary_utf8_setup_complete_is_accepted() {
        let text = binary_json_text(br#"{"setupComplete":{}}"#).expect("UTF-8 JSON");
        assert!(gemini().is_setup_ack(text));
        assert!(binary_json_text(&[0xff, 0xfe]).is_none());
    }

    #[test]
    fn auto_language_detection_uses_an_empty_bcp47_list() {
        let mut settings = settings();
        settings.language.clear();
        settings.language_hints.clear();
        let Message::Text(text) = Gemini::new(&settings, "test-key".into()).init_message().unwrap() else {
            panic!("Gemini setup must be JSON text");
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["setup"]["inputAudioTranscription"]["languageCodes"], json!([]));
    }

    #[test]
    fn maps_provider_neutral_ui_languages_to_gemini_bcp47() {
        assert_eq!(gemini_language_code("vi"), Some("vi-VN".into()));
        assert_eq!(gemini_language_code("en"), Some("en-US".into()));
        assert_eq!(gemini_language_code("zh"), Some("cmn-Hans-CN".into()));
        assert_eq!(gemini_language_code(" vi-VN "), Some("vi-VN".into()));
    }

    #[test]
    fn trims_and_url_encodes_the_key_only_in_the_websocket_query_parameter() {
        let request = Gemini::new(&settings(), "  test+key /?\n".into())
            .request()
            .unwrap();
        assert_eq!(
            request.uri().query(),
            Some("key=test%2Bkey%20%2F%3F")
        );
    }

    #[test]
    fn custom_vocabulary_is_trimmed_unique_and_bounded() {
        let terms = (0..102)
            .map(|index| format!(" term-{index} "))
            .chain(["TERM-0".to_string(), "  ".to_string()])
            .collect();
        let vocabulary = normalise_custom_vocabulary(terms);
        assert_eq!(vocabulary.len(), MAX_CUSTOM_VOCABULARY);
        assert_eq!(vocabulary.first().map(String::as_str), Some("term-0"));
        assert_eq!(vocabulary.last().map(String::as_str), Some("term-99"));
    }

    #[test]
    fn audio_is_pcm16_wrapped_as_base64_realtime_input() {
        let Message::Text(text) = gemini().audio_message(vec![0, 1, 2, 3]) else {
            panic!("Gemini audio must be JSON text");
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["realtimeInput"]["audio"]["data"], "AAECAw==");
        assert_eq!(
            value["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=16000"
        );
    }

    #[test]
    fn finish_signals_audio_stream_end() {
        let Message::Text(text) = gemini().finish_message() else {
            panic!("Gemini finish must be JSON text");
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value, json!({ "realtimeInput": { "audioStreamEnd": true } }));
    }

    #[test]
    fn parses_interim_and_final_transcriptions() {
        let mut protocol = gemini();
        let interim = protocol
            .parse(r#"{"serverContent":{"interimInputTranscription":{"text":"dây curoa"}}}"#)
            .unwrap();
        assert!(matches!(&interim[0], SttEvent::Partial(t) if t == "dây curoa"));

        let final_events = protocol
            .parse(r#"{"serverContent":{"inputTranscription":{"text":"dây curoa 7PK2604"}}}"#)
            .unwrap();
        assert!(matches!(&final_events[0], SttEvent::Final(t) if t == "dây curoa 7PK2604"));
    }

    #[test]
    fn does_not_duplicate_a_repeated_final_until_turn_complete() {
        let mut protocol = gemini();
        let frame = r#"{"serverContent":{"inputTranscription":{"text":"xin chào"}}}"#;
        assert_eq!(protocol.parse(frame).unwrap().len(), 1);
        assert!(protocol.parse(frame).unwrap().is_empty());
        protocol.parse(r#"{"serverContent":{"turnComplete":true}}"#).unwrap();
        assert_eq!(protocol.parse(frame).unwrap().len(), 1);
    }

    #[test]
    fn turn_complete_ends_drain_without_waiting_for_socket_close() {
        let protocol = gemini();
        assert!(protocol.drain_complete(r#"{"serverContent":{"turnComplete":true}}"#));
        assert!(!protocol.drain_complete(r#"{"serverContent":{"turnComplete":false}}"#));
    }

    #[test]
    fn provider_error_is_not_swallowed() {
        let err = gemini()
            .parse(r#"{"error":{"code":400,"message":"API key not valid","status":"INVALID_ARGUMENT"}}"#)
            .expect_err("Gemini errors must fail the stream");
        let message = format!("{err}");
        assert!(message.contains("API key not valid"), "{message}");
        assert!(message.contains("400"), "{message}");
    }

    #[test]
    fn setup_probe_is_handshake_only_and_diagnoses_each_enabled_setting() {
        let stages = setup_probe_stages(&gemini());
        let names: Vec<_> = stages.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec![
                "minimal (automatic language detection)",
                "language hints",
                "custom vocabulary",
                "language hints + custom vocabulary",
            ]
        );
        for (_, protocol) in stages {
            let Message::Text(text) = protocol.init_message().unwrap() else {
                panic!("probe must only create a JSON setup frame");
            };
            assert!(serde_json::from_str::<Value>(&text).unwrap().get("realtimeInput").is_none());
        }
    }

    #[test]
    fn base64_padding_is_correct() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }
}
