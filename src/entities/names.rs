//! Name-like span scanning (title-case spans and acronyms).

use crate::core::types::EntityMention;

use super::normalize::{casefold, claim, is_claimed, push_mention};
use super::provider::RuleBasedEntityProvider;

/// Generic organization suffixes across languages (legal forms and
/// institution words). Generic vocabulary, never dataset content.
const ORG_SUFFIXES: &[&str] = &[
    "inc",
    "llc",
    "ltd",
    "corp",
    "corporation",
    "company",
    "co",
    "group",
    "holdings",
    "partners",
    "gmbh",
    "ag",
    "ug",
    "sa",
    "sl",
    "srl",
    "sas",
    "sarl",
    "spa",
    "bv",
    "nv",
    "ab",
    "oy",
    "pty",
    "bank",
    "banks",
    "university",
    "college",
    "institute",
    "foundation",
    "association",
    "agency",
    "ministry",
    "airlines",
    "airways",
    "railways",
    "studios",
    "records",
    "press",
    "media",
];

fn is_title_word(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    let rest: String = chars.collect();
    rest.chars().count() >= 1
        && rest
            .chars()
            .all(|ch| ch.is_lowercase() || matches!(ch, '\'' | '-' | '.'))
        && word.chars().any(|ch| ch.is_alphabetic())
}

fn is_acronym(word: &str) -> bool {
    let count = word.chars().count();
    (2..=6).contains(&count)
        && word.chars().all(|ch| ch.is_uppercase())
        && word.chars().any(|ch| ch.is_alphabetic())
}

impl RuleBasedEntityProvider {
    pub(crate) fn scan_names(
        &self,
        text: &str,
        claimed: &mut [bool],
        mentions: &mut Vec<EntityMention>,
        language: Option<&str>,
    ) {
        // Word spans with byte offsets.
        let mut words: Vec<(usize, usize, String)> = Vec::new();
        let mut start: Option<usize> = None;
        for (offset, ch) in text.char_indices() {
            if ch.is_alphabetic() || matches!(ch, '\'' | '-') {
                if start.is_none() {
                    start = Some(offset);
                }
            } else if let Some(word_start) = start.take() {
                words.push((word_start, offset, text[word_start..offset].to_string()));
                let _ = word_start;
            }
        }
        if let Some(word_start) = start {
            words.push((word_start, text.len(), text[word_start..].to_string()));
        }
        let mut index = 0;
        while index < words.len() {
            let (word_start, word_end, word) = &words[index];
            if is_claimed(claimed, *word_start, *word_end) {
                index += 1;
                continue;
            }
            if is_acronym(word) {
                // Acronyms that are common words (`IT`, `US`) are capitalization,
                // not organizations.
                if self.is_common_word(word) {
                    index += 1;
                    continue;
                }
                claim(claimed, *word_start, *word_end);
                push_mention(
                    mentions,
                    "organization",
                    casefold(word),
                    *word_start,
                    *word_end,
                    0.5,
                    language,
                );
                index += 1;
                continue;
            }
            if !is_title_word(word) {
                index += 1;
                continue;
            }
            // Greedily extend over consecutive title words joined by one space.
            let mut end_word = index;
            while end_word + 1 < words.len() && end_word + 1 - index < 3 {
                let (next_start, next_end, next_word) = &words[end_word + 1];
                if text[words[end_word].1..*next_start] != *" " || !is_title_word(next_word) {
                    break;
                }
                if is_claimed(claimed, *next_start, *next_end) {
                    break;
                }
                end_word += 1;
            }
            let span_start = words[index].0;
            let span_end = words[end_word].1;
            let span_text = &text[span_start..span_end];
            // A lone title word that is a common word is sentence
            // capitalization (`Where`, `Thanks`), not a name. Multi-word spans
            // keep their heuristic reading.
            if end_word == index && self.is_common_word(&words[index].2) {
                index += 1;
                continue;
            }
            let last_word = &words[end_word].2;
            let is_org = ORG_SUFFIXES.contains(&casefold(last_word).as_str());
            let (entity_type, confidence) = if is_org {
                ("organization", 0.7)
            } else if end_word > index {
                ("person", 0.65)
            } else {
                ("person", 0.45)
            };
            claim(claimed, span_start, span_end);
            push_mention(
                mentions,
                entity_type,
                casefold(span_text),
                span_start,
                span_end,
                confidence,
                language,
            );
            index = end_word + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::RuleBasedEntityProvider;
    use crate::core::providers::EntityProvider;
    use crate::core::types::EntityMention;

    fn extract(text: &str) -> Vec<EntityMention> {
        RuleBasedEntityProvider::default().extract(text, Some("en"))
    }

    #[test]
    fn names_and_org_suffixes_classify() {
        let person = extract("Ada Lovelace wrote the notes");
        assert!(
            person
                .iter()
                .any(|mention| mention.entity_type == "person" && mention.value == "ada lovelace")
        );
        let org = extract("Ada Lovelace Bank approved the transfer");
        assert!(
            org.iter()
                .any(|mention| mention.entity_type == "organization")
        );
    }
}
