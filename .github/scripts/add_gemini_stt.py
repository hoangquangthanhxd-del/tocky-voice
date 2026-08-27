from pathlib import Path


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} occurrences, found {count}: {old!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


# ---------------------------------------------------------------- Rust settings / credentials
replace_exact(
    "src-tauri/src/settings/mod.rs",
    "pub enum SttProviderKind {\n    Soniox,\n    Deepgram,\n    AssemblyAi,\n}",
    "pub enum SttProviderKind {\n    Soniox,\n    Deepgram,\n    AssemblyAi,\n    Gemini,\n}",
)
replace_exact(
    "src-tauri/src/settings/mod.rs",
    "/// Hint list for providers that accept several (Soniox). Order matters — most likely first.",
    "/// Hint list for providers that accept several (Soniox/Gemini). Order matters — most likely first.",
)
replace_exact(
    "src-tauri/src/settings/mod.rs",
    'let mut accounts = vec!["soniox", "deepgram", "assemblyai"];',
    'let mut accounts = vec!["soniox", "deepgram", "assemblyai", "gemini"];',
)

replace_exact(
    "src-tauri/src/settings/secrets.rs",
    '        super::SttProviderKind::AssemblyAi => "assemblyai",\n',
    '        super::SttProviderKind::AssemblyAi => "assemblyai",\n        super::SttProviderKind::Gemini => "gemini",\n',
)

replace_exact(
    "src-tauri/src/commands.rs",
    'for account in ["soniox", "deepgram", "assemblyai"] {',
    'for account in ["soniox", "deepgram", "assemblyai", "gemini"] {',
)

# ---------------------------------------------------------------- Shared WebSocket transport
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "//! All three vendors follow the same shape — open a socket, optionally send a JSON",
    "//! All four vendors follow the same shape — open a socket, optionally send a JSON",
)
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "pub mod deepgram;\npub mod soniox;",
    "pub mod deepgram;\npub mod gemini;\npub mod soniox;",
)
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "    /// Frame that tells the vendor no more audio is coming.\n    fn finish_message(&self) -> Message;",
    "    /// Encodes one chunk of 16 kHz mono PCM16 for this vendor. Most providers take\n    /// binary frames directly; Gemini wraps the bytes as base64 inside `realtimeInput`.\n    fn audio_message(&self, bytes: Vec<u8>) -> Message {\n        Message::Binary(bytes)\n    }\n    /// Frame that tells the vendor no more audio is coming.\n    fn finish_message(&self) -> Message;",
)
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "        SttProviderKind::AssemblyAi => Box::new(assemblyai::AssemblyAi::new(api_key)),\n",
    "        SttProviderKind::AssemblyAi => Box::new(assemblyai::AssemblyAi::new(api_key)),\n        SttProviderKind::Gemini => Box::new(gemini::Gemini::with_terms(settings, api_key, terms)),\n",
)
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "send_bounded(&mut writer, Message::Binary(frame)).await",
    "send_bounded(&mut writer, protocol.audio_message(frame)).await",
)
replace_exact(
    "src-tauri/src/stt/mod.rs",
    "let tail = Message::Binary(std::mem::take(&mut pending));",
    "let tail = protocol.audio_message(std::mem::take(&mut pending));",
)

# ---------------------------------------------------------------- Gemini Live protocol
Path("src-tauri/src/stt/gemini.rs").write_text(r'''//! Google Gemini 3.5 Transcribe Live over the Gemini Live WebSocket API.
//!
//! Gemini differs from the other speech providers in one important transport detail:
//! PCM is base64-encoded inside a JSON `realtimeInput` frame rather than sent as a raw
//! binary WebSocket frame. The shared transport calls [`WsProtocol::audio_message`] so
//! this stays isolated here while capture, buffering, timeouts and transcript delivery
//! remain common to every provider.

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
''', encoding="utf-8")

# ---------------------------------------------------------------- Frontend provider catalogue
replace_exact(
    "src/lib/types.ts",
    'export type SttProviderKind = "soniox" | "deepgram" | "assembly_ai";',
    'export type SttProviderKind = "soniox" | "deepgram" | "assembly_ai" | "gemini";',
)
replace_exact(
    "src/lib/types.ts",
    '  badge: "best_vietnamese" | "free_credit";',
    '  badge: "best_vietnamese" | "free_credit" | "free_tier";',
)
replace_exact(
    "src/lib/types.ts",
    '''  {
    id: "deepgram",
    label: "Deepgram",
    secret: "deepgram",
    signupUrl: "https://console.deepgram.com/signup",
    badge: "free_credit",
    freeCredit: "$200",
    hourlyUsd: 0.29,
  },''',
    '''  {
    id: "gemini",
    label: "Google Gemini",
    secret: "gemini",
    signupUrl: "https://aistudio.google.com/apikey",
    badge: "free_tier",
    freeCredit: null,
    hourlyUsd: 0.54,
  },
  {
    id: "deepgram",
    label: "Deepgram",
    secret: "deepgram",
    signupUrl: "https://console.deepgram.com/signup",
    badge: "free_credit",
    freeCredit: "$200",
    hourlyUsd: 0.29,
  },''',
)

replace_exact(
    "src/components/stt-provider-badges.tsx",
    ''' *   Soniox      → Best for Vietnamese   (most accurate on mixed VI/EN; no free credit)
 *   Deepgram    → Free $200             (largest free allowance; ~690 hours)
 *   AssemblyAI  → Free $50              (English only on streaming)''',
    ''' *   Soniox      → Best for Vietnamese
 *   Google Gemini → Free tier
 *   Deepgram    → Free $200
 *   AssemblyAI  → Free $50''',
)
replace_exact(
    "src/components/stt-provider-badges.tsx",
    '''  const label =
    provider.badge === "free_credit"
      ? t.stt.free_credit.replace("{amount}", provider.freeCredit ?? "")
      : t.stt.best_vietnamese;

  // Green reads as "costs you nothing to try", amber as "this is the good one" — the
  // two reasons are different, so they must not share a colour.
  const tone = provider.badge === "free_credit" ? "chip--ok" : "chip--star";''',
    '''  const label =
    provider.badge === "free_credit"
      ? t.stt.free_credit.replace("{amount}", provider.freeCredit ?? "")
      : provider.badge === "free_tier"
        ? t.stt.free_tier
        : t.stt.best_vietnamese;

  // Green reads as "costs you nothing to try", amber as "this is the good one" — the
  // two reasons are different, so they must not share a colour.
  const tone = provider.badge === "best_vietnamese" ? "chip--star" : "chip--ok";''',
)
replace_exact(
    "src/components/stt-provider-badges.tsx",
    'return t.stt[provider.id as "soniox" | "deepgram" | "assembly_ai"];',
    'return t.stt[provider.id as "soniox" | "deepgram" | "assembly_ai" | "gemini"];',
)

# ---------------------------------------------------------------- UI copy
replace_exact(
    "src/lib/i18n.ts",
    '"Soniox accepts several at once — this is what makes a sentence that mixes Vietnamese and English work. Click to toggle; the number shows priority."',
    '"Soniox and Gemini accept several at once — useful when a sentence mixes Vietnamese and English. Click to toggle; the number shows priority."',
)
replace_exact(
    "src/lib/i18n.ts",
    '"Prioritises selected canonical terms in Soniox or Deepgram before local mapping runs."',
    '"Prioritises selected canonical terms in Soniox, Deepgram or Gemini before local mapping runs."',
)
replace_exact(
    "src/lib/i18n.ts",
    'keyStep1: "Open {provider} and sign up — no card needed for the free credit.",',
    'keyStep1: "Open {provider} and sign up — use its free tier or trial where available.",',
)
replace_exact(
    "src/lib/i18n.ts",
    '''    best_vietnamese: "Best for Vietnamese",
    free_credit: "Free {amount}",
    paid: "Paid — ${price}/hour",''',
    '''    best_vietnamese: "Best for Vietnamese",
    free_credit: "Free {amount}",
    free_tier: "Free tier",
    paid: "Paid — ${price}/hour",''',
)
replace_exact(
    "src/lib/i18n.ts",
    '''    soniox:
      "The most accurate when you mix Vietnamese and English in one sentence. Billed from the first minute, but the cheapest per hour.",
    deepgram:''',
    '''    soniox:
      "The most accurate when you mix Vietnamese and English in one sentence. Billed from the first minute, but the cheapest per hour.",
    gemini:
      "Gemini 3.5 Transcribe Live: free tier, 85+ languages, code-switching and custom vocabulary for domain terms.",
    deepgram:''',
)
replace_exact(
    "src/lib/i18n.ts",
    '"Soniox nhận nhiều ngôn ngữ cùng lúc — đây là lý do câu trộn tiếng Việt và tiếng Anh vẫn nghe tốt. Bấm để bật/tắt; số là thứ tự ưu tiên."',
    '"Soniox và Gemini nhận nhiều ngôn ngữ cùng lúc — hữu ích khi câu nói trộn tiếng Việt và tiếng Anh. Bấm để bật/tắt; số là thứ tự ưu tiên."',
)
replace_exact(
    "src/lib/i18n.ts",
    '"Ưu tiên các từ chuẩn đã chọn trong Soniox hoặc Deepgram trước khi map cục bộ."',
    '"Ưu tiên các từ chuẩn đã chọn trong Soniox, Deepgram hoặc Gemini trước khi map cục bộ."',
)
replace_exact(
    "src/lib/i18n.ts",
    'keyStep1: "Mở {provider} và đăng ký — không cần thẻ để nhận credit miễn phí.",',
    'keyStep1: "Mở {provider} và đăng ký — dùng free tier hoặc trial nếu nhà cung cấp có hỗ trợ.",',
)
replace_exact(
    "src/lib/i18n.ts",
    '''    best_vietnamese: "Chuẩn tiếng Việt",
    free_credit: "Miễn phí {amount}",
    paid: "Trả phí — ${price}/giờ",''',
    '''    best_vietnamese: "Chuẩn tiếng Việt",
    free_credit: "Miễn phí {amount}",
    free_tier: "Free tier",
    paid: "Trả phí — ${price}/giờ",''',
)
replace_exact(
    "src/lib/i18n.ts",
    '''    soniox:
      "Nghe chuẩn nhất khi bạn nói tiếng Việt lẫn tiếng Anh trong cùng một câu. Tính tiền ngay từ phút đầu, nhưng lại rẻ nhất theo giờ.",
    deepgram:''',
    '''    soniox:
      "Nghe chuẩn nhất khi bạn nói tiếng Việt lẫn tiếng Anh trong cùng một câu. Tính tiền ngay từ phút đầu, nhưng lại rẻ nhất theo giờ.",
    gemini:
      "Gemini 3.5 Transcribe Live: có free tier, hỗ trợ hơn 85 ngôn ngữ, trộn ngôn ngữ và custom vocabulary cho thuật ngữ chuyên ngành.",
    deepgram:''',
)

# Keep privacy copy accurate now that one provider uses a free tier rather than a credit grant.
replace_exact(
    "src/lib/i18n.ts",
    "Credit amounts above are what the vendors advertise and can change without notice — check the signup page.",
    "Free-tier and credit information above is what the vendors advertise and can change without notice — check the signup page.",
)
replace_exact(
    "src/lib/i18n.ts",
    "Các con số credit ở trên là do hãng quảng cáo và có thể đổi bất cứ lúc nào — hãy kiểm tra lại ở trang đăng ký.",
    "Thông tin free tier và credit ở trên là do hãng công bố và có thể đổi bất cứ lúc nào — hãy kiểm tra lại ở trang đăng ký.",
)

print("Gemini STT integration staged successfully")
