use crate::core::types::ConfusableCharacter;
use crate::normalization::confusables::{confusable_map, skeleton};
use crate::visual::scripts::script_name;

pub fn confusable_hits(text: &str) -> Vec<(usize, String, String, String)> {
    let map = confusable_map();
    let mut hits = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        let mapped = map.get(&ch).copied();
        let script = script_name(ch);
        if let (Some(mapped), Some(script)) = (mapped, script) {
            if !matches!(script, "Latin" | "Common" | "Symbol") {
                hits.push((index, ch.to_string(), script.to_string(), mapped.to_string()));
            }
        }
    }
    hits
}

pub fn confusable_characters(text: &str) -> Vec<ConfusableCharacter> {
    confusable_hits(text)
        .into_iter()
        .map(|(index, character, script, mapped)| ConfusableCharacter {
            character,
            index,
            script,
            confusable_with: Some(mapped),
        })
        .collect()
}

pub fn confusable_skeleton(text: &str) -> String {
    skeleton(text)
}

