use crate::lexical::ngrams::character_ngrams;

fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325u64 ^ seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn simhash(tokens: &[String], bits: usize) -> u64 {
    let bits = bits.clamp(1, 64);
    let mut sums = vec![0i32; bits];
    for token in tokens {
        let hash = fnv1a(token.as_bytes(), 0);
        for (index, sum) in sums.iter_mut().enumerate() {
            *sum += if (hash >> index) & 1 == 1 { 1 } else { -1 };
        }
    }
    sums.into_iter().enumerate().fold(0, |value, (index, sum)| {
        if sum > 0 { value | (1u64 << index) } else { value }
    })
}

pub fn minhash_signature(tokens: &[String], k: usize) -> Vec<u32> {
    if k == 0 {
        return Vec::new();
    }
    if tokens.is_empty() {
        return vec![u32::MAX; k];
    }
    (0..k)
        .map(|salt| {
            tokens
                .iter()
                .map(|token| fnv1a(token.as_bytes(), salt as u64).min(u64::from(u32::MAX)) as u32)
                .min()
                .unwrap_or(u32::MAX)
        })
        .collect()
}

pub fn minhash_from_text(text: &str, k: usize) -> Vec<u32> {
    let grams = character_ngrams(text, 3);
    minhash_signature(&grams, k)
}

pub fn minhash_similarity(a: &[u32], b: &[u32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).filter(|(left, right)| left == right).count() as f64 / a.len() as f64
}

