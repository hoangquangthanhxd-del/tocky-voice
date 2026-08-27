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
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::Message;

const ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const MODEL: &str = "gemini-3.5-transcribe-live";

pub struct Gemini {
    api_key: String,
    language_codes: Vec<String>,
    terms: Vec<String>,
}

impl Gemini {
    pub fn new(settings: &SttSettings, api_key: String) -> Self {
        Self::with_terms(settings, api_key, Vec::new())
    }

    pub fn with_terms(settings: &SttSettings, api_key: String, terms: Vec<String>) -> Self {
        let language_codes = if !settings.language_hints.is_empty() {
            settings.language_hints.clone()
        } else if settings.language.trim().is_empty() {
            Vec::new()
        } else {
            vec![settings.language.clone()]
        };
        Self {
            api_key,
            language_codes,
            terms,
        }
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

        // Final is authoritative. If a server frame ever carries both final and interim,
        // emitting only final avoids putting a stale partial back on screen after commit.
        if let Some(final_text) = transcription_text(content, "inputTranscription") {
            if !final_text.trim().is_empty() {
                return Ok(vec![SttEvent::Final(final_text.to_string())]);
            }
        }
        if let Some(interim) = transcription_text(content, "interimInputTranscription") {
            if !interim.trim().is_empty() {
                return Ok(vec![SttEvent::Partial(interim.to_string())]);
            }
        }
        Ok(Vec::new())
    }
}

fn transcription_text<'a>(content: &'a Value, key: &str) -> Option<&'a str> {
    content.get(key)?.get("text")?.as_str()
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
        assert_eq!(value["setup"]["model"], "models/gemini-3.5-transcribe-live");
        assert_eq!(
            value["setup"]["inputAudioTranscription"]["languageCodes"],
            json!(["vi", "en"])
        );
        assert_eq!(
            value["setup"]["inputAudioTranscription"]["customVocabulary"],
            json!(["7PK2604", "ROTUYN"])
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
    fn base64_padding_is_correct() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }
}
