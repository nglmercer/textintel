use std::collections::BTreeMap;

pub fn leet_map() -> BTreeMap<char, Vec<&'static str>> {
    [
        ('0', vec!["o", "0"]),
        ('1', vec!["i", "l", "1"]),
        ('2', vec!["z", "2"]),
        ('3', vec!["e", "3"]),
        ('4', vec!["a", "4"]),
        ('5', vec!["s", "5"]),
        ('6', vec!["g", "6"]),
        ('7', vec!["t", "7"]),
        ('8', vec!["b", "8"]),
        ('9', vec!["g", "9"]),
        ('@', vec!["a"]),
        ('$', vec!["s"]),
        ('!', vec!["i"]),
    ]
    .into_iter()
    .collect()
}

pub fn detect_leet(text: &str) -> bool {
    let map = leet_map();
    let chars: Vec<char> = text.chars().collect();
    chars.iter().enumerate().any(|(index, ch)| {
        if !map.contains_key(ch) || !ch.is_ascii_digit() {
            return false;
        }
        let left = index.checked_sub(1).and_then(|i| chars.get(i));
        let right = chars.get(index + 1);
        left.is_some_and(|value| value.is_alphabetic())
            || right.is_some_and(|value| value.is_alphabetic())
    })
}

/// Greedy leetspeak is an additional matching view, never a replacement for
/// the raw input.
pub fn apply_leet(text: &str) -> String {
    let map = leet_map();
    text.chars()
        .map(|ch| {
            map.get(&ch)
                .and_then(|readings| readings.first())
                .and_then(|reading| reading.chars().next())
                .unwrap_or(ch)
        })
        .collect()
}
