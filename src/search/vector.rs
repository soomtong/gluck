use std::path::Path;
pub struct VectorIndex;
impl VectorIndex {
    pub fn load(_path: &Path) -> Result<Self, String> { Ok(Self) }
    pub fn search(&self, _query: &[f32], _k: usize) -> Vec<(u64, f32)> { vec![] }
}
