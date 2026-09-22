//! Decoded-view overlap (rebus candidates + transliteration views).
//! Best cross-view character overlap plus confidence-weighted exact
//! decoding evidence.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::types::MessageFingerprint;
use crate::lexical::character::combined_character_similarity;
use crate::normalization::unicode::casefold_text;
use crate::transliteration::{strip_latin_vowels, view_is_consonantal};

use super::compact;

/// Folded decoded keys for one fingerprint: literal bases (raw,
/// normalized) plus rebus candidates in rank order with their raw
/// confidences. Folded once per compare and shared by every overlap
/// set, confidence map, and Phase-3 anchor below — the same strings
/// each builder folded separately before (three passes per side).
struct DecodedKeys {
    literals: [String; 2],
    candidates: Vec<(String, f64)>,
}

fn decoded_keys(fingerprint: &MessageFingerprint) -> DecodedKeys {
    DecodedKeys {
        literals: [
            compact(&casefold_text(&fingerprint.raw)),
            compact(&casefold_text(
                fingerprint.normalized.as_deref().unwrap_or(""),
            )),
        ],
        candidates: fingerprint
            .rebus_candidates
            .iter()
            .map(|candidate| {
                (
                    compact(&casefold_text(&candidate.text)),
                    candidate.confidence(),
                )
            })
            .collect(),
    }
}

/// One transliteration-view fold shared by both overlap entry points.
fn fold_views(views: &[(&str, f64)]) -> Vec<(String, f64)> {
    views
        .iter()
        .map(|(view, confidence)| (compact(&casefold_text(view)), *confidence))
        .collect()
}

/// Literal bases (raw/normalized text) count at 1.0 in every confidence map.
fn insert_literal_bases(map: &mut BTreeMap<String, f64>, literals: &[String; 2]) {
    for key in literals {
        if !key.is_empty() {
            map.entry(key.clone()).or_insert(1.0);
        }
    }
}

/// Rank-aware confidence map over raw/normalized text plus rebus
/// candidates: each candidate counts at its confidence discounted by decode
/// rank. Feeds the exact-decode belief channel, where steep rank discounting
/// separates shouts from whispers.
fn graded_confidence_map(keys: &DecodedKeys) -> BTreeMap<String, f64> {
    let mut map: BTreeMap<String, f64> = BTreeMap::new();
    insert_literal_bases(&mut map, &keys.literals);
    for (rank, (key, confidence)) in keys.candidates.iter().enumerate() {
        if key.is_empty() {
            continue;
        }
        let confidence = confidence.clamp(0.0, 1.0) / (rank + 1) as f64;
        map.entry(key.clone())
            .and_modify(|slot| *slot = slot.max(confidence))
            .or_insert(confidence);
    }
    map
}

/// Linkage map for decoded overlap. The rank-0 candidate (the decoder's
/// asserted reading, the same contract the top-1 rebus metric scores)
/// counts at 1.0 like literal text. Lower readings stay high but gently
/// rank-sensitive: beyond rank 0 the multilingual beam order is noisy
/// (wrong-language and literal competitors routinely outrank the true
/// reading by hundredths of a point), so steep discounting would punish true
/// links for beam noise. The gentle slope only breaks ties toward the
/// decoder's best belief.
fn asserted_confidence_map(keys: &DecodedKeys) -> BTreeMap<String, f64> {
    let mut map: BTreeMap<String, f64> = BTreeMap::new();
    insert_literal_bases(&mut map, &keys.literals);
    for (rank, (key, _)) in keys.candidates.iter().enumerate() {
        if key.is_empty() {
            continue;
        }
        let confidence = 1.0 / (1.0 + 0.05 * rank as f64);
        map.entry(key.clone())
            .and_modify(|slot| *slot = slot.max(confidence))
            .or_insert(confidence);
    }
    map
}

pub(crate) fn best_decoded_overlap(
    a: &MessageFingerprint,
    b: &MessageFingerprint,
    compatibility: f64,
    views_a: &[(&str, f64)],
    views_b: &[(&str, f64)],
) -> f64 {
    // Transliteration views contribute `similarity × confidence ×
    // compatibility`: provider confidence discounts the lossy conversion and
    // language/context compatibility discounts look-alikes without semantic,
    // entity, or language support (transliteration alone never creates a
    // strong match). Compatibility and views arrive precomputed from the
    // scorer, which shares them across channels.
    let keys_a = decoded_keys(a);
    let keys_b = decoded_keys(b);
    let mut left = BTreeSet::new();
    let mut right = BTreeSet::new();
    for key in keys_a
        .literals
        .iter()
        .chain(keys_a.candidates.iter().map(|(key, _)| key))
    {
        left.insert(key.clone());
    }
    for key in keys_b
        .literals
        .iter()
        .chain(keys_b.candidates.iter().map(|(key, _)| key))
    {
        right.insert(key.clone());
    }
    // Transliteration views join the overlap set so cross-script pairs
    // (`privet` ↔ `привет`) match through the converted view. Only
    // transliteration views participate — other normalization views stay out
    // so leet/casefold variants cannot inflate decoded similarity. Every view
    // carries its provider confidence: a view match contributes
    // `similarity * confidence`, never an unconditional 1.0.
    let views_left = fold_views(views_a);
    let views_right = fold_views(views_b);
    // Phase 1a: graded exact matches. Literal identity (raw/normalized)
    // still counts at 1.0, but candidate-mediated matches count at the
    // weaker side's rank-aware confidence — a low-rank whisper (`gr8`→
    // `grate`) must not weigh like the top reading (`gr8`→`great`).
    let mut best: f64 = 0.0;
    let graded_left = asserted_confidence_map(&keys_a);
    let graded_right = asserted_confidence_map(&keys_b);
    for (key, confidence) in &graded_left {
        if let Some(other) = graded_right.get(key) {
            best = best.max(confidence.min(*other));
        }
    }
    // Phase 1b: exact matches involving a transliteration view contribute
    // the view confidence times compatibility (similarity 1.0 times provider
    // confidence times language/context compatibility). View↔view matches
    // take the weaker confidence; view↔base matches take the view's.
    for (view, confidence) in &views_left {
        if view.is_empty() {
            continue;
        }
        if right.contains(view) {
            best = best.max(*confidence * compatibility);
        }
        for (other, other_confidence) in &views_right {
            if view == other {
                best = best.max(confidence.min(*other_confidence) * compatibility);
            }
        }
    }
    for (view, confidence) in &views_right {
        if view.is_empty() {
            continue;
        }
        if left.contains(view) {
            best = best.max(*confidence * compatibility);
        }
    }
    // Vowel-folded tier: consonantal views meet vocalized text as
    // skeletons (`hbyby` with `habibi` as `hbb`). The fold is a
    // normalization (like casefolding), so folded-exact counts at the
    // tier's confidence; only consonantal views participate (views that
    // already spell their vowels are complete, and bases are literal),
    // and empties never match.
    let folded_bases_left: BTreeSet<String> = left
        .iter()
        .map(|key| strip_latin_vowels(key))
        .filter(|key| !key.is_empty())
        .collect();
    let folded_bases_right: BTreeSet<String> = right
        .iter()
        .map(|key| strip_latin_vowels(key))
        .filter(|key| !key.is_empty())
        .collect();
    let folded_views_left: Vec<(String, f64)> = views_left
        .iter()
        .filter(|(view, _)| view_is_consonantal(view))
        .map(|(view, confidence)| (strip_latin_vowels(view), *confidence))
        .filter(|(view, _)| !view.is_empty())
        .collect();
    let folded_views_right: Vec<(String, f64)> = views_right
        .iter()
        .filter(|(view, _)| view_is_consonantal(view))
        .map(|(view, confidence)| (strip_latin_vowels(view), *confidence))
        .filter(|(view, _)| !view.is_empty())
        .collect();
    for (view, confidence) in &folded_views_left {
        if folded_bases_right.contains(view) {
            best = best.max(*confidence * compatibility);
        }
        for (other, other_confidence) in &folded_views_right {
            if view == other {
                best = best.max(confidence.min(*other_confidence) * compatibility);
            }
        }
    }
    for (view, confidence) in &folded_views_right {
        if folded_bases_left.contains(view) {
            best = best.max(*confidence * compatibility);
        }
    }
    // Phase 2: fuzzy overlap over the graded base maps (no views).
    // Candidate-mediated pairs count at string similarity times the weaker
    // side's rank-aware confidence, exactly like Phase 1a: an identical
    // low-rank whisper (`gr8`→`grate`) must not sneak back to 1.0 through
    // the fuzzy tier after Phase 1a graded it down. Literal↔literal pairs
    // keep confidence 1.0, so plain fuzzy behavior is unchanged.
    // Identical pairs share one value: every channel reads 1.0 on equal
    // non-empty inputs, so the blend is the same constant for all of them.
    // It is computed through the real function once, never hand-folded
    // (map keys are never empty — empties are skipped at build).
    let identical = combined_character_similarity("a", "a");
    for (left_value, left_confidence) in &graded_left {
        for (right_value, right_confidence) in &graded_right {
            let pair = if left_value == right_value {
                identical * left_confidence.min(*right_confidence)
            } else {
                combined_character_similarity(left_value, right_value)
                    * left_confidence.min(*right_confidence)
            };
            best = best.max(pair);
        }
    }
    // Phase 3: view↔raw rescue pairs run only when the base overlap is
    // below near-exact. Transliteration views exist to rescue cross-script
    // pairs; restricting the extra work to view↔raw/normalized pairs (a
    // handful per comparison) keeps same-script cost flat while still
    // matching `privet` against the `привет` → `privet` view. Pairs are
    // strictly cross-side: a view matching its own raw text proves nothing.
    if best < 0.9 && (!views_left.is_empty() || !views_right.is_empty()) {
        // Anchors are the literal keys: same strings, already folded.
        let anchors_left = &keys_a.literals;
        let anchors_right = &keys_b.literals;
        for (views, anchors) in [(&views_left, &anchors_right), (&views_right, &anchors_left)] {
            for (view, confidence) in views.iter() {
                if view.is_empty() {
                    continue;
                }
                for anchor in anchors.iter() {
                    if anchor.is_empty() {
                        continue;
                    }
                    best = best.max(
                        combined_character_similarity(view, anchor) * confidence * compatibility,
                    );
                }
            }
        }
    }
    best.clamp(0.0, 1.0)
}

/// Strongest confidence among exact cross-side matches: a rebus candidate
/// (or raw/normalized text) of one side equal to the other side's raw,
/// normalized, or candidate text, or a transliteration view exactly matching
/// the other side. Raw/normalized identity counts at 1.0; candidate matches
/// count at the candidate's rank-aware confidence discounted by decode rank
/// (a rank-5 reading is far less likely intended than the top reading, so a
/// `gr8`→`grate` whisper must not weigh like a `cheque`→`check` shout); view
/// matches count at the provider confidence — mirroring
/// [`best_decoded_overlap`]'s Phase 1a/1b, but as graded evidence instead of
/// a short-circuit.
pub(crate) fn exact_decode_confidence(
    a: &MessageFingerprint,
    b: &MessageFingerprint,
    compatibility: f64,
    views_a: &[(&str, f64)],
    views_b: &[(&str, f64)],
) -> f64 {
    let left = graded_confidence_map(&decoded_keys(a));
    let right = graded_confidence_map(&decoded_keys(b));
    let mut best: f64 = 0.0;
    for (key, confidence) in &left {
        if let Some(other) = right.get(key) {
            best = best.max(confidence.min(*other));
        }
    }
    let views_left = fold_views(views_a);
    let views_right = fold_views(views_b);
    for (view, confidence) in &views_left {
        if view.is_empty() {
            continue;
        }
        if right.contains_key(view) {
            best = best.max(*confidence * compatibility);
        }
        // View↔view matches are second-hand evidence: transliteration is
        // lossy, so two different words can share one transliteration
        // (`complement`/`compliment` collapse to the same Arabic form) while
        // a view matching the other side's literal text is first-hand. Halve
        // the weaker confidence so collisions whisper instead of shout.
        for (other, other_confidence) in &views_right {
            if view == other {
                best = best.max(confidence.min(*other_confidence) * 0.5 * compatibility);
            }
        }
    }
    for (view, confidence) in &views_right {
        if view.is_empty() {
            continue;
        }
        if left.contains_key(view) {
            best = best.max(*confidence * compatibility);
        }
    }
    // Vowel-folded tier (see `best_decoded_overlap`): only consonantal
    // views meet text as skeletons. View↔view stays halved as above.
    let folded_bases_left: BTreeSet<String> = left
        .keys()
        .map(|key| strip_latin_vowels(key))
        .filter(|key| !key.is_empty())
        .collect();
    let folded_bases_right: BTreeSet<String> = right
        .keys()
        .map(|key| strip_latin_vowels(key))
        .filter(|key| !key.is_empty())
        .collect();
    let folded_views_left: Vec<(String, f64)> = views_left
        .iter()
        .filter(|(view, _)| view_is_consonantal(view))
        .map(|(view, confidence)| (strip_latin_vowels(view), *confidence))
        .filter(|(view, _)| !view.is_empty())
        .collect();
    let folded_views_right: Vec<(String, f64)> = views_right
        .iter()
        .filter(|(view, _)| view_is_consonantal(view))
        .map(|(view, confidence)| (strip_latin_vowels(view), *confidence))
        .filter(|(view, _)| !view.is_empty())
        .collect();
    for (view, confidence) in &folded_views_left {
        if folded_bases_right.contains(view) {
            best = best.max(*confidence * compatibility);
        }
        for (other, other_confidence) in &folded_views_right {
            if view == other {
                best = best.max(confidence.min(*other_confidence) * 0.5 * compatibility);
            }
        }
    }
    for (view, confidence) in &folded_views_right {
        if folded_bases_left.contains(view) {
            best = best.max(*confidence * compatibility);
        }
    }
    best.clamp(0.0, 1.0)
}
