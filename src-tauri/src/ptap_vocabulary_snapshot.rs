//! Immutable, bundled PTAP automotive vocabulary cache for offline local dictation.
//!
//! The browser sends only revision + fingerprint. This prevents a web page from
//! replacing the local terminology and makes the cached snapshot explicit offline.

use crate::settings::TerminologySettings;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct VocabularySnapshot {
    revision: u32,
    fingerprint: String,
    entries: Vec<crate::settings::TerminologyEntry>,
}

static SNAPSHOT: OnceLock<VocabularySnapshot> = OnceLock::new();

fn cached() -> &'static VocabularySnapshot {
    SNAPSHOT.get_or_init(|| {
        serde_json::from_str(include_str!("ptap_vocabulary_snapshot_v1.json"))
            .expect("bundled PTAP vocabulary snapshot must be valid")
    })
}

pub fn matches(revision: u32, fingerprint: &str) -> bool {
    let snapshot = cached();
    revision == snapshot.revision && fingerprint == snapshot.fingerprint
}

pub fn terminology(revision: u32, fingerprint: &str) -> Option<TerminologySettings> {
    let snapshot = cached();
    matches(revision, fingerprint).then(|| TerminologySettings {
        enabled: true,
        send_to_stt: true,
        entries: snapshot.entries.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Corpus {
        revision: u32,
        fingerprint: String,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        input: String,
        expected: String,
    }

    #[test]
    fn bundled_snapshot_pins_revision_and_provider_terms() {
        let snapshot = cached();
        assert_eq!(snapshot.revision, 1);
        assert_eq!(crate::terminology::stt_terms(&terminology(1, &snapshot.fingerprint).unwrap(), 100).len(), 96);
        assert!(!matches(1, "sha256:wrong"));
    }

    #[test]
    fn automotive_regression_corpus_normalizes_with_the_cached_snapshot() {
        let corpus: Corpus = serde_json::from_str(include_str!("ptap_vocabulary_regression_v1.json")).unwrap();
        assert!(matches(corpus.revision, &corpus.fingerprint));
        let terminology = terminology(corpus.revision, &corpus.fingerprint).unwrap();
        for case in corpus.cases {
            assert_eq!(crate::terminology::apply(&case.input, &terminology), case.expected, "{}", case.input);
        }
    }
}
