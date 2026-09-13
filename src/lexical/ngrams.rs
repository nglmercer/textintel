pub fn word_ngrams(tokens: &[String], n: usize) -> Vec<String> {
    if n <= 1 {
        return tokens.to_vec();
    }
    tokens.windows(n).map(|window| window.join(" ")).collect()
}

pub fn character_ngrams(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if n == 0 || chars.len() < n {
        return Vec::new();
    }
    chars
        .windows(n)
        .map(|window| window.iter().collect())
        .collect()
}
