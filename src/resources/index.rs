use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::types::LanguageCandidate;
use crate::normalization::unicode::casefold_text;

use super::order::{normalize_key, IndexKey};
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

    pub fn lookup(&self, query: &str) -> LexiconLookup {
        let key = normalize_key(query);
        let matches = self
            .records
            .get(&IndexKey::new(&key))
            .cloned()
            .unwrap_or_default();
        lookup_result(query, key, matches)
    }

    pub fn lookup_in_language(&self, query: &str, language: &str) -> LexiconLookup {
        let key = normalize_key(query);
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
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| {
                records
                    .iter()
                    .any(|record| language_allowed(&record.language, languages))
            })
    }

    pub fn contains_in_language(&self, word: &str, language: &str) -> bool {
        let language = canonical_language(language);
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| records.iter().any(|record| record.language == language))
    }

    pub fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        let prefix = normalize_key(prefix);
        !prefix.is_empty()
            && self.records.iter().any(|(key, records)| {
                key.as_str().starts_with(&prefix)
                    && records
                        .iter()
                        .any(|record| language_allowed(&record.language, languages))
            })
    }

    pub fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        self.records
            .get(&IndexKey::new(word))
            .is_some_and(|records| {
                records
                    .iter()
                    .any(|record| record.stop_word && language_allowed(&record.language, languages))
            })
    }

    pub fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
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

    pub fn detect_languages(&self, text: &str) -> Vec<LanguageCandidate> {
        let words = lexical_words(text);
        if words.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let mut scores = BTreeMap::<String, f64>::new();
        for word in words {
            let Some(records) = self.records.get(&IndexKey::new(&word)) else {
                continue;
            };
            let mut word_scores = BTreeMap::<String, f64>::new();
            for record in records {
                word_scores
                    .entry(record.language.clone())
                    .and_modify(|score| *score = score.max(record.weight))
                    .or_insert(record.weight);
            }
            for (language, score) in word_scores {
                *scores.entry(language).or_default() += score;
            }
        }
        if scores.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let total = scores.values().sum::<f64>().max(f64::EPSILON);
        let mut candidates = scores
            .into_iter()
            .map(|(language, score)| LanguageCandidate::new(language, score / total))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
        candidates
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
        self.records = records;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    symbols: BTreeMap<String, SymbolResource>,
}

impl SymbolIndex {
    pub fn merge(&mut self, symbol: SymbolResource) {
        self.symbols
            .entry(symbol.token.clone())
            .and_modify(|existing| merge_symbol(existing, &symbol))
            .or_insert(symbol);
    }

    pub fn get(&self, token: &str) -> Option<&SymbolResource> {
        self.symbols.get(token)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
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
