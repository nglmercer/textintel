//! Explainable articulatory features for phoneme comparison.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Bilabial,
    Labiodental,
    Dental,
    Alveolar,
    Postalveolar,
    Palatal,
    Velar,
    Glottal,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Manner {
    Stop,
    Fricative,
    Affricate,
    Nasal,
    Liquid,
    Glide,
    Vowel,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Features {
    vowel: bool,
    voiced: bool,
    place: Place,
    manner: Manner,
    height: u8,
    backness: u8,
    rounded: bool,
}

/// A compact stable label useful for diagnostics and serialized fingerprints.
pub fn feature_label(phoneme: &str) -> &'static str {
    let features = classify_phoneme(phoneme);
    if features.vowel {
        if features.rounded {
            "vowel:rounded"
        } else {
            "vowel:unrounded"
        }
    } else {
        match (features.manner, features.place) {
            (Manner::Stop, Place::Bilabial) => "consonant:stop:bilabial",
            (Manner::Stop, Place::Alveolar) => "consonant:stop:alveolar",
            (Manner::Fricative, Place::Alveolar) => "consonant:fricative:alveolar",
            (Manner::Nasal, _) => "consonant:nasal",
            (Manner::Liquid, _) => "consonant:liquid",
            (Manner::Glide, _) => "consonant:glide",
            _ => "consonant:other",
        }
    }
}

/// Return a distance in `[0, 1]`, using place, manner, voicing, and vowel
/// dimensions rather than the old vowel/voicing-only approximation.
pub fn articulatory_distance(left: &str, right: &str) -> f64 {
    if left == right {
        return 0.0;
    }
    let left = classify_phoneme(left);
    let right = classify_phoneme(right);
    distance_for_features(&left, &right)
}

/// Distance over pre-classified features. [`articulatory_distance`] and the
/// similarity DP share this so hot paths classify each distinct phoneme
/// once instead of once per DP cell; the arithmetic is identical.
pub(crate) fn distance_for_features(left: &Features, right: &Features) -> f64 {
    if left.vowel != right.vowel {
        return 1.0;
    }
    if left.vowel {
        let height = f64::from(left.height.abs_diff(right.height)) / 3.0;
        let backness = f64::from(left.backness.abs_diff(right.backness)) / 2.0;
        let rounding = f64::from(left.rounded != right.rounded);
        return (0.45 * height + 0.40 * backness + 0.15 * rounding).clamp(0.0, 1.0);
    }
    let place = if left.place == right.place { 0.0 } else { 0.35 };
    let manner = if left.manner == right.manner {
        0.0
    } else {
        0.35
    };
    let voicing = f64::from(left.voiced != right.voiced) * 0.30;
    (place + manner + voicing).min(1.0)
}

pub(crate) fn classify_phoneme(phoneme: &str) -> Features {
    let value = phoneme.to_lowercase();
    let vowel = value.chars().any(|ch| "aeiouəɛɪɔʊɑɒɨʉɯyøœ".contains(ch));
    if vowel {
        let (height, backness, rounded) = match value.chars().next().unwrap_or('ə') {
            'i' => (3, 0, false),
            'ɪ' | 'e' => (2, 0, false),
            'ɛ' | 'æ' => (1, 0, false),
            'a' => (0, 1, false),
            'ɑ' => (0, 2, false),
            'o' | 'ɔ' => (2, 2, true),
            'u' | 'ʊ' => (3, 2, true),
            'y' | 'ø' | 'œ' => (2, 1, true),
            _ => (1, 1, false),
        };
        return Features {
            vowel: true,
            voiced: true,
            place: Place::Other,
            manner: Manner::Vowel,
            height,
            backness,
            rounded,
        };
    }
    let first = value.chars().next().unwrap_or_default();
    let place = match first {
        'p' | 'b' | 'm' => Place::Bilabial,
        'f' | 'v' => Place::Labiodental,
        'θ' | 'ð' => Place::Dental,
        'ʃ' | 'ʒ' => Place::Postalveolar,
        't' if value.starts_with("tʃ") => Place::Postalveolar,
        'd' if value.starts_with("dʒ") => Place::Postalveolar,
        't' | 'd' | 's' | 'z' | 'n' | 'l' | 'ɾ' | 'r' => Place::Alveolar,
        'j' | 'ʝ' | 'ɲ' => Place::Palatal,
        'k' | 'g' | 'x' => Place::Velar,
        'h' => Place::Glottal,
        _ => Place::Other,
    };
    let manner = match first {
        't' | 'd' if value.contains('ʃ') || value.contains('ʒ') => Manner::Affricate,
        'p' | 'b' | 't' | 'd' | 'k' | 'g' => Manner::Stop,
        'f' | 'v' | 's' | 'z' | 'ʃ' | 'ʒ' | 'x' | 'h' => Manner::Fricative,
        'm' | 'n' | 'ɲ' => Manner::Nasal,
        'l' | 'r' | 'ɾ' => Manner::Liquid,
        'j' | 'w' => Manner::Glide,
        _ => Manner::Other,
    };
    let voiced = value.chars().any(|ch| "bdgvzʒdɡmnrlɲʝj".contains(ch));
    Features {
        vowel: false,
        voiced,
        place,
        manner,
        height: 0,
        backness: 0,
        rounded: false,
    }
}
