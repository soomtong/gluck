use model2vec_rs::model::StaticModel;

use crate::search::SearchError;

pub const MODEL_ID: &str = "minishlab/potion-multilingual-128M";
pub const MODEL_DIM: usize = 256;

pub struct EmbeddingModel {
    inner: StaticModel,
}

impl EmbeddingModel {
    pub fn load() -> Result<Self, SearchError> {
        let inner = StaticModel::from_pretrained(MODEL_ID, None, None, None)
            .map_err(|e| SearchError::Embedding(e.to_string()))?;
        Ok(Self { inner })
    }

    pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, SearchError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let embeddings = self.inner.encode(texts);
        Ok(embeddings)
    }

    pub fn encode_single(&self, text: &str) -> Result<Vec<f32>, SearchError> {
        let batch = self.encode_batch(&[text.to_string()])?;
        batch
            .into_iter()
            .next()
            .ok_or_else(|| SearchError::Embedding("empty embedding output".to_string()))
    }

    pub fn dim(&self) -> usize {
        MODEL_DIM
    }
}
