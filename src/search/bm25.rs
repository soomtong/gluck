use std::path::Path;
use std::collections::HashMap;
use super::DocMeta;
pub struct Bm25Index;
impl Bm25Index {
    pub fn open(_path: &Path) -> tantivy::Result<Self> { Ok(Self) }
    pub fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<(u64, f32)>, super::SearchError> { Ok(vec![]) }
    pub fn scan_doc_store(&self) -> tantivy::Result<HashMap<u64, DocMeta>> { Ok(HashMap::new()) }
}
