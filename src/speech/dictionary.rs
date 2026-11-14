//! Pronunciation exception dictionary (grapheme rewrites, not phonemes).
//!
//! Caller-supplied whole-word exceptions can be matched before speech preparation.
//! Entries are opt-in; no domain-specific substitutions are applied implicitly.
//!
//! Guarantees: case-sensitive lexical boundaries, longest matches, one left-to-right pass
//! over the *original* text (replacements are never re-scanned, so rules
//! cannot loop); bounded entry count and lengths.

/// Maximum number of entries.
pub const MAX_ENTRIES: usize = 512;
/// Maximum characters of a key or a replacement.
pub const MAX_ENTRY_CHARS: usize = 64;

/// Error building a dictionary. Carries no entry text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DictionaryError {
    #[error("pronunciation dictionary exceeds the entry limit")]
    TooManyEntries,
    #[error("pronunciation dictionary entry exceeds the character limit")]
    EntryTooLong,
    #[error("pronunciation dictionary keys must not be empty")]
    EmptyKey,
    #[error("pronunciation dictionary contains duplicate keys")]
    DuplicateKey,
}

/// Validated, sorted dictionary.
#[derive(Clone, Debug, Default)]
pub struct Dictionary {
    entries: Vec<(String, String)>,
    starts: std::collections::BTreeMap<char, std::ops::Range<usize>>,
}

impl Dictionary {
    /// Build from `(written, spoken)` pairs.
    pub fn new(pairs: &[(&str, &str)]) -> Result<Self, DictionaryError> {
        if pairs.len() > MAX_ENTRIES {
            return Err(DictionaryError::TooManyEntries);
        }
        let mut entries: Vec<(String, String)> = Vec::with_capacity(pairs.len());
        for (key, value) in pairs {
            if key.is_empty() {
                return Err(DictionaryError::EmptyKey);
            }
            if key.chars().count() > MAX_ENTRY_CHARS || value.chars().count() > MAX_ENTRY_CHARS {
                return Err(DictionaryError::EntryTooLong);
            }
            entries.push(((*key).to_string(), (*value).to_string()));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        if entries.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(DictionaryError::DuplicateKey);
        }
        let mut starts: std::collections::BTreeMap<char, std::ops::Range<usize>> =
            Default::default();
        for (i, (key, _)) in entries.iter().enumerate() {
            let first = key
                .chars()
                .next()
                .expect("validated nonempty dictionary key");
            starts
                .entry(first)
                .and_modify(|range| range.end = i + 1)
                .or_insert(i..i + 1);
        }
        Ok(Self { entries, starts })
    }

    /// `true` when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replacement for a whole word, if any.
    pub fn lookup(&self, word: &str) -> Option<&str> {
        self.entries
            .binary_search_by(|(key, _)| key.as_str().cmp(word))
            .ok()
            .map(|index| self.entries[index].1.as_str())
    }

    /// Original byte spans `(start, end, replacement)`, longest matches first.
    /// Latin/Indic words respect word boundaries and combining marks. Known
    /// CJK phrases can match in unspaced text. Protected code/tags are skipped.
    /// Multiword pronunciation entries are supported without rescanning output.
    pub fn matches<'a>(&'a self, text: &str) -> Vec<(usize, usize, &'a str)> {
        let mut out = Vec::new();
        if self.is_empty() {
            return out;
        }
        let regions = super::lexer::protected_regions(text);
        let mut cursor = 0;
        let mut region_index = 0;
        while cursor < text.len() {
            while region_index < regions.len() && regions[region_index].end <= cursor {
                region_index += 1;
            }
            if let Some(region) = regions
                .get(region_index)
                .filter(|r| r.start <= cursor && cursor < r.end)
            {
                cursor = region.end;
                continue;
            }
            let first = text[cursor..].chars().next().expect("nonempty text suffix");
            let candidate = if super::core::left_boundary(text, cursor) {
                self.starts.get(&first).and_then(|range| {
                    self.entries[range.clone()]
                        .iter()
                        .filter_map(|(key, replacement)| {
                            let end = cursor + key.len();
                            if text[cursor..].starts_with(key)
                                && super::core::right_boundary(text, end)
                                && !regions.iter().any(|r| r.start < end && cursor < r.end)
                            {
                                Some((end, replacement.as_str()))
                            } else {
                                None
                            }
                        })
                        .max_by_key(|(end, _)| *end)
                })
            } else {
                None
            };
            if let Some((end, replacement)) = candidate {
                out.push((cursor, end, replacement));
                cursor = end;
            } else {
                cursor += first.len_utf8();
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_word_case_sensitive() {
        let dict = Dictionary::new(&[("Qwen", "Chuén")]).unwrap();
        let text = "Qwen, Qwen3 y qwen. (Qwen)";
        let spans = dict.matches(text);
        let words: Vec<&str> = spans.iter().map(|(s, e, _)| &text[*s..*e]).collect();
        assert_eq!(words, vec!["Qwen", "Qwen"]);
        assert!(spans.iter().all(|(_, _, r)| *r == "Chuén"));
    }

    #[test]
    fn no_recursion_and_limits() {
        // A replacement containing its own key is not re-scanned.
        let dict = Dictionary::new(&[("a", "a a")]).unwrap();
        assert_eq!(dict.matches("a b").len(), 1);
        assert_eq!(
            Dictionary::new(&[("", "x")]).unwrap_err(),
            DictionaryError::EmptyKey
        );
        assert_eq!(
            Dictionary::new(&[("k", "x"), ("k", "y")]).unwrap_err(),
            DictionaryError::DuplicateKey
        );
        let long = "x".repeat(MAX_ENTRY_CHARS + 1);
        assert_eq!(
            Dictionary::new(&[("k", long.as_str())]).unwrap_err(),
            DictionaryError::EntryTooLong
        );
        assert!(Dictionary::default().matches("anything").is_empty());
    }
}
