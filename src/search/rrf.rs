use std::collections::HashMap;

/// Reciprocal Rank Fusion over two ranked lists of (doc_id, score).
/// k: smoothing constant (typically 60.0)
/// limit: maximum results to return
pub fn rrf_fuse(
    bm25: &[(u64, f32)],
    vec: &[(u64, f32)],
    k: f32,
    limit: usize,
) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();

    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }

    let mut out: Vec<(u64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_both_empty() {
        assert!(rrf_fuse(&[], &[], 60.0, 10).is_empty());
    }

    #[test]
    fn test_single_source_bm25() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.5)];
        let out = rrf_fuse(&bm25, &[], 60.0, 10);
        assert_eq!(out.len(), 2);
        assert!(out[0].1 > out[1].1, "should be sorted descending");
        assert_eq!(out[0].0, 1u64);
    }

    #[test]
    fn test_overlap_boosts_score() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.5)];
        let vec  = vec![(1u64, 0.9)];
        let out = rrf_fuse(&bm25, &vec, 60.0, 10);
        let id1 = out.iter().find(|(id, _)| *id == 1).unwrap();
        let id2 = out.iter().find(|(id, _)| *id == 2).unwrap();
        assert!(id1.1 > id2.1, "overlap should boost score");
    }

    #[test]
    fn test_sorted_descending() {
        let bm25 = vec![(1u64, 1.0), (2u64, 0.8), (3u64, 0.3)];
        let vec  = vec![(1u64, 0.9), (3u64, 0.7)];
        let out = rrf_fuse(&bm25, &vec, 60.0, 10);
        for w in out.windows(2) {
            assert!(w[0].1 >= w[1].1);
        }
    }

    #[test]
    fn test_limit_respected() {
        let bm25: Vec<(u64, f32)> = (0..20).map(|i| (i, 1.0 / (i as f32 + 1.0))).collect();
        let out = rrf_fuse(&bm25, &[], 60.0, 5);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn test_vec_only() {
        let vec = vec![(10u64, 0.9), (20u64, 0.7)];
        let out = rrf_fuse(&[], &vec, 60.0, 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, 10u64);
    }
}
