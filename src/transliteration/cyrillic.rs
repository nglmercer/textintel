//! Cyrillic ↔ Latin rule tables (Russian-biased).

/// Case-match a mapped fragment into the output buffer: lowercase maps
/// pass through, uppercase inputs uppercase the first scalar. Identical
/// bytes to the old per-char `String` helper, no allocation per char.
fn push_case(output: &mut String, mapped: &str, upper: bool) {
    if !upper {
        output.push_str(mapped);
    } else {
        let mut chars = mapped.chars();
        if let Some(first) = chars.next() {
            for uppered in first.to_uppercase() {
                output.push(uppered);
            }
            output.push_str(chars.as_str());
        }
    }
}

pub(crate) fn cyrillic_to_latin(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
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
            _ => {
                output.push(ch);
                continue;
            }
        };
        push_case(&mut output, mapped, ch.is_uppercase());
    }
    output
}

pub(crate) fn latin_to_cyrillic(text: &str) -> String {
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
    let key_len = crate::transliteration::table_key_len(DIGRAPHS);
    let mut index = 0;
    while index < chars.len() {
        // ASCII windows match by byte (no suffix alloc); non-ASCII
        // windows keep the legacy lowered-suffix check verbatim.
        let window_end = (index + key_len).min(chars.len());
        let matched = if chars[index..window_end].iter().all(|ch| ch.is_ascii()) {
            crate::transliteration::ascii_table_match(&chars, index, DIGRAPHS)
                .map(|position| (DIGRAPHS[position].1, DIGRAPHS[position].0.len()))
        } else {
            let rest: String = chars[index..].iter().collect::<String>().to_lowercase();
            let mut found: Option<(&str, usize)> = None;
            for (latin, cyrl) in DIGRAPHS {
                if rest.starts_with(latin) {
                    found = Some((cyrl, latin.len()));
                    break;
                }
            }
            found
        };
        if let Some((cyrl, width)) = matched {
            push_case(&mut output, cyrl, chars[index].is_uppercase());
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
        push_case(&mut output, mapped, ch.is_uppercase());
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_round_trip() {
        assert_eq!(cyrillic_to_latin("привет"), "privet");
        assert_eq!(latin_to_cyrillic("privet"), "привет");
        assert_eq!(cyrillic_to_latin("Москва"), "Moskva");
    }
}
