use std::path::{Path, PathBuf};

use model2vec_rs::model::StaticModel;

use crate::search::SearchError;

pub const MODEL_ID: &str = "minishlab/potion-multilingual-128M";
pub const MODEL_DIM: usize = 256;

pub struct EmbeddingModel {
    inner: StaticModel,
}

impl EmbeddingModel {
    pub fn load_or_download(model_dir: &Path) -> Result<Self, SearchError> {
        let model_path = model_dir.to_str().unwrap_or(MODEL_ID);
        let model = if model_dir.exists() {
            StaticModel::from_pretrained(model_path, None, None, None)
        } else {
            StaticModel::from_pretrained(MODEL_ID, None, None, None)
        };
        let inner = model.map_err(|e| SearchError::Embedding(e.to_string()))?;
        Ok(Self { inner })
    }

    pub fn load(model_dir: PathBuf) -> Result<Self, SearchError> {
        Self::load_or_download(&model_dir)
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
