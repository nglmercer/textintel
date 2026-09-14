pub fn is_extra_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{180e}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

pub fn normalize_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() || is_extra_whitespace(ch) {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        output.push(ch);
    }
    output
}
