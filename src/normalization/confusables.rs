use std::collections::BTreeMap;

/// A deliberately small, auditable confusable table.  It focuses on common
/// cross-script spoofing and full-width forms; providers can add larger UTS-39
/// data without changing the visual API.
pub fn confusable_map() -> BTreeMap<char, char> {
    [
        ('а', 'a'),
        ('е', 'e'),
        ('о', 'o'),
        ('р', 'p'),
        ('с', 'c'),
        ('у', 'y'),
        ('х', 'x'),
        ('і', 'i'),
        ('ј', 'j'),
        ('ѕ', 's'),
        ('һ', 'h'),
        ('Α', 'A'),
        ('Β', 'B'),
        ('Ε', 'E'),
        ('Ζ', 'Z'),
        ('Η', 'H'),
        ('Ι', 'I'),
        ('Κ', 'K'),
        ('Μ', 'M'),
        ('Ν', 'N'),
        ('Ο', 'O'),
        ('Ρ', 'P'),
        ('Τ', 'T'),
        ('Υ', 'Y'),
        ('Χ', 'X'),
        ('α', 'a'),
        ('ν', 'v'),
        ('τ', 't'),
        ('υ', 'u'),
        ('Ⅰ', 'I'),
        ('А', 'A'),
        ('В', 'B'),
        ('Е', 'E'),
        ('К', 'K'),
        ('М', 'M'),
        ('Н', 'H'),
        ('О', 'O'),
        ('Р', 'P'),
        ('С', 'C'),
        ('Т', 'T'),
        ('Х', 'X'),
        ('０', '0'),
        ('１', '1'),
        ('２', '2'),
        ('３', '3'),
        ('４', '4'),
        ('５', '5'),
        ('６', '6'),
        ('７', '7'),
        ('８', '8'),
        ('９', '9'),
        ('ａ', 'a'),
        ('ｐ', 'p'),
        ('ｙ', 'y'),
        ('ｌ', 'l'),
    ]
    .into_iter()
    .collect()
}

pub fn skeleton(text: &str) -> String {
    let map = confusable_map();
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|ch| map.get(&ch).copied().unwrap_or(ch))
        .collect()
}
