use std::cmp::Ordering;
use std::collections::HashMap;

pub fn rrf_fuse(bm25: &[(u64, f32)], vec: &[(u64, f32)], k: f32, limit: usize) -> Vec<(u64, f32)> {
    rrf_fuse_weighted(bm25, vec, k, limit, 1.0, 1.0)
}

/// 가중 RRF: BM25/Vector 각 리스트에 가중치를 곱한다.
/// 한국어 쿼리처럼 본문 매칭 표면이 약해서 BM25가 노이즈가 되는 경우
/// vector 기여를 키워서 신호를 보존하기 위해 사용한다.
/// BM25 top-N을 결과 앞에 anchor한 뒤 나머지를 weighted RRF로 채운다.
/// 한국어 쿼리처럼 BM25가 path 별칭으로 정답을 잡지만 vector가 의미적 노이즈
/// (commits 등)에 끌려가는 경우, RRF 누적이 BM25-only 강한 매칭을 묻어버린다.
/// anchor는 이 비대칭을 깨기 위해 BM25 top-N을 강제로 보존한다.
pub fn rrf_fuse_with_bm25_anchor(
    bm25: &[(u64, f32)],
    vec: &[(u64, f32)],
    k: f32,
    limit: usize,
    w_bm25: f32,
    w_vec: f32,
    n_anchor: usize,
) -> Vec<(u64, f32)> {
    let fused = rrf_fuse_weighted(bm25, vec, k, limit + n_anchor, w_bm25, w_vec);
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(u64, f32)> = Vec::with_capacity(limit);
    for (id, score) in bm25.iter().take(n_anchor) {
        if seen.insert(*id) {
            out.push((*id, *score));
            if out.len() >= limit {
                return out;
            }
        }
    }
    for (id, score) in fused {
        if seen.insert(id) {
            out.push((id, score));
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

pub fn rrf_fuse_weighted(
    bm25: &[(u64, f32)],
    vec: &[(u64, f32)],
    k: f32,
    limit: usize,
    w_bm25: f32,
    w_vec: f32,
) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();
    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += w_bm25 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += w_vec / (k + rank as f32 + 1.0);
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
    fn test_weighted_rrf_boosts_vector_only_docs() {
        // BM25에만 있는 doc 1과 vector에만 있는 doc 2를 두고
        // vector 가중치를 1.5x 주면 두 doc이 같은 rank에서 vec쪽이 이긴다.
        let bm25 = vec![(1u64, 0.9)];
        let vec_hits = vec![(2u64, 0.5)];
        let result = rrf_fuse_weighted(&bm25, &vec_hits, 60.0, 10, 1.0, 1.5);
        let pos_1 = result.iter().position(|(id, _)| *id == 1).unwrap();
        let pos_2 = result.iter().position(|(id, _)| *id == 2).unwrap();
        assert!(
            pos_2 < pos_1,
            "1.5x vector weight should put vec-only doc above bm25-only doc at same rank"
        );
    }

    #[test]
    fn test_weighted_rrf_default_weights_match_unweighted() {
        let bm25 = vec![(1u64, 0.9), (2, 0.7)];
        let vec_hits = vec![(2u64, 0.95), (3, 0.6)];
        let a = rrf_fuse(&bm25, &vec_hits, 60.0, 10);
        let b = rrf_fuse_weighted(&bm25, &vec_hits, 60.0, 10, 1.0, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn test_bm25_anchor_pins_bm25_top_to_front() {
        // BM25 #1=A, #2=B / Vector에서 C가 #1로 정답 노이즈를 만들고
        // C는 BM25 #5에도 있어 RRF에서 A·B를 누름. anchor=2면 A·B가 앞에 고정.
        let bm25 = vec![(1u64, 30.0), (2, 15.0), (5, 5.0), (6, 4.0), (3, 3.0)];
        let vec_hits = vec![(3u64, 0.5), (4, 0.4)];
        let result = rrf_fuse_with_bm25_anchor(&bm25, &vec_hits, 60.0, 5, 1.0, 1.5, 2);
        assert_eq!(result[0].0, 1, "BM25 #1이 1위에 anchor되어야 한다");
        assert_eq!(result[1].0, 2, "BM25 #2가 2위에 anchor되어야 한다");
    }

    #[test]
    fn test_bm25_anchor_zero_falls_back_to_pure_rrf() {
        let bm25 = vec![(1u64, 0.9), (2, 0.7)];
        let vec_hits = vec![(2u64, 0.95), (3, 0.6)];
        let a = rrf_fuse_weighted(&bm25, &vec_hits, 60.0, 10, 1.0, 1.5);
        let b = rrf_fuse_with_bm25_anchor(&bm25, &vec_hits, 60.0, 10, 1.0, 1.5, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn test_rrf_limit_respected() {
        let bm25: Vec<(u64, f32)> = (0..20).map(|i| (i, 1.0)).collect();
        let result = rrf_fuse(&bm25, &[], 60.0, 5);
        assert_eq!(result.len(), 5);
    }
}
