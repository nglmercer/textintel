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
    let raw =
        crate::lexical::character::combined_character_similarity(a.raw.as_str(), b.raw.as_str());
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
