//! Localized month names and unit/currency patterns from Unicode CLDR.
//! Source: <https://github.com/unicode-org/cldr-json> (retrieved 2026-11-13).
//! Only the small speech-normalization subset is bundled; no runtime download.
/*
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

*/
use super::types::SpeechLocale;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn data() -> &'static BTreeMap<String, Value> {
    static DATA: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(LEXICONS).expect("bundled CLDR speech lexicons must be valid")
    })
}

pub(super) fn month(locale: SpeechLocale, month: u32) -> Option<&'static str> {
    data()
        .get(locale.code())?
        .get("months")?
        .get(month.checked_sub(1)? as usize)?
        .as_str()
}

pub(super) fn unit_pattern(
    locale: SpeechLocale,
    key: &str,
    category: &str,
) -> Option<(&'static str, Option<&'static str>)> {
    let value = data().get(locale.code())?.get("units")?.get(key)?;
    let key = format!("unitPattern-count-{category}");
    let pattern = value
        .get(&key)
        .or_else(|| value.get("unitPattern-count-other"))?
        .as_str()?;
    Some((pattern, value.get("gender").and_then(Value::as_str)))
}

/// Optional spoken-language patterns, falling back to CLDR display wording.
pub(super) fn spoken_unit_pattern(
    locale: SpeechLocale,
    key: &str,
    category: &str,
) -> Option<(&'static str, Option<&'static str>)> {
    let value = data().get(locale.code())?.get("units")?.get(key)?;
    if let Some(pattern) = value
        .get(format!("speechPattern-count-{category}"))
        .or_else(|| value.get("speechPattern-count-other"))
        .and_then(Value::as_str)
    {
        Some((pattern, value.get("gender").and_then(Value::as_str)))
    } else {
        unit_pattern(locale, key, category)
    }
}

pub(super) fn currency_name(
    locale: SpeechLocale,
    code: &str,
    category: &str,
) -> Option<&'static str> {
    let value = data().get(locale.code())?.get("currencies")?.get(code)?;
    let key = format!("displayName-count-{category}");
    value
        .get(&key)
        .or_else(|| value.get("displayName-count-other"))?
        .as_str()
}

/// Written unit nouns derived from the same patterns used by verbalization.
/// Indexing once avoids maintaining a second localized unit vocabulary.
pub(super) fn unit_forms(locale: SpeechLocale) -> &'static [(String, &'static str)] {
    use unicode_normalization::UnicodeNormalization;
    type UnitForms = BTreeMap<String, Vec<(String, &'static str)>>;
    static FORMS: OnceLock<UnitForms> = OnceLock::new();
    let forms = FORMS.get_or_init(|| {
        let mut forms = BTreeMap::new();
        for (language, entry) in data() {
            let mut words = Vec::new();
            if let Some(units) = entry.get("units").and_then(Value::as_object) {
                for (key, entry) in units {
                    let Some(patterns) = entry.as_object() else {
                        continue;
                    };
                    if let Some(aliases) = entry.get("aliases").and_then(Value::as_array) {
                        for alias in aliases.iter().filter_map(Value::as_str) {
                            words.push((
                                alias.nfkc().flat_map(char::to_lowercase).collect(),
                                key.as_str(),
                            ));
                        }
                    }
                    for (category, value) in patterns {
                        if !category.contains("-count-") {
                            continue;
                        }
                        let Some(form) = value
                            .as_str()
                            .and_then(|s| s.strip_prefix("{0}"))
                            .map(str::trim)
                        else {
                            continue;
                        };
                        if form.is_empty() || form.contains('{') {
                            continue;
                        }
                        let normalized: String = form.nfkc().flat_map(char::to_lowercase).collect();
                        words.push((normalized, key.as_str()));
                    }
                }
            }
            words.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
            words.dedup();
            forms.insert(language.clone(), words);
        }
        forms
    });
    forms.get(locale.code()).map(Vec::as_slice).unwrap_or(&[])
}

const LEXICONS: &str = include_str!("data/lexicons.json");
