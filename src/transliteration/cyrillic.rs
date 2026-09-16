//! Cyrillic ↔ Latin rule tables (Russian-biased).

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

pub(crate) fn cyrillic_to_latin(text: &str) -> String {
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
