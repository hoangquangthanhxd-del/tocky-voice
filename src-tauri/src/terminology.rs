//! Deterministic terminology mapping for domain-specific dictation.
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
