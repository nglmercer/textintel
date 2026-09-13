use crate::lexical::character::combined_character_similarity;
use crate::normalization::unicode::casefold_text;
use crate::visual::homoglyph::confusable_skeleton;

pub fn visual_similarity(a: &str, b: &str) -> f64 {
    let sa = casefold_text(&confusable_skeleton(a));
    let sb = casefold_text(&confusable_skeleton(b));
    combined_character_similarity(&sa, &sb)
}
