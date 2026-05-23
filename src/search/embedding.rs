pub struct EmbeddingModel;
impl EmbeddingModel {
    pub fn new() -> Result<Self, String> { Ok(Self) }
    pub fn embed(&self, _text: &str) -> Result<Vec<f32>, String> { Ok(vec![]) }
    pub fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, String> { Ok(vec![]) }
    pub fn dim(&self) -> usize { 384 }
}
