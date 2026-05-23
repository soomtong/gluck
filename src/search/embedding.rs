use fastembed::{EmbeddingModel as FastembedModel, InitOptions, TextEmbedding};

// 모델 설정 상수 (교체 시 여기만 변경)
// 추천: JinaEmbeddingsV3 (1024-dim, 한국어+코드 지원)
// 현재: AllMiniLML6V2 (384-dim, 영어 위주, 빠른 다운로드 < 50MB)
const MODEL: FastembedModel = FastembedModel::AllMiniLML6V2;
pub const MODEL_NAME: &str = "all-MiniLM-L6-v2";
pub const MODEL_DIM: usize = 384;

pub struct EmbeddingModel {
    inner: EmbeddingBackend,
}

enum EmbeddingBackend {
    Fastembed(TextEmbedding),
    Stub(usize), // dim — 테스트용
}

impl EmbeddingModel {
    /// Production: fastembed 모델 (첫 실행 시 자동 다운로드 → ~/.cache/huggingface/)
    pub fn new() -> Result<Self, String> {
        let model = TextEmbedding::try_new(
            InitOptions::new(MODEL).with_show_download_progress(true),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { inner: EmbeddingBackend::Fastembed(model) })
    }

    /// Test stub: 결정론적 해시 기반 벡터 (모델 다운로드 없음, 고품질 아님)
    pub fn new_stub(dim: usize) -> Self {
        Self { inner: EmbeddingBackend::Stub(dim) }
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        match &self.inner {
            EmbeddingBackend::Fastembed(model) => {
                let mut results = model
                    .embed(vec![text.to_string()], None)
                    .map_err(|e| e.to_string())?;
                Ok(results.remove(0))
            }
            EmbeddingBackend::Stub(dim) => Ok(stub_embed(text, *dim)),
        }
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        match &self.inner {
            EmbeddingBackend::Fastembed(model) => {
                let owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
                model.embed(owned, None).map_err(|e| e.to_string())
            }
            EmbeddingBackend::Stub(dim) => {
                Ok(texts.iter().map(|t| stub_embed(t, *dim)).collect())
            }
        }
    }

    pub fn dim(&self) -> usize {
        match &self.inner {
            EmbeddingBackend::Fastembed(_) => MODEL_DIM,
            EmbeddingBackend::Stub(dim) => *dim,
        }
    }
}

/// 결정론적 해시 기반 임베딩 (테스트/오프라인 fallback)
/// 동일 입력 → 동일 출력 (재현 가능)
/// 품질은 random에 가깝지만 파이프라인 검증에 충분
fn stub_embed(text: &str, dim: usize) -> Vec<f32> {
    let seed = text
        .bytes()
        .fold(0x517cc1b727220a95u64, |acc, b| {
            acc.wrapping_mul(0x517cc1b727220a95).wrapping_add(b as u64)
        });

    let mut v: Vec<f32> = (0..dim)
        .map(|i| {
            let x = seed
                .wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15))
                .wrapping_mul(0x6c62272e07bb0142);
            let x = x ^ (x >> 32);
            (x as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
        })
        .collect();

    // L2-normalize (turbovec은 hypersphere 위 벡터를 가정)
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut v {
        *x /= norm;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_embed_dimension() {
        let model = EmbeddingModel::new_stub(128);
        let emb = model.embed("hello world").unwrap();
        assert_eq!(emb.len(), 128);
    }

    #[test]
    fn test_stub_embed_normalized() {
        let model = EmbeddingModel::new_stub(64);
        let emb = model.embed("test text").unwrap();
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "should be L2-normalized, got {norm}");
    }

    #[test]
    fn test_stub_embed_deterministic() {
        let model = EmbeddingModel::new_stub(32);
        let a = model.embed("same text").unwrap();
        let b = model.embed("same text").unwrap();
        assert_eq!(a, b, "stub should be deterministic");
    }

    #[test]
    fn test_stub_embed_different_texts_differ() {
        let model = EmbeddingModel::new_stub(32);
        let a = model.embed("hello").unwrap();
        let b = model.embed("world").unwrap();
        assert_ne!(a, b, "different texts should produce different embeddings");
    }

    #[test]
    fn test_stub_embed_batch() {
        let model = EmbeddingModel::new_stub(16);
        let batch = model.embed_batch(&["foo", "bar", "baz"]).unwrap();
        assert_eq!(batch.len(), 3);
        for emb in &batch {
            assert_eq!(emb.len(), 16);
        }
    }
}
