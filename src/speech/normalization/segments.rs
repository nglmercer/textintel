//! Protected sentence boundaries and grapheme-safe hard splitting.
//!
//! A `.`, `?`, `!`, `;` or `…` ends a sentence only when the terminator run
//! (plus closing quotes/brackets) is followed by whitespace or the end of
//! the text, and a single `.` is not a known abbreviation or an initial.
//! Therefore decimals (`3.14`), IPs (`10.0.0.1:8080`), versions (`v1.2.3`)
//! and URLs (`https://example.org/a?b=3.14`) never split. `。 ？ ！ ；`,
//! Arabic `؟`, Urdu `۔`, Indic `। ॥` and newlines also end a sentence
//! without requiring following whitespace. Adjacent terminators and closing
//! punctuation stay together; a run never crosses a newline or protected
//! region. Terminators inside backtick code or `<tags>` are ignored.
//!
//! The added native signs have Sentence_Break=STerm in the pinned Unicode
//! 17.0.0 data: <https://www.unicode.org/Public/17.0.0/ucd/auxiliary/SentenceBreakProperty.txt>.
//! Grouping terminators/closers follows the principle of UAX #29 revision 47,
//! SB8a/SB9: <https://www.unicode.org/reports/tr29/tr29-47.html#Sentence_Boundaries>.
//! This is a bounded, tailored heuristic, not a complete UAX #29 implementation;
//! the existing ASCII abbreviation and whitespace policies remain in place.

use std::ops::Range;

use unicode_normalization::char::is_combining_mark;

use crate::speech::{
    AbbreviationPolicy, SpeechOptions,
    abbreviations::{Matcher, legacy_is_non_terminal},
};

use super::lexer::{LexicalSource, in_regions};

fn is_closer(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '”' | '’' | '»' | '›' | ')' | ']' | '}' | '」' | '』'
    )
}

fn is_spaced_terminator(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | ';' | '…')
}

fn is_hard_terminator(c: char) -> bool {
    // Newlines have a separate branch and are never literal URL punctuation.
    matches!(c, '。' | '？' | '！' | '；' | '؟' | '۔' | '।' | '॥')
}

/// Word (letters and inner dots) ending right before byte `dot`.
fn word_before(text: &str, dot: usize) -> &str {
    let head = &text[..dot];
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic() || *c == '.')
        .last()
        .map(|(i, _)| i)
        .unwrap_or(dot);
    &head[start..]
}

fn is_non_terminal(word: &str, legacy: bool) -> bool {
    if legacy && legacy_is_non_terminal(word) {
        return true;
    }
    let mut chars = word.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_uppercase())
}

/// Contiguous byte ranges covering `text`, one per sentence. Each range
/// ends after its terminator and closers; following whitespace starts the
/// next range.
pub fn sentence_spans(text: &str) -> Vec<Range<usize>> {
    sentence_spans_impl(text, None)
}

/// Original UTF-8 byte ranges using locale-aware abbreviation contexts.
///
/// Contextual/bibliographic policies protect periods only inside reviewed,
/// context-matched abbreviations, plus uppercase initials. Preserve retains
/// the legacy global abbreviation boundaries of [`sentence_spans`]. No text
/// is expanded, and code/tags, URL punctuation and newline rules are shared
/// with the legacy path.
pub fn sentence_spans_with_options(text: &str, options: &SpeechOptions) -> Vec<Range<usize>> {
    sentence_spans_impl(
        text,
        (options.abbreviation_policy != AbbreviationPolicy::Preserve).then_some(options),
    )
}

fn sentence_spans_impl(text: &str, options: Option<&SpeechOptions>) -> Vec<Range<usize>> {
    let source = LexicalSource::new(text);
    let regions = source.regions();
    let abbreviation_regions: Vec<_> = options
        .map(|options| {
            Matcher::new(text, regions, options.locale, options.abbreviation_policy)
                .matches()
                .into_iter()
                .map(|matched| matched.start..matched.end)
                .collect()
        })
        .unwrap_or_default();
    // A non-ASCII terminator can also be literal URL-path data. Preserve
    // explicit URL fields instead of introducing a cut inside those tokens.
    // Keep legacy ASCII punctuation handling (including a URL's final dot).
    // A native sign at a URL field's end remains ambiguous URL data.
    let url_regions: Vec<_> = source
        .fields()
        .into_iter()
        .filter(|field| {
            let core = &text[field.core_start..field.core_end];
            !field.protected
                && (core.contains("://")
                    || core
                        .get(..4)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www.")))
        })
        .map(|field| field.core_start..field.core_end)
        .collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte_after = |k: usize| chars.get(k + 1).map(|(i, _)| *i).unwrap_or(text.len());
    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut k = 0usize;
    while k < chars.len() {
        let (i, c) = chars[k];
        if in_regions(regions, i)
            || (c == '.' && in_regions(&abbreviation_regions, i))
            || (is_hard_terminator(c) && in_regions(&url_regions, i))
        {
            k += 1;
            continue;
        }
        // A newline is its own boundary, never part of a punctuation run.
        if c == '\n' {
            let mut e = k;
            while e + 1 < chars.len() && is_closer(chars[e + 1].1) {
                e += 1;
            }
            let end = byte_after(e);
            spans.push(start..end);
            start = end;
            k = e + 1;
            continue;
        }
        if is_spaced_terminator(c) || is_hard_terminator(c) {
            let mut e = k;
            let mut hard = is_hard_terminator(c);
            let mut terminators = 1;
            while let Some(&(next_byte, next)) = chars.get(e + 1) {
                if next == '\n'
                    || in_regions(regions, next_byte)
                    || (next == '.' && in_regions(&abbreviation_regions, next_byte))
                    || (is_hard_terminator(next) && in_regions(&url_regions, next_byte))
                {
                    break;
                }
                if is_spaced_terminator(next) || is_hard_terminator(next) {
                    hard |= is_hard_terminator(next);
                    terminators += 1;
                } else if !is_closer(next) {
                    break;
                }
                e += 1;
            }
            let followed_by_space = e + 1 >= chars.len() || chars[e + 1].1.is_whitespace();
            let abbreviation = c == '.'
                && terminators == 1
                && is_non_terminal(word_before(text, i), options.is_none());
            if (hard || followed_by_space) && !abbreviation {
                let end = byte_after(e);
                spans.push(start..end);
                start = end;
            }
            k = e + 1;
            continue;
        }
        k += 1;
    }
    if start < text.len() {
        spans.push(start..text.len());
    }
    spans
}

fn is_cluster_extender(c: char) -> bool {
    is_combining_mark(c)
        || matches!(
            c,
            '\u{200D}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{1F3FB}'..='\u{1F3FF}'
                | '\u{E0020}'..='\u{E007F}'
                | '\u{E0100}'..='\u{E01EF}'
        )
}

fn is_regional_indicator(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}

/// Approximate extended grapheme clusters: combining marks, variation
/// selectors, skin-tone modifiers, tag characters and ZWJ sequences stay
/// attached to their base; regional indicators pair into flags.
pub fn clusters(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut prev: Option<char> = None;
    let mut ri_in_cluster = 0usize;
    for (i, c) in text.char_indices() {
        let joins = match prev {
            None => false,
            Some(p) => {
                is_cluster_extender(c)
                    || p == '\u{200D}'
                    || (is_regional_indicator(c) && ri_in_cluster == 1 && is_regional_indicator(p))
            }
        };
        if !joins && i > start {
            out.push(&text[start..i]);
            start = i;
            ri_in_cluster = 0;
        }
        if is_regional_indicator(c) {
            ri_in_cluster += 1;
        }
        prev = Some(c);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Split `text` into pieces of at most `max_chars` chars, cutting only
/// between clusters. A single cluster longer than `max_chars` is split by
/// chars as a last resort.
pub fn hard_split(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for cluster in clusters(text) {
        let len = cluster.chars().count();
        if len > max_chars {
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
                current_len = 0;
            }
            let chars: Vec<char> = cluster.chars().collect();
            pieces.extend(
                chars
                    .chunks(max_chars)
                    .map(|c| c.iter().collect::<String>()),
            );
            continue;
        }
        if current_len + len > max_chars {
            pieces.push(std::mem::take(&mut current));
            current_len = 0;
        }
        current.push_str(cluster);
        current_len += len;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::SpeechLocale;

    fn sentences(text: &str) -> Vec<&str> {
        sentence_spans(text)
            .into_iter()
            .map(|r| text[r].trim())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn contextual_sentences<'a>(text: &'a str, options: &SpeechOptions) -> Vec<&'a str> {
        sentence_spans_with_options(text, options)
            .into_iter()
            .map(|range| text[range].trim())
            .filter(|sentence| !sentence.is_empty())
            .collect()
    }

    #[test]
    fn spans_cover_text() {
        for text in ["", "a", "Hola. Adiós.", "x\n\ny", "Uno… dos? tres!"] {
            let spans = sentence_spans(text);
            let mut cursor = 0;
            for span in &spans {
                assert_eq!(span.start, cursor);
                cursor = span.end;
            }
            assert_eq!(cursor, text.len());
        }
    }

    #[test]
    fn protected_tokens_do_not_split() {
        for token in [
            "3.14",
            "10.0.0.1:8080",
            "https://example.org/a?b=3.14",
            "https://example.org/سؤال؟x",
            "www.example.org/سؤال؟x",
            "WWW.example.org/سؤال؟x",
            "Www.example.org/سؤال؟x",
            "https://example.org/甲。x",
            "https://example.org/甲？x",
            "https://example.org/甲！x",
            "WWW.example.org/甲；x",
            "https://example.org/اردو۔x",
            "https://example.org/दंड।x",
            "www.example.org/दंड॥x",
            "v1.2.3-beta.2",
            "Minecraft 1.21.11",
            "user@example.org",
            "1.234,56",
        ] {
            let text = format!("Valor {token} aquí. Fin.");
            let got = sentences(&text);
            assert_eq!(
                got,
                vec![format!("Valor {token} aquí."), "Fin.".to_string()]
            );
        }
    }

    #[test]
    fn abbreviations_and_initials() {
        assert_eq!(
            sentences("Vino el Dr. García. Luego J. K. Rowling habló."),
            vec!["Vino el Dr. García.", "Luego J. K. Rowling habló."]
        );
        assert_eq!(sentences("Fin etc. Otro."), vec!["Fin etc.", "Otro."]);
    }

    #[test]
    fn contextual_titles_and_number_labels_protect_their_periods() {
        let english = SpeechOptions::new(SpeechLocale::En);
        let german = SpeechOptions::new(SpeechLocale::De);
        assert_eq!(
            contextual_sentences("Dr. Smith arrived. Mr. Jones left.", &english),
            vec!["Dr. Smith arrived.", "Mr. Jones left."]
        );
        for digit in ["5", "５", "५", "٥"] {
            let text = format!("Nr. {digit} ist da. Weiter.");
            let first = format!("Nr. {digit} ist da.");
            assert_eq!(
                contextual_sentences(&text, &german),
                vec![first, "Weiter.".to_owned()],
                "{text}"
            );
        }
        assert_eq!(
            contextual_sentences("Dr. smith arrived. End.", &english),
            vec!["Dr.", "smith arrived.", "End."]
        );
        assert_eq!(
            contextual_sentences("Nr. AB5 ist da. Weiter.", &german),
            vec!["Nr.", "AB5 ist da.", "Weiter."]
        );
        assert_eq!(
            contextual_sentences("nr. 5 ist da. Weiter.", &german),
            vec!["nr.", "5 ist da.", "Weiter."]
        );
    }

    #[test]
    fn preserve_retains_legacy_boundaries_and_initials_remain_shared() {
        for locale in SpeechLocale::ALL {
            let mut options = SpeechOptions::new(locale);
            options.abbreviation_policy = AbbreviationPolicy::Preserve;
            for text in [
                "Dr. smith arrived. End.",
                "Nr. ５ ist da. Weiter.",
                "Prof. X. Name. Next.",
                "Ver e.g. esto. Fin.",
                "أ؟!\nب۔",
                "www.example.org/甲？x aquí. Fin.",
                "run `Dr. Name.\nNr. 5` now. Done.",
            ] {
                assert_eq!(
                    sentence_spans_with_options(text, &options),
                    sentence_spans(text),
                    "{locale:?}: {text}"
                );
            }
        }
        let options = SpeechOptions::new(SpeechLocale::En);
        assert_eq!(
            contextual_sentences("J. K. Rowling spoke. Next.", &options),
            vec!["J. K. Rowling spoke.", "Next."]
        );
    }

    #[test]
    fn other_terminators() {
        assert_eq!(
            sentences("¿Qué? ¡Sí! Bien; ok"),
            vec!["¿Qué?", "¡Sí!", "Bien;", "ok"]
        );
        assert_eq!(sentences("日本語。テスト！"), vec!["日本語。", "テスト！"]);
        assert_eq!(
            sentences("Dijo \"hola.\" Luego"),
            vec!["Dijo \"hola.\"", "Luego"]
        );
        assert_eq!(sentences("Espera... ya"), vec!["Espera...", "ya"]);
        assert_eq!(sentences("a\nb"), vec!["a", "b"]);
        assert_eq!(
            sentences("run `a. b` now. Done"),
            vec!["run `a. b` now.", "Done"]
        );
    }

    fn assert_partition(text: &str) {
        assert_ranges_partition(text, sentence_spans(text));
    }

    fn assert_ranges_partition(text: &str, spans: Vec<Range<usize>>) {
        let mut cursor = 0;
        for span in &spans {
            assert_eq!(span.start, cursor, "{text:?}: {spans:?}");
            assert!(span.end > span.start);
            assert!(text.is_char_boundary(span.start));
            assert!(text.is_char_boundary(span.end));
            cursor = span.end;
        }
        assert_eq!(cursor, text.len());
        let reconstructed: String = spans.iter().map(|span| &text[span.clone()]).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn contextual_ranges_keep_utf8_protections_and_newline_boundaries() {
        let options = SpeechOptions::new(SpeechLocale::De);
        for text in [
            "🙂 Nr. ５ ist da. Weiter.",
            "Nr.\n５ ist da. Weiter.",
            "`Nr. ５ ist da.\nDr. Name` folgt. Weiter.",
            "<x title='Nr. ５'>Nr. ５ ist da. Weiter.",
            "WWW.example.org/甲？x Nr. ５ ist da. Weiter.",
            "https://example.org/Nr./५ Nr. ５ ist da. Weiter.",
        ] {
            assert_ranges_partition(text, sentence_spans_with_options(text, &options));
        }
        assert_eq!(
            contextual_sentences("Nr.\n５ ist da. Weiter.", &options),
            vec!["Nr.", "５ ist da.", "Weiter."]
        );
        assert_eq!(
            contextual_sentences("🙂 Nr. ５ ist da. Weiter.", &options),
            vec!["🙂 Nr. ５ ist da.", "Weiter."]
        );
        assert_eq!(
            contextual_sentences("`Nr. ５ ist da.` folgt. Weiter.", &options),
            vec!["`Nr. ５ ist da.` folgt.", "Weiter."]
        );
    }

    #[test]
    fn native_titles_and_company_aliases_keep_original_text_boundaries() {
        let hindi = SpeechOptions::new(SpeechLocale::Hi);
        let japanese = SpeechOptions::new(SpeechLocale::Ja);
        assert_eq!(
            contextual_sentences("डॉ. शर्मा आए।अगला वाक्य।", &hindi),
            vec!["डॉ. शर्मा आए।", "अगला वाक्य।"]
        );
        assert_eq!(
            contextual_sentences("प्रो. शर्मा आए।अगला वाक्य।", &hindi),
            vec!["प्रो. शर्मा आए।", "अगला वाक्य।"]
        );
        assert_eq!(
            contextual_sentences("डॉ. ५ आए।अगला वाक्य।", &hindi),
            vec!["डॉ.", "५ आए।", "अगला वाक्य।"]
        );
        for (text, expected) in [
            ("(株)トヨタ。次です。", vec!["(株)トヨタ。", "次です。"]),
            ("(有)会社。次です。", vec!["(有)会社。", "次です。"]),
        ] {
            assert_eq!(contextual_sentences(text, &japanese), expected);
            assert_ranges_partition(text, sentence_spans_with_options(text, &japanese));
        }
        assert_ranges_partition(
            "🙂 डॉ. शर्मा आए।अगला वाक्य।",
            sentence_spans_with_options("🙂 डॉ. शर्मा आए।अगला वाक्य।", &hindi),
        );
    }

    #[test]
    fn bibliographic_opt_in_protects_internal_and_final_dots_only() {
        let mut arabic = SpeechOptions::new(SpeechLocale::Ar);
        let text = "د.ت. ٢٠٢٠. التالي.";
        assert_eq!(
            contextual_sentences(text, &arabic),
            vec!["د.ت.", "٢٠٢٠.", "التالي."]
        );
        arabic.abbreviation_policy = AbbreviationPolicy::Bibliographic;
        assert_eq!(
            contextual_sentences(text, &arabic),
            vec!["د.ت. ٢٠٢٠.", "التالي."]
        );
        assert_eq!(
            contextual_sentences("د.م. القاهرة. التالي.", &arabic),
            vec!["د.م. القاهرة.", "التالي."]
        );
        assert_ranges_partition(text, sentence_spans_with_options(text, &arabic));
        let mut urdu = SpeechOptions::new(SpeechLocale::Ur);
        urdu.abbreviation_policy = AbbreviationPolicy::Bibliographic;
        // This alias has no period. Its opt-in must not suppress the native
        // terminator after the referenced number, or consume the next sentence.
        assert_eq!(
            contextual_sentences("ص ۱۲۔اگلا۔", &urdu),
            vec!["ص ۱۲۔", "اگلا۔"]
        );
        assert_ranges_partition("ص ۱۲۔اگلا۔", sentence_spans_with_options("ص ۱۲۔اگلا۔", &urdu));
    }

    #[test]
    fn native_sterm_signs_split_without_whitespace_and_preserve_offsets() {
        // These are segmentation examples, not pronunciation/translation golds.
        for (text, expected) in [
            ("هل وصلت؟نعم وصلت.", vec!["هل وصلت؟", "نعم وصلت."]),
            ("آپ آگئے۔جی آگئے۔", vec!["آپ آگئے۔", "جی آگئے۔"]),
            ("यह ठीक है।अगला वाक्य॥", vec!["यह ठीक है।", "अगला वाक्य॥"]),
            ("এটি ঠিক।পরের বাক্য॥", vec!["এটি ঠিক।", "পরের বাক্য॥"]),
            ("ਪਹਿਲਾ।ਦੂਜਾ॥", vec!["ਪਹਿਲਾ।", "ਦੂਜਾ॥"]),
            ("पहिले।दुसरे॥", vec!["पहिले।", "दुसरे॥"]),
        ] {
            assert_eq!(sentences(text), expected, "{text}");
            assert_partition(text);
        }
    }

    #[test]
    fn repeated_mixed_terminators_and_closers_form_one_run() {
        for (text, expected) in [
            ("أ؟!ب۔", vec!["أ؟!", "ب۔"]),
            ("अ।।ब॥", vec!["अ।।", "ब॥"]),
            ("甲？！乙。", vec!["甲？！", "乙。"]),
            ("甲!？乙。", vec!["甲!？", "乙。"]),
            ("أ?؟ب۔", vec!["أ?؟", "ب۔"]),
            ("أ؟»!ب۔", vec!["أ؟»!", "ب۔"]),
            ("甲？！)」乙。", vec!["甲？！)」", "乙。"]),
            ("Wait?! Next...", vec!["Wait?!", "Next..."]),
        ] {
            assert_eq!(sentences(text), expected, "{text}");
            assert_partition(text);
        }
        // ASCII punctuation still needs whitespace or the end of the text.
        assert_eq!(sentences("One?!Two"), vec!["One?!Two"]);
    }

    #[test]
    fn native_runs_stop_before_newlines_and_protected_regions() {
        let text = "أ؟!\nب۔";
        let raw: Vec<_> = sentence_spans(text)
            .into_iter()
            .map(|span| &text[span])
            .collect();
        assert_eq!(raw, vec!["أ؟!", "\n", "ب۔"]);
        for (text, expected) in [
            ("أ؟`x؟!`ب۔", vec!["أ؟", "`x؟!`ب۔"]),
            (
                "<x title='؟!'>نعم۔التالي",
                vec!["<x title='؟!'>نعم۔", "التالي"],
            ),
            (
                "قبل ```أ؟!\nب॥``` بعد۔التالي",
                vec!["قبل ```أ؟!\nب॥``` بعد۔", "التالي"],
            ),
        ] {
            assert_eq!(sentences(text), expected, "{text}");
            assert_partition(text);
        }
    }

    #[test]
    fn common_punctuation_is_script_independent_for_all_speech_locales() {
        let samples = [
            (SpeechLocale::En, "Hello"),
            (SpeechLocale::Es, "Hola"),
            (SpeechLocale::Fr, "Bonjour"),
            (SpeechLocale::De, "Hallo"),
            (SpeechLocale::Pt, "Olá"),
            (SpeechLocale::Ru, "Привет"),
            (SpeechLocale::Zh, "你好"),
            (SpeechLocale::Ja, "こんにちは"),
            (SpeechLocale::Ko, "안녕하세요"),
            (SpeechLocale::Ar, "مرحبا"),
            (SpeechLocale::Hi, "नमस्ते"),
            (SpeechLocale::Bn, "নমস্কার"),
            (SpeechLocale::Ur, "سلام"),
            (SpeechLocale::Pa, "ਸਤਿ"),
            (SpeechLocale::Mr, "नमस्कार"),
            (SpeechLocale::Ta, "வணக்கம்"),
            (SpeechLocale::Te, "నమస్కారం"),
            (SpeechLocale::Id, "Halo"),
            (SpeechLocale::Tr, "Merhaba"),
            (SpeechLocale::Vi, "Xin chào"),
        ];
        for locale in SpeechLocale::ALL {
            let word = samples.iter().find(|(key, _)| *key == locale).unwrap().1;
            let first = format!("{word}?!");
            let second = format!("{word}.");
            let text = format!("{first} {second}");
            assert_eq!(sentences(&text), vec![first, second], "{locale:?}");
            assert_partition(&text);
            let options = SpeechOptions::new(locale);
            assert_eq!(
                contextual_sentences(&text, &options),
                sentences(&text),
                "{locale:?}"
            );
            assert_ranges_partition(&text, sentence_spans_with_options(&text, &options));
        }
    }

    #[test]
    fn clusters_keep_zwj_and_marks() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        assert_eq!(clusters(family), vec![family]);
        assert_eq!(clusters("e\u{301}a"), vec!["e\u{301}", "a"]);
        assert_eq!(clusters("🇵🇪🇪🇸"), vec!["🇵🇪", "🇪🇸"]);
        assert_eq!(clusters("🎙️x"), vec!["🎙️", "x"]);
        assert_eq!(clusters("👍🏽"), vec!["👍🏽"]);
    }

    #[test]
    fn hard_split_never_cuts_clusters() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦"; // 7 chars
        let text = family.repeat(5);
        let pieces = hard_split(&text, 10);
        assert!(pieces.iter().all(|p| p == family));
        assert_eq!(pieces.concat(), text);
        // A cluster longer than the limit falls back to char splitting.
        let pieces = hard_split(family, 3);
        assert!(pieces.iter().all(|p| p.chars().count() <= 3));
        assert_eq!(pieces.concat(), family);
    }
}
