//! Ordered parallel chunk mapping: multi-threaded runs rejoin in
//! input order (identical to sequential), and chunk errors fail loudly.

use textintel::core::parallel::map_chunks_ordered;

fn items(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("text-{index:04}")).collect()
}

#[test]
fn parallel_matches_sequential_for_all_job_counts() {
    let inputs = items(137);
    let expected: Vec<String> = map_chunks_ordered(&inputs, 16, 1, |chunk| {
        Ok::<Vec<String>, String>(chunk.to_vec())
    })
    .expect("sequential");
    assert_eq!(expected, inputs);
    for jobs in [0, 2, 3, 4, 8, 64, 1000] {
        let actual: Vec<String> = map_chunks_ordered(&inputs, 16, jobs, |chunk| {
            Ok::<Vec<String>, String>(chunk.to_vec())
        })
        .expect("parallel");
        assert_eq!(actual, expected, "jobs={jobs}");
    }
}

#[test]
fn mapper_output_concatenates_in_chunk_order() {
    let inputs = items(50);
    let doubled: Vec<String> = map_chunks_ordered(&inputs, 7, 4, |chunk| {
        Ok::<Vec<String>, String>(
            chunk
                .iter()
                .flat_map(|text| [text.clone(), text.clone()])
                .collect::<Vec<_>>(),
        )
    })
    .expect("mapped");
    assert_eq!(doubled.len(), 100);
    assert_eq!(doubled[0], "text-0000");
    assert_eq!(doubled[1], "text-0000");
    assert_eq!(doubled[99], "text-0049");
}

#[test]
fn first_chunk_error_in_order_wins() {
    let inputs = items(50);
    // Every chunk fails; the sequential-first error must surface
    // regardless of thread timing.
    let error: Result<Vec<String>, String> =
        map_chunks_ordered(&inputs, 7, 4, |chunk| Err(format!("bad {}", chunk[0])));
    assert_eq!(error, Err("bad text-0000".to_string()));
    // Single chunk / empty input take the sequential path.
    assert!(
        map_chunks_ordered(&[], 7, 4, |chunk| Ok::<Vec<String>, String>(chunk.to_vec()))
            .expect("empty")
            .is_empty()
    );
    let single: Vec<String> = map_chunks_ordered(&inputs[..3], 64, 8, |chunk| {
        Ok::<Vec<String>, String>(chunk.iter().rev().cloned().collect())
    })
    .expect("single");
    assert_eq!(single, vec!["text-0002", "text-0001", "text-0000"]);
}
