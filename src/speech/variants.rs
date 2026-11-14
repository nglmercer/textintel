//! Conservative, auditable equivalence of spoken renderings.
//!
//! This comparator does not repair a reference or establish linguistic accuracy.
//! It recognizes a small, closed set of presentation and declared reading-policy
//! variants of a supported typed source. Exact matches remain a separate evaluation metric. Unlisted
//! differences, including plausible semantic paraphrases, remain failures.
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use super::expressions::ExpressionValue;
use super::types::{
    NumberNotation, NumberReading, ReadingStyle, SpeechDocument, SpeechInterpretation,
    SpeechLocale, SpeechOptions, SpeechValue,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantMatch {
    /// Stable identifiers into [`rule_catalog`], in application order.
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct VariantRuleInfo {
    pub id: &'static str,
    pub rationale: &'static str,
    pub sources: &'static [&'static str],
}

/// The bounded rules implemented here, including their scope and review sources.
/// Spacing rules describe equivalent spoken tokenization, not approval of every
/// alternative as standard written orthography.
pub fn rule_catalog() -> &'static [VariantRuleInfo] {
    &[
        VariantRuleInfo {
            id: "unicode_nfc",
            rationale: "Unicode canonical equivalence only; compatibility forms and diacritics are retained.",
            sources: &["https://www.unicode.org/reports/tr15/"],
        },
        VariantRuleInfo {
            id: "outer_whitespace",
            rationale: "Leading and trailing layout whitespace does not change a supported utterance; interior prose remains exact.",
            sources: &["https://www.unicode.org/reports/tr44/#White_Space"],
        },
        VariantRuleInfo {
            id: "numeric_hyphen_en_fr",
            rationale: "Within a single typed numeric reading, an inter-letter ASCII hyphen and a space separate the same spoken words. Signs are preserved. French traditional and rectified number hyphenation coexist.",
            sources: &[
                "https://www.academie-francaise.fr/questions-de-langue",
                "https://dictionary.cambridge.org/grammar/british-grammar/commas",
            ],
        },
        VariantRuleInfo {
            id: "english_literal_label_separator",
            rationale: "An explicitly typed literal label with a source hyphen may retain that joiner or use a space in speech orthography. The exact label and complete shared numeric reading, including its sign and fractional zeros, remain unchanged; generic hyphen folding never touches the label.",
            sources: &[
                "https://dictionary.cambridge.org/dictionary/english/hyphen",
                "https://www.w3.org/TR/speech-synthesis/#S3.1.9",
            ],
        },
        VariantRuleInfo {
            id: "spanish_whole_hour_noun",
            rationale: "A typed exact-hour clock may explicitly state the correctly agreed hora/horas noun. Hour value, meridiem, and any explicitly spoken zero-second precision are retained. Nonzero minute/second readings and durations are excluded.",
            sources: &["https://www.rae.es/dpd/hora"],
        },
        VariantRuleInfo {
            id: "french_1980s_loanword",
            rationale: "Larousse attests colloquial French eighties for the 1980s; the Académie discourages the borrowing. Only an explicitly typed 1980 decade permits it, never a century-unspecified 80, another century, or a plain number.",
            sources: &[
                "https://www.larousse.fr/dictionnaires/francais/eighties/28097",
                "https://www.academie-francaise.fr/les-sixties-les-seventies",
            ],
        },
        VariantRuleInfo {
            id: "english_hundred_and",
            rationale: "An optional and after hundred and before a tens/unit word distinguishes established English number conventions. Mixed fractions are excluded.",
            sources: &[
                "https://dictionaryblog.cambridge.org/2016/08/31/one-thousand-one-hundred-and-ninety-three-how-to-say-numbers-1/",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/en.xml",
            ],
        },
        VariantRuleInfo {
            id: "german_numeric_case",
            rationale: "Case of verified numeral words and canonical title-case quantity nouns is a spoken presentation difference; acronyms and ordinary text are excluded.",
            sources: &["https://www.duden.de/sprachwissen/sprachratgeber/Million"],
        },
        VariantRuleInfo {
            id: "german_compound_spacing",
            rationale: "Whitespace may split a verified canonical number compound without changing its letters or spoken components. This does not endorse separated spelling as standard German.",
            sources: &["https://www.duden.de/sprachwissen/sprachratgeber/Million"],
        },
        VariantRuleInfo {
            id: "german_unit_compound_spacing",
            rationale: "Only a typed unit's shared-dictionary compound may be split into the same letters, preserving Quadrat/Kubik and the unit. This is spoken tokenization, not standard separated German spelling.",
            sources: &[
                "https://www.duden.de/rechtschreibung/Quadratmeter",
                "https://www.duden.de/rechtschreibung/Kubikzentimeter",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/de.xml",
            ],
        },
        VariantRuleInfo {
            id: "vietnamese_unit_component_spacing",
            rationale: "Only a typed unit's shared-dictionary word may be split into identical letter sequences (kilôgam/ki lô gam). Diacritics and unit identity stay exact.",
            sources: &[
                "https://moet.gov.vn/content/vanban/Lists/VBDT/Attachments/1559/2.%20Ch%C6%B0%C6%A1ng%20tr%C3%ACnh%20m%C3%B4n%20To%C3%A1n.pdf",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/vi.xml",
            ],
        },
        VariantRuleInfo {
            id: "vietnamese_typed_unit_alias",
            rationale: "A typed unit may use a source-documented spoken alias from its shared metadata. In particular, Ministry-defined cm=xăng-ti-mét coexists with CLDR xentimét. No global spelling or phoneme folding occurs.",
            sources: &[
                "https://moet.gov.vn/content/vanban/Lists/VBDT/Attachments/1559/2.%20Ch%C6%B0%C6%A1ng%20tr%C3%ACnh%20m%C3%B4n%20To%C3%A1n.pdf",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/vi.xml",
            ],
        },
        VariantRuleInfo {
            id: "alternative_integer_digit_policy",
            rationale: "An explicit Number interpretation may choose the existing digit-by-digit renderer for the identical canonical integer. This is a different reading policy, not canonical output accuracy; fractions, padding, identifiers, quantities, and automatic context are excluded.",
            sources: &[
                "https://www.w3.org/TR/speech-synthesis/#S3.1.9",
                "https://www.unicode.org/reports/tr35/tr35-numbers.html",
            ],
        },
        VariantRuleInfo {
            id: "alternative_currency_minor_unit_policy",
            rationale: "A standalone German EUR or Portuguese BRL amount with a nonzero fraction of one or two decimal digits may use the existing Precise decimal or Natural major/minor-unit renderer, provided changing only reading_style reparses to the identical source IR and both renders are supported. This is an alternate reading policy, not canonical accuracy; other currencies, precision, types and surrounding prose are excluded.",
            sources: &[
                "https://www.duden.de/sprachwissen/sprachratgeber/Schreibung-von-Zahlen",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/de.xml",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/pt.xml",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/pt.xml",
                "https://www.rio.rj.gov.br/dlstatic/10112/4539715/4115434/3ANO_1BIM_ALUNO_2014.pdf",
            ],
        },
        VariantRuleInfo {
            id: "chinese_currency_name_spacing",
            rationale: "For one standalone typed Chinese currency amount, layout whitespace between the unchanged numeric reading and that exact ISO code's shared-lexicon currency name may vary. No spacing inside the currency name, qualification, value, sign or currency identity is changed.",
            sources: &[
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/zh.xml",
                "https://www.unicode.org/reports/tr35/tr35-numbers.html",
            ],
        },
        VariantRuleInfo {
            id: "alternative_japanese_number_notation",
            rationale: "For one standalone Auto-classified Japanese Number, accept the complete existing Orthographic render of the identical reparsed IR while preserving every other option. This is an alternate notation policy, not canonical accuracy or general kana/kanji equivalence. Units, dates, counters, labels, identifiers and surrounding prose are excluded.",
            sources: &[
                "https://oercollective.caul.edu.au/japanese/chapter/8-0/",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/ja.xml",
            ],
        },
        VariantRuleInfo {
            id: "russian_clock_conjunction",
            rationale: "The hour/minute quantities of a validated clock may be read with or without connecting и. Seconds and explicit periods are preserved.",
            sources: &[
                "https://slaviccenters.duke.edu/sites/slaviccenters.duke.edu/files/site-images/LeapingIntoRussian-8.pdf",
            ],
        },
        VariantRuleInfo {
            id: "russian_clock_zero_minutes",
            rationale: "A validated exact hour may state its zero minutes explicitly; nonzero minutes and seconds are never dropped.",
            sources: &[
                "https://slaviccenters.duke.edu/sites/slaviccenters.duke.edu/files/site-images/LeapingIntoRussian-8.pdf",
                "https://mezhdunami.ku.edu/unit05/5_4/language.shtml",
            ],
        },
        VariantRuleInfo {
            id: "russian_clock_relative",
            rationale: "For unqualified 1–12-hour clocks without seconds, derive past/half/quarter/to expressions from the exact minute and next hour. No 24-hour or AM/PM distinction is erased.",
            sources: &[
                "https://russian.cornell.edu/grammar/html/le79_86_b_b.htm",
                "https://russian.cornell.edu/grammar/html/le79_86_e.htm",
                "https://russian.cornell.edu/grammar/html/le79_86_d.htm",
                "https://pressbooks.uiowa.edu/russiancareer/chapter/practice-phone-numbers/",
            ],
        },
        VariantRuleInfo {
            id: "russian_usd_qualification",
            rationale: "Only typed ISO USD permits an optional spoken США qualification after the unchanged currency amount; other dollar currencies and abbreviations remain distinct.",
            sources: &[
                "https://gramota.ru/storage/public/normdicts/slovar_inostr_slov.pdf",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/ru.xml",
                "https://gramota.ru/biblioteka/spravochniki/pismovnik/kak-pravilno-upotreblyat-abbreviatury",
            ],
        },
        VariantRuleInfo {
            id: "cjk_numeric_unit_spacing",
            rationale: "Layout whitespace adjacent to known CJK numeral and intact unit symbols may vary within one typed reading. Unit words, names, and signs remain intact.",
            sources: &[
                "https://www.unicode.org/reports/tr35/tr35-numbers.html",
                "https://www.mext.go.jp/component/a_menu/education/micro_detail/__icsFiles/afieldfile/2015/10/06/1235807_022.pdf",
            ],
        },
        VariantRuleInfo {
            id: "korean_number_group_spacing",
            rationale: "Within a typed number, spacing after 만/억/조 before another numeric group changes layout only; arbitrary Hangul or lower-place splitting is excluded.",
            sources: &[
                "https://www.korean.go.kr/front/onlineQna/onlineQnaView.do?mn_id=216&pageIndex=1&qna_seq=305987",
            ],
        },
        VariantRuleInfo {
            id: "korean_quantity_unit_spacing",
            rationale: "Only the boundary between a typed measure's number and its intact shared-dictionary unit may be spaced. Standard written Korean spaces unit nouns; compact TTS tokenization has the same words.",
            sources: &[
                "https://www.korean.go.kr/front/onlineQna/onlineQnaView.do?mn_id=201&pageIndex=1&qna_seq=334776",
            ],
        },
        VariantRuleInfo {
            id: "japanese_initial_one_thousand",
            rationale: "At a numeric group boundary, 千 and 一千 express the same thousand. No other 一 is removed.",
            sources: &[
                "https://oercollective.caul.edu.au/japanese/chapter/8-0/",
                "https://www.jstage.jst.go.jp/article/bunken/76/3/76_38/_article/-char/ja/",
            ],
        },
        VariantRuleInfo {
            id: "arabic_hundred_spelling",
            rationale: "Only the documented hundred spellings مائة and مئة are equated inside typed numeric readings. No general hamza, maddah, or diacritic folding is performed.",
            sources: &[
                "https://www.arabiclanguageic.org/view_page.php?id=2796",
                "https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/ar.xml",
            ],
        },
        VariantRuleInfo {
            id: "vietnamese_zero_tens_linker",
            rationale: "Only linh/lẻ between trăm and a final unit word are equivalent skipped-tens linkers. Names and other dialect differences remain exact.",
            sources: &["https://openbooks.lib.msu.edu/vietnamese/chapter/u3-vocabulary/"],
        },
        VariantRuleInfo {
            id: "russian_numeric_yo",
            rationale: "A closed set of independently documented numeral inflections permits ё/е spelling. This is not general ё folding and never equates всё/все.",
            sources: &[
                "https://gramota.ru/biblioteka/spravochniki/pismovnik/kak-pravilno-upotreblyat-chislitelnye",
                "https://gramota.ru/poisk?mode=slovari&query=четыре",
                "https://gramota.ru/poisk?mode=slovari&query=четыреста",
                "https://gramota.ru/meta/chetvertyy",
            ],
        },
    ]
}

fn record(rules: &mut Vec<String>, id: &str) {
    if !rules.iter().any(|rule| rule == id) {
        rules.push(id.into());
    }
}
fn changed(before: &str, after: String, id: &str, rules: &mut Vec<String>) -> String {
    if before != after {
        record(rules, id);
    }
    after
}
fn nfc_trim(text: &str, rules: &mut Vec<String>) -> String {
    let normalized = changed(text, text.nfc().collect(), "unicode_nfc", rules);
    changed(
        &normalized,
        normalized.trim().to_owned(),
        "outer_whitespace",
        rules,
    )
}

/// Recognize a proven presentation variant, returning `None` when uncertain.
///
/// The source must reparse identically under these options and render completely.
/// `actual` must equal that canonical render. Ordinary text is an exact anchor;
/// language-specific transforms never alter literal labels, names, or unsupported spans.
/// Diagnostic readings are excluded even when an explicit literal-reading policy
/// can render them, such as the components of an impossible calendar date.
/// Limits are fixed even when the caller configures larger parser budgets.
pub fn compare_spoken(
    document: &SpeechDocument,
    options: &SpeechOptions,
    actual: &str,
    reference: &str,
) -> Option<VariantMatch> {
    if actual == reference
        || document.text.chars().take(8193).count() > 8192
        || document.spans.len() > 512
        || actual.chars().take(65537).count() > 65536
        || reference.chars().take(65537).count() > 65536
    {
        return None;
    }
    options.validate().ok()?;
    if super::parser::parse_for_speech(&document.text, options)
        .ok()?
        .ne(document)
    {
        return None;
    }
    let rendered = super::verbalize::verbalize_document_with_options(document, options).ok()?;
    if !rendered.fully_supported
        || rendered.text != actual
        || rendered
            .readings
            .iter()
            .any(|reading| reading.reason.is_some())
    {
        return None;
    }
    let mut rules = Vec::new();
    let left = nfc_trim(actual, &mut rules);
    let right = nfc_trim(reference, &mut rules);
    if left == right {
        return Some(VariantMatch { rules });
    }
    if chinese_currency_name_spacing(document, options, &left, &right) {
        record(&mut rules, "chinese_currency_name_spacing");
        return Some(VariantMatch { rules });
    }
    if let Some(variant) = currency_minor_unit_policy(document, options, actual, &right) {
        for rule in variant.rules {
            record(&mut rules, &rule);
        }
        return Some(VariantMatch { rules });
    }
    if options.locale == SpeechLocale::Ja
        && options.interpret_as == SpeechInterpretation::Auto
        && options.number_notation == NumberNotation::Phonetic
        && document
            .spans
            .iter()
            .filter(|span| matches!(span.value, SpeechValue::Number { .. }))
            .count()
            == 1
        && document.spans.iter().all(|span| {
            matches!(span.value, SpeechValue::Number { .. })
                || (matches!(span.value, SpeechValue::Text)
                    && span.source.chars().all(char::is_whitespace))
        })
    {
        let mut alternate_options = options.clone();
        alternate_options.number_notation = NumberNotation::Orthographic;
        // A render-only policy may not reinterpret the original source. This
        // also excludes compatibility IR differences in calendar/time parsing.
        if super::parser::parse_for_speech(&document.text, &alternate_options)
            .ok()
            .as_ref()
            == Some(document)
            && let Ok(alternate) =
                super::verbalize::verbalize_document_with_options(document, &alternate_options)
            && alternate.fully_supported
            && alternate.text != actual
            && alternate.text.chars().take(65537).count() <= 65536
        {
            let generated_match = alternate.text.nfc().collect::<String>().trim() == right;
            // At most one policy hop: Orthographic options cannot enter this
            // Phonetic-only branch again. The same bounded comparator can now
            // prove already reviewed spacing/initial-one-thousand variants.
            let presentation = if generated_match {
                None
            } else {
                compare_spoken(document, &alternate_options, &alternate.text, reference)
            };
            if generated_match || presentation.is_some() {
                record(&mut rules, "alternative_japanese_number_notation");
                if let Some(presentation) = presentation {
                    for rule in presentation.rules {
                        record(&mut rules, &rule);
                    }
                }
                return Some(VariantMatch { rules });
            }
        }
    }

    // Keep literals as exact anchors. Ambiguous anchor alignment is deliberately
    // rejected rather than searched fuzzily through the reference.
    let mut pieces: Vec<(String, Option<&SpeechValue>, &str)> = Vec::new();
    for (span, reading) in document.spans.iter().zip(&rendered.readings) {
        let spoken: String = reading.spoken.nfc().collect();
        let value = if matches!(span.value, SpeechValue::Text) {
            None
        } else {
            Some(&span.value)
        };
        if value.is_none() && pieces.last().is_some_and(|p| p.1.is_none()) {
            pieces.last_mut()?.0.push_str(&spoken);
        } else {
            pieces.push((spoken, value, &span.source));
        }
    }
    if let Some(first) = pieces.first_mut() {
        first.0 = first.0.trim_start().into();
    }
    if let Some(last) = pieces.last_mut() {
        last.0 = last.0.trim_end().into();
    }
    pieces.retain(|p| !p.0.is_empty());
    if pieces.iter().map(|p| p.0.as_str()).collect::<String>() != left {
        return None;
    }
    let mut cursor = 0;
    for (index, (spoken, value, source)) in pieces.iter().enumerate() {
        if let Some(value) = value {
            // Adjacent structured spans have no reliable literal separator.
            let end = match pieces.get(index + 1) {
                Some((anchor, None, _)) => {
                    let mut occurrences = right[cursor..].match_indices(anchor);
                    let (offset, _) = occurrences.next()?;
                    if occurrences.next().is_some() {
                        return None;
                    }
                    cursor + offset
                }
                Some(_) => return None,
                None => right.len(),
            };
            if !compare_block(
                spoken,
                &right[cursor..end],
                value,
                source,
                options,
                &mut rules,
            ) {
                return None;
            }
            cursor = end;
        } else {
            if !right[cursor..].starts_with(spoken) {
                return None;
            }
            cursor += spoken.len();
        }
    }
    (cursor == right.len() && !rules.is_empty()).then_some(VariantMatch { rules })
}

fn standalone_currency(document: &SpeechDocument) -> Option<&SpeechValue> {
    let mut currencies = document.spans.iter().filter_map(|span| {
        matches!(span.value, SpeechValue::Currency { .. }).then_some(&span.value)
    });
    let currency = currencies.next()?;
    if currencies.next().is_some()
        || !document.spans.iter().all(|span| {
            matches!(span.value, SpeechValue::Currency { .. })
                || (matches!(span.value, SpeechValue::Text)
                    && span.source.chars().all(char::is_whitespace))
        })
    {
        return None;
    }
    Some(currency)
}

fn chinese_currency_name_spacing(
    document: &SpeechDocument,
    options: &SpeechOptions,
    actual: &str,
    reference: &str,
) -> bool {
    if options.locale != SpeechLocale::Zh {
        return false;
    }
    let Some(SpeechValue::Currency { code, .. }) = standalone_currency(document) else {
        return false;
    };
    ["one", "other"].iter().any(|category| {
        let Some(name) = super::lexicons::currency_name(options.locale, code, category) else {
            return false;
        };
        let Some(number) = actual.strip_suffix(name) else {
            return false;
        };
        let Some(other_number) = reference.strip_suffix(name) else {
            return false;
        };
        let number = number.trim_end();
        !number.is_empty() && number == other_number.trim_end()
    })
}

fn currency_minor_unit_policy(
    document: &SpeechDocument,
    options: &SpeechOptions,
    actual: &str,
    reference: &str,
) -> Option<VariantMatch> {
    let typed @ SpeechValue::Currency { code, value } = standalone_currency(document)? else {
        return None;
    };
    if !matches!(
        (options.locale, code.as_str()),
        (SpeechLocale::De, "EUR") | (SpeechLocale::Pt, "BRL")
    ) {
        return None;
    }
    let fraction = value.fraction.as_deref()?;
    if !(1..=2).contains(&fraction.len()) || !fraction.bytes().any(|digit| digit != b'0') {
        return None;
    }
    let mut alternate_options = options.clone();
    alternate_options.reading_style = match options.reading_style {
        ReadingStyle::Precise => ReadingStyle::Natural,
        ReadingStyle::Natural => ReadingStyle::Precise,
        ReadingStyle::Digits => return None,
    };
    if super::parser::parse_for_speech(&document.text, &alternate_options)
        .ok()
        .as_ref()
        != Some(document)
    {
        return None;
    }
    let alternate =
        super::verbalize::verbalize_document_with_options(document, &alternate_options).ok()?;
    if !alternate.fully_supported
        || alternate
            .readings
            .iter()
            .any(|reading| reading.reason.is_some())
        || alternate.text == actual
        || alternate.text.chars().take(65537).count() > 65536
    {
        return None;
    }
    let alternate: String = alternate.text.nfc().collect();
    let mut presentation = Vec::new();
    // compare_block has no style-policy recursion. Its existing German case
    // and numeric-compound presentation rules remain bounded to this currency.
    if alternate.trim() != reference
        && !compare_block(
            alternate.trim(),
            reference,
            typed,
            &document.text,
            &alternate_options,
            &mut presentation,
        )
    {
        return None;
    }
    let mut rules = vec!["alternative_currency_minor_unit_policy".to_string()];
    rules.extend(presentation);
    Some(VariantMatch { rules })
}

fn compare_block(
    actual: &str,
    reference: &str,
    value: &SpeechValue,
    source: &str,
    options: &SpeechOptions,
    rules: &mut Vec<String>,
) -> bool {
    if actual == reference {
        return true;
    }
    if matches!(value, SpeechValue::LabelledNumber { .. }) {
        // This dedicated path changes only the source-declared separator.
        // Never pass label words through numeric case/hyphen/phoneme rules.
        if literal_label_separator_variant(reference, value, options) {
            record(rules, "english_literal_label_separator");
            return true;
        }
        return false;
    }
    if options.locale == SpeechLocale::Es
        && spanish_whole_hour_variant(actual, reference, value, options)
    {
        record(rules, "spanish_whole_hour_noun");
        return true;
    }
    if options.locale == SpeechLocale::Fr
        && matches!(
            value,
            SpeechValue::Temporal {
                value: super::temporal::TemporalValue::Decade { year: 1980 }
            }
        )
        && reference == "eighties"
    {
        record(rules, "french_1980s_loanword");
        return true;
    }
    if let SpeechValue::Number {
        value,
        reading: NumberReading::Cardinal,
    } = value
        && options.interpret_as == SpeechInterpretation::Number
        && options.reading_style != ReadingStyle::Digits
        && value.fraction.is_none()
        && (value.integer.len() == 1 || !value.integer.starts_with('0'))
        && let Some(digits) = super::numbers::verbalize_number_with_options(
            value,
            options.locale,
            NumberReading::Cardinal,
            ReadingStyle::Digits,
            options.number_notation,
        )
        && digits == reference
    {
        record(rules, "alternative_integer_digit_policy");
        return true;
    }
    let mut a = actual.to_owned();
    let mut b = reference.to_owned();
    macro_rules! apply {
        ($id:literal, $f:expr) => {{
            a = changed(&a, $f(&a), $id, rules);
            b = changed(&b, $f(&b), $id, rules);
        }};
    }
    let locale = options.locale;
    match locale {
        SpeechLocale::En | SpeechLocale::Fr => {
            apply!("numeric_hyphen_en_fr", interletter_hyphen);
            let mixed = matches!(
                value,
                SpeechValue::Expression {
                    value: ExpressionValue::MixedFraction { .. }
                } | SpeechValue::MeasuredExpression {
                    value: ExpressionValue::MixedFraction { .. },
                    ..
                }
            );
            if locale == SpeechLocale::En && !mixed {
                apply!("english_hundred_and", hundred_and);
            }
        }
        SpeechLocale::De => {
            let nouns: Vec<String> = actual
                .split_whitespace()
                .filter(|word| {
                    let mut chars = word.chars();
                    chars.next().is_some_and(char::is_uppercase)
                        && chars.clone().count() >= 2
                        && chars.all(char::is_lowercase)
                })
                .map(str::to_lowercase)
                .collect();
            apply!("german_numeric_case", |s: &str| german_case(s, &nouns));
            let compounds: Vec<String> = a
                .split_whitespace()
                .filter(|word| german_numeric_word(word))
                .map(str::to_owned)
                .collect();
            apply!("german_compound_spacing", |s: &str| german_spacing(
                s, &compounds
            ));
            let units = unit_decompositions(value, locale);
            apply!("german_unit_compound_spacing", |s: &str| {
                unit_component_spacing(s, &units, true)
            });
        }
        SpeechLocale::Zh | SpeechLocale::Ja => {
            apply!("cjk_numeric_unit_spacing", cjk_spacing);
            if locale == SpeechLocale::Ja {
                apply!("japanese_initial_one_thousand", japanese_thousand);
            }
        }
        SpeechLocale::Ar => {
            apply!("arabic_hundred_spelling", |s: &str| s
                .replace("مائة", "مئة"));
        }
        SpeechLocale::Ko => {
            apply!("korean_number_group_spacing", |s: &str| layout_spaces(
                s,
                |a, b| "만억조".contains(a) && korean_number(b)
            ));
            if let SpeechValue::Measure { unit, .. } = value
                && let Some((pattern, _)) =
                    super::lexicons::spoken_unit_pattern(SpeechLocale::Ko, unit, "other")
                && let Some(suffix) = pattern
                    .strip_prefix("{0}")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            {
                apply!("korean_quantity_unit_spacing", |s: &str| {
                    korean_unit_spacing(s, suffix)
                });
            }
        }
        SpeechLocale::Vi => {
            apply!("vietnamese_zero_tens_linker", vietnamese_linker);
            let units = unit_decompositions(value, locale);
            apply!("vietnamese_unit_component_spacing", |s: &str| {
                unit_component_spacing(s, &units, false)
            });
            let aliases = typed_unit_aliases(value, locale);
            apply!("vietnamese_typed_unit_alias", |s: &str| {
                unit_component_spacing(s, &aliases, false)
            });
        }
        SpeechLocale::Ru => {
            apply!("russian_numeric_yo", russian_numeric_yo);
            for (variant, rule) in russian_clock_variants(value, source, options) {
                let normalized = russian_numeric_yo(&variant);
                if normalized == b {
                    if normalized != variant {
                        record(rules, "russian_numeric_yo");
                    }
                    record(rules, rule);
                    return true;
                }
            }
            if matches!(value, SpeechValue::Currency { code, .. } if code == "USD") {
                apply!("russian_usd_qualification", russian_usd_qualification);
            }
        }
        _ => {}
    }
    a == b
}

fn literal_label_separator_variant(
    reference: &str,
    typed: &SpeechValue,
    options: &SpeechOptions,
) -> bool {
    if options.locale != SpeechLocale::En || options.interpret_as != SpeechInterpretation::Measure {
        return false;
    }
    let SpeechValue::LabelledNumber {
        label,
        value,
        number_first,
        separator,
    } = typed
    else {
        return false;
    };
    if separator != "-" {
        return false;
    }
    let Some(words) =
        super::numbers::verbalize_with_options(value, options, NumberReading::Cardinal)
    else {
        return false;
    };
    let label: String = label.nfc().collect();
    let words: String = words.nfc().collect();
    [" ", "-", "- "].iter().any(|separator| {
        let candidate = if *number_first {
            format!("{words}{separator}{label}")
        } else {
            format!("{label}{separator}{words}")
        };
        candidate == reference
    })
}

fn spanish_whole_hour_variant(
    actual: &str,
    reference: &str,
    typed: &SpeechValue,
    options: &SpeechOptions,
) -> bool {
    use super::temporal::TemporalValue;
    if options.reading_style == ReadingStyle::Digits {
        return false;
    }
    let (hour, minute, second) = match typed {
        SpeechValue::Time {
            hour,
            minute,
            second,
        } => (*hour, Some(*minute), *second),
        SpeechValue::Temporal {
            value:
                TemporalValue::Clock {
                    hour,
                    minute,
                    second,
                    period: None,
                    half: false,
                    ..
                },
        } => (*hour, *minute, *second),
        _ => return false,
    };
    if hour > 24 || minute.is_some_and(|m| m != 0) || second.is_some_and(|s| s != 0) {
        return false;
    }
    let value = super::verbalize::value(hour);
    let Some(bare) = super::numbers::verbalize_number_feminine(
        &value,
        SpeechLocale::Es,
        NumberReading::Cardinal,
    ) else {
        return false;
    };
    let Some(explicit) = super::verbalize::quantity(SpeechLocale::Es, "duration-hour", &value)
    else {
        return false;
    };
    // The remaining canonical suffix is immutable. This retains AM/PM wording
    // and explicit zero seconds rather than treating exact hour as permission
    // to discard temporal qualifiers or written precision.
    [(&bare, &explicit), (&explicit, &bare)]
        .iter()
        .any(|(from, to)| {
            actual
                .strip_prefix(from.as_str())
                .filter(|suffix| {
                    suffix.is_empty()
                        || suffix.starts_with(" de la ")
                        || (second == Some(0) && suffix.starts_with(" y "))
                })
                .is_some_and(|suffix| format!("{to}{suffix}") == reference)
        })
}

fn interletter_hyphen(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if c == '-'
                && i > 0
                && i + 1 < chars.len()
                && chars[i - 1].is_alphabetic()
                && chars[i + 1].is_alphabetic()
            {
                ' '
            } else {
                c
            }
        })
        .collect()
}
const EN_SMALL: &[&str] = &[
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
];
fn hundred_and(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    words
        .iter()
        .enumerate()
        .filter_map(|(i, word)| {
            (!(words[i] == "and"
                && i > 0
                && words[i - 1] == "hundred"
                && words.get(i + 1).is_some_and(|w| EN_SMALL.contains(w))))
            .then_some(*word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const DE_PARTS: &[&str] = &[
    "null", "eins", "ein", "eine", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht",
    "neun", "zehn", "elf", "zwölf", "sechzehn", "siebzehn", "zwanzig", "dreißig", "vierzig",
    "fünfzig", "sechzig", "siebzig", "achtzig", "neunzig", "hundert", "tausend", "und",
];
fn german_number(word: &str) -> bool {
    if word.is_empty() || word == "und" || word.len() > 256 {
        return false;
    }
    let mut reachable = vec![false; word.len() + 1];
    reachable[0] = true;
    for i in 0..word.len() {
        if !reachable[i] {
            continue;
        }
        for part in DE_PARTS {
            if word[i..].starts_with(part) {
                reachable[i + part.len()] = true;
            }
        }
    }
    reachable[word.len()]
}
fn german_numeric_word(word: &str) -> bool {
    german_number(word)
        || [
            "ster", "stes", "stem", "sten", "ste", "ter", "tes", "tem", "ten", "te",
        ]
        .iter()
        .any(|ending| word.strip_suffix(ending).is_some_and(german_number))
}
fn map_words(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut result = String::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if !ch.is_alphabetic() {
            result.push_str(&f(&text[start..index]));
            result.push(ch);
            start = index + ch.len_utf8();
        }
    }
    result.push_str(&f(&text[start..]));
    result
}
fn german_case(text: &str, nouns: &[String]) -> String {
    map_words(text, |word| {
        let lower = word.to_lowercase();
        if german_numeric_word(&lower) || nouns.contains(&lower) {
            lower
        } else {
            word.into()
        }
    })
}
fn german_spacing(text: &str, compounds: &[String]) -> String {
    let mut result = text.to_owned();
    // Canonical compounds supply the exact allowed letters. Never concatenate
    // two independent canonical numbers or remove spaces in a unit/name.
    for target in compounds {
        let words: Vec<&str> = result.split(' ').collect();
        let mut output = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let mut joined = String::new();
            let mut matched = None;
            for (j, word) in words.iter().enumerate().skip(i) {
                let lower = word.to_lowercase();
                if lower != "und" && !german_numeric_word(&lower) {
                    break;
                }
                joined.push_str(&lower);
                if !target.starts_with(&joined) {
                    break;
                }
                if &joined == target {
                    matched = Some(j + 1);
                    break;
                }
            }
            if let Some(end) = matched {
                output.push(target.as_str());
                i = end;
            } else {
                output.push(words[i]);
                i += 1;
            }
        }
        result = output.join(" ");
    }
    result
}

// The typed key supplies the canonical unit spelling. Only known morphological
// boundaries may be split; arbitrary letter-by-letter spelling is not speech
// equivalence. No second list of units or alternate measurement keys is kept.
fn unit_decompositions(value: &SpeechValue, locale: SpeechLocale) -> Vec<(String, String)> {
    let unit = match value {
        SpeechValue::Measure { unit, .. } | SpeechValue::MeasuredExpression { unit, .. } => unit,
        _ => return Vec::new(),
    };
    let mut result = Vec::new();
    for key in unit.trim_start_matches("per:").split("/per:") {
        let Some((pattern, _)) = super::lexicons::spoken_unit_pattern(locale, key, "other") else {
            continue;
        };
        for word in pattern.split_whitespace() {
            let canonical = word.to_lowercase();
            let decomposed = match locale {
                SpeechLocale::De => ["quadrat", "kubik"].iter().find_map(|prefix| {
                    canonical
                        .strip_prefix(prefix)
                        .filter(|s| !s.is_empty())
                        .map(|base| format!("{prefix} {base}"))
                }),
                SpeechLocale::Vi => {
                    [("kilô", "ki lô"), ("mili", "mi li")]
                        .iter()
                        .find_map(|(prefix, split)| {
                            canonical
                                .strip_prefix(prefix)
                                .filter(|s| !s.is_empty())
                                .map(|base| format!("{split} {base}"))
                        })
                }
                _ => None,
            };
            if let Some(decomposed) = decomposed {
                result.push((canonical, decomposed));
            }
        }
    }
    result
}
fn unit_component_spacing(text: &str, units: &[(String, String)], fold_case: bool) -> String {
    let mut result = text.to_owned();
    for (canonical, decomposed) in units {
        if fold_case {
            result = map_words(&result, |word| {
                let lower = word.to_lowercase();
                if decomposed.split(' ').any(|part| part == lower) {
                    lower
                } else {
                    word.into()
                }
            });
        }
        let mut replaced = String::new();
        let mut cursor = 0;
        for (offset, _) in result.match_indices(decomposed) {
            let end = offset + decomposed.len();
            if result[..offset]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic)
                || result[end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphabetic)
            {
                continue;
            }
            replaced.push_str(&result[cursor..offset]);
            replaced.push_str(canonical);
            cursor = end;
        }
        replaced.push_str(&result[cursor..]);
        result = replaced;
    }
    result
}
fn typed_unit_aliases(value: &SpeechValue, locale: SpeechLocale) -> Vec<(String, String)> {
    let unit = match value {
        SpeechValue::Measure { unit, .. } | SpeechValue::MeasuredExpression { unit, .. } => unit,
        _ => return Vec::new(),
    };
    let Some((pattern, _)) = super::lexicons::spoken_unit_pattern(locale, unit, "other") else {
        return Vec::new();
    };
    let Some(canonical) = pattern
        .strip_prefix("{0}")
        .map(str::trim)
        .filter(|s| s.chars().all(char::is_alphabetic))
    else {
        return Vec::new();
    };
    super::lexicons::unit_forms(locale)
        .iter()
        .filter(|(form, key)| {
            *key == unit
                && form != canonical
                && form.contains([' ', '-'])
                && form
                    .chars()
                    .all(|ch| ch.is_alphabetic() || matches!(ch, ' ' | '-'))
        })
        .map(|(alias, _)| (canonical.into(), alias.clone()))
        .collect()
}

fn cjk_number(c: char) -> bool {
    "零〇一二三四五六七八九十百千万萬亿億兆两兩".contains(c)
}
fn cjk_unit(c: char) -> bool {
    "年月日時时点點分秒元円度米第".contains(c)
}
fn cjk_spacing(text: &str) -> String {
    layout_spaces(text, |a, b| {
        (cjk_number(a) && (cjk_number(b) || cjk_unit(b))) || (cjk_unit(a) && cjk_number(b))
    })
}
// One linear pass, including a long whitespace run from an untrusted reference.
fn layout_spaces(text: &str, remove: impl Fn(char, char) -> bool) -> String {
    let mut result = String::new();
    let mut pending = String::new();
    let mut previous = None;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending.push(ch);
            continue;
        }
        if !pending.is_empty() {
            if previous.is_none_or(|before| !remove(before, ch)) {
                result.push_str(&pending);
            }
            pending.clear();
        }
        result.push(ch);
        previous = Some(ch);
    }
    result.push_str(&pending);
    result
}
fn korean_number(ch: char) -> bool {
    "영공일이삼사오육칠팔구십백천만억조".contains(ch)
}
fn korean_unit_spacing(text: &str, suffix: &str) -> String {
    let Some(prefix) = text.strip_suffix(suffix) else {
        return text.into();
    };
    let number = prefix.trim_end();
    if number.chars().last().is_some_and(korean_number) {
        format!("{number}{suffix}")
    } else {
        text.into()
    }
}
fn japanese_thousand(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            if c == '一'
                && chars.get(i + 1) == Some(&'千')
                && (i == 0
                    || chars[i - 1].is_whitespace()
                    || "万萬億兆".contains(chars[i - 1])
                    || chars[..i].ends_with(&['マ', 'イ', 'ナ', 'ス']))
            {
                None
            } else {
                Some(c)
            }
        })
        .collect()
}
fn vietnamese_linker(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    words
        .iter()
        .enumerate()
        .map(|(i, &word)| {
            if word == "lẻ"
                && i > 0
                && words[i - 1] == "trăm"
                && words.get(i + 1).is_some_and(|w| {
                    [
                        "một", "hai", "ba", "bốn", "năm", "sáu", "bảy", "tám", "chín",
                    ]
                    .contains(w)
                })
            {
                "linh"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn russian_numeric_yo(text: &str) -> String {
    const FORMS: &[&str] = &[
        "трёх",
        "четырёх",
        "трём",
        "четырём",
        "трёхсот",
        "четырёхсот",
        "четвёртого",
    ];
    map_words(text, |word| {
        if FORMS.contains(&word) {
            word.replace('ё', "е")
        } else {
            word.into()
        }
    })
}

fn russian_usd_qualification(text: &str) -> String {
    [" США", " сэ шэ а", " сэ-шэ-а"]
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(text)
        .into()
}

fn russian_clock_variants(
    value: &SpeechValue,
    source: &str,
    options: &SpeechOptions,
) -> Vec<(String, &'static str)> {
    use super::temporal::TemporalValue;
    // Compact Time stores a 24-hour value but no source period metadata.
    // Consult the shared temporal parser before offering a colloquial form:
    // an explicitly written AM/PM or period must not silently disappear.
    let (end, parsed) = super::temporal::parse_numeric_time(source, 0, source.len(), options);
    if end != source.len()
        || !matches!(
            parsed,
            TemporalValue::Clock {
                period: None,
                meridiem: None,
                ..
            }
        )
    {
        return Vec::new();
    }
    let Some(SpeechValue::Time {
        hour,
        minute,
        second: None,
    }) = super::temporal::compact_value(value)
    else {
        return Vec::new();
    };
    let Some(hours) = super::verbalize::quantity(
        SpeechLocale::Ru,
        "duration-hour",
        &super::verbalize::value(hour),
    ) else {
        return Vec::new();
    };
    let Some(minutes) = super::verbalize::quantity(
        SpeechLocale::Ru,
        "duration-minute",
        &super::verbalize::value(minute),
    ) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    if minute == 0 {
        // The documented zero-minute convention is symmetric: a compact
        // Precise clock may already state ноль минут, while its alternate
        // exact-hour reading leaves those zero minutes implicit.
        result.push((hours.clone(), "russian_clock_zero_minutes"));
        result.push((format!("{hours} {minutes}"), "russian_clock_zero_minutes"));
        result.push((format!("{hours} и {minutes}"), "russian_clock_zero_minutes"));
    } else {
        result.push((format!("{hours} {minutes}"), "russian_clock_conjunction"));
        result.push((format!("{hours} и {minutes}"), "russian_clock_conjunction"));
    }
    // A colloquial twelve-hour expression must not erase an explicit 24-hour
    // reading or a period/meridiem. Midnight (0) is likewise not mapped to noon.
    if !(1..=12).contains(&hour) || minute == 0 {
        return result;
    }
    let next_hour = hour % 12 + 1;
    let Some(next_ordinal) = super::verbalize::russian_year(next_hour) else {
        return result;
    };
    if minute <= 30 {
        result.push((
            format!("{minutes} {next_ordinal}"),
            "russian_clock_relative",
        ));
        if minute == 15 {
            result.push((format!("четверть {next_ordinal}"), "russian_clock_relative"));
        }
        if minute == 30 {
            result.push((format!("половина {next_ordinal}"), "russian_clock_relative"));
            result.push((format!("пол{next_ordinal}"), "russian_clock_relative"));
        }
    } else {
        let remaining = 60 - minute;
        let count = super::verbalize::value(remaining);
        let inflected = SpeechValue::InflectedNumber {
            value: count,
            inflection: super::grammar::NumberInflection::Genitive,
        };
        let Some(mut genitive) = super::grammar::render(&inflected, options) else {
            return result;
        };
        let singular = remaining % 10 == 1 && remaining % 100 != 11;
        if singular {
            let Some(prefix) = genitive.strip_suffix("одного") else {
                return result;
            };
            genitive = format!("{prefix}одной");
        }
        let Some(mut next) = super::numbers::verbalize_number(
            &super::verbalize::value(next_hour),
            SpeechLocale::Ru,
            NumberReading::Cardinal,
        ) else {
            return result;
        };
        if next_hour == 1 {
            let Some((pattern, _)) =
                super::lexicons::unit_pattern(SpeechLocale::Ru, "duration-hour", "one")
            else {
                return result;
            };
            let Some(hour_noun) = pattern.strip_prefix("{0}").map(str::trim) else {
                return result;
            };
            next = hour_noun.into();
        }
        let noun = if singular {
            "минуты"
        } else {
            "минут"
        };
        result.push((
            format!("без {genitive} {noun} {next}"),
            "russian_clock_relative",
        ));
        if remaining != 1 {
            result.push((format!("без {genitive} {next}"), "russian_clock_relative"));
        }
        if minute == 45 {
            result.push((format!("без четверти {next}"), "russian_clock_relative"));
        }
    }
    result
}
