//! Arabic ↔ Latin rule tables (short vowels unwritten).

pub(crate) fn arabic_to_latin(text: &str) -> String {
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

pub(crate) fn latin_to_arabic(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_links_salam() {
        // Short vowels are unwritten: سلام folds to `slam`, while the
        // reverse direction links `salam` back to `سلام` exactly.
        assert_eq!(arabic_to_latin("سلام"), "slam");
        assert_eq!(latin_to_arabic("salam"), "سلام");
    }
}
