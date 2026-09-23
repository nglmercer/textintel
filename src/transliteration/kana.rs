//! Japanese kana → Latin (Hepburn, no foreign contractions).

/// Hepburn romaji for one kana: the hiragana and katakana syllabaries
/// share readings. Small ya/yu/yo, the sokuon, the chōonpu, and syllabic ん
/// resolve with context in [`kana_to_latin`]; standalone small kana read
/// plainly.
fn kana_base(ch: char) -> Option<&'static str> {
    Some(match ch {
        'あ' | 'ア' | 'ぁ' | 'ァ' => "a",
        'い' | 'イ' | 'ぃ' | 'ィ' => "i",
        'う' | 'ウ' | 'ぅ' | 'ゥ' => "u",
        'え' | 'エ' | 'ぇ' | 'ェ' => "e",
        'お' | 'オ' | 'ぉ' | 'ォ' => "o",
        'か' | 'カ' | 'ヵ' => "ka",
        'き' | 'キ' => "ki",
        'く' | 'ク' => "ku",
        'け' | 'ケ' | 'ヶ' => "ke",
        'こ' | 'コ' => "ko",
        'さ' | 'サ' => "sa",
        'し' | 'シ' => "shi",
        'す' | 'ス' => "su",
        'せ' | 'セ' => "se",
        'そ' | 'ソ' => "so",
        'た' | 'タ' => "ta",
        'ち' | 'チ' => "chi",
        'つ' | 'ツ' => "tsu",
        'て' | 'テ' => "te",
        'と' | 'ト' => "to",
        'な' | 'ナ' => "na",
        'に' | 'ニ' => "ni",
        'ぬ' | 'ヌ' => "nu",
        'ね' | 'ネ' => "ne",
        'の' | 'ノ' => "no",
        'は' | 'ハ' => "ha",
        'ひ' | 'ヒ' => "hi",
        'ふ' | 'フ' => "fu",
        'へ' | 'ヘ' => "he",
        'ほ' | 'ホ' => "ho",
        'ま' | 'マ' => "ma",
        'み' | 'ミ' => "mi",
        'む' | 'ム' => "mu",
        'め' | 'メ' => "me",
        'も' | 'モ' => "mo",
        'や' | 'ヤ' | 'ゃ' | 'ャ' => "ya",
        'ゆ' | 'ユ' | 'ゅ' | 'ュ' => "yu",
        'よ' | 'ヨ' | 'ょ' | 'ョ' => "yo",
        'ら' | 'ラ' => "ra",
        'り' | 'リ' => "ri",
        'る' | 'ル' => "ru",
        'れ' | 'レ' => "re",
        'ろ' | 'ロ' => "ro",
        'わ' | 'ワ' | 'ゎ' | 'ヮ' => "wa",
        'ゐ' | 'ヰ' => "wi",
        'ゑ' | 'ヱ' => "we",
        'を' | 'ヲ' => "wo",
        'ん' | 'ン' => "n",
        'が' | 'ガ' => "ga",
        'ぎ' | 'ギ' => "gi",
        'ぐ' | 'グ' => "gu",
        'げ' | 'ゲ' => "ge",
        'ご' | 'ゴ' => "go",
        'ざ' | 'ザ' => "za",
        'じ' | 'ジ' | 'ぢ' | 'ヂ' => "ji",
        'ず' | 'ズ' | 'づ' | 'ヅ' => "zu",
        'ぜ' | 'ゼ' => "ze",
        'ぞ' | 'ゾ' => "zo",
        'だ' | 'ダ' => "da",
        'で' | 'デ' => "de",
        'ど' | 'ド' => "do",
        'ば' | 'バ' => "ba",
        'び' | 'ビ' => "bi",
        'ぶ' | 'ブ' => "bu",
        'べ' | 'ベ' => "be",
        'ぼ' | 'ボ' => "bo",
        'ぱ' | 'パ' => "pa",
        'ぴ' | 'ピ' => "pi",
        'ぷ' | 'プ' => "pu",
        'ぺ' | 'ペ' => "pe",
        'ぽ' | 'ポ' => "po",
        'ヴ' => "vu",
        _ => return None,
    })
}

/// Initial letter of a kana mora for sokuon doubling, resolving one yoon
/// contraction so `っきゃ` doubles to `kk…`.
fn kana_onset(ch: char, following: Option<char>) -> Option<char> {
    let base = kana_base(ch)?;
    let contracted = base.ends_with('i')
        && following.is_some_and(|next| matches!(next, 'ゃ' | 'ャ' | 'ゅ' | 'ュ' | 'ょ' | 'ョ'));
    let head = if contracted {
        base.strip_suffix('i').unwrap_or(base)
    } else {
        base
    };
    head.chars().next().filter(|c| c.is_ascii_alphabetic())
}

pub(crate) fn kana_to_latin(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        // Sokuon: double the next mora's initial consonant.
        if ch == 'っ' || ch == 'ッ' {
            if let Some(onset) = chars
                .get(index + 1)
                .and_then(|next| kana_onset(*next, chars.get(index + 2).copied()))
            {
                output.push(onset);
            }
            index += 1;
            continue;
        }
        // Chōonpu: lengthen the previous vowel.
        if ch == 'ー' {
            if let Some(vowel) = output
                .chars()
                .next_back()
                .filter(|c| matches!(c, 'a' | 'i' | 'u' | 'e' | 'o'))
            {
                output.push(vowel);
            }
            index += 1;
            continue;
        }
        // Syllabic ん assimilates to `m` before bilabials.
        if ch == 'ん' || ch == 'ン' {
            let labial = chars
                .get(index + 1)
                .and_then(|next| kana_base(*next))
                .and_then(|romaji| romaji.chars().next())
                .is_some_and(|initial| matches!(initial, 'b' | 'p' | 'm'));
            output.push_str(if labial { "m" } else { "n" });
            index += 1;
            continue;
        }
        let Some(base) = kana_base(ch) else {
            output.push(ch);
            index += 1;
            continue;
        };
        // Yoon: an -i kana plus small ya/yu/yo contracts (`きゃ` → `kya`);
        // sibilant stems absorb the glide (`しゃ` → `sha`, not `shya`).
        if base.ends_with('i')
            && let Some(small) = chars.get(index + 1)
        {
            let glide = match small {
                'ゃ' | 'ャ' => Some("ya"),
                'ゅ' | 'ュ' => Some("yu"),
                'ょ' | 'ョ' => Some("yo"),
                _ => None,
            };
            if let Some(glide) = glide {
                let stem = base.strip_suffix('i').unwrap_or(base);
                if !stem.is_empty() {
                    output.push_str(stem);
                    let contracted = stem.ends_with("sh") || stem.ends_with("ch") || stem == "j";
                    output.push_str(if contracted { &glide[1..] } else { glide });
                    index += 2;
                    continue;
                }
            }
        }
        output.push_str(base);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_kana_to_hepburn() {
        assert_eq!(kana_to_latin("ありがとう"), "arigatou");
        assert_eq!(kana_to_latin("コーヒー"), "koohii");
        assert_eq!(kana_to_latin("がっこう"), "gakkou");
        assert_eq!(kana_to_latin("しゃしん"), "shashin");
        assert_eq!(kana_to_latin("きゃく"), "kyaku");
        assert_eq!(kana_to_latin("せんぱい"), "sempai");
        assert_eq!(kana_to_latin("さんぽ"), "sampo");
        // Foreign-word contractions stay approximate (documented limit).
        assert_eq!(kana_to_latin("ファ"), "fua");
    }
}
