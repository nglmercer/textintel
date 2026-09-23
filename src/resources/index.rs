use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::types::LanguageCandidate;
use crate::normalization::unicode::casefold_text;
use crate::visual::scripts::scripts_in;

use super::order::{IndexKey, normalize_key};
use super::pack::{LanguagePack, SymbolResource};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LookupStatus {
    NotFound,
    Unique,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LexiconRecord {
    pub key: String,
    pub term: String,
    pub language: String,
    pub lemma: String,
    pub stop_word: bool,
    pub weight: f64,
    pub source: String,
    pub provenance: Option<String>,
    pub license: Option<String>,
    pub origin: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LexiconLookup {
    pub query: String,
    pub key: String,
    pub status: LookupStatus,
    pub matches: Vec<LexiconRecord>,
}

#[derive(Debug, Clone)]
struct LoadedLanguagePack {
    pack: LanguagePack,
    source_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct LanguageIndex {
    packs: BTreeMap<String, Vec<LoadedLanguagePack>>,
    records: BTreeMap<IndexKey, Vec<LexiconRecord>>,
    /// Longest normalized key in bytes. A key can carry a byte-prefix only
    /// when it is at least as long, so longer prefixes answer `false`
    /// without scanning the table.
    max_key_bytes: usize,
    /// True when at least one key contains whitespace (multi-word
    /// entries). Spaceless indexes answer spaced-prefix `starts_with`
    /// queries `false` without scanning: no key can carry the prefix.
    has_spaced_keys: bool,
}

impl LanguageIndex {
    pub fn add_pack(&mut self, mut pack: LanguagePack, source_path: &Path) {
        pack.language = canonical_language(&pack.language);
        self.packs
            .entry(pack.language.clone())
            .or_default()
            .push(LoadedLanguagePack {
                pack,
                source_path: source_path.display().to_string(),
            });
        self.rebuild();
    }

    pub fn languages(&self) -> Vec<String> {
        self.packs.keys().cloned().collect()
    }

    pub fn language_count(&self) -> usize {
        self.packs.len()
    }

    pub fn word_count(&self) -> usize {
        self.records.len()
    }

    pub fn record_count(&self) -> usize {
        self.records.values().map(Vec::len).sum()
    }

    pub fn index_keys(&self) -> Vec<String> {
        self.records
            .keys()
            .map(|key| key.as_str().to_string())
            .collect()
    }

    pub fn language_pack(&self, language: &str) -> Option<&LanguagePack> {
        self.packs
            .get(&canonical_language(language))
            .and_then(|packs| packs.first())
            .map(|loaded| &loaded.pack)
    }

    pub fn language_packs(&self, language: &str) -> Vec<&LanguagePack> {
        self.packs
            .get(&canonical_language(language))
            .map(|packs| packs.iter().map(|loaded| &loaded.pack).collect())
            .unwrap_or_default()
    }

    pub fn profile_texts(&self) -> BTreeMap<String, Vec<String>> {
        let mut profiles = BTreeMap::new();
        for (language, packs) in &self.packs {
            let samples = profiles.entry(language.clone()).or_insert_with(Vec::new);
            for loaded in packs {
                samples.extend(loaded.pack.entries.iter().map(|entry| entry.word.clone()));
                samples.extend(loaded.pack.words.iter().cloned());
                samples.extend(loaded.pack.slang.iter().cloned());
                samples.extend(loaded.pack.examples.iter().cloned());
            }
        }
        profiles
    }

    /// True when `word` cannot match any key: ASCII words longer than
    /// every key stay longer after normalization (trimming runs first
    /// and NFKC-casefold preserves ASCII length), so no key can equal
    /// them. Non-ASCII words may shrink under NFKC composition and
    /// always proceed to lookup. Short words pay one length compare.
    fn cannot_match(&self, word: &str) -> bool {
        if word.len() <= self.max_key_bytes {
            return false;
        }
        let trimmed = word.trim();
        trimmed.len() > self.max_key_bytes && trimmed.is_ascii()
    }

    pub fn lookup(&self, query: &str) -> LexiconLookup {
        let key = normalize_key(query);
        if key.len() > self.max_key_bytes {
            return lookup_result(query, key, Vec::new());
        }
        let matches = self
            .records
            .get(&IndexKey::new(&key))
            .cloned()
            .unwrap_or_default();
        lookup_result(query, key, matches)
    }

    pub fn lookup_in_language(&self, query: &str, language: &str) -> LexiconLookup {
        let key = normalize_key(query);
        if key.len() > self.max_key_bytes {
            return lookup_result(query, key, Vec::new());
        }
        let language = canonical_language(language);
        let matches = self
            .records
            .get(&IndexKey::new(&key))
            .map(|records| {
                records
                    .iter()
                    .filter(|record| record.language == language)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        lookup_result(query, key, matches)
    }

    pub fn records_for_key(&self, key: &str) -> Vec<LexiconRecord> {
        if self.cannot_match(key) {
            return Vec::new();
        }
        self.records
            .get(&IndexKey::new(key))
            .cloned()
            .unwrap_or_default()
    }

    pub fn lookup_languages(&self, word: &str) -> Vec<String> {
        let mut languages = self
            .lookup(word)
            .matches
            .into_iter()
            .map(|record| record.language)
            .collect::<BTreeSet<_>>();
        languages.remove("unknown");
        languages.into_iter().collect()
    }

    pub fn contains(&self, word: &str, languages: Option<&[String]>) -> bool {
        if self.cannot_match(word) {
            return false;
        }
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| {
                records
                    .iter()
                    .any(|record| language_allowed(&record.language, languages))
            })
    }

    pub fn contains_in_language(&self, word: &str, language: &str) -> bool {
        if self.cannot_match(word) {
            return false;
        }
        let language = canonical_language(language);
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| records.iter().any(|record| record.language == language))
    }

    /// True when at least one key contains whitespace, i.e. a pack
    /// contributed multi-word entries.
    pub fn has_spaced_keys(&self) -> bool {
        self.has_spaced_keys
    }

    pub fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        let prefix = normalize_key(prefix);
        if prefix.is_empty() || prefix.len() > self.max_key_bytes {
            return false;
        }
        // Spaced prefix on a spaceless index: a matching key would carry
        // the prefix's whitespace, which no key has.
        if !self.has_spaced_keys && prefix.chars().any(|ch| ch.is_whitespace()) {
            return false;
        }
        self.records.iter().any(|(key, records)| {
            key.as_str().starts_with(&prefix)
                && records
                    .iter()
                    .any(|record| language_allowed(&record.language, languages))
        })
    }

    pub fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        if self.cannot_match(word) {
            return false;
        }
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| {
                records
                    .iter()
                    .any(|record| record.stop_word && language_allowed(&record.language, languages))
            })
    }

    pub fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
        if self.cannot_match(word) {
            return None;
        }
        let records = self.records.get(&IndexKey::new(word))?;
        if let Some(languages) = languages.filter(|values| !values.is_empty()) {
            for language in languages {
                let language = canonical_language(language);
                if matches!(language.as_str(), "unknown" | "und") {
                    continue;
                }
                if let Some(record) = records.iter().find(|record| record.language == language) {
                    return Some(record.lemma.clone());
                }
            }
        }
        records.first().map(|record| record.lemma.clone())
    }

    pub fn frequency(&self, word: &str, languages: Option<&[String]>) -> Option<f64> {
        if self.cannot_match(word) {
            return None;
        }
        let records = self.records.get(&IndexKey::new(word))?;
        records
            .iter()
            .filter(|record| language_allowed(&record.language, languages))
            .map(|record| record.weight)
            .max_by(|left, right| left.total_cmp(right))
    }

    pub fn detect_languages(&self, text: &str) -> Vec<LanguageCandidate> {
        self.detect_languages_with_limit(text, 8)
    }

    pub fn detect_languages_with_limit(
        &self,
        text: &str,
        max_candidates: usize,
    ) -> Vec<LanguageCandidate> {
        let words = lexical_words(text);
        if words.is_empty() {
            let mut candidates = script_hint_scores(text)
                .into_iter()
                .map(|(language, score)| LanguageCandidate::new(language, score))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return vec![LanguageCandidate::new("unknown", 1.0)];
            }
            candidates.push(LanguageCandidate::new("unknown", 0.15));
            return normalize_candidates(candidates, max_candidates);
        }
        let mut scores = BTreeMap::<String, f64>::new();
        let mut matched_words = 0usize;
        let word_count = words.len();
        for word in words {
            let Some(records) = self.records.get(&IndexKey::new(&word)) else {
                continue;
            };
            matched_words += 1;
            let mut word_scores = BTreeMap::<String, f64>::new();
            for record in records {
                word_scores
                    .entry(record.language.clone())
                    .and_modify(|score| *score = score.max(record.weight.max(0.01)))
                    .or_insert(record.weight.max(0.01));
            }
            for (language, score) in word_scores {
                *scores.entry(language).or_default() += 1.0 + score.ln_1p();
            }
        }

        let script_hints = script_hint_scores(text);
        for (language, score) in script_hints {
            *scores.entry(language).or_default() += if matched_words == 0 {
                score
            } else {
                score * 0.15
            };
        }

        if scores.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        if matched_words < word_count {
            let unknown_ratio = 1.0 - matched_words as f64 / word_count.max(1) as f64;
            scores.insert("unknown".to_string(), unknown_ratio * 0.35);
        }
        let candidates = scores
            .into_iter()
            .map(|(language, score)| LanguageCandidate::new(language, score))
            .collect::<Vec<_>>();
        normalize_candidates(candidates, max_candidates)
    }

    fn rebuild(&mut self) {
        let mut records = BTreeMap::<IndexKey, Vec<LexiconRecord>>::new();
        for language_packs in self.packs.values() {
            for loaded in language_packs {
                for entry in &loaded.pack.entries {
                    add_record(
                        &mut records,
                        LexiconRecord {
                            key: normalize_key(&entry.word),
                            term: entry.word.clone(),
                            language: loaded.pack.language.clone(),
                            lemma: normalize_key(entry.lemma.as_deref().unwrap_or(&entry.word)),
                            stop_word: entry.stop_word,
                            weight: entry.weight,
                            source: loaded.source_path.clone(),
                            provenance: entry.source.clone().or_else(|| loaded.pack.source.clone()),
                            license: loaded.pack.license.clone(),
                            origin: "entry".to_string(),
                            revision: loaded.pack.revision.clone(),
                        },
                    );
                }
                for word in &loaded.pack.words {
                    add_record(
                        &mut records,
                        compact_record(&loaded.pack, &loaded.source_path, word, false, "word"),
                    );
                }
                for word in &loaded.pack.stop_words {
                    add_record(
                        &mut records,
                        compact_record(&loaded.pack, &loaded.source_path, word, true, "stop_word"),
                    );
                }
                for example in &loaded.pack.examples {
                    for word in lexical_words(example) {
                        add_record(
                            &mut records,
                            compact_record(
                                &loaded.pack,
                                &loaded.source_path,
                                &word,
                                false,
                                "example",
                            ),
                        );
                    }
                }
            }
        }
        self.max_key_bytes = records
            .keys()
            .map(|key| key.as_str().len())
            .max()
            .unwrap_or(0);
        self.has_spaced_keys = records
            .keys()
            .any(|key| key.as_str().chars().any(|ch| ch.is_whitespace()));
        self.records = records;
    }
}

fn normalize_candidates(
    mut candidates: Vec<LanguageCandidate>,
    max_candidates: usize,
) -> Vec<LanguageCandidate> {
    for candidate in &mut candidates {
        candidate.probability = candidate.probability.max(0.0);
    }
    candidates.sort_by(|left, right| {
        right
            .probability
            .total_cmp(&left.probability)
            .then_with(|| left.language.cmp(&right.language))
    });
    candidates.truncate(max_candidates.max(1));
    let total = candidates
        .iter()
        .map(|candidate| candidate.probability)
        .sum::<f64>()
        .max(f64::EPSILON);
    for candidate in &mut candidates {
        candidate.probability /= total;
    }
    candidates
}

fn script_hint_scores(text: &str) -> Vec<(String, f64)> {
    let scripts = scripts_in(text);
    let mut scores = BTreeMap::<String, f64>::new();
    for script in scripts {
        let hints: &[(&str, f64)] = match script.as_str() {
            "Arabic" => &[("ar", 0.60), ("fa", 0.25), ("ur", 0.15)],
            "Cyrillic" => &[("ru", 0.55), ("uk", 0.20), ("bg", 0.15), ("sr", 0.10)],
            "Devanagari" => &[("hi", 0.85), ("mr", 0.15)],
            "Han" => &[("zh", 0.60), ("ja", 0.40)],
            "Hiragana" | "Katakana" => &[("ja", 1.0)],
            "Hangul" => &[("ko", 1.0)],
            "Hebrew" => &[("he", 1.0)],
            "Thai" => &[("th", 1.0)],
            _ => &[],
        };
        for (language, score) in hints {
            *scores.entry((*language).to_string()).or_default() += score;
        }
    }
    scores.into_iter().collect()
}

#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    symbols: BTreeMap<IndexKey, SymbolResource>,
}

impl SymbolIndex {
    pub fn merge(&mut self, symbol: SymbolResource) {
        let key = IndexKey::new(&symbol.token);
        self.symbols
            .entry(key)
            .and_modify(|existing| merge_symbol(existing, &symbol))
            .or_insert(symbol);
    }

    pub fn get(&self, token: &str) -> Option<&SymbolResource> {
        self.symbols.get(&IndexKey::new(token))
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn tokens(&self) -> Vec<String> {
        self.symbols
            .values()
            .map(|symbol| symbol.token.clone())
            .collect()
    }
}

fn lookup_result(query: &str, key: String, matches: Vec<LexiconRecord>) -> LexiconLookup {
    let languages = matches
        .iter()
        .map(|record| record.language.as_str())
        .collect::<BTreeSet<_>>();
    let status = match languages.len() {
        0 => LookupStatus::NotFound,
        1 => LookupStatus::Unique,
        _ => LookupStatus::Ambiguous,
    };
    LexiconLookup {
        query: query.to_string(),
        key,
        status,
        matches,
    }
}

fn compact_record(
    pack: &LanguagePack,
    source_path: &str,
    word: &str,
    stop_word: bool,
    origin: &str,
) -> LexiconRecord {
    LexiconRecord {
        key: normalize_key(word),
        term: word.to_string(),
        language: pack.language.clone(),
        lemma: normalize_key(word),
        stop_word,
        weight: 1.0,
        source: source_path.to_string(),
        provenance: pack.source.clone(),
        license: pack.license.clone(),
        origin: origin.to_string(),
        revision: pack.revision.clone(),
    }
}

fn add_record(records: &mut BTreeMap<IndexKey, Vec<LexiconRecord>>, record: LexiconRecord) {
    if record.key.is_empty() {
        return;
    }
    let values = records.entry(IndexKey::new(&record.key)).or_default();
    if let Some(existing) = values.iter_mut().find(|existing| {
        existing.language == record.language
            && existing.term == record.term
            && existing.lemma == record.lemma
            && existing.origin == record.origin
            && existing.source == record.source
    }) {
        existing.stop_word |= record.stop_word;
        existing.weight = existing.weight.max(record.weight);
        return;
    }
    values.push(record);
}

fn lexical_words(text: &str) -> Vec<String> {
    let normalized = casefold_text(text);
    normalized
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(normalize_key)
        .filter(|word| !word.is_empty())
        .collect()
}

fn canonical_language(language: &str) -> String {
    language.trim().to_lowercase().replace('_', "-")
}

fn language_allowed(language: &str, languages: Option<&[String]>) -> bool {
    match languages {
        None | Some([]) => true,
        Some(values) => values
            .iter()
            .map(|value| canonical_language(value))
            .any(|value| matches!(value.as_str(), "unknown" | "und") || value == language),
    }
}

fn merge_symbol(existing: &mut SymbolResource, incoming: &SymbolResource) {
    if existing.unicode_name.is_none() {
        existing.unicode_name = incoming.unicode_name.clone();
    }
    for concept in &incoming.concepts {
        if !existing.concepts.iter().any(|value| value.id == concept.id) {
            existing.concepts.push(concept.clone());
        }
    }
    for reading in &incoming.readings {
        if !existing.readings.iter().any(|value| value == reading) {
            existing.readings.push(reading.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_with_words(language: &str, words: &[&str]) -> LanguagePack {
        serde_json::from_value(serde_json::json!({
            "language": language,
            "words": words,
        }))
        .expect("test pack must parse")
    }

    #[test]
    fn spaced_prefix_gate_respects_spaced_keys() {
        // Spaceless index: spaced prefixes answer false without scanning.
        let mut spaceless = LanguageIndex::default();
        spaceless.add_pack(
            pack_with_words("en", &["hello", "world"]),
            std::path::Path::new("test"),
        );
        assert!(!spaceless.has_spaced_keys());
        assert!(spaceless.starts_with("hel", None));
        assert!(!spaceless.starts_with("hello world", None));
        assert!(!spaceless.starts_with("  hello world  ", None));
        // Spaced keys present: the gate stays off and prefix matches work.
        let mut spaced = LanguageIndex::default();
        spaced.add_pack(
            pack_with_words("en", &["hello", "hello brave world"]),
            std::path::Path::new("test"),
        );
        assert!(spaced.has_spaced_keys());
        assert!(spaced.starts_with("hello brave", None));
        assert!(spaced.contains("hello brave world", None));
    }

    #[test]
    fn overlong_ascii_words_skip_lookup() {
        // Keys "é" and "ex": max key length is 2 bytes.
        let mut index = LanguageIndex::default();
        index.add_pack(
            pack_with_words("en", &["é", "ex"]),
            std::path::Path::new("test"),
        );
        // Boundary length still looks up and hits.
        assert!(index.contains("ex", None));
        assert_eq!(index.lemma("ex", None).as_deref(), Some("ex"));
        // Overlong ASCII cannot match (normalization preserves length).
        assert!(!index.contains("exx", None));
        assert_eq!(index.lemma("exx", None), None);
        assert_eq!(index.frequency("exx", None), None);
        assert!(!index.is_stop_word("exx", None));
        assert!(index.lookup("exx").matches.is_empty());
        // Overlong non-ASCII still proceeds: NFKC composes "e\u{301}"
        // down to the "é" key.
        assert!(index.contains("e\u{301}", None));
    }
}
