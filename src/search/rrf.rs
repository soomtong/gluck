use std::cmp::Ordering;
use std::collections::HashMap;

pub fn rrf_fuse(bm25: &[(u64, f32)], vec: &[(u64, f32)], k: f32, limit: usize) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();
    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    let mut out: Vec<(u64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_both_empty() {
        let result = rrf_fuse(&[], &[], 60.0, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_bm25_only() {
        let bm25 = vec![(1u64, 0.9), (2, 0.7)];
        let result = rrf_fuse(&bm25, &[], 60.0, 10);
        assert_eq!(result.len(), 2);
        assert!(result[0].1 > result[1].1);
    }

    #[test]
    fn test_rrf_overlap_boosts_score() {
        let bm25 = vec![(1u64, 0.9), (2, 0.7)];
        let vec_hits = vec![(1u64, 0.95), (3, 0.6)];
        let result = rrf_fuse(&bm25, &vec_hits, 60.0, 10);
        let id1 = result.iter().find(|(id, _)| *id == 1).unwrap();
        let id2 = result.iter().find(|(id, _)| *id == 2).unwrap();
        assert!(
            id1.1 > id2.1,
            "id 1 appears in both lists, should score higher"
        );
    }

    #[test]
    fn test_rrf_limit_respected() {
        let bm25: Vec<(u64, f32)> = (0..20).map(|i| (i, 1.0)).collect();
        let result = rrf_fuse(&bm25, &[], 60.0, 5);
        assert_eq!(result.len(), 5);
    }
}
