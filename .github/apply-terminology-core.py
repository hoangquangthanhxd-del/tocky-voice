from pathlib import Path

ROOT = Path.cwd()
if not (ROOT / 'src-tauri').exists():
    raise RuntimeError('run from repository root')


def replace_once(path, old, new):
    p = ROOT / path
    text = p.read_text(encoding='utf-8')
    if old not in text:
        raise RuntimeError(f'missing expected source in {path}: {old[:100]!r}')
    p.write_text(text.replace(old, new, 1), encoding='utf-8')

replace_once('src-tauri/src/lib.rs', 'mod tray;\nmod state;\n\nuse tauri::Manager;', 'mod tray;\nmod state;\nmod terminology;\n\nuse tauri::Manager;')

basic_structs = '''#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminologyEntry {
    pub canonical: String,
    pub aliases: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminologySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub send_to_stt: bool,
    #[serde(default)]
    pub entries: Vec<TerminologyEntry>,
}

impl Default for TerminologySettings {
    fn default() -> Self {
        Self { enabled: true, send_to_stt: true, entries: Vec::new() }
    }
}

'''
replace_once(
    'src-tauri/src/settings/mod.rs',
    '    pub audio_retention_days: i64,\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct AppSettings {',
    '    pub audio_retention_days: i64,\n}\n\n' + basic_structs + '#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct AppSettings {')
replace_once(
    'src-tauri/src/settings/mod.rs',
    'pub struct AppSettings {\n    pub stt: SttSettings,\n    pub llm: LlmSettings,',
    'pub struct AppSettings {\n    pub stt: SttSettings,\n    /// Domain vocabulary and deterministic transcript replacements.\n    #[serde(default)]\n    pub terminology: TerminologySettings,\n    pub llm: LlmSettings,')

replace_once(
    'src-tauri/src/settings/defaults.rs',
'''        stt: SttSettings {\n            provider: SttProviderKind::Soniox,\n            soniox_model: "stt-rt-preview".into(),\n            deepgram_model: "nova-2".into(),\n            language: "vi".into(),\n            language_hints: vec!["vi".into(), "en".into()],\n        },\n        llm: LlmSettings {''',
'''        stt: SttSettings {\n            provider: SttProviderKind::Soniox,\n            soniox_model: "stt-rt-preview".into(),\n            deepgram_model: "nova-2".into(),\n            language: "vi".into(),\n            language_hints: vec!["vi".into(), "en".into()],\n        },\n        terminology: TerminologySettings::default(),\n        llm: LlmSettings {''')

(ROOT / 'src-tauri/src/terminology.rs').write_text('// populated by PTAP terminology seed step\n', encoding='utf-8')

replace_once('src-tauri/src/session/pipeline.rs', 'use crate::{audio, inject};', 'use crate::{audio, inject, terminology};')
replace_once(
    'src-tauri/src/session/pipeline.rs',
'''    let final_text = if mode.ai_cleanup {\n        state::emit_status(app, Phase::Refining, mode_id);\n        refine_or_fall_back(app, &settings, &mode, &transcript).await\n    } else {\n        transcript.clone()\n    };''',
'''    let mapped_transcript = terminology::apply(&transcript, &settings.terminology);\n    let refined_text = if mode.ai_cleanup {\n        state::emit_status(app, Phase::Refining, mode_id);\n        refine_or_fall_back(app, &settings, &mode, &mapped_transcript).await\n    } else {\n        mapped_transcript\n    };\n    let final_text = terminology::apply(&refined_text, &settings.terminology);''')

replace_once('src-tauri/src/stt/mod.rs', 'use crate::settings::{SttProviderKind, SttSettings};', 'use crate::settings::{SttProviderKind, SttSettings, TerminologySettings};')
replace_once(
    'src-tauri/src/stt/mod.rs',
'''pub fn build_protocol(settings: &SttSettings, api_key: String) -> Box<dyn WsProtocol> {\n    match settings.provider {\n        SttProviderKind::Soniox => Box::new(soniox::Soniox::new(settings, api_key)),\n        SttProviderKind::Deepgram => Box::new(deepgram::Deepgram::new(settings, api_key)),\n        SttProviderKind::AssemblyAi => Box::new(assemblyai::AssemblyAi::new(api_key)),\n    }\n}''',
'''pub fn build_protocol(settings: &SttSettings, api_key: String) -> Box<dyn WsProtocol> {\n    build_protocol_with_terminology(settings, &TerminologySettings::default(), api_key)\n}\n\npub fn build_protocol_with_terminology(\n    settings: &SttSettings,\n    terminology_settings: &TerminologySettings,\n    api_key: String,\n) -> Box<dyn WsProtocol> {\n    let terms = crate::terminology::stt_terms(terminology_settings, 100);\n    match settings.provider {\n        SttProviderKind::Soniox => Box::new(soniox::Soniox::with_terms(settings, api_key, terms)),\n        SttProviderKind::Deepgram => Box::new(deepgram::Deepgram::with_terms(settings, api_key, terms)),\n        SttProviderKind::AssemblyAi => Box::new(assemblyai::AssemblyAi::new(api_key)),\n    }\n}''')
replace_once('src-tauri/src/session/mod.rs', '    let protocol = stt::build_protocol(&settings.stt, api_key);', '    let protocol = stt::build_protocol_with_terminology(&settings.stt, &settings.terminology, api_key);')

replace_once(
    'src-tauri/src/stt/soniox.rs',
'''pub struct Soniox {\n    api_key: String,\n    model: String,\n    language_hints: Vec<String>,\n}''',
'''pub struct Soniox {\n    api_key: String,\n    model: String,\n    language_hints: Vec<String>,\n    terms: Vec<String>,\n}''')
replace_once(
    'src-tauri/src/stt/soniox.rs',
'''    pub fn new(settings: &SttSettings, api_key: String) -> Self {\n        Self {\n            api_key,\n            model: settings.soniox_model.clone(),\n            language_hints: settings.language_hints.clone(),\n        }\n    }''',
'''    pub fn new(settings: &SttSettings, api_key: String) -> Self {\n        Self::with_terms(settings, api_key, Vec::new())\n    }\n\n    pub fn with_terms(settings: &SttSettings, api_key: String, terms: Vec<String>) -> Self {\n        Self {\n            api_key,\n            model: settings.soniox_model.clone(),\n            language_hints: settings.language_hints.clone(),\n            terms,\n        }\n    }''')
replace_once('src-tauri/src/stt/soniox.rs', '        let config = json!({', '        let mut config = json!({')
replace_once(
    'src-tauri/src/stt/soniox.rs',
'''            "enable_endpoint_detection": true,\n        });\n        Some(Message::Text(config.to_string()))''',
'''            "enable_endpoint_detection": true,\n        });\n        if !self.terms.is_empty() {\n            config["context"] = json!({ "terms": &self.terms });\n        }\n        Some(Message::Text(config.to_string()))''')

replace_once(
    'src-tauri/src/stt/deepgram.rs',
'''pub struct Deepgram {\n    api_key: String,\n    model: String,\n    language: String,\n}''',
'''pub struct Deepgram {\n    api_key: String,\n    model: String,\n    language: String,\n    terms: Vec<String>,\n}''')
replace_once(
    'src-tauri/src/stt/deepgram.rs',
'''    pub fn new(settings: &SttSettings, api_key: String) -> Self {\n        Self {\n            api_key,\n            model: settings.deepgram_model.clone(),\n            language: settings.language.clone(),\n        }\n    }''',
'''    pub fn new(settings: &SttSettings, api_key: String) -> Self {\n        Self::with_terms(settings, api_key, Vec::new())\n    }\n\n    pub fn with_terms(settings: &SttSettings, api_key: String, terms: Vec<String>) -> Self {\n        Self {\n            api_key,\n            model: settings.deepgram_model.clone(),\n            language: settings.language.clone(),\n            terms,\n        }\n    }''')
replace_once('src-tauri/src/stt/deepgram.rs', '        format!(\n            "wss://api.deepgram.com/v1/listen\\', '        let mut url = format!(\n            "wss://api.deepgram.com/v1/listen\\')
replace_once(
    'src-tauri/src/stt/deepgram.rs',
'''            rate = TARGET_SAMPLE_RATE,\n        )\n    }''',
'''            rate = TARGET_SAMPLE_RATE,\n        );\n        for term in &self.terms {\n            url.push_str("&keywords=");\n            url.push_str(&urlencoding::encode(term));\n            url.push_str(":1.5");\n        }\n        url\n    }''')

basic_ts = '''export interface TerminologyEntry {\n  canonical: string;\n  aliases: string[];\n  enabled: boolean;\n}\n\nexport interface TerminologySettings {\n  enabled: boolean;\n  send_to_stt: boolean;\n  entries: TerminologyEntry[];\n}\n\n'''
replace_once('src/lib/types.ts', 'export interface AppSettings {\n  stt: SttSettings;\n  llm: LlmSettings;', basic_ts + 'export interface AppSettings {\n  stt: SttSettings;\n  terminology: TerminologySettings;\n  llm: LlmSettings;')

print('terminology core applied')
