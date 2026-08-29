//! PTAP terminology snapshots used by native dictation.
//!
//! The backend owns the vocabulary. This module only validates a backend snapshot,
//! builds deterministic local indexes, persists a checksummed offline cache, and pins
//! one immutable snapshot for the lifetime of a dictation session.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};

pub const PROVIDER_TERM_CAP: usize = 100;
pub const CACHE_FILE: &str = "ptap-vocabulary-cache.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VocabularyEntry {
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub send_to_stt: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub provider_hint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VocabularySnapshot {
    pub revision: u64,
    pub fingerprint: String,
    #[serde(default)]
    pub effective_dictionary_fingerprint: String,
    #[serde(default)]
    pub provider_projection_fingerprint: String,
    #[serde(default)]
    pub provider_projection: Vec<String>,
    pub entries: Vec<VocabularyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEnvelope {
    schema_version: u8,
    checksum: String,
    snapshot: VocabularySnapshot,
}

#[derive(Debug, Clone)]
struct Candidate {
    canonical: String,
    units: Vec<char>,
    priority: i32,
    source: String,
    numeric_vehicle_alias: bool,
}

#[derive(Debug)]
pub struct CompiledVocabulary {
    snapshot: VocabularySnapshot,
    candidates: Vec<Candidate>,
}

#[derive(Debug, Default)]
pub struct VocabularyManager {
    current: RwLock<Option<Arc<CompiledVocabulary>>>,
}

pub fn cache_path(app: &AppHandle) -> Result<PathBuf> {
    let directory = app
        .path()
        .app_data_dir()
        .context("no application data directory for vocabulary cache")?;
    Ok(directory.join(CACHE_FILE))
}

fn default_true() -> bool {
    true
}

fn normalized_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn checksum(snapshot: &VocabularySnapshot) -> Result<String> {
    let encoded =
        serde_json::to_vec(snapshot).context("serializing vocabulary checksum payload")?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

impl VocabularySnapshot {
    pub fn validate(&self) -> Result<()> {
        if self.revision == 0 {
            bail!("vocabulary revision must be positive");
        }
        if !valid_sha256(&self.fingerprint) {
            bail!("vocabulary fingerprint is not a lowercase SHA-256 identity");
        }
        for identity in [
            &self.effective_dictionary_fingerprint,
            &self.provider_projection_fingerprint,
        ] {
            if !identity.is_empty() && !valid_sha256(identity) {
                bail!("vocabulary component fingerprint is invalid");
            }
        }
        if self.entries.is_empty() {
            bail!("vocabulary snapshot contains no entries");
        }

        if self.provider_projection.len() > PROVIDER_TERM_CAP {
            bail!("vocabulary provider projection exceeds the hard cap");
        }
        let eligible = self
            .entries
            .iter()
            .filter(|entry| entry.enabled && entry.send_to_stt)
            .map(|entry| entry.canonical.as_str())
            .collect::<HashSet<_>>();
        let mut projected = HashSet::new();
        for term in &self.provider_projection {
            if !eligible.contains(term.as_str()) {
                bail!("vocabulary provider projection contains an ineligible term");
            }
            if !projected.insert(normalized_key(term)) {
                bail!("vocabulary provider projection contains a duplicate term");
            }
        }
        if eligible.len() <= PROVIDER_TERM_CAP
            && !self.provider_projection.is_empty()
            && self.provider_projection.len() != eligible.len()
        {
            bail!("vocabulary provider projection is incomplete");
        }

        let mut outputs = BTreeMap::<String, String>::new();
        for entry in &self.entries {
            let canonical = entry.canonical.trim();
            if canonical.is_empty() {
                bail!("vocabulary entry has an empty canonical");
            }
            for phrase in std::iter::once(canonical).chain(entry.aliases.iter().map(String::as_str))
            {
                let key = normalized_key(phrase);
                if key.is_empty() {
                    bail!("vocabulary entry has an empty alias");
                }
                if let Some(existing) = outputs.insert(key, canonical.to_string()) {
                    if existing != canonical {
                        bail!("unsafe alias collision between {existing:?} and {canonical:?}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn provider_terms(&self) -> Result<Vec<String>> {
        self.validate()?;
        if !self.provider_projection.is_empty() {
            return Ok(self.provider_projection.clone());
        }
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| entry.enabled && entry.send_to_stt)
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| {
                    normalized_key(&left.canonical).cmp(&normalized_key(&right.canonical))
                })
                .then_with(|| left.source.cmp(&right.source))
                .then_with(|| left.canonical.cmp(&right.canonical))
        });
        let mut seen = HashSet::new();
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let key = normalized_key(&entry.canonical);
                seen.insert(key).then(|| entry.canonical.clone())
            })
            .take(PROVIDER_TERM_CAP)
            .collect())
    }
}

impl CompiledVocabulary {
    pub fn compile(snapshot: VocabularySnapshot) -> Result<Self> {
        snapshot.validate()?;
        let mut candidates = snapshot
            .entries
            .iter()
            .filter(|entry| entry.enabled)
            .flat_map(|entry| {
                std::iter::once(entry.canonical.as_str())
                    .chain(entry.aliases.iter().map(String::as_str))
                    .map(|phrase| Candidate {
                        canonical: entry.canonical.clone(),
                        units: phrase.to_lowercase().chars().collect(),
                        priority: entry.priority,
                        source: entry.source.clone(),
                        numeric_vehicle_alias: entry.source == "vehicle"
                            && phrase.chars().all(|character| character.is_ascii_digit()),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .units
                .len()
                .cmp(&left.units.len())
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| {
                    normalized_key(&left.canonical).cmp(&normalized_key(&right.canonical))
                })
                .then_with(|| left.source.cmp(&right.source))
        });
        Ok(Self {
            snapshot,
            candidates,
        })
    }

    pub fn snapshot(&self) -> &VocabularySnapshot {
        &self.snapshot
    }

    pub fn provider_terms(&self) -> Result<Vec<String>> {
        self.snapshot.provider_terms()
    }

    pub fn normalize(&self, value: &str) -> String {
        let characters = value.trim().chars().collect::<Vec<_>>();
        let lowered = characters
            .iter()
            .flat_map(|character| character.to_lowercase())
            .collect::<Vec<_>>();
        // Vietnamese case folding is one scalar for the vocabulary used here, so the
        // original/lower vectors retain matching indexes.
        if lowered.len() != characters.len() {
            return value.trim().to_string();
        }

        let mut output = String::new();
        let mut index = 0;
        let mut last_match_source: Option<String> = None;
        let mut only_space_since_match = false;
        while index < characters.len() {
            let candidate = self.candidates.iter().find(|candidate| {
                let end = index + candidate.units.len();
                end <= characters.len()
                    && (index == 0 || !is_word_character(characters[index - 1]))
                    && (end == characters.len() || !is_word_character(characters[end]))
                    && candidate
                        .units
                        .iter()
                        .enumerate()
                        .all(|(offset, unit)| lowered[index + offset] == *unit)
                    && (!candidate.numeric_vehicle_alias
                        || matches!(
                            last_match_source.as_deref(),
                            Some("product_category" | "PRODUCT_PHRASE" | "product_phrase")
                        ))
            });
            if let Some(candidate) = candidate {
                if only_space_since_match
                    && last_match_source.as_deref() == Some("brand")
                    && matches!(
                        candidate.source.as_str(),
                        "origin" | "quality" | "manufacturing_type"
                    )
                {
                    while output.ends_with(char::is_whitespace) {
                        output.pop();
                    }
                    output.push_str(" + ");
                }
                output.push_str(&candidate.canonical);
                index += candidate.units.len();
                last_match_source = Some(candidate.source.clone());
                only_space_since_match = false;
            } else {
                let character = characters[index];
                output.push(character);
                only_space_since_match = last_match_source.is_some() && character.is_whitespace();
                if !character.is_whitespace() {
                    last_match_source = None;
                }
                index += 1;
            }
        }
        output
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

impl VocabularyManager {
    pub fn install(
        &self,
        snapshot: VocabularySnapshot,
        expected_revision: u64,
        expected_fingerprint: &str,
    ) -> Result<Arc<CompiledVocabulary>> {
        if snapshot.revision != expected_revision {
            bail!(
                "vocabulary revision mismatch: expected {expected_revision}, received {}",
                snapshot.revision
            );
        }
        if snapshot.fingerprint != expected_fingerprint {
            bail!("vocabulary fingerprint mismatch");
        }
        let compiled = Arc::new(CompiledVocabulary::compile(snapshot)?);
        *self
            .current
            .write()
            .map_err(|_| anyhow!("vocabulary lock poisoned"))? = Some(compiled.clone());
        Ok(compiled)
    }

    pub fn pin(&self) -> Result<Arc<CompiledVocabulary>> {
        self.current
            .read()
            .map_err(|_| anyhow!("vocabulary lock poisoned"))?
            .clone()
            .ok_or_else(|| anyhow!("no usable vocabulary is cached"))
    }

    pub fn load_cache(&self, path: &Path) -> Result<Arc<CompiledVocabulary>> {
        let snapshot = load_cache(path)?;
        let compiled = Arc::new(CompiledVocabulary::compile(snapshot)?);
        *self
            .current
            .write()
            .map_err(|_| anyhow!("vocabulary lock poisoned"))? = Some(compiled.clone());
        Ok(compiled)
    }
}

pub fn save_cache(path: &Path, snapshot: &VocabularySnapshot) -> Result<()> {
    snapshot.validate()?;
    let envelope = CacheEnvelope {
        schema_version: 1,
        checksum: checksum(snapshot)?,
        snapshot: snapshot.clone(),
    };
    let encoded = serde_json::to_vec(&envelope).context("serializing vocabulary cache")?;
    let parent = path
        .parent()
        .context("vocabulary cache has no parent directory")?;
    std::fs::create_dir_all(parent).context("creating vocabulary cache directory")?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, encoded).context("writing vocabulary cache temporary file")?;
    if path.exists() {
        std::fs::remove_file(path).context("replacing vocabulary cache")?;
    }
    std::fs::rename(&temporary, path).context("committing vocabulary cache")
}

pub fn load_cache(path: &Path) -> Result<VocabularySnapshot> {
    let raw = std::fs::read(path).context("reading vocabulary cache")?;
    let envelope: CacheEnvelope =
        serde_json::from_slice(&raw).context("vocabulary cache is corrupt")?;
    if envelope.schema_version != 1 {
        bail!("unsupported vocabulary cache schema");
    }
    let actual = checksum(&envelope.snapshot)?;
    if actual != envelope.checksum {
        bail!("vocabulary cache checksum mismatch");
    }
    envelope.snapshot.validate()?;
    Ok(envelope.snapshot)
}

/// Speech code runs stay outside terminology. The resulting identifier is suitable
/// for the existing exact SKU lookup; this function never consults aliases.
pub fn normalize_spoken_code_run(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(canonical: &str, aliases: &[&str], source: &str) -> VocabularyEntry {
        VocabularyEntry {
            canonical: canonical.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
            enabled: true,
            send_to_stt: false,
            priority: 600,
            source: source.into(),
            provider_hint: false,
        }
    }

    fn snapshot(
        revision: u64,
        marker: char,
        mut entries: Vec<VocabularyEntry>,
    ) -> VocabularySnapshot {
        while entries.len() < 1_316 {
            let index = entries.len();
            entries.push(entry(&format!("TEST TERM {index:04}"), &[], "test"));
        }
        VocabularySnapshot {
            revision,
            fingerprint: format!("sha256:{}", marker.to_string().repeat(64)),
            effective_dictionary_fingerprint: String::new(),
            provider_projection_fingerprint: String::new(),
            provider_projection: Vec::new(),
            entries,
        }
    }

    fn v6() -> VocabularySnapshot {
        let mut entries = vec![
            entry("PORTER 2", &["PT2"], "vehicle"),
            entry("MAZDA 3", &["MD3"], "vehicle"),
            entry("DENSO", &[], "brand"),
            entry("NSK", &[], "brand"),
            entry("TRUNG QUỐC", &["TQ"], "origin"),
            entry("JAPAN", &[], "origin"),
            entry(
                "CAO SU CÂN BẰNG TRƯỚC",
                &["CAO SU ÔM CÂN BẰNG TRƯỚC"],
                "PRODUCT_PHRASE",
            ),
            entry(
                "CAO SU CÂN BẰNG SAU",
                &["CAO SU ỐP CÂN BẰNG SAU"],
                "PRODUCT_PHRASE",
            ),
            entry("MẶT GƯƠNG CHIẾU HẬU", &["MẶT GƯƠNG"], "product_category"),
            entry("HEO BÁNH", &[], "product_category"),
            entry("TOWNER 950", &["950"], "vehicle"),
        ];
        for index in 0..79 {
            let mut provider = entry(&format!("PROVIDER {index:03}"), &[], "product_category");
            provider.send_to_stt = true;
            provider.priority = 1_000 - index;
            entries.push(provider);
        }
        let mut value = snapshot(2, 'a', entries);
        value.provider_projection = (0..79)
            .rev()
            .map(|index| format!("PROVIDER {index:03}"))
            .collect();
        value
    }

    #[test]
    fn full_v6_semantic_normalization_and_numeric_vehicle_guard() {
        let compiled = CompiledVocabulary::compile(v6()).unwrap();
        assert_eq!(compiled.snapshot().entries.len(), 1_316);
        assert_eq!(compiled.normalize("PT2"), "PORTER 2");
        assert_eq!(compiled.normalize("MD3"), "MAZDA 3");
        assert_eq!(compiled.normalize("DENSO TQ"), "DENSO + TRUNG QUỐC");
        assert_eq!(compiled.normalize("NSK JAPAN"), "NSK + JAPAN");
        assert_eq!(
            compiled.normalize("CAO SU ÔM CÂN BẰNG TRƯỚC"),
            "CAO SU CÂN BẰNG TRƯỚC"
        );
        assert_eq!(
            compiled.normalize("CAO SU ỐP CÂN BẰNG SAU"),
            "CAO SU CÂN BẰNG SAU"
        );
        assert_eq!(compiled.normalize("MẶT GƯƠNG"), "MẶT GƯƠNG CHIẾU HẬU");
        assert_eq!(compiled.normalize("heo bánh 950"), "HEO BÁNH TOWNER 950");
        assert_eq!(compiled.normalize("giá 950"), "giá 950");
    }

    #[test]
    fn provider_projection_is_deterministic_deduplicated_and_capped() {
        let current = v6();
        let projected = current.provider_terms().unwrap();
        assert_eq!(projected.len(), 79);
        assert_eq!(projected.first().map(String::as_str), Some("PROVIDER 078"));
        let mut oversized = snapshot(3, 'b', Vec::new());
        for (index, entry) in oversized.entries.iter_mut().enumerate() {
            entry.send_to_stt = true;
            entry.priority = (index % 7) as i32;
        }
        let first = oversized.provider_terms().unwrap();
        oversized.entries.reverse();
        assert_eq!(first, oversized.provider_terms().unwrap());
        assert_eq!(first.len(), PROVIDER_TERM_CAP);
    }

    #[test]
    fn disabled_terms_do_not_normalize_or_reach_provider() {
        let mut value = snapshot(1, 'c', vec![entry("MAZDA 3", &["MD3"], "vehicle")]);
        value.entries[0].enabled = false;
        value.entries[0].send_to_stt = true;
        let compiled = CompiledVocabulary::compile(value).unwrap();
        assert_eq!(compiled.normalize("MD3"), "MD3");
        assert!(compiled.provider_terms().unwrap().is_empty());
    }

    #[test]
    fn collisions_and_revision_or_fingerprint_mismatches_fail_closed() {
        let collision = VocabularySnapshot {
            revision: 1,
            fingerprint: format!("sha256:{}", "d".repeat(64)),
            effective_dictionary_fingerprint: String::new(),
            provider_projection_fingerprint: String::new(),
            provider_projection: Vec::new(),
            entries: vec![entry("A", &["same"], "test"), entry("B", &["SAME"], "test")],
        };
        assert!(collision
            .validate()
            .unwrap_err()
            .to_string()
            .contains("collision"));
        let manager = VocabularyManager::default();
        let value = v6();
        assert!(manager
            .install(value.clone(), 3, &value.fingerprint)
            .is_err());
        assert!(manager
            .install(value.clone(), 2, &format!("sha256:{}", "e".repeat(64)))
            .is_err());
    }

    #[test]
    fn cache_round_trip_offline_and_corruption_rejection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CACHE_FILE);
        let value = v6();
        save_cache(&path, &value).unwrap();
        assert_eq!(load_cache(&path).unwrap(), value);
        let mut raw = std::fs::read(&path).unwrap();
        let index = raw.len() / 2;
        raw[index] ^= 1;
        std::fs::write(&path, raw).unwrap();
        assert!(load_cache(&path).is_err());
        assert!(load_cache(&directory.path().join("empty.json")).is_err());
    }

    #[test]
    fn active_session_keeps_its_revision_when_manager_refreshes() {
        let manager = VocabularyManager::default();
        let first = v6();
        let session_a = manager
            .install(first.clone(), 2, &first.fingerprint)
            .unwrap();
        let next = snapshot(3, 'f', vec![entry("NEXT", &[], "test")]);
        let session_b = manager.install(next.clone(), 3, &next.fingerprint).unwrap();
        assert_eq!(session_a.snapshot().revision, 2);
        assert_eq!(session_b.snapshot().revision, 3);
        assert_eq!(manager.pin().unwrap().snapshot().revision, 3);
    }

    #[test]
    fn sku_code_runs_never_pass_through_terminology() {
        assert_eq!(normalize_spoken_code_run("x o 40 k"), "XO40K");
        assert!(v6()
            .entries
            .iter()
            .all(|entry| !entry.source.to_lowercase().contains("sku")));
    }
}
