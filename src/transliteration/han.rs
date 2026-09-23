//! Han ↔ Latin pinyin-style table (common characters).

fn han_syllable(ch: char) -> Option<&'static str> {
    Some(match ch {
        '你' => "ni",
        '好' => "hao",
        '我' => "wo",
        '是' => "shi",
        '不' => "bu",
        '的' => "de",
        '一' => "yi",
        '了' => "le",
        '在' => "zai",
        '有' => "you",
        '人' => "ren",
        '中' => "zhong",
        '国' => "guo",
        '大' => "da",
        '学' => "xue",
        '爱' => "ai",
        '他' => "ta",
        '她' => "ta",
        '们' => "men",
        '和' => "he",
        '这' => "zhe",
        '那' => "na",
        '个' => "ge",
        '上' => "shang",
        '下' => "xia",
        '来' => "lai",
        '去' => "qu",
        '看' => "kan",
        '很' => "hen",
        '也' => "ye",
        '都' => "dou",
        '就' => "jiu",
        '还' => "hai",
        '要' => "yao",
        '会' => "hui",
        '能' => "neng",
        '可' => "ke",
        '到' => "dao",
        '家' => "jia",
        '年' => "nian",
        '天' => "tian",
        '小' => "xiao",
        '多' => "duo",
        '少' => "shao",
        '朋' => "peng",
        '友' => "you",
        '语' => "yu",
        '文' => "wen",
        '字' => "zi",
        '汉' => "han",
        '吗' => "ma",
        '呢' => "ne",
        '吧' => "ba",
        '啊' => "a",
        '哦' => "o",
        '说' => "shuo",
        '听' => "ting",
        '谢' => "xie",
        '见' => "jian",
        '再' => "zai",
        '吃' => "chi",
        '喝' => "he",
        '老' => "lao",
        '师' => "shi",
        '生' => "sheng",
        '现' => "xian",
        '时' => "shi",
        '分' => "fen",
        '钟' => "zhong",
        '月' => "yue",
        '日' => "ri",
        '星' => "xing",
        '期' => "qi",
        '号' => "hao",
        '钱' => "qian",
        '买' => "mai",
        '卖' => "mai",
        '店' => "dian",
        '车' => "che",
        '站' => "zhan",
        '饭' => "fan",
        '菜' => "cai",
        '茶' => "cha",
        '水' => "shui",
        '火' => "huo",
        '木' => "mu",
        '金' => "jin",
        '行' => "xing",
        '走' => "zou",
        '跑' => "pao",
        '笑' => "xiao",
        '高' => "gao",
        '短' => "duan",
        '新' => "xin",
        '旧' => "jiu",
        '热' => "re",
        '冷' => "leng",
        '快' => "kuai",
        '慢' => "man",
        '对' => "dui",
        '错' => "cuo",
        '开' => "kai",
        '关' => "guan",
        '东' => "dong",
        '南' => "nan",
        '西' => "xi",
        '北' => "bei",
        '每' => "mei",
        '些' => "xie",
        _ => return None,
    })
}

pub(crate) fn han_to_latin(text: &str) -> String {
    // Syllables joined by single spaces, as `Vec::join(" ")` — without
    // the per-character `String`s. Same bytes.
    let mut output = String::with_capacity(text.len());
    let mut first = true;
    for ch in text.chars() {
        let syllable = han_syllable(ch);
        if syllable.is_none() && ch.is_whitespace() {
            continue;
        }
        if !first {
            output.push(' ');
        }
        match syllable {
            Some(syllable) => output.push_str(syllable),
            None => output.push(ch),
        }
        first = false;
    }
    output
}

fn latin_syllable_to_han(word: &str) -> Option<char> {
    Some(match word {
        "ni" => '你',
        "hao" => '好',
        "wo" => '我',
        "shi" => '是',
        "bu" => '不',
        "de" => '的',
        "yi" => '一',
        "le" => '了',
        "zai" => '在',
        "you" => '有',
        "ren" => '人',
        "zhong" => '中',
        "guo" => '国',
        "da" => '大',
        "xue" => '学',
        "ai" => '爱',
        "ta" => '他',
        "men" => '们',
        "he" => '和',
        "zhe" => '这',
        "na" => '那',
        "ge" => '个',
        "shang" => '上',
        "xia" => '下',
        "lai" => '来',
        "qu" => '去',
        "kan" => '看',
        "hen" => '很',
        "ye" => '也',
        "dou" => '都',
        "jiu" => '就',
        "hai" => '还',
        "yao" => '要',
        "hui" => '会',
        "neng" => '能',
        "ke" => '可',
        "dao" => '到',
        "jia" => '家',
        "nian" => '年',
        "tian" => '天',
        "xiao" => '小',
        "duo" => '多',
        "shao" => '少',
        "peng" => '朋',
        "yu" => '语',
        "wen" => '文',
        "zi" => '字',
        "han" => '汉',
        "ma" => '吗',
        "ne" => '呢',
        "ba" => '吧',
        "shuo" => '说',
        "ting" => '听',
        "xie" => '谢',
        "jian" => '见',
        "chi" => '吃',
        "lao" => '老',
        "sheng" => '生',
        "xian" => '现',
        "fen" => '分',
        "yue" => '月',
        "ri" => '日',
        "qi" => '期',
        "qian" => '钱',
        "mai" => '买',
        "dian" => '店',
        "che" => '车',
        "zhan" => '站',
        "fan" => '饭',
        "cai" => '菜',
        "cha" => '茶',
        "shui" => '水',
        "huo" => '火',
        "mu" => '木',
        "jin" => '金',
        "zou" => '走',
        "pao" => '跑',
        "gao" => '高',
        "duan" => '短',
        "xin" => '新',
        "re" => '热',
        "leng" => '冷',
        "kuai" => '快',
        "man" => '慢',
        "dui" => '对',
        "cuo" => '错',
        "kai" => '开',
        "guan" => '关',
        "dong" => '东',
        "nan" => '南',
        "xi" => '西',
        "bei" => '北',
        "mei" => '每',
        _ => return None,
    })
}

pub(crate) fn latin_to_han(text: &str) -> String {
    let mut mapped_any = false;
    let mut output = String::new();
    for word in text.split_whitespace() {
        let folded = word.to_lowercase();
        if let Some(han) = latin_syllable_to_han(&folded) {
            output.push(han);
            mapped_any = true;
        } else {
            output.push_str(word);
        }
    }
    if mapped_any { output } else { text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_links_ni_hao() {
        assert_eq!(han_to_latin("你好"), "ni hao");
        assert_eq!(latin_to_han("ni hao"), "你好");
    }

    #[test]
    fn chinese_covers_common_words() {
        assert_eq!(han_to_latin("谢谢"), "xie xie");
        assert_eq!(latin_to_han("xie xie"), "谢谢");
        assert_eq!(han_to_latin("再见"), "zai jian");
        // Lossy by design: `zai` maps to the more common 在, so the
        // Latin→Han direction renders 在见; pairs still link through the
        // Han→Latin view.
        assert_eq!(latin_to_han("zai jian"), "在见");
        assert_eq!(han_to_latin("中国"), "zhong guo");
        assert_eq!(han_to_latin("吃"), "chi");
        assert_eq!(latin_to_han("chi"), "吃");
    }
}
