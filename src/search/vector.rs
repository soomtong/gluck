use std::path::Path;

use turbovec::IdMapIndex;

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

impl VectorIndex {
    pub fn new(dim: usize) -> Self {
        Self {
            inner: IdMapIndex::new(dim, BIT_WIDTH)
                .expect("turbovec IdMapIndex::new with valid bit width"),
        }
    }

    pub fn add(&mut self, ids: &[u64], vectors: &[Vec<f32>]) -> Result<(), SearchError> {
        if ids.is_empty() {
            return Ok(());
        }
        let flat: Vec<f32> = vectors.iter().flat_map(|v| l2_normalize(v)).collect();
        self.inner
            .add_with_ids(&flat, ids)
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> bool {
        self.inner.remove(id)
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        let q = l2_normalize(query);
        let (scores, ids) = self.inner.search(&q, k);
        ids.into_iter().zip(scores).collect()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SearchError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.inner
            .write(path.to_str().unwrap_or("index.tvim"))
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(SearchError::IndexNotFound(path.to_path_buf()));
        }
        let inner = IdMapIndex::load(path.to_str().unwrap_or("index.tvim"))
            .map_err(|e| SearchError::Io(std::io::Error::other(e.to_string())))?;
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
}
