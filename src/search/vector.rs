use std::path::Path;

use turbovec::TurboQuantIndex;

#[derive(Debug)]
pub enum VectorError {
    DimensionMismatch { expected: usize, actual: usize },
    Turbovec(String),
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionMismatch { expected, actual } =>
                write!(f, "dimension mismatch: expected {expected}, got {actual}"),
            Self::Turbovec(e) => write!(f, "turbovec: {e}"),
        }
    }
}

pub struct VectorIndex {
    inner: TurboQuantIndex,
    id_map: Vec<u64>,   // turbovec index → doc_id
    dim: usize,
}

impl VectorIndex {
    pub const BIT_WIDTH: usize = 4;

    pub fn new(dim: usize) -> Self {
        // turboquant requires dim to be a multiple of 8
        assert!(dim % 8 == 0, "dim must be a multiple of 8, got {dim}");
        Self {
            inner: TurboQuantIndex::new(dim, Self::BIT_WIDTH),
            id_map: Vec::new(),
            dim,
        }
    }

    /// vectors: flat row-major [v0_0, v0_1, ..., v0_dim, v1_0, ...]
    /// ids: parallel array of u64 doc_ids
    pub fn add(&mut self, vectors: &[f32], ids: &[u64]) -> Result<(), VectorError> {
        if vectors.len() != ids.len() * self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: ids.len() * self.dim,
                actual: vectors.len(),
            });
        }
        let normalized = l2_normalize_batch(vectors, self.dim);
        for &id in ids {
            self.id_map.push(id);
        }
        self.inner.add(&normalized);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        if self.inner.is_empty() || k == 0 {
            return vec![];
        }
        let normalized = l2_normalize(query);
        let results = self.inner.search(&normalized, k);
        results.indices.iter()
            .zip(results.scores.iter())
            .filter_map(|(&idx, &score)| {
                if idx < 0 {
                    return None;
                }
                let doc_id = self.id_map.get(idx as usize).copied()?;
                Some((doc_id, score))
            })
            .collect()
    }

    pub fn write(&self, path: &Path) -> Result<(), VectorError> {
        let path_str = path.to_str().expect("non-utf8 path");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VectorError::Turbovec(e.to_string()))?;
        }
        self.inner.write(path_str).map_err(|e| VectorError::Turbovec(e.to_string()))?;
        // Write id_map sidecar
        let idmap_path = path.with_extension("idmap");
        let idmap_bytes: Vec<u8> = self.id_map.iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        std::fs::write(&idmap_path, idmap_bytes)
            .map_err(|e| VectorError::Turbovec(e.to_string()))?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, VectorError> {
        let path_str = path.to_str().expect("non-utf8 path");
        let inner = TurboQuantIndex::load(path_str)
            .map_err(|e| VectorError::Turbovec(e.to_string()))?;
        let dim = inner.dim();

        // Load id_map sidecar
        let idmap_path = path.with_extension("idmap");
        let idmap_bytes = std::fs::read(&idmap_path)
            .map_err(|e| VectorError::Turbovec(e.to_string()))?;
        let id_map: Vec<u64> = idmap_bytes.chunks(8)
            .map(|chunk| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(chunk);
                u64::from_le_bytes(bytes)
            })
            .collect();

        Ok(Self { inner, id_map, dim })
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / norm).collect()
}

pub fn l2_normalize_batch(vectors: &[f32], dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(vectors.len());
    for chunk in vectors.chunks(dim) {
        out.extend(l2_normalize(chunk));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn unit_vec(dim: usize, hot: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; dim];
        v[hot] = 1.0;
        v
    }

    #[test]
    fn test_l2_normalize_unit_vector() {
        let v = vec![1.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < 1e-6);
        assert!(n[1].abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_non_unit() {
        let v = vec![3.0f32, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert_eq!(n.len(), 8);
    }

    #[test]
    fn test_l2_normalize_batch() {
        let dim = 8;
        let vectors = vec![3.0f32, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0];
        let out = l2_normalize_batch(&vectors, dim);
        assert_eq!(out.len(), 16);
        for chunk in out.chunks(dim) {
            let norm: f32 = chunk.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-5, "not normalized: {norm}");
        }
    }

    #[test]
    fn test_add_and_self_search() {
        let dim = 8;
        let mut idx = VectorIndex::new(dim);
        let v0 = unit_vec(dim, 0);
        let v1 = unit_vec(dim, 1);
        let v2 = unit_vec(dim, 2);

        let vectors: Vec<f32> = [v0.clone(), v1.clone(), v2.clone()].concat();
        idx.add(&vectors, &[100u64, 200u64, 300u64]).unwrap();

        let results = idx.search(&v0, 1);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 100u64);
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let mut idx = VectorIndex::new(8);
        let err = idx.add(&[1.0, 2.0], &[1u64, 2u64]);
        assert!(matches!(err, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_write_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors/index.tvim");

        let dim = 8;
        let mut idx = VectorIndex::new(dim);
        let v0 = unit_vec(dim, 0);
        let v1 = unit_vec(dim, 7);
        let vectors: Vec<f32> = [v0.clone(), v1.clone()].concat();
        idx.add(&vectors, &[42u64, 99u64]).unwrap();
        idx.write(&path).unwrap();

        let loaded = VectorIndex::load(&path).unwrap();
        assert_eq!(loaded.dim(), dim);

        let results = loaded.search(&v0, 1);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 42u64);
    }
}
