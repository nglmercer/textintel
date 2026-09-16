//! Similarity features: decode scoping plus the confusable-separation
//! interaction features (`confusable_swap`, `single_word_exact`).
//!
//! Pins the behaviors, not the trained weights: candidates decode through
//! the right scope, cross-language swaps (`mi`/`my`) do not pay the
//! confusable penalty, same-language swaps (`right`/`rite`) do, and
//! single-word exact decodes (`cheque`→`check`) surface as their own
//! evidence.

use textintel::{EngineConfig, TextIntelligence};

fn engine() -> TextIntelligence {
    TextIntelligence::new(EngineConfig::default())
}

fn decoded_texts(engine: &TextIntelligence, text: &str) -> Vec<String> {
    engine
        .analyze(text)
        .expect("analyze must succeed")
        .rebus_candidates
        .iter()
        .map(|candidate| candidate.text.clone())
        .collect()
}

#[test]
fn uncertain_single_token_decodes_globally() {
    // `luv` detects as nothing confident, so scoping must not cut the
    // English reading: the globally-best `love` reading wins on its own.
    let engine = engine();
    let candidates = decoded_texts(&engine, "luv");
    assert!(
        candidates.iter().any(|text| text == "love"),
        "missing luv → love: {candidates:?}"
    );
}

#[test]
fn uncertain_multitoken_stays_scoped() {
    // `2nite` is uncertain but multi-token, so the top-3 scope still
    // applies and composition finds `tonight` at rank one.
    let engine = engine();
    let candidates = decoded_texts(&engine, "2nite");
    assert_eq!(
        candidates.first().map(String::as_str),
        Some("tonight"),
        "2nite top candidate: {candidates:?}"
    );
}

#[test]
fn confident_detection_stays_narrow() {
    // `ya` is confidently Indonesian, so the English slang reading stays
    // out of scope: genuinely ambiguous without context.
    let engine = engine();
    let candidates = decoded_texts(&engine, "ya");
    assert!(
        !candidates.iter().any(|text| text == "you"),
        "ya should not decode to you under id scope: {candidates:?}"
    );
}

#[test]
fn single_word_exact_fires_on_decoded_variants() {
    let engine = engine();
    let decoded = engine
        .compare("cheque", "check")
        .expect("compare must succeed");
    assert!(
        decoded.single_word_exact > 0.3,
        "cheque/check should carry single-word exact evidence: {}",
        decoded.single_word_exact
    );
    let confusable = engine
        .compare("their", "there")
        .expect("compare must succeed");
    assert_eq!(
        confusable.single_word_exact, 0.0,
        "their/there has no decode: {}",
        confusable.single_word_exact
    );
}

#[test]
fn confusable_swap_fires_same_language() {
    let engine = engine();
    let strong = engine
        .compare("complement the chef tonight", "compliment the chef tonight")
        .expect("compare must succeed");
    assert!(
        strong.confusable_swap > 0.5,
        "complement/compliment should pay the full confusable penalty: {}",
        strong.confusable_swap
    );
    let weak = engine
        .compare(
            "he chose the right path through the forest",
            "he chose the rite path through the forest",
        )
        .expect("compare must succeed");
    assert!(
        weak.confusable_swap > 0.0,
        "right/rite is unsuppressed same-language evidence: {}",
        weak.confusable_swap
    );
}

#[test]
fn vowel_fold_links_consonantal_views() {
    let engine = engine();
    let result = engine
        .compare("habibi", "حبيبي")
        .expect("compare must succeed");
    assert!(
        result.exact_decode >= 0.5,
        "habibi view should meet as skeletons: {}",
        result.exact_decode
    );
}

#[test]
fn vowel_fold_skips_complete_views() {
    // `аpple` yields the complete Cyrillic-derived view `apple`, which
    // must not skeleton-match `apply`: folding would destroy real vowel
    // information. (Regression: the unrestricted fold merged them.)
    let engine = engine();
    let result = engine
        .compare("аpple", "apply")
        .expect("compare must succeed");
    assert_eq!(
        result.exact_decode, 0.0,
        "complete views never fold-match: {}",
        result.exact_decode
    );
}

#[test]
fn substring_containment_gates_on_substance() {
    let engine = engine();
    let sub = engine
        .compare("good morning", "morning")
        .expect("compare must succeed");
    assert_eq!(
        sub.substring_containment, 1.0,
        "morning sits inside good morning: {}",
        sub.substring_containment
    );
    let cjk = engine
        .compare("早上好", "早上")
        .expect("compare must succeed");
    assert_eq!(
        cjk.substring_containment, 1.0,
        "CJK substring counts unsegmented: {}",
        cjk.substring_containment
    );
    let short = engine.compare("he", "the").expect("compare must succeed");
    assert_eq!(
        short.substring_containment, 0.0,
        "two-letter containment is meaningless: {}",
        short.substring_containment
    );
    // Impure by design: carpet sits inside carpeta (the learned weight
    // prices the net value; short confusables stay out by the gate).
    let false_friend = engine
        .compare("carpet", "carpeta")
        .expect("compare must succeed");
    assert_eq!(
        false_friend.substring_containment, 1.0,
        "carpet/carpeta fires honestly: {}",
        false_friend.substring_containment
    );
}

#[test]
fn cross_script_agreement_marks_transliteration_shape() {
    let engine = engine();
    let pinyin = engine.compare("hao", "好").expect("compare must succeed");
    assert_eq!(
        pinyin.cross_script_agreement, 1.0,
        "hao/好 agree across scripts: {}",
        pinyin.cross_script_agreement
    );
    let lookalike = engine.compare("net", "нет").expect("compare must succeed");
    assert_eq!(
        lookalike.cross_script_agreement, 0.0,
        "net/нет disagree across scripts: {}",
        lookalike.cross_script_agreement
    );
}

#[test]
fn confusable_swap_suppressed_across_languages() {
    let engine = engine();
    let result = engine
        .compare("I love mi casa", "I love my casa")
        .expect("compare must succeed");
    assert_eq!(
        result.confusable_swap, 0.0,
        "mi/my is a language switch, not a confusable: {}",
        result.confusable_swap
    );
}
