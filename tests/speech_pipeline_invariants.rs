use proptest::prelude::*;
use textintel::speech::normalization::{
    self, NormalizationMode, NormalizeLimits, NormalizeOptions,
};
use textintel::speech::segmentation::{SegmentPolicy, segment_text};

proptest! {
    #[test]
    fn chunks_are_bounded_ordered_and_preserve_non_whitespace(
        text in ".{0,600}", limit in 0usize..80
    ) {
        let fragments = segment_text(&text, &SegmentPolicy::bounded(limit));
        for (i, part) in fragments.iter().enumerate() {
            prop_assert_eq!(part.index, i);
            prop_assert_eq!(part.total, fragments.len());
            prop_assert!(!part.text.is_empty());
            prop_assert!(part.char_count() <= limit.max(1));
        }
        let actual: String = fragments.iter().flat_map(|f| f.text.chars()).filter(|c| !c.is_whitespace()).collect();
        let expected: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn compatible_normalization_reports_valid_splices_without_hidden_text_changes(text in ".{0,500}") {
        let options = NormalizeOptions::resolve(NormalizationMode::Auto,"es",Some("es-PE"),NormalizeLimits::default(),true).unwrap();
        let result = normalization::normalize(&text, &options).unwrap();
        let mut cursor = 0;
        let mut reconstructed = String::new();
        for change in &result.changes {
            prop_assert!(change.start >= cursor && change.start < change.end);
            prop_assert!(text.get(change.start..change.end).is_some());
            reconstructed.push_str(&text[cursor..change.start]);
            reconstructed.push_str(&change.replacement);
            cursor = change.end;
        }
        reconstructed.push_str(&text[cursor..]);
        prop_assert_eq!(reconstructed, result.spoken);
        for warning in &result.warnings {
            prop_assert!(warning.start <= warning.end);
            prop_assert!(text.get(warning.start..warning.end).is_some());
        }
    }
}
