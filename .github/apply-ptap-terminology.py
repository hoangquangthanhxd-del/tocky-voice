from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parents[2] if '.github' in str(Path(__file__).resolve()) else Path.cwd()
if not (ROOT / 'src-tauri').exists():
    ROOT = Path.cwd()


def replace_once(path, old, new):
    p = ROOT / path
    text = p.read_text(encoding='utf-8')
    if old not in text:
        raise RuntimeError(f'missing expected text in {path}: {old[:80]!r}')
    p.write_text(text.replace(old, new, 1), encoding='utf-8')

replace_once(
    'src-tauri/src/settings/mod.rs',
'''    /// Lets users temporarily disable a mapping without deleting it.\n    #[serde(default = "default_true")]\n    pub enabled: bool,\n}''',
'''    /// Lets users temporarily disable a mapping without deleting it.\n    #[serde(default = "default_true")]\n    pub enabled: bool,\n    /// User-created terms default above bundled seed terms, so they win provider caps.\n    #[serde(default = "default_user_term_priority")]\n    pub priority: i32,\n    /// Provenance for bundled/imported terms; user-created entries normally leave this empty.\n    #[serde(default)]\n    pub source: Option<String>,\n    /// Whether this canonical form is also sent to the speech provider as a recognition hint.\n    #[serde(default = "default_true")]\n    pub provider_hint: bool,\n}''')
replace_once(
    'src-tauri/src/settings/mod.rs',
'''    /// Domain vocabulary and deterministic transcript replacements.\n    #[serde(default)]\n    pub terminology: TerminologySettings,''',
'''    /// Domain vocabulary and deterministic transcript replacements.\n    #[serde(default = "defaults::default_terminology_settings")]\n    pub terminology: TerminologySettings,''')
replace_once(
    'src-tauri/src/settings/mod.rs',
'''fn default_true() -> bool {\n    true\n}\n''',
'''fn default_true() -> bool {\n    true\n}\n\nfn default_user_term_priority() -> i32 {\n    2_000\n}\n''')

settings_mod = ROOT / 'src-tauri/src/settings/mod.rs'
text = settings_mod.read_text(encoding='utf-8')
needle = '''    #[test]\n    fn a_dictation_key_the_user_chose_themselves_is_left_alone() {\n'''
insert = '''    #[test]\n    fn settings_from_before_terminology_receive_the_ptap_seed() {\n        let mut raw = serde_json::to_value(defaults::default_settings()).unwrap();\n        raw.as_object_mut().unwrap().remove("terminology");\n\n        let parsed: AppSettings = serde_json::from_value(raw).unwrap();\n\n        assert_eq!(parsed.terminology.entries.len(), 100);\n    }\n\n'''
if insert not in text:
    if needle not in text:
        raise RuntimeError('cannot insert settings compatibility test')
    text = text.replace(needle, insert + needle, 1)
    settings_mod.write_text(text, encoding='utf-8')

CATEGORIES = [
    'ROTUYN','VÒNG BI','MÁ PHANH ĐĨA','PHỚT','CUROA',
    'CAO SU CÀNG A, CAO SU GIẰNG, CAO SU CHÂN GIẢM XÓC','GIẢM XÓC (PHUỘC)','LỌC NHỚT','LỌC GIÓ ĐỘNG CƠ',
    'HEO BÁNH, XI LANH PHANH SAU, BẦU PHANH SAU',
    'THƯỚC LÁI + TRỤC LÁI + TRỤC CÁC ĐĂNG + KHỚP TRUYỀN LÁI + BẠC THƯỚC LÁI + BẠT THƯỚC LÁI',
    'MÁ PHANH CÀNG','PISTON PHANH','LỌC XĂNG','LÁ CÔN','Mâm ép Bàn ép','LỌC DẦU','BUGI',
    'ĐẦU LÁP - CÂY LÁP TRƯỚC','ỐNG NƯỚC','ROĂNG QUY LÁT','CHỔI GẠT MƯA','BƠM NƯỚC',
    'ROĂNG GIÀN CÒ - NẮP GIÀN CÒ','BI TÊ, ỐNG XẢ E BITE'
]
PRODUCT_NAMES = [
    'BURI CHÂN NGẮN GIẮC 16','Mô tơ bơm xăng GIM TO','BURI HD KIA M12 T16 CHÂN DÀI','XE ÔM',
    'MÁ PHANH SAU GUỐC SA045','BURI CHÂN DÀI M12 T14 TẦM NHIỆT = 6','LÁ CÔN F10A','Lọc nhớt 10303',
    'ROTUYN LÁI NGANG $22','LỌC DẦU M12X1.25 2E900 D3900','MÂM ÉP F10A $18 CM LỒI','BURI SUZUKI K14',
    'ẮC BẠC TRỤ ĐỨNG KIA FRONTIER','HEO BÁNH TRƯỚC (1 PISTON)','MÁ PHANH D2253','GIẢM XÓC TRƯỚC THACO',
    'CAO SU CÀNG A TO','MÁ PHANH SP1401','MÁ PHANH TRƯỚC SP1399','ROTUYN CÂN BẰNG TRƯỚC $12 Z L283 TÂM',
    'ROTUYN CÂN BẰNG TRƯỚC','BƠM XĂNG GIM TO MPU-K300','BI 6207-2RS','CÀNG I TOWNER 950 990','LÁ CÔN KD-31'
]
SKUS = [
    'XO','XO40K','MPU-K300','MTBX_GT-DSX','PVN12462','PVN13956','10303 Lafien','MTBX_GT-DSTQ','BKR5EYA-11',
    'GIP-501DY','MTBX_GT-BX','MTBX_GT-KC','MTBX_GT-LUC','MPU-118 23220-75040/0C050','PK20TT','65x90x13',
    'K20TT','SA045','SA045HQ','IK20TT','13T0138B','90919T1002-XIN TOYOTA INDO 90919-T1002','97311x4','BKR6EGP 97311','BRT1002-HOP'
]
VEHICLES = [
    'TRANSIT','I10','ISUZU','CIVIC','k2700','NAVARA','KENBO','K3000','VIOS08','VIOS14','ranger','CAMRY','MORNING',
    'COUNTY','MATIZ','ELANTRA','CRUZE','ORLANDO','GIMTO','PAJEROV93','CERATO','K3','AVANTE','I30','CARENS'
]
MANUAL_ALIASES = {
    'ROTUYN': ['ro tuyn', 'rô tuyn'],
    'PHỚT': ['phốt'],
    'CUROA': ['cu roa', 'cua roa', 'cu-roa'],
    'BUGI': ['bu gi'],
    'ROĂNG QUY LÁT': ['gioăng quy lát', 'ron quy lát'],
    'ROĂNG GIÀN CÒ - NẮP GIÀN CÒ': ['gioăng giàn cò nắp giàn cò', 'ron giàn cò nắp giàn cò'],
    'BURI CHÂN NGẮN GIẮC 16': ['bu ri chân ngắn giắc 16'],
    'BURI HD KIA M12 T16 CHÂN DÀI': ['bu ri hd kia m 12 t 16 chân dài'],
    'BURI CHÂN DÀI M12 T14 TẦM NHIỆT = 6': ['bu ri chân dài m 12 t 14 tầm nhiệt 6'],
    'BURI SUZUKI K14': ['bu ri suzuki k 14'],
    'I10': ['i 10'], 'I30': ['i 30'], 'K3': ['k 3'], 'k2700': ['k 2700'], 'K3000': ['k 3000'],
    'VIOS08': ['vios 08'], 'VIOS14': ['vios 14'], 'PAJEROV93': ['pajero v93', 'pajero v 93'],
}

def punct_variant(value):
    return re.sub(r'\s+', ' ', re.sub(r'[-‐‑‒–—_./\\|]+', ' ', value)).strip()

def generic_aliases(value):
    result = list(MANUAL_ALIASES.get(value, []))
    v = punct_variant(value)
    if v != value and v not in result:
        result.append(v)
    return result[:4]

def sku_aliases(value):
    result = []
    v = punct_variant(value)
    if v != value:
        result.append(v)
    w = re.sub(r'([A-Za-z])([0-9])', r'\1 \2', v)
    w = re.sub(r'([0-9])([A-Za-z])', r'\1 \2', w)
    w = re.sub(r'\s+', ' ', w).strip()
    if w != value and w not in result:
        result.append(w)
    parts = []
    for token in w.split():
        if re.fullmatch(r'[A-Za-z]{2,5}', token) and token.upper() not in {'XIN','INDO','HOP'}:
            parts.append(' '.join(token))
        else:
            parts.append(token)
    spelled = ' '.join(parts)
    if spelled != value and spelled not in result:
        result.append(spelled)
    return result[:4]

def vehicle_aliases(value):
    result = list(MANUAL_ALIASES.get(value, []))
    if re.search(r'[A-Za-z]\d|\d[A-Za-z]', value):
        spaced = re.sub(r'([A-Za-z])([0-9])', r'\1 \2', value)
        spaced = re.sub(r'([0-9])([A-Za-z])', r'\1 \2', spaced)
        if spaced != value and spaced.casefold() not in {a.casefold() for a in result}:
            result.append(spaced)
    return result[:4]

def make_entry(canonical, aliases, priority, source, provider_hint=True):
    return {
        'canonical': canonical,
        'aliases': aliases,
        'enabled': True,
        'priority': priority,
        'source': source,
        'provider_hint': provider_hint,
    }

entries = []
for i, value in enumerate(SKUS):
    entries.append(make_entry(value, sku_aliases(value), 1000 - i, 'ptap_sku'))
for i, value in enumerate(PRODUCT_NAMES):
    entries.append(make_entry(value, generic_aliases(value), 900 - i, 'ptap_product_name', len(value) <= 64 and '$' not in value))
for i, value in enumerate(VEHICLES):
    entries.append(make_entry(value, vehicle_aliases(value), 850 - i, 'ptap_vehicle'))
for i, value in enumerate(CATEGORIES):
    entries.append(make_entry(value, generic_aliases(value), 800 - i, 'ptap_product_category', len(value) <= 64 and '+' not in value))

assert len(entries) == 100
seed_path = ROOT / 'src-tauri/src/ptap_terminology_seed.json'
seed_path.write_text(json.dumps(entries, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

defaults = ROOT / 'src-tauri/src/settings/defaults.rs'
text = defaults.read_text(encoding='utf-8')
anchor = 'pub fn default_settings() -> AppSettings {\n'
seed_fn = '''pub fn default_terminology_settings() -> TerminologySettings {\n    let entries: Vec<TerminologyEntry> = serde_json::from_str(include_str!("../ptap_terminology_seed.json"))\n        .expect("bundled PTAP terminology seed must be valid JSON");\n    TerminologySettings { enabled: true, send_to_stt: true, entries }\n}\n\n'''
if seed_fn not in text:
    if anchor not in text:
        raise RuntimeError('cannot insert default_terminology_settings')
    text = text.replace(anchor, seed_fn + anchor, 1)
text = text.replace('terminology: TerminologySettings::default(),', 'terminology: default_terminology_settings(),', 1)
end_marker = '\n/// Factory hotkeys for macOS.'
seed_test = '''\n#[cfg(test)]\nmod terminology_seed_tests {\n    use super::*;\n    use std::collections::HashMap;\n\n    #[test]\n    fn ptap_seed_is_balanced_across_the_four_requested_fields() {\n        let settings = default_terminology_settings();\n        assert_eq!(settings.entries.len(), 100);\n        let mut counts: HashMap<&str, usize> = HashMap::new();\n        for entry in &settings.entries {\n            *counts.entry(entry.source.as_deref().unwrap_or("unknown")).or_default() += 1;\n        }\n        assert_eq!(counts.get("ptap_product_category"), Some(&25));\n        assert_eq!(counts.get("ptap_product_name"), Some(&25));\n        assert_eq!(counts.get("ptap_sku"), Some(&25));\n        assert_eq!(counts.get("ptap_vehicle"), Some(&25));\n    }\n}\n'''
if seed_test not in text:
    if end_marker not in text:
        raise RuntimeError('cannot insert terminology seed test')
    text = text.replace(end_marker, seed_test + end_marker, 1)
defaults.write_text(text, encoding='utf-8')

terminology_rs = r'''//! Deterministic terminology mapping for domain-specific dictation.
//!
//! Provider hints improve recognition, but only this local pass guarantees exact output.
//! It runs before optional AI cleanup and again immediately before paste.

use crate::settings::TerminologySettings;
use std::collections::HashSet;

#[derive(Debug)]
struct Candidate<'a> {
    canonical: &'a str,
    units: Vec<String>,
    priority: i32,
}

fn lower_unit(c: char) -> String {
    c.to_lowercase().collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn boundary_ok(chars: &[char], start: usize, end: usize) -> bool {
    let left_ok = start == 0 || !is_word_char(chars[start - 1]);
    let right_ok = end == chars.len() || !is_word_char(chars[end]);
    left_ok && right_ok
}

pub fn apply(text: &str, settings: &TerminologySettings) -> String {
    if !settings.enabled || settings.entries.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let mut candidates = Vec::new();
    for entry in settings.entries.iter().filter(|entry| entry.enabled) {
        let canonical = entry.canonical.trim();
        if canonical.is_empty() {
            continue;
        }
        for alias in std::iter::once(canonical).chain(entry.aliases.iter().map(String::as_str)) {
            let alias = alias.trim();
            if alias.is_empty() {
                continue;
            }
            candidates.push(Candidate {
                canonical,
                units: alias.chars().map(lower_unit).collect(),
                priority: entry.priority,
            });
        }
    }
    candidates.sort_by(|a, b| {
        b.units.len().cmp(&a.units.len()).then_with(|| b.priority.cmp(&a.priority))
    });
    if candidates.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let lowered: Vec<String> = chars.iter().copied().map(lower_unit).collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let matched = candidates.iter().find(|candidate| {
            let end = i + candidate.units.len();
            end <= chars.len() && boundary_ok(&chars, i, end) && lowered[i..end] == candidate.units[..]
        });
        if let Some(candidate) = matched {
            out.push_str(candidate.canonical);
            i += candidate.units.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn stt_terms(settings: &TerminologySettings, limit: usize) -> Vec<String> {
    if !settings.enabled || !settings.send_to_stt || limit == 0 {
        return Vec::new();
    }
    let mut entries: Vec<_> = settings.entries.iter().filter(|entry| entry.enabled && entry.provider_hint).collect();
    entries.sort_by(|a, b| b.priority.cmp(&a.priority));
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for entry in entries {
        let canonical = entry.canonical.trim();
        if canonical.is_empty() {
            continue;
        }
        let key = canonical.to_lowercase();
        if seen.insert(key) {
            result.push(canonical.to_string());
            if result.len() == limit {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{TerminologyEntry, TerminologySettings};

    fn dictionary(entries: Vec<TerminologyEntry>) -> TerminologySettings {
        TerminologySettings { enabled: true, send_to_stt: true, entries }
    }

    fn entry(canonical: &str, aliases: &[&str]) -> TerminologyEntry {
        TerminologyEntry {
            canonical: canonical.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
            enabled: true,
            priority: 2_000,
            source: None,
            provider_hint: true,
        }
    }

    #[test]
    fn maps_a_spoken_part_code_to_exact_canonical_text() {
        let settings = dictionary(vec![entry("7PK2604", &["7 pk 2604", "bảy pê ka hai sáu không bốn"])]);
        assert_eq!(apply("lấy dây 7 pk 2604 giúp tôi", &settings), "lấy dây 7PK2604 giúp tôi");
    }

    #[test]
    fn maps_common_ptap_domain_aliases() {
        let settings = dictionary(vec![entry("CUROA", &["cu roa", "cua roa"]), entry("PHỚT", &["phốt"])]);
        assert_eq!(apply("lấy cu roa và phốt", &settings), "lấy CUROA và PHỚT");
    }

    #[test]
    fn longest_alias_wins() {
        let settings = dictionary(vec![entry("PK", &["pê ka"]), entry("7PK2604", &["bảy pê ka hai sáu không bốn"])]);
        assert_eq!(apply("bảy pê ka hai sáu không bốn", &settings), "7PK2604");
    }

    #[test]
    fn priority_breaks_equal_length_alias_ties() {
        let mut low = entry("LOW", &["abc"]);
        low.priority = 10;
        let mut high = entry("HIGH", &["abc"]);
        high.priority = 20;
        assert_eq!(apply("abc", &dictionary(vec![low, high])), "HIGH");
    }

    #[test]
    fn does_not_replace_inside_a_larger_word() {
        let settings = dictionary(vec![entry("API", &["api"])]);
        assert_eq!(apply("rapid api test", &settings), "rapid API test");
    }

    #[test]
    fn disabled_entries_are_ignored() {
        let mut disabled = entry("4G92", &["4 g 92"]);
        disabled.enabled = false;
        assert_eq!(apply("4 g 92", &dictionary(vec![disabled])), "4 g 92");
    }

    #[test]
    fn stt_terms_are_unique_bounded_and_priority_ordered() {
        let mut seed = entry("SEED", &[]);
        seed.priority = 800;
        let user = entry("USER", &[]);
        let duplicate = entry("user", &[]);
        assert_eq!(stt_terms(&dictionary(vec![seed, duplicate, user]), 2), vec!["user", "SEED"]);
    }

    #[test]
    fn provider_hint_can_be_disabled_without_disabling_local_mapping() {
        let mut local_only = entry("ROTUYN", &["ro tuyn"]);
        local_only.provider_hint = false;
        let settings = dictionary(vec![local_only]);
        assert_eq!(apply("ro tuyn", &settings), "ROTUYN");
        assert!(stt_terms(&settings, 100).is_empty());
    }
}
'''
(ROOT / 'src-tauri/src/terminology.rs').write_text(terminology_rs, encoding='utf-8')

replace_once(
    'src/lib/types.ts',
'''export interface TerminologyEntry {\n  canonical: string;\n  aliases: string[];\n  enabled: boolean;\n}''',
'''export interface TerminologyEntry {\n  canonical: string;\n  aliases: string[];\n  enabled: boolean;\n  priority: number;\n  source: string | null;\n  provider_hint: boolean;\n}''')

print('PTAP terminology seed applied:', len(entries), 'entries')
