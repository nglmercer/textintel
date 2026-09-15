//! Rule-based script transliteration (`privet ↔ привет`, …).
//!
//! [`RuleBasedTransliterationProvider`] converts between Latin, Cyrillic,
//! Arabic, and (a small table of) Simplified Chinese. It is a deterministic
//! `Basic` fallback: per-character maps plus a few digraphs and word entries,
//! with unknown characters passed through unchanged. Views are stored as
//! additional `transliteration:<script>` fingerprint views and participate in
//! decoded-overlap comparison; [`MessageFingerprint::raw`](crate::core::types::MessageFingerprint)
//! is never replaced.
//!
//! Known limits (documented, not hidden): Arabic short vowels are unwritten,
//! so `سلام` folds to `slam` (the reverse direction still links `salam` →
//! `سلام`); the Han table covers common characters only; Latin→Cyrillic is
//! Russian-biased.

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::{Transliteration, TransliterationProvider};

/// Deterministic rule-based transliteration for Latn/Cyrl/Arab/Hans.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedTransliterationProvider;

impl RuleBasedTransliterationProvider {
    fn views_for(text: &str) -> Vec<Transliteration> {
        use crate::normalization::unicode::casefold_text;
        let compact_input: String = casefold_text(text)
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        let mut views = Vec::new();
        let push = |views: &mut Vec<Transliteration>,
                    output: String,
                    script: &str,
                    language: Option<&str>,
                    confidence: f64| {
            // Views must carry new information: outputs equal modulo
            // case/whitespace (e.g. pass-through CJK with inserted syllable
            // spaces) are dropped instead of stored.
            let compact_output: String = casefold_text(&output)
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            if output != text && !output.is_empty() && compact_output != compact_input {
                views.push(Transliteration::new(
                    output,
                    script,
                    language.map(str::to_string),
                    confidence,
                ));
            }
        };
        if contains_script(text, Script::Cyrillic) {
            push(&mut views, cyrillic_to_latin(text), "Latn", Some("ru"), 0.6);
        }
        if contains_script(text, Script::Arabic) {
            push(&mut views, arabic_to_latin(text), "Latn", Some("ar"), 0.6);
        }
        if contains_script(text, Script::Han) {
            push(&mut views, han_to_latin(text), "Latn", Some("zh"), 0.5);
        }
        if latin_heavy(text) {
            push(&mut views, latin_to_cyrillic(text), "Cyrl", Some("ru"), 0.6);
            push(&mut views, latin_to_arabic(text), "Arab", Some("ar"), 0.55);
            push(&mut views, latin_to_han(text), "Hans", Some("zh"), 0.5);
        }
        // One view per target script: keep the first (highest confidence).
        let mut seen = std::collections::BTreeSet::new();
        views.retain(|view| seen.insert(view.target_script.clone()));
        views
    }
}

impl TransliterationProvider for RuleBasedTransliterationProvider {
    fn transliterate(&self, text: &str) -> Vec<Transliteration> {
        Self::views_for(text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("rule_based_transliteration")
            .with_languages(["ru", "ar", "zh", "und"])
            .with_quality(CapabilityLevel::Basic)
            .with_fallback(
                "per-character rule tables; prefer a trained transliterator for production",
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Cyrillic,
    Arabic,
    Han,
}

fn contains_script(text: &str, script: Script) -> bool {
    text.chars().any(|ch| match script {
        Script::Cyrillic => matches!(ch, '\u{400}'..='\u{52f}'),
        Script::Arabic => {
            matches!(ch, '\u{600}'..='\u{6ff}' | '\u{750}'..='\u{77f}' | '\u{8a0}'..='\u{8ff}')
        }
        Script::Han => matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'),
    })
}

fn latin_heavy(text: &str) -> bool {
    let mut latin = 0usize;
    let mut letters = 0usize;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            letters += 1;
            if ch.is_ascii_alphabetic() {
                latin += 1;
            }
        }
    }
    latin >= 2 && latin * 2 >= letters
}

fn match_case(mapped: &str, upper: bool) -> String {
    if !upper {
        mapped.to_string()
    } else {
        let mut chars = mapped.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

fn cyrillic_to_latin(text: &str) -> String {
    text.chars()
        .map(|ch| {
            let lower = ch.to_lowercase().next().unwrap_or(ch);
            let mapped = match lower {
                'а' => "a",
                'б' => "b",
                'в' => "v",
                'г' => "g",
                'д' => "d",
                'е' => "e",
                'ё' => "yo",
                'ж' => "zh",
                'з' => "z",
                'и' => "i",
                'й' => "y",
                'к' => "k",
                'л' => "l",
                'м' => "m",
                'н' => "n",
                'о' => "o",
                'п' => "p",
                'р' => "r",
                'с' => "s",
                'т' => "t",
                'у' => "u",
                'ф' => "f",
                'х' => "kh",
                'ц' => "ts",
                'ч' => "ch",
                'ш' => "sh",
                'щ' => "shch",
                'ъ' => "",
                'ы' => "y",
                'ь' => "'",
                'э' => "e",
                'ю' => "yu",
                'я' => "ya",
                'і' => "i",
                'ї' => "yi",
                'є' => "ye",
                'ґ' => "g",
                'ў' => "w",
                _ => return ch.to_string(),
            };
            match_case(mapped, ch.is_uppercase())
        })
        .collect()
}

fn latin_to_cyrillic(text: &str) -> String {
    const DIGRAPHS: &[(&str, &str)] = &[
        ("shch", "щ"),
        ("zh", "ж"),
        ("kh", "х"),
        ("ts", "ц"),
        ("ch", "ч"),
        ("sh", "ш"),
        ("yu", "ю"),
        ("ya", "я"),
        ("yo", "ё"),
        ("ye", "е"),
        ("yi", "и"),
        ("ks", "кс"),
    ];
    let mut output = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let rest: String = chars[index..].iter().collect::<String>().to_lowercase();
        let mut matched: Option<(&str, usize)> = None;
        for (latin, cyrl) in DIGRAPHS {
            if rest.starts_with(latin) {
                matched = Some((cyrl, latin.len()));
                break;
            }
        }
        if let Some((cyrl, width)) = matched {
            output.push_str(&match_case(cyrl, chars[index].is_uppercase()));
            index += width;
            continue;
        }
        let ch = chars[index];
        let mapped = match ch.to_lowercase().next().unwrap_or(ch) {
            'a' => "а",
            'b' => "б",
            'c' => "к",
            'd' => "д",
            'e' => "е",
            'f' => "ф",
            'g' => "г",
            'h' => "х",
            'i' => "и",
            'j' => "ж",
            'k' => "к",
            'l' => "л",
            'm' => "м",
            'n' => "н",
            'o' => "о",
            'p' => "п",
            'q' => "к",
            'r' => "р",
            's' => "с",
            't' => "т",
            'u' => "у",
            'v' => "в",
            'w' => "в",
            'x' => "кс",
            'y' => "й",
            'z' => "з",
            '\'' => "ь",
            _ => {
                output.push(ch);
                index += 1;
                continue;
            }
        };
        output.push_str(&match_case(mapped, ch.is_uppercase()));
        index += 1;
    }
    output
}

fn arabic_to_latin(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            'ا' | 'أ' | 'إ' | 'آ' | 'ى' | 'ة' => output.push('a'),
            'ب' => output.push('b'),
            'ت' => output.push('t'),
            'ث' => output.push_str("th"),
            'ج' => output.push('j'),
            'ح' => output.push('h'),
            'خ' => output.push_str("kh"),
            'د' => output.push('d'),
            'ذ' => output.push_str("dh"),
            'ر' => output.push('r'),
            'ز' => output.push('z'),
            'س' => output.push('s'),
            'ش' => output.push_str("sh"),
            'ص' => output.push('s'),
            'ض' => output.push('d'),
            'ط' => output.push('t'),
            'ظ' => output.push('z'),
            'ع' => output.push('\''),
            'غ' => output.push_str("gh"),
            'ف' => output.push('f'),
            'ق' => output.push('q'),
            'ك' => output.push('k'),
            'ل' => output.push('l'),
            'م' => output.push('m'),
            'ن' => output.push('n'),
            'ه' => output.push('h'),
            'و' => output.push('w'),
            'ي' => output.push('y'),
            '\u{64b}'..='\u{652}' => {}
            _ => output.push(ch),
        }
    }
    output
}

/// Frequent words whose short/long vowels no character rule can recover
/// (`salam` → `سلام` drops the first `a` but keeps the second). A tiny
/// exception lexicon is standard practice for rule-based systems; everything
/// else goes through the character tables below.
fn latin_arabic_word(word: &str) -> Option<&'static str> {
    Some(match word {
        "salam" => "سلام",
        "salaam" => "سلام",
        "islam" => "إسلام",
        "muslim" => "مسلم",
        "allah" => "الله",
        "mohamed" | "mohammed" | "muhammad" => "محمد",
        "ahmed" | "ahmad" => "أحمد",
        "quran" => "قرآن",
        "ramadan" => "رمضان",
        _ => return None,
    })
}

fn latin_to_arabic(text: &str) -> String {
    const DIGRAPHS: &[(&str, char)] = &[
        ("th", 'ث'),
        ("dh", 'ذ'),
        ("kh", 'خ'),
        ("sh", 'ش'),
        ("gh", 'غ'),
        ("aa", 'آ'),
    ];
    // Whole-word exceptions first (whitespace-separated, case-insensitive),
    // then per-character conversion for the rest.
    let words: Vec<&str> = text.split_whitespace().collect();
    if !words.is_empty()
        && words.iter().all(|word| {
            word.chars()
                .all(|ch| ch.is_ascii_alphabetic() || ch == '\'')
        })
    {
        let mut mapped = Vec::with_capacity(words.len());
        let mut any_exception = false;
        for word in &words {
            if let Some(arabic) = latin_arabic_word(&word.to_lowercase()) {
                mapped.push(arabic.to_string());
                any_exception = true;
            } else {
                mapped.push(latin_to_arabic_chars(word, DIGRAPHS));
            }
        }
        if any_exception {
            return mapped.join(" ");
        }
    }
    latin_to_arabic_chars(text, DIGRAPHS)
}

fn latin_to_arabic_chars(text: &str, digraphs: &[(&str, char)]) -> String {
    let mut output = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let rest: String = chars[index..].iter().collect::<String>().to_lowercase();
        let mut matched: Option<(char, usize)> = None;
        for (latin, arab) in digraphs {
            if rest.starts_with(latin) {
                matched = Some((*arab, latin.len()));
                break;
            }
        }
        if let Some((arab, width)) = matched {
            output.push(arab);
            index += width;
            continue;
        }
        let ch = chars[index].to_lowercase().next().unwrap_or(chars[index]);
        let mapped = match ch {
            'a' => Some('ا'),
            'b' => Some('ب'),
            't' => Some('ت'),
            'j' => Some('ج'),
            'h' => Some('ه'),
            'd' => Some('د'),
            'r' => Some('ر'),
            'z' => Some('ز'),
            's' => Some('س'),
            'e' => Some('ي'),
            'i' | 'y' => Some('ي'),
            'f' => Some('ف'),
            'q' => Some('ق'),
            'k' => Some('ك'),
            'l' => Some('ل'),
            'm' => Some('م'),
            'n' => Some('ن'),
            'o' | 'u' | 'w' => Some('و'),
            'v' => Some('ف'),
            'g' => Some('ج'),
            'c' => Some('ك'),
            'x' => None,
            'p' => Some('ب'),
            '\'' => Some('ع'),
            _ => None,
        };
        match mapped {
            Some(arab) => output.push(arab),
            None if ch == 'x' => output.push_str("كس"),
            None => output.push(chars[index]),
        }
        index += 1;
    }
    output
}

fn han_syllable(ch: char) -> Option<&'static str> {
    Some(match ch {
        '你' => "ni",
        '好' => "hao",
        '我' => "wo",
        '是' => "shi",
        '不' => "bu",
        '的' => "de",
        '一' => "yi",
        '了' => "le",
        '在' => "zai",
        '有' => "you",
        '人' => "ren",
        '中' => "zhong",
        '国' => "guo",
        '大' => "da",
        '学' => "xue",
        '爱' => "ai",
        '他' => "ta",
        '她' => "ta",
        '们' => "men",
        '和' => "he",
        '这' => "zhe",
        '那' => "na",
        '个' => "ge",
        '上' => "shang",
        '下' => "xia",
        '来' => "lai",
        '去' => "qu",
        '看' => "kan",
        '很' => "hen",
        '也' => "ye",
        '都' => "dou",
        '就' => "jiu",
        '还' => "hai",
        '要' => "yao",
        '会' => "hui",
        '能' => "neng",
        '可' => "ke",
        '到' => "dao",
        '家' => "jia",
        '年' => "nian",
        '天' => "tian",
        '小' => "xiao",
        '多' => "duo",
        '少' => "shao",
        '朋' => "peng",
        '友' => "you",
        '语' => "yu",
        '文' => "wen",
        '字' => "zi",
        '汉' => "han",
        '吗' => "ma",
        '呢' => "ne",
        '吧' => "ba",
        '啊' => "a",
        '哦' => "o",
        '说' => "shuo",
        '听' => "ting",
        _ => return None,
    })
}

fn han_to_latin(text: &str) -> String {
    let mut syllables = Vec::new();
    for ch in text.chars() {
        if let Some(syllable) = han_syllable(ch) {
            syllables.push(syllable.to_string());
        } else if !ch.is_whitespace() {
            syllables.push(ch.to_string());
        }
    }
    syllables.join(" ")
}

fn latin_syllable_to_han(word: &str) -> Option<char> {
    Some(match word {
        "ni" => '你',
        "hao" => '好',
        "wo" => '我',
        "shi" => '是',
        "bu" => '不',
        "de" => '的',
        "yi" => '一',
        "le" => '了',
        "zai" => '在',
        "you" => '有',
        "ren" => '人',
        "zhong" => '中',
        "guo" => '国',
        "da" => '大',
        "xue" => '学',
        "ai" => '爱',
        "ta" => '他',
        "men" => '们',
        "he" => '和',
        "zhe" => '这',
        "na" => '那',
        "ge" => '个',
        "shang" => '上',
        "xia" => '下',
        "lai" => '来',
        "qu" => '去',
        "kan" => '看',
        "hen" => '很',
        "ye" => '也',
        "dou" => '都',
        "jiu" => '就',
        "hai" => '还',
        "yao" => '要',
        "hui" => '会',
        "neng" => '能',
        "ke" => '可',
        "dao" => '到',
        "jia" => '家',
        "nian" => '年',
        "tian" => '天',
        "xiao" => '小',
        "duo" => '多',
        "shao" => '少',
        "peng" => '朋',
        "yu" => '语',
        "wen" => '文',
        "zi" => '字',
        "han" => '汉',
        "ma" => '吗',
        "ne" => '呢',
        "ba" => '吧',
        "shuo" => '说',
        "ting" => '听',
        _ => return None,
    })
}

fn latin_to_han(text: &str) -> String {
    let mut mapped_any = false;
    let mut output = String::new();
    for word in text.split_whitespace() {
        let folded = word.to_lowercase();
        if let Some(han) = latin_syllable_to_han(&folded) {
            output.push(han);
            mapped_any = true;
        } else {
            output.push_str(word);
        }
    }
    if mapped_any {
        output
    } else {
        text.to_string()
    }
}

/// Confidence-weighted transliteration evidence for a fingerprint pair:
/// raw cross-view string similarity plus the provider confidence behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransliterationEvidence {
    pub similarity: f64,
    pub confidence: f64,
}

impl TransliterationEvidence {
    /// Decision-relevant evidence: raw similarity discounted by how much the
    /// provider trusts its own conversion. Low-confidence rule-based mappings
    /// can no longer produce unconditional 1.0 matches.
    pub fn weighted(&self) -> f64 {
        (self.similarity * self.confidence).clamp(0.0, 1.0)
    }
}

/// Best cross-view evidence over raw texts plus their `transliteration:*`
/// views. Returns `None` when neither side carries a transliteration view, so
/// monolingual pairs report `absent` instead of a misleading score.
/// Confidence is the minimum over the converted views forming the best pair
/// (raw anchors contribute 1.0); several views may match, in which case the
/// most trusted one wins.
pub fn transliteration_evidence(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> Option<TransliterationEvidence> {
    let views_a = a.transliteration_views();
    let views_b = b.transliteration_views();
    if views_a.is_empty() && views_b.is_empty() {
        return None;
    }
    let mut left = vec![(a.raw.as_str(), 1.0)];
    left.extend(views_a);
    let mut right = vec![(b.raw.as_str(), 1.0)];
    right.extend(views_b);
    // Exact cross-view matches short-circuit before edit-distance work.
    let mut exact_confidence: Option<f64> = None;
    for (one, confidence_one) in &left {
        for (other, confidence_other) in &right {
            if !one.is_empty() && one == other {
                let confidence = confidence_one.min(*confidence_other);
                exact_confidence =
                    Some(exact_confidence.map_or(confidence, |best| best.max(confidence)));
            }
        }
    }
    if let Some(confidence) = exact_confidence {
        return Some(TransliterationEvidence {
            similarity: 1.0,
            confidence,
        });
    }
    // Same-script pairs already match through the raw texts; the remaining
    // fuzzy work only pays off for cross-script pairs with weak raw overlap.
    let raw =
        crate::lexical::character::combined_character_similarity(a.raw.as_str(), b.raw.as_str());
    if raw >= 0.5 {
        return Some(TransliterationEvidence {
            similarity: raw,
            confidence: 1.0,
        });
    }
    let mut best = TransliterationEvidence {
        similarity: raw,
        confidence: 1.0,
    };
    for (one, confidence_one) in &left {
        for (other, confidence_other) in &right {
            let similarity = crate::lexical::character::combined_character_similarity(one, other);
            let confidence = confidence_one.min(*confidence_other);
            if similarity > best.similarity
                || (similarity == best.similarity && confidence > best.confidence)
            {
                best = TransliterationEvidence {
                    similarity,
                    confidence,
                };
            }
        }
    }
    Some(best)
}

/// Best cross-view character similarity over raw texts plus their
/// `transliteration:*` views. Returns `None` when neither side carries a
/// transliteration view, so monolingual pairs report `absent` instead of a
/// misleading score. This is the raw string similarity; see
/// [`transliteration_evidence`] for the confidence-weighted evidence used in
/// scoring decisions.
pub fn transliteration_similarity(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> Option<f64> {
    transliteration_evidence(a, b).map(|evidence| evidence.similarity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latin_views(text: &str) -> Vec<String> {
        RuleBasedTransliterationProvider
            .transliterate(text)
            .into_iter()
            .filter(|view| view.target_script == "Latn")
            .map(|view| view.text)
            .collect()
    }

    #[test]
    fn russian_round_trip() {
        assert_eq!(cyrillic_to_latin("привет"), "privet");
        assert_eq!(latin_to_cyrillic("privet"), "привет");
        assert_eq!(cyrillic_to_latin("Москва"), "Moskva");
    }

    #[test]
    fn arabic_links_salam() {
        // Short vowels are unwritten: سلام folds to `slam`, while the
        // reverse direction links `salam` back to `سلام` exactly.
        assert_eq!(arabic_to_latin("سلام"), "slam");
        assert_eq!(latin_to_arabic("salam"), "سلام");
    }

    #[test]
    fn chinese_links_ni_hao() {
        assert_eq!(han_to_latin("你好"), "ni hao");
        assert_eq!(latin_to_han("ni hao"), "你好");
    }

    #[test]
    fn views_emit_only_for_covered_scripts() {
        assert!(RuleBasedTransliterationProvider
            .transliterate("hello world")
            .iter()
            .any(|view| view.target_script == "Cyrl"));
        // Pure ASCII gets no Latin view (nothing changed).
        assert!(latin_views("hello world").is_empty());
        // Emoji-only input yields no views rather than junk.
        assert!(RuleBasedTransliterationProvider
            .transliterate("🏠🔥")
            .is_empty());
    }

    #[test]
    fn unknown_characters_pass_through() {
        // `e` has no single Arabic letter; tables document approximations
        // and never drop input silently.
        assert!(!latin_to_arabic("hello").is_empty());
        assert_eq!(cyrillic_to_latin("привет!"), "privet!");
    }
}
