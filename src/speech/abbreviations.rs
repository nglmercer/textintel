//! Reviewed, case-sensitive abbreviation data and bounded contextual matching.
//!
//! Entries describe written expansions, not a pronunciation or native-accuracy
//! guarantee. Contexts deliberately leave ambiguous bare tokens unchanged.
use std::{ops::Range, sync::OnceLock};

use serde::{Deserialize, Serialize};

use super::{
    core::{digit, is_word},
    lexer::{Field, is_protected_identifier, scan_fields},
    types::{AbbreviationPolicy, SpeechLocale},
};

pub(crate) mod compatibility;

/// The minimum source context required by a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbbreviationContext {
    CapitalizedWord,
    CapitalizedWordOrNumber,
    NotSentenceEnd,
    NextLetter,
    NextNumber,
    CompanyName,
    Bibliographic,
}

/// A source explicitly documenting an expansion, or its versioned legacy table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSource {
    pub url: String,
    pub note: String,
}

/// One exact spelling. Different case or punctuation requires another entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub locale: String,
    pub written: String,
    pub spoken: String,
    pub context: AbbreviationContext,
    pub sources: Vec<CatalogSource>,
    pub legacy: bool,
}

/// The three contexts retained by the versioned normalization adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyContext {
    CapitalizedWord,
    CapitalizedWordOrNumber,
    NotSentenceEnd,
}

/// Auditable data, including the extra Italian versioned entries.
pub fn entries() -> &'static [CatalogEntry] {
    static CATALOG: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("data/abbreviations.json"))
            .expect("compiled abbreviation catalog must be valid JSON")
    })
}

pub fn entry(id: &str) -> Option<&'static CatalogEntry> {
    entries().iter().find(|entry| entry.id == id)
}

/// Number of declared spellings, not measured precision or language completeness.
/// Telugu and Chinese currently return zero: no general expansions are inferred
/// from translated examples or numeric unit symbols.
pub fn declared_coverage(locale: SpeechLocale) -> usize {
    entries()
        .iter()
        .filter(|entry| entry.locale == locale.code())
        .count()
}

/// Catalog spellings reachable under the selected policy. This is an inventory
/// count, not the number that match a particular input or measured accuracy.
pub fn enabled_coverage(locale: SpeechLocale, policy: AbbreviationPolicy) -> usize {
    if policy == AbbreviationPolicy::Preserve {
        return 0;
    }
    entries()
        .iter()
        .filter(|entry| {
            entry.locale == locale.code()
                && (policy == AbbreviationPolicy::Bibliographic
                    || entry.context != AbbreviationContext::Bibliographic)
        })
        .count()
}

/// The old adapter passes a word without its final period. This lookup only
/// exposes original entries; newly reviewed spellings cannot widen old behavior.
pub fn legacy_lookup(language: &str, word: &str) -> Option<(&'static str, LegacyContext)> {
    let language = super::locales::for_language(language)?;
    let entry = entries().iter().find(|entry| {
        entry.legacy
            && entry.locale == language.code
            && entry.written.strip_suffix('.') == Some(word)
    })?;
    let context = match entry.context {
        AbbreviationContext::CapitalizedWord => LegacyContext::CapitalizedWord,
        AbbreviationContext::CapitalizedWordOrNumber => LegacyContext::CapitalizedWordOrNumber,
        AbbreviationContext::NotSentenceEnd => LegacyContext::NotSentenceEnd,
        _ => return None,
    };
    Some((&entry.spoken, context))
}

/// Compatibility-only sentence-boundary vocabulary. Membership does not license
/// expansion. Initial-letter handling remains the segmenter's separate rule.
pub fn legacy_is_non_terminal(word: &str) -> bool {
    const WORDS: &[&str] = &[
        "Dr", "Dra", "Sr", "Sra", "Srta", "Sres", "Av", "Avda", "Prof", "Ud", "Uds", "Mr", "Mrs",
        "Ms", "St", "vs", "e.g", "i.e", "approx", "aprox", "núm", "pág",
    ];
    WORDS.contains(&word)
}

/// An expansion over original UTF-8 byte offsets, excluding surrounding quotes.
#[derive(Debug, Clone, Copy)]
pub struct AbbreviationMatch {
    pub start: usize,
    pub end: usize,
    pub entry: &'static CatalogEntry,
}

/// Which context grammar the matcher uses for catalog entries.
///
/// The public typed pipeline intentionally uses the reviewed catalog policy.
/// The versioned frontend additionally needs the exact legacy boundary for
/// entries that existed before the catalog migration; newer entries continue
/// to use the typed policy even in that mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MatcherMode {
    Typed,
    Versioned,
}

/// One lexical field scan per source, reusing the caller's protected ranges.
/// `regions` should be sorted and disjoint, as returned by the shared lexer.
pub struct Matcher<'a> {
    text: &'a str,
    fields: Vec<Field>,
    blocked: Vec<bool>,
    candidates: Vec<&'static CatalogEntry>,
    policy: AbbreviationPolicy,
    mode: MatcherMode,
    disabled: bool,
}

impl<'a> Matcher<'a> {
    pub fn new(
        text: &'a str,
        regions: &[Range<usize>],
        locale: SpeechLocale,
        policy: AbbreviationPolicy,
    ) -> Self {
        Self::with_mode(text, regions, locale, policy, MatcherMode::Typed)
    }

    pub(crate) fn with_mode(
        text: &'a str,
        regions: &[Range<usize>],
        locale: SpeechLocale,
        policy: AbbreviationPolicy,
        mode: MatcherMode,
    ) -> Self {
        // Structured documents are data even when a quoted string resembles a
        // treatment followed by a name. This check is once, never per alias.
        let structured = matches!(text.trim_start().chars().next(), Some('{' | '['))
            && serde_json::from_str::<serde_json::Value>(text)
                .is_ok_and(|value| value.is_object() || value.is_array());
        let disabled = policy == AbbreviationPolicy::Preserve || structured;
        let mut candidates: Vec<_> = entries()
            .iter()
            .filter(|entry| entry.locale == locale.code())
            .collect();
        // Longest exact spelling first makes overlapping catalog additions
        // deterministic without any case or punctuation folding.
        candidates.sort_by_key(|entry| std::cmp::Reverse(entry.written.len()));
        let disabled = disabled || candidates.is_empty();
        let fields = if disabled {
            Vec::new()
        } else {
            scan_fields(text, regions)
        };
        let blocked = fields
            .iter()
            .map(|field| {
                let raw = &text[field.start..field.end];
                field.protected
                    || is_protected_identifier(raw)
                    || raw.contains('/')
                    || raw.contains('`')
            })
            .collect();
        Self {
            text,
            fields,
            blocked,
            candidates,
            policy,
            mode,
            disabled,
        }
    }

    pub fn match_at(&self, start: usize) -> Option<AbbreviationMatch> {
        if self.disabled || !self.text.is_char_boundary(start) {
            return None;
        }
        let index = self.fields.partition_point(|field| field.end <= start);
        let field = *self.fields.get(index)?;
        if start < field.start
            || field.protected
            || (self.mode == MatcherMode::Typed && self.blocked[index])
        {
            return None;
        }
        self.candidates.iter().find_map(|&entry| {
            let end = start.checked_add(entry.written.len())?;
            if self.text.get(start..end)? != entry.written || end > field.end {
                return None;
            }
            let legacy_compatibility = self.mode == MatcherMode::Versioned && entry.legacy;
            if !legacy_compatibility && self.blocked[index] {
                return None;
            }
            if legacy_compatibility {
                // The legacy adapter recognizes the complete lexical core plus
                // exactly one following period. Punctuation after that period
                // belongs to the legacy continuation rule, not to a general
                // closing-punctuation boundary.
                if start != field.core_start
                    || end != field.core_end + 1
                    || !entry.written.ends_with('.')
                {
                    return None;
                }
            } else {
                let company = entry.context == AbbreviationContext::CompanyName;
                if !company
                    && (!self.text[field.start..start].chars().all(is_opening)
                        || !self.text[..start]
                            .chars()
                            .next_back()
                            .is_none_or(|c| !is_word(c))
                        || !self.text[end..field.end].chars().all(is_closing)
                        || self.text[end..field.end]
                            .chars()
                            .any(|c| matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？')))
                {
                    return None;
                }
            }
            let matches = if legacy_compatibility {
                self.legacy_context_matches(entry, index)
            } else {
                self.context_matches(entry, index, start, end)
            };
            matches.then_some(AbbreviationMatch { start, end, entry })
        })
    }

    /// Ordered, disjoint source spans suitable for the lexer's range helpers.
    pub fn matches(&self) -> Vec<AbbreviationMatch> {
        if self.disabled {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut end = 0;
        for (start, _) in self.text.char_indices() {
            if start >= end
                && let Some(found) = self.match_at(start)
            {
                end = found.end;
                matches.push(found);
            }
        }
        matches
    }

    fn next(&self, index: usize) -> Option<char> {
        let field = self.fields[index];
        let next = *self.fields.get(index + 1)?;
        let gap = &self.text[field.end..next.start];
        if gap.contains(['\n', '\r']) || self.blocked[index + 1] {
            return None;
        }
        let core = self.text[next.start..next.end]
            .trim_start_matches(is_opening)
            .trim_end_matches(is_closing);
        let first = core.chars().next()?;
        // A number is a complete Nd token; a name may contain combining marks,
        // apostrophes or hyphens, but cannot be an alphanumeric identifier.
        let numeric = digit(first).is_some() && core.chars().all(|c| digit(c).is_some());
        let word = first.is_alphabetic()
            && core.chars().all(|c| {
                c.is_alphabetic()
                    || crate::core::unicode::is_combining_mark(c)
                    || matches!(c, '-' | '\'' | '’')
            });
        (numeric || word).then_some(first)
    }

    /// Exact versioned context policy for pre-catalog entries. This mirrors
    /// `compatibility::legacy_match`: Unicode case is significant, only
    /// ASCII digits continue a name/number context, LF alone blocks the next
    /// field while CR does not, and capitalized contexts require the period
    /// to be the field's final punctuation.
    fn legacy_context_matches(&self, entry: &CatalogEntry, index: usize) -> bool {
        let field = self.fields[index];
        let dot = field.core_end;
        let after_dot_empty = dot + 1 == field.end;
        let next_first = self.fields.get(index + 1).and_then(|next| {
            let gap = &self.text[field.end..next.start];
            (!gap.contains('\n') && next.bare_start())
                .then(|| self.text[next.start..].chars().next())
                .flatten()
        });
        match entry.context {
            AbbreviationContext::CapitalizedWord => {
                after_dot_empty && next_first.is_some_and(char::is_uppercase)
            }
            AbbreviationContext::CapitalizedWordOrNumber => {
                after_dot_empty
                    && next_first.is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
            }
            AbbreviationContext::NotSentenceEnd => {
                compatibility::period_continues(self.text, &self.fields, index, dot)
            }
            _ => false,
        }
    }

    fn context_matches(
        &self,
        entry: &CatalogEntry,
        index: usize,
        start: usize,
        end: usize,
    ) -> bool {
        let field = self.fields[index];
        let next = self.next(index);
        match entry.context {
            AbbreviationContext::CapitalizedWord => next.is_some_and(char::is_uppercase),
            AbbreviationContext::CapitalizedWordOrNumber => {
                next.is_some_and(|c| c.is_uppercase() || digit(c).is_some())
            }
            AbbreviationContext::NextLetter => next.is_some_and(char::is_alphabetic),
            AbbreviationContext::NextNumber => next.is_some_and(|c| digit(c).is_some()),
            AbbreviationContext::Bibliographic => {
                self.policy == AbbreviationPolicy::Bibliographic
                    && (entry.locale != "ur" || next.is_some_and(|c| digit(c).is_some()))
            }
            AbbreviationContext::NotSentenceEnd => {
                let tail = &self.text[end..field.end];
                if tail
                    .chars()
                    .any(|c| matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？'))
                {
                    return false;
                }
                tail.chars().any(|c| matches!(c, ',' | ';' | ':' | ')'))
                    || next.is_some_and(|c| {
                        c.is_lowercase()
                            || digit(c).is_some()
                            || (c.is_alphabetic() && !c.is_uppercase())
                    })
            }
            AbbreviationContext::CompanyName => {
                // Only this context permits attachment to letters. A suffix
                // cannot hide in an inflected token or arbitrary identifier.
                let before = &self.text[field.start..start];
                let after = &self.text[end..field.end];
                if field.end - field.start > 4096 {
                    return false;
                }
                let prefix = before.chars().all(is_opening)
                    && (name_letters(after)
                        || (after.chars().all(is_closing)
                            && next.is_some_and(char::is_alphabetic)));
                let suffix = name_letters(before) && after.chars().all(is_closing);
                let spaced_suffix = before.chars().all(is_opening)
                    && after.chars().all(is_closing)
                    && index.checked_sub(1).is_some_and(|prev| {
                        let previous = self.fields[prev];
                        !self.blocked[prev]
                            && !self.text[previous.end..field.start].contains(['\n', '\r'])
                            && name_letters(&self.text[previous.core_start..previous.core_end])
                    });
                prefix || suffix || spaced_suffix
            }
        }
    }
}

fn is_opening(c: char) -> bool {
    matches!(
        c,
        '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '«' | '¿' | '¡' | '„' | '‹' | '「' | '『'
    )
}

fn is_closing(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '"'
            | '\''
            | '”'
            | '’'
            | '»'
            | '›'
            | '…'
            | '。'
            | '、'
            | '！'
            | '？'
            | '」'
            | '』'
            | '।'
            | '۔'
    )
}

fn name_letters(text: &str) -> bool {
    let body = text.trim_matches(|c| is_opening(c) || is_closing(c));
    body.chars().any(char::is_alphabetic)
        && body
            .chars()
            .all(|c| c.is_alphabetic() || crate::core::unicode::is_combining_mark(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(
        text: &str,
        locale: SpeechLocale,
        policy: AbbreviationPolicy,
    ) -> Vec<AbbreviationMatch> {
        Matcher::new(
            text,
            &super::super::lexer::protected_regions(text),
            locale,
            policy,
        )
        .matches()
    }

    #[test]
    fn catalog_is_auditable_unique_and_explicit_about_missing_languages() {
        let mut ids = std::collections::HashSet::new();
        let mut spellings = std::collections::HashSet::new();
        for candidate in entries() {
            assert!(ids.insert(&candidate.id), "duplicate {}", candidate.id);
            assert!(spellings.insert((&candidate.locale, &candidate.written)));
            assert_eq!(entry(&candidate.id), Some(candidate));
            assert!(!candidate.written.is_empty() && !candidate.spoken.is_empty());
            assert!(!candidate.sources.is_empty());
            assert!(
                candidate
                    .sources
                    .iter()
                    .all(|source| source.url.starts_with("https://") && !source.note.is_empty())
            );
        }
        assert_eq!(entry("unknown"), None);
        assert_eq!(declared_coverage(SpeechLocale::Te), 0);
        assert_eq!(declared_coverage(SpeechLocale::Zh), 0);
        assert_eq!(entries().iter().filter(|entry| entry.legacy).count(), 20);
    }

    #[test]
    fn legacy_lookup_exposes_only_original_spellings_and_contexts() {
        assert_eq!(
            legacy_lookup("en", "Dr"),
            Some(("doctor", LegacyContext::CapitalizedWord))
        );
        assert_eq!(
            legacy_lookup("es", "Av"),
            Some(("avenida", LegacyContext::CapitalizedWordOrNumber))
        );
        assert_eq!(
            legacy_lookup("en", "etc"),
            Some(("etcetera", LegacyContext::NotSentenceEnd))
        );
        assert_eq!(
            legacy_lookup("it", "Dott"),
            Some(("dottor", LegacyContext::CapitalizedWord))
        );
        for (language, word) in [("en", "dr"), ("en", "Dr."), ("hi", "डॉ"), ("unknown", "Dr")] {
            assert_eq!(legacy_lookup(language, word), None);
        }
        assert!(legacy_is_non_terminal("Avda"));
        assert!(legacy_is_non_terminal("e.g"));
        assert!(!legacy_is_non_terminal("D"));
        assert!(!legacy_is_non_terminal("डॉ"));
    }

    #[test]
    fn enabled_inventory_respects_policy_without_claiming_default_bibliography() {
        for locale in SpeechLocale::ALL {
            assert_eq!(enabled_coverage(locale, AbbreviationPolicy::Preserve), 0);
            assert_eq!(
                enabled_coverage(locale, AbbreviationPolicy::Bibliographic),
                declared_coverage(locale)
            );
            assert!(
                enabled_coverage(locale, AbbreviationPolicy::Contextual)
                    <= declared_coverage(locale)
            );
        }
        for locale in [SpeechLocale::Te, SpeechLocale::Zh] {
            assert_eq!(declared_coverage(locale), 0);
            assert_eq!(enabled_coverage(locale, AbbreviationPolicy::Contextual), 0);
            assert_eq!(
                enabled_coverage(locale, AbbreviationPolicy::Bibliographic),
                0
            );
        }
        for (locale, count) in [(SpeechLocale::Ar, 2), (SpeechLocale::Ur, 1)] {
            assert_eq!(declared_coverage(locale), count);
            assert_eq!(enabled_coverage(locale, AbbreviationPolicy::Contextual), 0);
            assert_eq!(
                enabled_coverage(locale, AbbreviationPolicy::Bibliographic),
                count
            );
        }
        assert_eq!(
            SpeechLocale::ALL
                .into_iter()
                .filter(|&locale| enabled_coverage(locale, AbbreviationPolicy::Contextual) > 0)
                .count(),
            16
        );
        assert_eq!(
            SpeechLocale::ALL
                .into_iter()
                .filter(|&locale| enabled_coverage(locale, AbbreviationPolicy::Bibliographic) > 0)
                .count(),
            18
        );
        assert_eq!(
            enabled_coverage(SpeechLocale::En, AbbreviationPolicy::Contextual),
            3
        );
    }

    #[test]
    fn titles_require_same_line_complete_names_and_exact_case() {
        for source in [
            "Dr. Smith",
            "Dr. ‘Smith’",
            "“Dr.” García",
            "「Dr.」 『Smith』",
            "Dr. García-López",
        ] {
            assert_eq!(
                found(source, SpeechLocale::En, AbbreviationPolicy::Contextual).len(),
                1,
                "{source}"
            );
        }
        for source in [
            "Dr.",
            "Dr. smith",
            "Dr.3",
            "Dr. 3",
            "xDr. Smith",
            "Dr.foo Smith",
            "Dr.. Smith",
            "Dr. AB5",
            "Dr.\nSmith",
            "Dr.\rSmith",
            "dr. Smith",
            "Dr. @Smith",
            "Dr. `Smith`",
        ] {
            assert!(
                found(source, SpeechLocale::En, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
        let doctor = found(
            "dr. Hasan",
            SpeechLocale::Id,
            AbbreviationPolicy::Contextual,
        );
        let doctorate = found(
            "Dr. Hasan",
            SpeechLocale::Id,
            AbbreviationPolicy::Contextual,
        );
        assert_eq!(doctor[0].entry.spoken, "dokter");
        assert_eq!(doctorate[0].entry.spoken, "doktor");
        assert!(
            found(
                "DR. Hasan",
                SpeechLocale::Id,
                AbbreviationPolicy::Contextual
            )
            .is_empty()
        );
    }

    #[test]
    fn native_title_contexts_keep_distinct_forms_and_original_utf8_offsets() {
        let cases = [
            (SpeechLocale::Hi, "डॉ. शर्मा", "डॉक्टर"),
            (SpeechLocale::Hi, "प्रो. शर्मा", "प्रोफेसर"),
            (SpeechLocale::Bn, "ড. রহমান", "ডক্টর"),
            (SpeechLocale::Bn, "ডা. রহমান", "ডাক্তার"),
            (SpeechLocale::Pa, "ਡਾ: ਸਿੰਘ", "ਡਾਕਟਰ"),
            (SpeechLocale::Pa, "ਪ੍ਰਿੰ: ਸਿੰਘ", "ਪ੍ਰਿੰਸੀਪਲ"),
            (SpeechLocale::Mr, "मा. देशमुख", "माननीय"),
            (SpeechLocale::Mr, "सौ. देशमुख", "सौभाग्यवती"),
            (SpeechLocale::Ru, "г-н Иванов", "господин"),
            (SpeechLocale::Tr, "Prof. Yılmaz", "Profesör"),
            (SpeechLocale::Vi, "TS Nguyễn", "Tiến sĩ"),
        ];
        for (locale, source, spoken) in cases {
            let source = format!("🙂 «{source}»");
            let matches = found(&source, locale, AbbreviationPolicy::Contextual);
            assert_eq!(matches.len(), 1, "{source}");
            let matched = matches[0];
            assert_eq!(matched.start, "🙂 «".len());
            assert_eq!(&source[matched.start..matched.end], matched.entry.written);
            assert_eq!(matched.entry.spoken, spoken);
        }
        for (locale, source) in [
            (SpeechLocale::Pa, "ਡਾ. ਸਿੰਘ"),
            (SpeechLocale::Bn, "ডাঃ রহমান"),
            (SpeechLocale::Ru, "г-ну Иванову"),
            (SpeechLocale::Ru, "г-жами Ивановыми"),
            (SpeechLocale::Hi, "डॉ.शर्मा"),
            (SpeechLocale::Vi, "TS. Nguyễn"),
        ] {
            assert!(
                found(source, locale, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
    }

    #[test]
    fn references_require_complete_native_numbers_and_explicit_bibliographic_policy() {
        for source in ["Nr. ５", "Nr. ५", "Nr. 12", "Nr. «１２»"] {
            assert_eq!(
                found(source, SpeechLocale::De, AbbreviationPolicy::Contextual).len(),
                1,
                "{source}"
            );
        }
        for source in ["Nr. AB5", "Nr. 5a", "Nr.5", "Nr. 1.5"] {
            assert!(
                found(source, SpeechLocale::De, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
        assert_eq!(
            found("அத். ௧௨", SpeechLocale::Ta, AbbreviationPolicy::Contextual).len(),
            1
        );
        assert!(found("அத். பெயர்", SpeechLocale::Ta, AbbreviationPolicy::Contextual).is_empty());
        for source in ["ص ۱۲", "ص 12", "ص «۱۲»"] {
            assert!(found(source, SpeechLocale::Ur, AbbreviationPolicy::Contextual).is_empty());
            let matches = found(source, SpeechLocale::Ur, AbbreviationPolicy::Bibliographic);
            assert_eq!(matches.len(), 1, "{source}");
            assert_eq!(matches[0].entry.spoken, "صفحہ نمبر");
        }
        for source in ["ص", "ص۱۲", "ص. ۱۲", "ص نام", "ص ۱۲abc"] {
            assert!(
                found(source, SpeechLocale::Ur, AbbreviationPolicy::Bibliographic).is_empty(),
                "{source}"
            );
        }
        assert!(
            found(
                "د.ت. القاهرة",
                SpeechLocale::Ar,
                AbbreviationPolicy::Contextual
            )
            .is_empty()
        );
        assert_eq!(
            found(
                "د.ت. القاهرة",
                SpeechLocale::Ar,
                AbbreviationPolicy::Bibliographic
            )
            .len(),
            1
        );
        for source in ["د.ت.", "د.م.", "كتاب (د.ت.)", "كتاب (د.م.)"] {
            assert!(found(source, SpeechLocale::Ar, AbbreviationPolicy::Contextual).is_empty());
            assert_eq!(
                found(source, SpeechLocale::Ar, AbbreviationPolicy::Bibliographic).len(),
                1,
                "{source}"
            );
        }
    }

    #[test]
    fn company_names_are_the_only_adjacent_letter_context() {
        for source in ["(株)任天堂", "任天堂(株)", "(株) 任天堂", "任天堂 (株)"] {
            let matches = found(source, SpeechLocale::Ja, AbbreviationPolicy::Contextual);
            assert_eq!(matches.len(), 1, "{source}");
            assert_eq!(&source[matches[0].start..matches[0].end], "(株)");
            assert_eq!(matches[0].entry.spoken, "株式会社");
        }
        assert_eq!(
            found("(주)삼성", SpeechLocale::Ko, AbbreviationPolicy::Contextual).len(),
            1
        );
        assert_eq!(
            found("삼성(주)", SpeechLocale::Ko, AbbreviationPolicy::Contextual).len(),
            1
        );
        for source in [
            "(株)",
            "(株)3",
            "3(株)",
            "㈱任天堂",
            "（株）任天堂",
            "x_(株)任天堂",
            "任天堂(株)42",
            "Smith_12 (株)",
            "https://example.org/任天堂(株)",
            "/任天堂(株)",
            "`任天堂(株)`",
            "任天堂(株)@example.org",
            "任天堂(株).txt",
        ] {
            assert!(
                found(source, SpeechLocale::Ja, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
    }

    #[test]
    fn protected_fields_and_preserve_policy_never_expand() {
        for source in [
            "`Dr. Smith`",
            "```Dr. Smith```",
            "<Dr. Smith>",
            "https://example.org/Dr. Smith",
            "www.example.org/Dr. Smith",
            "a@Dr. Smith",
            "/Dr. Smith",
            "dir/Dr. Smith",
            "./Dr. Smith",
            r"C:\Dr. Smith",
            "#Dr. Smith",
            r#"{"title":"Dr. Smith"}"#,
            r#"["Dr. Smith"]"#,
        ] {
            assert!(
                found(source, SpeechLocale::En, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
        for locale in SpeechLocale::ALL {
            let sample = entries()
                .iter()
                .find(|entry| entry.locale == locale.code())
                .map(|entry| format!("{} Smith 12", entry.written))
                .unwrap_or_default();
            let matcher = Matcher::new(&sample, &[], locale, AbbreviationPolicy::Preserve);
            assert!(matcher.matches().is_empty());
            assert!(matcher.match_at(0).is_none());
            for entry in entries()
                .iter()
                .filter(|entry| entry.locale == locale.code())
            {
                let protected = format!("`{} Smith 12`", entry.written);
                assert!(
                    found(&protected, locale, AbbreviationPolicy::Bibliographic).is_empty(),
                    "{protected}"
                );
            }
        }
        let matcher = Matcher::new(
            "🙂 Dr. Smith",
            &[],
            SpeechLocale::En,
            AbbreviationPolicy::Contextual,
        );
        assert!(matcher.match_at(1).is_none());
        assert!(matcher.match_at(usize::MAX).is_none());
    }

    #[test]
    fn continuation_and_multiple_matches_are_bounded_and_disjoint() {
        for source in ["etc. apples", "etc., apples", "etc. ５", "etc. ‘apples’"] {
            assert_eq!(
                found(source, SpeechLocale::En, AbbreviationPolicy::Contextual).len(),
                1,
                "{source}"
            );
        }
        for source in [
            "etc.",
            "etc. Apples",
            "etc.\napples",
            "etc... apples",
            "etc.!” apples",
        ] {
            assert!(
                found(source, SpeechLocale::En, AbbreviationPolicy::Contextual).is_empty(),
                "{source}"
            );
        }
        let source = "Dr. Smith, Mr. Jones, etc. apples";
        let matches = found(source, SpeechLocale::En, AbbreviationPolicy::Contextual);
        assert_eq!(matches.len(), 3);
        assert!(matches.windows(2).all(|pair| pair[0].end <= pair[1].start));
        assert!(
            found(
                &format!("{}(株)", "名".repeat(1400)),
                SpeechLocale::Ja,
                AbbreviationPolicy::Contextual
            )
            .is_empty()
        );
    }
}
