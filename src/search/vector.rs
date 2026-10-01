use std::path::Path;

use turbovec::{CalibrationState, IdMapIndex};

use crate::search::SearchError;

const BIT_WIDTH: usize = 4;

pub struct VectorIndex {
    inner: IdMapIndex,
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-12 {
        v.to_vec()
    } else {
        v.iter().map(|x| x / norm).collect()
    }
}

fn io_err(e: impl std::fmt::Display) -> SearchError {
    SearchError::Io(std::io::Error::other(e.to_string()))
}

impl VectorIndex {
    pub fn new(dim: usize) -> Self {
        Self {
            inner: IdMapIndex::new(dim, BIT_WIDTH)
                .expect("turbovec IdMapIndex::new with valid bit width"),
        }
    }

    /// Fit TQ+ calibration from `vectors` before the first `add`.
    ///
    /// turbovec 1.0 no longer calibrates implicitly on the first add; an
    /// uncalibrated index silently loses recall. Samples evenly spaced rows
    /// (deterministic, so rebuilds are reproducible) up to turbovec's
    /// recommended sample size. Too few rows leaves the index uncalibrated.
    pub fn calibrate(&mut self, vectors: &[Vec<f32>]) -> Result<(), SearchError> {
        if vectors.len() < turbovec::MIN_CALIBRATION_ROWS {
            return Ok(());
        }
        let n = vectors.len().min(turbovec::RECOMMENDED_CALIBRATION_ROWS);
        let sample: Vec<f32> = (0..n)
            .flat_map(|i| l2_normalize(&vectors[i * vectors.len() / n]))
            .collect();
        self.inner.calibrate(&sample).map_err(io_err)
    }

    pub fn is_calibrated(&self) -> bool {
        self.inner.calibration_state() == CalibrationState::Calibrated
    }

    pub fn add(&mut self, ids: &[u64], vectors: &[Vec<f32>]) -> Result<(), SearchError> {
        if ids.is_empty() {
            return Ok(());
        }
        let flat: Vec<f32> = vectors.iter().flat_map(|v| l2_normalize(v)).collect();
        self.inner.add_with_ids(&flat, ids).map_err(io_err)?;
        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> bool {
        self.inner.remove(id)
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        self.search_inner(query, k, None)
    }

    /// Search only among `allowlist` ids. Ids absent from the index are
    /// dropped (turbovec rejects unknown ids); an empty effective allowlist
    /// returns no hits.
    pub fn search_allowlist(&self, query: &[f32], k: usize, allowlist: &[u64]) -> Vec<(u64, f32)> {
        let known: Vec<u64> = allowlist
            .iter()
            .copied()
            .filter(|id| self.inner.contains(*id))
            .collect();
        if known.is_empty() {
            return Vec::new();
        }
        self.search_inner(query, k.min(known.len()), Some(&known))
    }

    fn search_inner(&self, query: &[f32], k: usize, allowlist: Option<&[u64]>) -> Vec<(u64, f32)> {
        let q = l2_normalize(query);
        match self.inner.try_search_with_allowlist(&q, k, allowlist) {
            Ok(res) => res.ids.into_iter().zip(res.scores).collect(),
            Err(e) => {
                tracing::warn!("vector search failed: {e}");
                Vec::new()
            }
        }
    }

    /// Persist via turbovec's incremental `sync`: the first call for a path
    /// writes the whole index, later calls append only adds/removes.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), SearchError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.inner.sync(path).map_err(io_err)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(SearchError::IndexNotFound(path.to_path_buf()));
        }
        let inner = IdMapIndex::load(path).map_err(io_err)?;
        // Build packed layout + id map now so the first search/remove doesn't stall.
        inner.prepare();
        Ok(Self { inner })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vec(val: f32, dim: usize) -> Vec<f32> {
        vec![val; dim]
    }

    #[test]
    fn test_l2_normalize_unit_vector() {
        let v = vec![1.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert_eq!(n, v);
    }

    #[test]
    fn test_search_allowlist_restricts_and_skips_unknown_ids() {
        let dim = 16;
        let mut idx = VectorIndex::new(dim);
        idx.add(
            &[1, 2, 3],
            &[make_vec(1.0, dim), make_vec(0.9, dim), make_vec(-1.0, dim)],
        )
        .unwrap();
        // id 99 is not in the index and must not turn the search into an error.
        let hits = idx.search_allowlist(&make_vec(1.0, dim), 10, &[3, 99]);
        let ids: Vec<u64> = hits.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![3]);
        assert!(idx
            .search_allowlist(&make_vec(1.0, dim), 10, &[99])
            .is_empty());
        assert!(idx
            .search_allowlist(&make_vec(1.0, dim), 10, &[])
            .is_empty());
    }

    #[test]
    fn test_add_and_search() {
        let dim = 16;
        let mut idx = VectorIndex::new(dim);
        idx.add(&[1, 2], &[make_vec(1.0, dim), make_vec(0.1, dim)])
            .unwrap();
        let results = idx.search(&make_vec(1.0, dim), 2);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.tvim");
        let dim = 16;
        let mut idx = VectorIndex::new(dim);
        idx.add(&[10], &[make_vec(0.5, dim)]).unwrap();
        idx.save(&path).unwrap();
        let loaded = VectorIndex::load(&path).unwrap();
        let results = loaded.search(&make_vec(0.5, dim), 1);
        assert_eq!(results[0].0, 10);
    }

    #[test]
    fn test_remove_drops_from_search() {
        let dim = 16;
        let mut idx = VectorIndex::new(dim);
        idx.add(&[1, 2], &[make_vec(1.0, dim), make_vec(0.1, dim)])
            .unwrap();
        assert!(idx.remove(1));
        let results = idx.search(&make_vec(1.0, dim), 5);
        assert!(results.iter().all(|(id, _)| *id != 1));
        assert!(!idx.remove(1), "second remove of same id returns false");
    }

    #[test]
    fn test_add_duplicate_id_returns_error() {
        let dim = 16;
        let mut idx = VectorIndex::new(dim);
        idx.add(&[42], &[make_vec(1.0, dim)]).unwrap();
        let err = idx.add(&[42], &[make_vec(0.5, dim)]);
        assert!(err.is_err(), "duplicate id should not be silently accepted");
    }

    fn varied_vecs(n: usize, dim: usize) -> Vec<Vec<f32>> {
        (0..n)
            .map(|i| (0..dim).map(|j| ((i * 31 + j * 7) as f32).sin()).collect())
            .collect()
    }

    #[test]
    fn test_calibrate_persists_through_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.tvim");
        let dim = 16;
        let vecs = varied_vecs(64, dim);
        let ids: Vec<u64> = (0..64).collect();
        let mut idx = VectorIndex::new(dim);
        assert!(!idx.is_calibrated());
        idx.calibrate(&vecs).unwrap();
        assert!(idx.is_calibrated());
        idx.add(&ids, &vecs).unwrap();
        idx.save(&path).unwrap();
        let loaded = VectorIndex::load(&path).unwrap();
        assert!(loaded.is_calibrated());
        assert_eq!(loaded.search(&vecs[5], 1)[0].0, 5);
    }

    #[test]
    fn test_calibrate_skips_tiny_sample() {
        let mut idx = VectorIndex::new(16);
        idx.calibrate(&varied_vecs(1, 16)).unwrap();
        assert!(!idx.is_calibrated());
    }

    #[test]
    fn test_incremental_save_after_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.tvim");
        let dim = 16;
        let vecs = varied_vecs(8, dim);
        let mut idx = VectorIndex::new(dim);
        idx.calibrate(&vecs).unwrap();
        idx.add(&[1, 2, 3, 4], &vecs[..4]).unwrap();
        idx.save(&path).unwrap();

        let mut reopened = VectorIndex::load(&path).unwrap();
        assert!(reopened.remove(2));
        reopened.add(&[5, 6], &vecs[4..6]).unwrap();
        reopened.save(&path).unwrap();

        let loaded = VectorIndex::load(&path).unwrap();
        let ids: Vec<u64> = loaded
            .search(&vecs[4], 8)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids[0], 5);
        assert!(!ids.contains(&2));
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn test_search_wrong_dim_returns_empty() {
        let mut idx = VectorIndex::new(16);
        idx.add(&[1], &[make_vec(1.0, 16)]).unwrap();
        assert!(idx.search(&make_vec(1.0, 8), 1).is_empty());
    }
}
