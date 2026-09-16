//! Devanagari → Latin (Hunterian-style, schwa handling).

/// Devanagari independent vowel or consonant with its inherent schwa.
/// Matras, halant, and nukta resolve with context in
/// [`devanagari_to_latin`].
fn devanagari_base(ch: char) -> Option<&'static str> {
    Some(match ch {
        'अ' => "a",
        'आ' => "a",
        'इ' => "i",
        'ई' => "i",
        'उ' => "u",
        'ऊ' => "u",
        'ऋ' | 'ॠ' => "ri",
        'ऌ' => "lri",
        'ए' | 'ऍ' => "e",
        'ऐ' => "ai",
        'ऑ' | 'ऒ' | 'ओ' => "o",
        'औ' => "au",
        'क' => "ka",
        'ख' => "kha",
        'ग' => "ga",
        'घ' => "gha",
        'ङ' => "nga",
        'च' => "cha",
        'छ' => "chha",
        'ज' => "ja",
        'झ' => "jha",
        'ञ' => "nya",
        'ट' => "ta",
        'ठ' => "tha",
        'ड' => "da",
        'ढ' => "dha",
        'ण' => "na",
        'त' => "ta",
        'थ' => "tha",
        'द' => "da",
        'ध' => "dha",
        'न' => "na",
        'प' => "pa",
        'फ' => "pha",
        'ब' => "ba",
        'भ' => "bha",
        'म' => "ma",
        'य' => "ya",
        'र' => "ra",
        'ल' => "la",
        'व' => "va",
        'श' | 'ष' => "sha",
        'स' => "sa",
        'ह' => "ha",
        'ळ' => "la",
        '\u{958}' => "qa",
        '\u{959}' => "kha",
        '\u{95a}' => "gha",
        '\u{95b}' => "za",
        '\u{95c}' => "ra",
        '\u{95d}' => "rha",
        '\u{95e}' => "fa",
        '\u{95f}' => "ya",
        _ => return None,
    })
}

/// Devanagari dependent vowel sign (matra), replacing the pending
/// consonant's inherent schwa. Long vowels fold to short (Hunterian-style)
/// so views link with colloquial romanization (`धन्यवाद` → `dhanyavad`).
fn devanagari_matra(ch: char) -> Option<&'static str> {
    Some(match ch {
        'ा' => "a",
        'ि' => "i",
        'ी' => "i",
        'ु' => "u",
        'ू' => "u",
        'ृ' | 'ॄ' => "ri",
        'ॅ' | 'ॆ' | 'े' => "e",
        'ै' => "ai",
        'ॉ' | 'ो' => "o",
        'ौ' => "au",
        _ => return None,
    })
}

pub(crate) fn devanagari_to_latin(text: &str) -> String {
    let mut output = String::new();
    // Consonant awaiting a matra, halant, or release with its schwa.
    let mut pending: Option<&'static str> = None;
    // Output length where the current word started: word-final schwa
    // deletes (Hindi `कमल` → `kamal`), but a lone consonant keeps its
    // vowel (`क` → `ka`).
    let mut word_start = 0;
    let flush = |pending: &mut Option<&'static str>, output: &mut String| {
        if let Some(consonant) = pending.take() {
            output.push_str(consonant);
        }
    };
    let flush_word_end =
        |pending: &mut Option<&'static str>, output: &mut String, word_start: usize| {
            if let Some(consonant) = pending.take() {
                if output.len() > word_start {
                    output.push_str(consonant.strip_suffix('a').unwrap_or(consonant));
                } else {
                    output.push_str(consonant);
                }
            }
        };
    for ch in text.chars() {
        if let Some(matra) = devanagari_matra(ch) {
            match pending.take() {
                Some(consonant) => {
                    output.push_str(consonant.strip_suffix('a').unwrap_or(consonant));
                    output.push_str(matra);
                }
                // Stray matra with no consonant: read the vowel itself.
                None => output.push_str(matra),
            }
            continue;
        }
        // Halant kills the pending schwa (`न्` → `n`).
        if ch == '्' {
            if let Some(consonant) = pending.take() {
                output.push_str(consonant.strip_suffix('a').unwrap_or(consonant));
            }
            continue;
        }
        // Nukta remaps the pending consonant (`क` + `़` → `qa`).
        if ch == '़' {
            pending = match pending {
                Some("ka") => Some("qa"),
                Some("ga") => Some("gha"),
                Some("ja") => Some("za"),
                Some("pha") => Some("fa"),
                Some("da") => Some("ra"),
                Some("dha") => Some("rha"),
                other => other,
            };
            continue;
        }
        if ch == 'ं' || ch == 'ँ' {
            flush(&mut pending, &mut output);
            output.push('n');
            continue;
        }
        if ch == 'ः' {
            flush(&mut pending, &mut output);
            output.push('h');
            continue;
        }
        if ch == 'ऽ' {
            flush(&mut pending, &mut output);
            output.push('\'');
            continue;
        }
        if ('०'..='९').contains(&ch) {
            flush(&mut pending, &mut output);
            let digit = '0' as u32 + (ch as u32 - '०' as u32);
            output.push(char::from_u32(digit).unwrap_or(ch));
            continue;
        }
        if let Some(base) = devanagari_base(ch) {
            // Independent vowels emit at once; consonants wait for a
            // possible matra, halant, or nukta.
            let is_consonant = base.ends_with('a') && ch >= 'क';
            if is_consonant {
                flush(&mut pending, &mut output);
                pending = Some(base);
            } else {
                flush(&mut pending, &mut output);
                output.push_str(base);
            }
            continue;
        }
        flush_word_end(&mut pending, &mut output, word_start);
        output.push(ch);
        if ch.is_whitespace() {
            word_start = output.len();
        }
    }
    flush_word_end(&mut pending, &mut output, word_start);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devanagari_to_colloquial_latin() {
        assert_eq!(devanagari_to_latin("धन्यवाद"), "dhanyavad");
        assert_eq!(devanagari_to_latin("नमस्ते"), "namaste");
        assert_eq!(devanagari_to_latin("हिन्दी"), "hindi");
        assert_eq!(devanagari_to_latin("क़लम"), "qalam");
        // Word-final schwa deletes (Hindi); medial schwas stay
        // (documented limit), and a lone consonant keeps its vowel.
        assert_eq!(devanagari_to_latin("कमल"), "kamal");
        assert_eq!(devanagari_to_latin("क"), "ka");
        // Lossy by design (Hunterian-style): long vowels fold to short,
        // so `काल` (time) and `कल` (yesterday) share one view; pairs
        // still link through the shared form.
        assert_eq!(devanagari_to_latin("काल"), "kal");
        assert_eq!(devanagari_to_latin("कल"), "kal");
    }
}
