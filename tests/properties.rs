use proptest::prelude::*;

use textintel::language::segmentation::segment_message;
use textintel::normalization::unicode::{casefold_text, nfc, nfkc};
use textintel::visual::unicode_features::analyze_unicode;

proptest! {
    #[test]
    fn unicode_views_are_total(input in any::<String>()) {
        let features = analyze_unicode(&input);
        prop_assert!(features.suspicious_unicode_score.is_finite());
        prop_assert!((0.0..=1.0).contains(&features.suspicious_unicode_score));
        prop_assert_eq!(nfc(&input), features.nfc);
        prop_assert_eq!(nfkc(&input), features.nfkc);
        prop_assert_eq!(casefold_text(&input), features.casefolded);
    }

    #[test]
    fn segmentation_preserves_valid_utf8_ranges(input in any::<String>()) {
        let segments = segment_message(&input, 128);
        for segment in segments {
            prop_assert!(segment.start <= segment.end);
            prop_assert!(segment.end <= input.len());
            prop_assert!(input.is_char_boundary(segment.start));
            prop_assert!(input.is_char_boundary(segment.end));
            prop_assert_eq!(&input[segment.start..segment.end], segment.text.as_str());
        }
    }
}
