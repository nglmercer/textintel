//! Confidence-weighted cross-view transliteration evidence.

/// Confidence-weighted transliteration evidence for a fingerprint pair:
/// raw cross-view string similarity plus the provider confidence behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransliterationEvidence {
    pub similarity: f64,
    pub confidence: f64,
}

impl TransliterationEvidence {
    /// Decision-relevant evidence: raw similarity discounted by how much the
    /// provider trusts its own conversion. Low-confidence rule-based mappings
    /// can no longer produce unconditional 1.0 matches.
    pub fn weighted(&self) -> f64 {
        (self.similarity * self.confidence).clamp(0.0, 1.0)
    }
}

/// Best cross-view evidence over raw texts plus their `transliteration:*`
/// views. Returns `None` when neither side carries a transliteration view, so
/// monolingual pairs report `absent` instead of a misleading score.
/// Confidence is the minimum over the converted views forming the best pair
/// (raw anchors contribute 1.0); several views may match, in which case the
/// most trusted one wins.
pub fn transliteration_evidence(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> Option<TransliterationEvidence> {
    transliteration_evidence_inner(a, b, None)
}

/// [`transliteration_evidence`] with the raw-text similarity precomputed:
/// the scorer already holds `combined_character_similarity` over the raw
/// pair for its character channel, so it passes that value instead of
/// paying for the same comparison twice. Same inputs, same value.
pub(crate) fn transliteration_evidence_with_raw(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
    raw: f64,
) -> Option<TransliterationEvidence> {
    transliteration_evidence_inner(a, b, Some(raw))
}

fn transliteration_evidence_inner(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
    raw: Option<f64>,
) -> Option<TransliterationEvidence> {
    let views_a = a.transliteration_views();
    let views_b = b.transliteration_views();
    if views_a.is_empty() && views_b.is_empty() {
        return None;
    }
    let mut left = vec![(a.raw.as_str(), 1.0)];
    left.extend(views_a);
    let mut right = vec![(b.raw.as_str(), 1.0)];
    right.extend(views_b);
    // Exact cross-view matches short-circuit before edit-distance work.
    let mut exact_confidence: Option<f64> = None;
    for (one, confidence_one) in &left {
        for (other, confidence_other) in &right {
            if !one.is_empty() && one == other {
                let confidence = confidence_one.min(*confidence_other);
                exact_confidence =
                    Some(exact_confidence.map_or(confidence, |best| best.max(confidence)));
            }
        }
    }
    if let Some(confidence) = exact_confidence {
        return Some(TransliterationEvidence {
            similarity: 1.0,
            confidence,
        });
    }
    // Same-script pairs already match through the raw texts; the remaining
    // fuzzy work only pays off for cross-script pairs with weak raw overlap.
    let raw = raw.unwrap_or_else(|| {
        crate::lexical::character::combined_character_similarity(a.raw.as_str(), b.raw.as_str())
    });
    if raw >= 0.5 {
        return Some(TransliterationEvidence {
            similarity: raw,
            confidence: 1.0,
        });
    }
    let mut best = TransliterationEvidence {
        similarity: raw,
        confidence: 1.0,
    };
    for (one, confidence_one) in &left {
        for (other, confidence_other) in &right {
            let similarity = crate::lexical::character::combined_character_similarity(one, other);
            let confidence = confidence_one.min(*confidence_other);
            if similarity > best.similarity
                || (similarity == best.similarity && confidence > best.confidence)
            {
                best = TransliterationEvidence {
                    similarity,
                    confidence,
                };
            }
        }
    }
    Some(best)
}

/// Best cross-view character similarity over raw texts plus their
/// `transliteration:*` views. Returns `None` when neither side carries a
/// transliteration view, so monolingual pairs report `absent` instead of a
/// misleading score. This is the raw string similarity; see
/// [`transliteration_evidence`] for the confidence-weighted evidence used in
/// scoring decisions.
pub fn transliteration_similarity(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> Option<f64> {
    transliteration_evidence(a, b).map(|evidence| evidence.similarity)
}

/// Language/context compatibility discount for transliteration evidence, in
/// `[0.0, 1.0]`. Transliteration alone must never create a strong match, so
/// the decision-relevant evidence is
/// `similarity × provider_confidence × compatibility`, where compatibility
/// multiplies four independent discounts:
///
/// - language: `1.0` when the top languages agree, `0.7` otherwise (true
///   transliteration often crosses languages, so the cut is mild);
/// - semantic: `0.5 + 0.5 × cosine` when both sides carry
///   Production-quality (transformer) embeddings, `1.0` otherwise. The
///   feature-hash fallback reports near-zero cosine for every cross-script
///   pair — including genuine transliterations — so discounting on it
///   would punish true pairs for the fallback's blindness rather than for
///   a meaning mismatch. Missing or fallback evidence never penalizes;
/// - entity: `1.0 - 0.5 × entity_conflict`;
/// - validity: `0.5` when both sides are lexicon-valid words in different
///   languages (the valid-but-different-word false-friend signature),
///   `1.0` otherwise.
///
/// Pairs without transliteration views read `1.0` (nothing to discount).
pub fn transliteration_compatibility(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> f64 {
    if a.transliteration_views().is_empty() && b.transliteration_views().is_empty() {
        return 1.0;
    }
    let top = |fingerprint: &crate::core::types::MessageFingerprint| {
        fingerprint
            .language_candidates
            .first()
            .map(|candidate| candidate.language.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };
    let languages_agree = top(a) == top(b);
    let language_factor = if languages_agree { 1.0 } else { 0.7 };
    let production_pair = |fingerprint: &crate::core::types::MessageFingerprint| {
        fingerprint
            .metadata
            .get("semantic_quality")
            .is_some_and(|quality| quality == "production")
    };
    let semantic_factor = match (
        a.semantic_embeddings.get("default"),
        b.semantic_embeddings.get("default"),
    ) {
        (Some(left), Some(right)) if production_pair(a) && production_pair(b) => {
            0.5 + 0.5 * crate::semantic::similarity::cosine(left, right).clamp(0.0, 1.0)
        }
        _ => 1.0,
    };
    let entity_factor =
        1.0 - 0.5 * crate::entities::entity_conflict(&a.entities, &b.entities).clamp(0.0, 1.0);
    let validity_factor = if a.lexicon_coverage.min(b.lexicon_coverage) > 0.8 && !languages_agree {
        0.5
    } else {
        1.0
    };
    (language_factor * semantic_factor * entity_factor * validity_factor).clamp(0.0, 1.0)
}

/// Decision-relevant transliteration evidence: raw cross-view similarity
/// discounted by provider confidence and language/context compatibility.
/// `None` when neither side carries a transliteration view.
pub fn effective_transliteration_evidence(
    a: &crate::core::types::MessageFingerprint,
    b: &crate::core::types::MessageFingerprint,
) -> Option<f64> {
    transliteration_evidence(a, b).map(|evidence| {
        (evidence.similarity * evidence.confidence * transliteration_compatibility(a, b))
            .clamp(0.0, 1.0)
    })
}
