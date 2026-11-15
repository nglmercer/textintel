//! Unicode-aware lexical scanning with byte offsets.
//!
//! The text is split into whitespace-delimited *fields*. Each field has a
//! *core* with leading opening punctuation (`(`, `¿`, `"`, ...) and trailing
//! closing/sentence punctuation (`.`, `,`, `)`, ...) removed. Byte offsets
//! always fall on char boundaries because they come from `char_indices`.
//!
//! *Protected regions* (inline/fenced code in backticks and `<tags>`) may
//! span whitespace; every field that overlaps one is marked protected and is
//! never classified. Field-level identifier protection (URLs, emails,
//! handles, paths, versions, IPs, ...) is applied by
//! [`is_protected_identifier`] and by the strictness of the numeric
//! matchers: a core is only rewritten when it matches a pattern in full.

use std::{collections::HashMap, ops::Range};

/// Immutable source with one shared protected-region scan. Both classification
/// adapters borrow these ranges, so neither can reinterpret code/tag contents.
pub(crate) struct LexicalSource<'a> {
    text: &'a str,
    regions: Vec<Range<usize>>,
}

impl<'a> LexicalSource<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            regions: protected_regions(text),
        }
    }

    pub(crate) fn text(&self) -> &'a str {
        self.text
    }

    pub(crate) fn regions(&self) -> &[Range<usize>] {
        &self.regions
    }

    pub(crate) fn fields(&self) -> Vec<Field> {
        scan_fields(self.text, &self.regions)
    }
}

/// One whitespace-delimited field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub start: usize,
    pub end: usize,
    pub core_start: usize,
    pub core_end: usize,
    pub protected: bool,
}

impl Field {
    /// `true` when the field has no leading punctuation.
    pub(crate) fn bare_start(&self) -> bool {
        self.core_start == self.start
    }

    /// `true` when the field has no trailing punctuation.
    pub(crate) fn bare_end(&self) -> bool {
        self.core_end == self.end
    }

    /// `true` when the core is non-empty.
    pub(crate) fn has_core(&self) -> bool {
        self.core_end > self.core_start
    }
}

/// Maximum bytes of a `<tag>`.
const MAX_TAG_SPAN: usize = 256;

fn is_leading_punct(c: char) -> bool {
    matches!(
        c,
        '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '«' | '¿' | '¡' | '„' | '‹'
    )
}

fn is_trailing_punct(c: char) -> bool {
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
    )
}

/// Byte ranges of backtick code spans and `<tag>`s.
pub(crate) fn protected_regions(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut regions = Vec::new();
    // Index runs once so unmatched delimiters cannot cause quadratic rescans.
    let mut runs = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'`' {
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] == b'`' {
                cursor += 1;
            }
            runs.push((start, cursor - start));
        } else {
            cursor += 1;
        }
    }
    let mut next_by_length = HashMap::new();
    let mut closing = HashMap::new();
    for &(start, len) in runs.iter().rev() {
        if let Some(&next) = next_by_length.get(&len) {
            closing.insert(start, next + len);
        }
        next_by_length.insert(len, start);
    }
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
                let search_from = i + run;
                match closing.get(&i).copied() {
                    Some(end) => {
                        regions.push(i..end);
                        i = end;
                    }
                    None => i = search_from,
                }
            }
            b'<' => {
                let next = bytes.get(i + 1).copied();
                let tag_like =
                    next.is_some_and(|b| b.is_ascii_alphabetic() || b == b'/' || b == b'!');
                let mut end = None;
                if tag_like {
                    for (offset, b) in bytes[i + 1..].iter().enumerate().take(MAX_TAG_SPAN) {
                        match b {
                            b'>' => {
                                end = Some(i + 1 + offset + 1);
                                break;
                            }
                            b'\n' | b'<' => break,
                            _ => {}
                        }
                    }
                }
                match end {
                    Some(end) => {
                        regions.push(i..end);
                        i = end;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    regions
}

/// `true` when `pos` lies inside one of `regions` (sorted, disjoint).
pub(crate) fn in_regions(regions: &[Range<usize>], pos: usize) -> bool {
    let idx = regions.partition_point(|r| r.end <= pos);
    regions
        .get(idx)
        .is_some_and(|r| r.start <= pos && pos < r.end)
}

fn overlaps(regions: &[Range<usize>], start: usize, end: usize) -> bool {
    let idx = regions.partition_point(|r| r.end <= start);
    regions.get(idx).is_some_and(|r| r.start < end)
}

/// Split `text` into whitespace-delimited fields.
pub(crate) fn scan_fields(text: &str, regions: &[Range<usize>]) -> Vec<Field> {
    let mut fields = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                fields.push(make_field(text, s, i, regions));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        fields.push(make_field(text, s, text.len(), regions));
    }
    fields
}

fn make_field(text: &str, start: usize, end: usize, regions: &[Range<usize>]) -> Field {
    let raw = &text[start..end];
    let mut core_start = start;
    for (i, c) in raw.char_indices() {
        if is_leading_punct(c) {
            core_start = start + i + c.len_utf8();
        } else {
            break;
        }
    }
    let mut core_end = end;
    for (i, c) in text[core_start..end].char_indices().rev() {
        if is_trailing_punct(c) {
            core_end = core_start + i;
        } else {
            break;
        }
    }
    Field {
        start,
        end,
        core_start,
        core_end: core_end.max(core_start),
        protected: overlaps(regions, start, end),
    }
}

/// `true` for identifier-shaped cores that must never be reinterpreted:
/// URLs, emails, @handles, #tags, paths, `key=value` pairs, `::` (IPv6 or
/// code paths), versions such as `v1.2.3`, and underscores.
pub(crate) fn is_protected_identifier(core: &str) -> bool {
    if core.contains("://")
        || core.starts_with("www.")
        || core.contains('@')
        || core.starts_with('#')
        || core.contains('\\')
        || core.contains('_')
        || core.contains('=')
        || core.contains('?')
        || core.contains('&')
        || core.contains("::")
        || core.starts_with('/')
        || core.starts_with("./")
        || core.starts_with("~/")
    {
        return true;
    }
    let mut chars = core.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('v' | 'V'), Some(d)) if d.is_ascii_digit()
    )
}

/// Preserve explicit path prefixes as one field before numeric classification.
/// Ordinary slash dates/fractions do not have any of these path prefixes.
pub(crate) fn protected_path_end(text: &str, start: usize) -> Option<usize> {
    let rest = text.get(start..)?;
    let mut chars = rest.chars();
    let drive = matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('/' | '\\')) if letter.is_ascii_alphabetic()
    );
    if !drive
        && !rest.starts_with(['/', '\\'])
        && !rest.starts_with("./")
        && !rest.starts_with("~/")
    {
        return None;
    }
    let end = rest
        .char_indices()
        .find(|(_, ch)| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '<' | '>' | '"' | '\'' | '。' | '、' | '，' | '；' | '！' | '？'
                )
        })
        .map_or(text.len(), |(offset, _)| start + offset);
    let core = text[start..end].trim_end_matches(is_trailing_punct);
    (!core.is_empty()).then_some(start + core.len())
}

/// End of a bounded common filename at an original UTF-8 byte boundary.
///
/// This deliberately recognizes a closed extension vocabulary rather than
/// treating every dotted word as a file (`1.Jan` remains a named-date candidate).
/// It does not replace the existing URL/email/mention/path protections, and
/// closing or sentence punctuation is excluded from the returned filename.
pub(crate) fn filename_end(text: &str, start: usize) -> Option<usize> {
    const MAX_FILENAME_CHARS: usize = 256;
    const EXTENSIONS: &[&str] = &["txt", "json", "wav", "mp3", "csv", "pdf"];
    let rest = text.get(start..)?;
    if !super::core::left_boundary(text, start) {
        return None;
    }
    let mut end = start;
    let mut candidate = None;
    let mut basename_has_letter_or_digit = false;
    for (count, (offset, ch)) in rest.char_indices().enumerate() {
        let at = start + offset;
        if candidate == Some(at) && super::core::is_cjk(ch) {
            break;
        }
        if ch.is_whitespace()
            || is_leading_punct(ch)
            || (is_trailing_punct(ch) && !matches!(ch, '.' | ':'))
            || matches!(ch, '。' | '、' | '，' | '；' | '！' | '？' | '<' | '>')
        {
            break;
        }
        // URLs, Windows paths, email and handles keep their existing classes.
        // Relative POSIX paths may match when their final component is a
        // recognized filename, preventing a numeric directory becoming a date.
        if matches!(ch, '\\' | '@' | '#' | '?' | '&' | '=') {
            return None;
        }
        if !(ch.is_alphanumeric()
            || crate::core::unicode::is_combining_mark(ch)
            || matches!(ch, '_' | '-' | '−' | '+' | '.' | ':' | '/'))
        {
            break;
        }
        if count == MAX_FILENAME_CHARS {
            return None;
        }
        end = at + ch.len_utf8();
        if ch == '.' && basename_has_letter_or_digit {
            for extension in EXTENSIONS {
                let extension_end = end.checked_add(extension.len())?;
                if text
                    .get(end..extension_end)
                    .is_some_and(|s| s.eq_ignore_ascii_case(extension))
                    && super::core::right_boundary(text, extension_end)
                {
                    candidate = Some(extension_end);
                }
            }
        }
        basename_has_letter_or_digit |= ch.is_alphanumeric();
    }
    while end > start && text[..end].ends_with(['.', ':']) {
        end -= 1;
    }
    let end = candidate.filter(|candidate| *candidate == end)?;
    (!is_protected_identifier(&text[start..end])).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_strip_punctuation() {
        let text = "¿Cuesta (S/ 150,50)? Sí.";
        let fields = LexicalSource::new(text).fields();
        let cores: Vec<&str> = fields
            .iter()
            .map(|f| &text[f.core_start..f.core_end])
            .collect();
        assert_eq!(cores, vec!["Cuesta", "S/", "150,50", "Sí"]);
        assert!(!fields[1].bare_start());
        assert!(!fields[2].bare_end());
    }

    #[test]
    fn protected_code_and_tags() {
        let text = "run `pip 1.2 3` then <b class=\"x 4\">5</b> and ``a`b``";
        let regions = protected_regions(text);
        let spans: Vec<&str> = regions.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(
            spans,
            vec!["`pip 1.2 3`", "<b class=\"x 4\">", "</b>", "``a`b``"]
        );
        let fields = LexicalSource::new(text).fields();
        assert!(fields.iter().any(|f| f.protected));
        // Unclosed backtick and lone '<' are not regions.
        assert!(protected_regions("a ` b < 3").is_empty());
    }

    #[test]
    fn lexical_source_preserves_one_protection_map_across_field_and_typed_adapters() {
        let text = "🙂 «12» `3 ٤` <b data-x=\"5 ٦\">7</b> ``8`9`` 十";
        let source = LexicalSource::new(text);
        let expected = ["`3 ٤`", "<b data-x=\"5 ٦\">", "</b>", "``8`9``"];
        assert_eq!(source.text(), text);
        assert_eq!(
            source
                .regions()
                .iter()
                .map(|r| &text[r.clone()])
                .collect::<Vec<_>>(),
            expected
        );
        let fields = source.fields();
        assert!(
            fields
                .iter()
                .any(|f| &text[f.core_start..f.core_end] == "12" && !f.protected)
        );
        assert!(
            fields
                .iter()
                .any(|f| &text[f.start..f.end] == "`3" && f.protected)
        );
        // A whitespace field can overlap tags and the intervening numeral;
        // the typed adapter still exposes the numeral outside those tags.
        assert!(
            fields
                .iter()
                .any(|f| text[f.start..f.end].contains("7</b>") && f.protected)
        );
        for locale in super::super::SpeechLocale::ALL {
            let options = super::super::SpeechOptions::new(locale);
            let parsed = super::super::parser::parse_lexical_source(&source, &options).unwrap();
            let protected = parsed
                .spans
                .iter()
                .filter_map(|span| {
                    matches!(span.value, super::super::SpeechValue::Identifier { .. })
                        .then_some(span.source.as_str())
                })
                .collect::<Vec<_>>();
            assert_eq!(protected, expected, "{}", locale.code());
            let numbers = parsed
                .spans
                .iter()
                .filter_map(|span| {
                    if let super::super::SpeechValue::Number { value, .. } = &span.value {
                        Some((span.source.as_str(), value.integer.as_str()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(numbers, [("12", "12"), ("7", "7")], "{}", locale.code());
            assert_eq!(
                parsed
                    .spans
                    .iter()
                    .map(|span| span.source.as_str())
                    .collect::<String>(),
                text
            );
            for span in &parsed.spans {
                assert_eq!(text.get(span.start..span.end), Some(span.source.as_str()));
            }
        }
    }

    #[test]
    fn identifiers() {
        for core in [
            "https://example.org/a?b=3.14",
            "user@example.org",
            "@handle",
            "#tag",
            "C:\\x\\y",
            "/usr/bin",
            "v1.2.3-beta.2",
            "2001:db8::1",
            "snake_case_1",
        ] {
            assert!(is_protected_identifier(core), "{core}");
        }
        for core in ["3.14", "12,5%", "S/", "1st", "vino"] {
            assert!(!is_protected_identifier(core), "{core}");
        }
    }

    #[test]
    fn all_punctuation_field_has_empty_core() {
        let fields = LexicalSource::new("... ¿?").fields();
        assert!(fields.iter().all(|f| !f.has_core()));
    }

    #[test]
    fn common_filenames_are_bounded_and_keep_existing_identifier_classes() {
        assert_eq!(filename_end("3kg.txt.", 0), Some("3kg.txt".len()));
        assert_eq!(filename_end("3kg.txt:", 0), Some("3kg.txt".len()));
        assert_eq!(
            filename_end("2024-03-12.JSON，", 0),
            Some("2024-03-12.JSON".len())
        );
        assert_eq!(
            filename_end("記録3kg.wavの結果", 0),
            Some("記録3kg.wav".len())
        );
        assert_eq!(filename_end("12:30.mp3", 0), Some("12:30.mp3".len()));
        for source in ["a/3kg.txt", "2024/03/12.csv"] {
            assert_eq!(filename_end(source, 0), Some(source.len()), "{source}");
        }
        for source in [
            "1.Jan.2024",
            "1.enero.2024",
            "3kg.",
            "3kg.txtx",
            "https://3kg.txt",
            "user@3kg.txt",
            "@3kg.txt",
            "#3kg.txt",
            "./3kg.txt",
            "3kg.txt/a",
            "C:\\3kg.txt",
            "v1.2.3.txt",
            "foo_3kg.txt",
        ] {
            assert_eq!(filename_end(source, 0), None, "{source}");
        }
        assert_eq!(filename_end("🙂3kg.txt", 1), None);
        assert_eq!(filename_end("3kg.txt", usize::MAX), None);
        assert_eq!(filename_end(&format!("{}.txt", "a".repeat(257)), 0), None);
    }
}
