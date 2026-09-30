//! Compare optimized rolling rows with independent full-matrix recurrences.
use proptest::prelude::*;
use textintel::lexical::character::{damerau_levenshtein, lcs_len, levenshtein};

fn reference(a: &str, b: &str, transpose: bool, subsequence: bool) -> usize {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let mut matrix = vec![vec![0; b.len() + 1]; a.len() + 1];
    if !subsequence {
        for (i, row) in matrix.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, value) in matrix[0].iter_mut().enumerate() {
            *value = j;
        }
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            matrix[i][j] = if subsequence {
                if a[i - 1] == b[j - 1] {
                    matrix[i - 1][j - 1] + 1
                } else {
                    matrix[i - 1][j].max(matrix[i][j - 1])
                }
            } else {
                (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]))
            };
            if transpose && i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                matrix[i][j] = matrix[i][j].min(matrix[i - 2][j - 2] + 1);
            }
        }
    }
    matrix[a.len()][b.len()]
}

proptest! {
    #[test]
    fn unicode_rows_match_full_matrix(a in "[abcé中🏠]{0,18}", b in "[abcé中🏠]{0,18}") {
        prop_assert_eq!(levenshtein(&a, &b), reference(&a, &b, false, false));
        prop_assert_eq!(damerau_levenshtein(&a, &b), reference(&a, &b, true, false));
        prop_assert_eq!(lcs_len(&a, &b), reference(&a, &b, false, true));
        let prefix = "common é中 prefix ";
        let suffix = " shared 🏠 suffix";
        let left = format!("{prefix}{a}{suffix}");
        let right = format!("{prefix}{b}{suffix}");
        prop_assert_eq!(levenshtein(&left, &right), reference(&left, &right, false, false));
    }
}
