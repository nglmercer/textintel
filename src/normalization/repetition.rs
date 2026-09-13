pub fn collapse_repetition(text: &str, keep: usize) -> String {
    if text.is_empty() {
        return String::new();
    }
    let keep = keep.max(1);
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let mut end = index + 1;
        while end < chars.len() && chars[end] == ch {
            end += 1;
        }
        let run = end - index;
        if run >= 3 && (ch.is_alphanumeric() || matches!(ch, '!' | '?' | '.')) {
            for _ in 0..keep {
                output.push(ch);
            }
        } else {
            for _ in index..end {
                output.push(ch);
            }
        }
        index = end;
    }
    output
}

pub fn repetition_ratio(text: &str) -> f64 {
    let length = text.chars().count();
    if length == 0 {
        return 0.0;
    }
    let collapsed = collapse_repetition(text, 1).chars().count();
    ((length.saturating_sub(collapsed)) as f64 / length as f64).min(1.0)
}

