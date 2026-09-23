//! Ordered parallel chunk mapping for batch tools (training feature
//! extraction, bulk embedding). Chunks rejoin in input order, so
//! multi-threaded runs produce byte-identical outputs to sequential
//! ones; worker panics and chunk errors fail loudly instead of
//! silently skipping work.

/// Map `f` over `items` in `chunk_size` pieces, optionally on `jobs`
/// worker threads, rejoining chunks in order. Values below 2 (or a
/// single chunk) run the plain sequential path.
pub fn map_chunks_ordered<T, F, E>(
    items: &[String],
    chunk_size: usize,
    jobs: usize,
    f: F,
) -> Result<Vec<T>, E>
where
    T: Send,
    E: Send + From<String>,
    F: Fn(&[String]) -> Result<Vec<T>, E> + Sync,
{
    let chunks: Vec<&[String]> = items.chunks(chunk_size.max(1)).collect();
    if jobs < 2 || chunks.len() < 2 {
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(f(chunk)?);
        }
        return Ok(out);
    }
    let workers = jobs.min(chunks.len());
    let mut groups: Vec<Vec<&[String]>> = Vec::with_capacity(workers);
    for group in chunks.chunks(chunks.len().div_ceil(workers)) {
        groups.push(group.to_vec());
    }
    let mut joined = Vec::with_capacity(groups.len());
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(groups.len());
        for group in groups {
            handles.push(scope.spawn(|| {
                let mut pieces = Vec::new();
                for chunk in group {
                    pieces.extend(f(chunk)?);
                }
                Ok::<Vec<T>, E>(pieces)
            }));
        }
        for handle in handles {
            joined.push(handle.join());
        }
    });
    let mut out = Vec::new();
    for result in joined {
        match result {
            Ok(Ok(pieces)) => out.extend(pieces),
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(E::from(
                    "worker thread panicked during chunk mapping".to_string(),
                ));
            }
        }
    }
    Ok(out)
}
