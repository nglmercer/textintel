use unicode_segmentation::UnicodeSegmentation;

fn is_emoji_cluster(cluster: &str) -> bool {
    cluster.chars().any(|ch| {
        let code = ch as u32;
        (0x1f000..=0x1faff).contains(&code)
            || (0x2600..=0x27bf).contains(&code)
            || matches!(ch, '❤' | '⭐')
    })
}

/// Tokenize without losing emoji, Unicode words, URLs, or punctuation.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for (_, cluster) in text.grapheme_indices(true) {
        if cluster.trim().is_empty() {
            continue;
        }
        if is_emoji_cluster(cluster) {
            tokens.push(cluster.to_string());
        } else if cluster.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\'') {
            // The common case is handled below by a Unicode-aware run parser;
            // this branch is only useful for apostrophes in a grapheme.
            tokens.push(cluster.to_string());
        } else {
            tokens.push(cluster.to_string());
        }
    }

    // Merge adjacent word/digit graphemes while leaving emoji and punctuation
    // as independent evidence. URLs remain as one token because their dots and
    // slashes are meaningful to spam and search features.
    let mut merged = Vec::new();
    let mut index = 0;
    let chars: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    while index < chars.len() {
        let (_, cluster) = chars[index];
        if cluster.trim().is_empty() {
            index += 1;
            continue;
        }
        if is_emoji_cluster(cluster) {
            merged.push(cluster.to_string());
            index += 1;
            continue;
        }
        let is_wordish = cluster.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\'');
        if is_wordish {
            let start = index;
            index += 1;
            while index < chars.len() {
                let next = chars[index].1;
                if next.trim().is_empty() || is_emoji_cluster(next) {
                    break;
                }
                if !next.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\'') {
                    break;
                }
                index += 1;
            }
            merged.push(chars[start..index].iter().map(|(_, part)| *part).collect());
        } else {
            merged.push(cluster.to_string());
            index += 1;
        }
    }

    // Reconstitute URL-like spans so `https://…` is not fragmented.
    let mut output = Vec::new();
    let mut i = 0;
    while i < merged.len() {
        if (merged[i] == "http" || merged[i] == "https")
            && merged.get(i + 1).is_some_and(|part| part == ":")
            && merged.get(i + 2).is_some_and(|part| part == "/")
            && merged.get(i + 3).is_some_and(|part| part == "/")
        {
            let mut url = String::new();
            let mut j = i;
            while j < merged.len() && !merged[j].chars().all(char::is_whitespace) {
                url.push_str(&merged[j]);
                j += 1;
                if j < merged.len() && merged[j] == "@" {
                    break;
                }
                if j < merged.len() && merged[j].len() == 1 && ",!?;".contains(&merged[j]) {
                    break;
                }
            }
            output.push(url);
            i = j;
        } else {
            output.push(merged[i].clone());
            i += 1;
        }
    }
    output
}

pub fn simple_lemmas(tokens: &[String]) -> Vec<String> {
    let suffixes = ["ing", "ed", "es", "s", "mente", "cion", "ción", "ando", "iendo"];
    tokens
        .iter()
        .map(|token| {
            let mut value: String = token.chars().flat_map(char::to_lowercase).collect();
            if value.chars().all(char::is_alphabetic) && value.chars().count() > 5 {
                for suffix in suffixes {
                    if value.ends_with(suffix) && value.chars().count() - suffix.chars().count() >= 3 {
                        value.truncate(value.len() - suffix.len());
                        break;
                    }
                }
            }
            value
        })
        .collect()
}

pub fn is_emoji(ch: char) -> bool {
    let code = ch as u32;
    (0x1f000..=0x1faff).contains(&code)
        || (0x2600..=0x27bf).contains(&code)
        || matches!(ch, '❤' | '⭐')
}

