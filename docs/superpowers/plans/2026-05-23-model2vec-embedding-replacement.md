# model2vec-rs 임베딩 교체 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `fastembed` (ONNX Runtime 기반)를 `model2vec-rs` (pure Rust)로 교체하여 native 의존성을 제거한다.

**Architecture:** `src/search/embedding.rs`를 재작성해 `model2vec_rs::model::StaticModel`을 직접 래핑한다. 내부 `Backend` enum은 프로덕션 경로(`Live`)와 테스트 stub(`Stub`) 분기를 위해 유지한다. `Meta::CURRENT_VERSION`을 3으로 올려 기존 인덱스 재빌드를 강제한다.

**Tech Stack:** `model2vec-rs 0.2.1`, `turbovec 0.1.3`, `tantivy 0.22` (변경 없음)

---

### Task 1: 의존성 교체 (Cargo.toml + build.rs)

**Files:**
- Modify: `Cargo.toml`
- Modify: `build.rs`

- [ ] **Step 1: Cargo.toml에서 fastembed 제거 후 model2vec-rs 추가**

`Cargo.toml`의 `[dependencies]` 섹션을:
```toml
turbovec = "0.1.3"
tantivy = "0.22"
fastembed = "4"
```
아래로 변경한다:
```toml
turbovec = "0.1.3"
tantivy = "0.22"
model2vec-rs = "0.2.1"
```

- [ ] **Step 2: build.rs에서 Accelerate 링크 제거**

`build.rs` 전체를 아래로 교체한다:
```rust
fn main() {}
```

- [ ] **Step 3: cargo check로 컴파일 에러 확인 (embedding.rs가 아직 fastembed를 참조하므로 실패 예상)**

```bash
cargo check 2>&1 | head -20
```
Expected: `error[E0432]: unresolved import 'fastembed'` 또는 유사한 에러. 정상이다. 다음 Task에서 수정한다.

- [ ] **Step 4: 커밋**

```bash
git add Cargo.toml build.rs Cargo.lock
git commit -m "Replace fastembed with model2vec-rs dependency"
```

---

### Task 2: embedding.rs 재작성

**Files:**
- Modify: `src/search/embedding.rs`

- [ ] **Step 1: 기존 테스트가 실패함을 확인**

```bash
cargo test search::embedding 2>&1 | tail -10
```
Expected: 컴파일 에러 (fastembed import 실패).

- [ ] **Step 2: embedding.rs 전체를 아래로 교체**

```rust
use model2vec_rs::model::StaticModel;

pub const MODEL_NAME: &str = "minishlab/M2V_multilingual_output";
pub const MODEL_DIM: usize = 256;

enum Backend {
    Live(StaticModel),
    Stub(usize),
}

pub struct EmbeddingModel {
    backend: Backend,
}

impl EmbeddingModel {
    /// 첫 실행 시 HF Hub에서 자동 다운로드 → ~/.cache/huggingface/
    pub fn new() -> Result<Self, String> {
        let model = StaticModel::from_pretrained(MODEL_NAME, None)
            .map_err(|e| e.to_string())?;
        Ok(Self { backend: Backend::Live(model) })
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        match &self.backend {
            Backend::Live(model) => model.encode(texts, None).map_err(|e| e.to_string()),
            Backend::Stub(dim) => Ok(texts.iter().map(|t| stub_embed(t, *dim)).collect()),
        }
    }

    /// 단건 편의 메서드 — SearchEngine::search 에서 사용
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_batch(&[text])?
            .into_iter()
            .next()
            .ok_or_else(|| "empty embed result".to_string())
    }

    pub fn dim(&self) -> usize {
        match &self.backend {
            Backend::Live(_) => MODEL_DIM,
            Backend::Stub(d) => *d,
        }
    }
}

/// 결정론적 해시 기반 임베딩 (테스트/오프라인 전용)
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
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut v {
        *x /= norm;
    }
    v
}

#[cfg(test)]
impl EmbeddingModel {
    pub fn new_stub(dim: usize) -> Self {
        Self { backend: Backend::Stub(dim) }
    }
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
        assert_eq!(a, b);
    }

    #[test]
    fn test_stub_embed_different_texts_differ() {
        let model = EmbeddingModel::new_stub(32);
        let a = model.embed("hello").unwrap();
        let b = model.embed("world").unwrap();
        assert_ne!(a, b);
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

    #[test]
    fn test_stub_dim() {
        let model = EmbeddingModel::new_stub(48);
        assert_eq!(model.dim(), 48);
    }
}
```

- [ ] **Step 3: embedding 테스트 통과 확인**

```bash
cargo test search::embedding 2>&1
```
Expected: `test search::embedding::tests::test_stub_embed_dimension ... ok` 등 6개 테스트 PASS.  
`test_stub_embed_normalized`, `test_stub_embed_deterministic`, `test_stub_embed_different_texts_differ`, `test_stub_embed_batch`, `test_stub_dim` 모두 ok.

- [ ] **Step 4: 커밋**

```bash
git add src/search/embedding.rs
git commit -m "Rewrite embedding.rs with model2vec-rs, remove fastembed"
```

---

### Task 3: Meta 버전 3으로 올리기 + 테스트 수정

**Files:**
- Modify: `src/search/mod.rs` (CURRENT_VERSION, 테스트 2곳)
- Modify: `src/search/indexer.rs` (테스트 1곳, 주석 1곳)

- [ ] **Step 1: mod.rs의 CURRENT_VERSION과 관련 테스트 수정**

`src/search/mod.rs`에서 아래 세 곳을 수정한다.

**(A)** `Meta::CURRENT_VERSION` 변경:
```rust
// 변경 전
pub const CURRENT_VERSION: u32 = 2;

// 변경 후
pub const CURRENT_VERSION: u32 = 3;
```

**(B)** `test_meta_verify_version_current` 테스트 수정 (version 2→3, vector_dim 384→256):
```rust
#[test]
fn test_meta_verify_version_current() {
    let meta = Meta {
        version: 3,
        head_oid: "abc".into(),
        doc_count: 0,
        indexed_at: "".into(),
        model_name: "minishlab/M2V_multilingual_output".into(),
        vector_dim: 256,
        vector_backend: "turboquant_4bit".into(),
    };
    assert!(meta.verify_version().is_ok());
}
```

**(C)** `test_meta_verify_version_old` 테스트 수정 (version 2도 이제 "old"이므로 version 2 케이스 추가):
```rust
#[test]
fn test_meta_verify_version_old() {
    // v1 (brute-force f32 시절)
    let meta_v1 = Meta {
        version: 1,
        head_oid: "abc".into(),
        doc_count: 0,
        indexed_at: "".into(),
        model_name: "test".into(),
        vector_dim: 768,
        vector_backend: "brute_force".into(),
    };
    assert!(matches!(meta_v1.verify_version(), Err(SearchError::IncompatibleIndex { version: 1 })));

    // v2 (fastembed 384-dim 시절) — 이제 구버전
    let meta_v2 = Meta {
        version: 2,
        head_oid: "abc".into(),
        doc_count: 0,
        indexed_at: "".into(),
        model_name: "all-MiniLM-L6-v2".into(),
        vector_dim: 384,
        vector_backend: "turboquant_4bit".into(),
    };
    assert!(matches!(meta_v2.verify_version(), Err(SearchError::IncompatibleIndex { version: 2 })));
}
```

- [ ] **Step 2: mod.rs의 stale index 테스트 내 meta.toml 내용 수정**

`test_search_engine_stale_on_different_head` 테스트에서 `version = 2` → `version = 3`으로 변경한다:

```rust
std::fs::write(index_root.join("meta.toml"),
    "head_oid = \"abc123\"\nversion = 3\ndoc_count = 0\nindexed_at = \"\"\nmodel_name = \"minishlab/M2V_multilingual_output\"\nvector_dim = 256\nvector_backend = \"turboquant_4bit\"\n"
).unwrap();
```

- [ ] **Step 3: indexer.rs의 주석과 테스트 수정**

`src/search/indexer.rs`에서:

**(A)** 주석 수정 (줄 32 근처):
```rust
// 변경 전
/// `embedding_model` = None → uses EmbeddingModel::new() (fastembed)

// 변경 후
/// `embedding_model` = None → uses EmbeddingModel::new() (model2vec)
```

**(B)** `test_meta_version_is_2` 테스트를 `test_meta_version_is_3`으로 수정:
```rust
#[test]
fn test_meta_version_is_3() {
    let (repo_dir, repo) = init_test_repo();
    add_file_commit(&repo, "a.rs", b"fn f() {}", "Add f");

    let (_index_dir, output) = build_test_index(repo_dir.path());
    let content = std::fs::read_to_string(output.join("meta.toml")).unwrap();
    let meta: Meta = toml::from_str(&content).unwrap();
    assert_eq!(meta.version, 3);
    assert_eq!(meta.vector_backend, "turboquant_4bit");
    assert_eq!(meta.model_name, "minishlab/M2V_multilingual_output");
    assert_eq!(meta.vector_dim, 256);
}
```

- [ ] **Step 4: 전체 테스트 스위트 실행**

```bash
cargo test 2>&1
```
Expected: 모든 테스트 PASS. `fastembed` 관련 에러 없음.

실패 시: 에러 메시지를 확인하고 해당 파일의 타입/상수 불일치를 수정한다.

- [ ] **Step 5: 커밋**

```bash
git add src/search/mod.rs src/search/indexer.rs
git commit -m "Bump index meta version to 3 for model2vec migration"
```

---

### Task 4: Cargo.lock 확인 및 최종 검증

**Files:**
- Verify: `Cargo.lock` (fastembed/ort 관련 항목 제거 확인)

- [ ] **Step 1: fastembed 및 ort 의존성이 완전히 제거됐는지 확인**

```bash
grep -c "fastembed\|ort-sys\|onnxruntime" Cargo.lock
```
Expected: `0`

- [ ] **Step 2: model2vec-rs가 추가됐는지 확인**

```bash
grep "model2vec" Cargo.lock | head -5
```
Expected: `name = "model2vec-rs"` 항목 존재.

- [ ] **Step 3: release 빌드로 최종 확인**

```bash
cargo build --release 2>&1 | tail -5
```
Expected: `Finished release profile` (에러 없음).

- [ ] **Step 4: 최종 커밋**

```bash
git add Cargo.lock
git commit -m "Remove ort/ONNX Runtime transitive deps via model2vec-rs migration"
```

---

## 주의사항

- `model2vec_rs::model::StaticModel::from_pretrained(MODEL_NAME, None)` — 두 번째 인자 `None`은 캐시 디렉토리 기본값 사용(~/.cache/huggingface/hub/)을 의미한다. API가 다를 경우 `cargo doc --open`으로 확인.
- `model.encode(texts, None)` — `None`은 기본 배치 사이즈 사용. 반환 타입이 `Vec<Vec<f32>>`인지 확인.
- `glc index` 첫 실행 시 ~500MB 다운로드가 발생하므로 네트워크 환경이 필요하다. CI 환경에서는 `EmbeddingModel::new_stub`을 사용하는 테스트만 실행된다.
