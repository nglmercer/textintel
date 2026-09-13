use crate::core::types::{ObfuscationFeatures, UnicodeFeatures};
use crate::normalization::leetspeak::detect_leet;
use crate::normalization::repetition::repetition_ratio;

fn punctuation_flood(text: &str) -> bool {
    let mut run = 0;
    for ch in text.chars() {
        if matches!(ch, '!' | '?' | '.') {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn uppercase_ratio(text: &str) -> f64 {
    let letters = text.chars().filter(|ch| ch.is_alphabetic()).count();
    let uppercase = text.chars().filter(|ch| ch.is_uppercase()).count();
    if letters == 0 {
        0.0
    } else {
        uppercase as f64 / letters as f64
    }
}

pub fn obfuscation_features(text: &str, unicode: &UnicodeFeatures) -> ObfuscationFeatures {
    let leetspeak = detect_leet(text);
    let repetition_score = repetition_ratio(text);
    let repetition = repetition_score >= 0.08;
    let flood = punctuation_flood(text);
    let mixed_scripts = unicode.mixed_scripts;
    let confusables = !unicode.confusable_characters.is_empty();
    let emoji_count = text
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            (0x1f000..=0x1faff).contains(&code) || (0x2600..=0x27bf).contains(&code)
        })
        .count();
    let letters = text.chars().filter(|ch| ch.is_alphabetic()).count();
    let digits = text.chars().filter(|ch| ch.is_numeric()).count();
    let fragmentation_score = if letters > 0 && (digits > 0 || emoji_count > 0) {
        ((digits + emoji_count) as f64 / letters as f64).min(1.0)
    } else {
        0.0
    };
    let symbol_substitution_score = if emoji_count > 0 && letters > 0 {
        0.35
    } else {
        0.0
    };
    let leet_score = if leetspeak {
        (digits as f64 / letters.max(1) as f64).clamp(0.35, 1.0)
    } else {
        0.0
    };
    let homoglyph_score = if confusables {
        (unicode.confusable_characters.len() as f64 * 0.35).min(1.0)
    } else {
        0.0
    };
    let unicode_score = unicode.suspicious_unicode_score;

    let mut flags = Vec::new();
    if leetspeak {
        flags.push("leetspeak".to_string());
    }
    if repetition {
        flags.push("repetition".to_string());
    }
    if flood {
        flags.push("punctuation_flood".to_string());
    }
    if mixed_scripts {
        flags.push("mixed_scripts".to_string());
    }
    if confusables {
        flags.push("confusables".to_string());
    }
    if !unicode.invisible_characters.is_empty() {
        flags.push("invisible_characters".to_string());
    }
    if !unicode.bidirectional_controls.is_empty() {
        flags.push("bidirectional_controls".to_string());
    }
    if fragmentation_score >= 0.2 {
        flags.push("fragmentation".to_string());
    }
    if uppercase_ratio(text) > 0.9 && letters >= 4 {
        flags.push("uppercase_flood".to_string());
    }

    let mut score = 0.0;
    score += leet_score * 0.35;
    score += repetition_score.min(1.0) * 0.25;
    score += if flood { 0.15 } else { 0.0 };
    score += if mixed_scripts { 0.2 } else { 0.0 };
    score += homoglyph_score * 0.25;
    score += unicode_score * 0.25;
    score += fragmentation_score * 0.15;
    score += symbol_substitution_score * 0.15;
    score = score.min(1.0);
    ObfuscationFeatures {
        detected: score >= 0.25 || !flags.is_empty(),
        score,
        leet_score,
        homoglyph_score,
        unicode_score,
        fragmentation_score,
        symbol_substitution_score,
        repetition_score,
        leetspeak,
        repetition,
        punctuation_flood: flood,
        mixed_scripts,
        confusables,
        flags,
    }
}
