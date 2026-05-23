# model2vec-rs 임베딩 교체 설계

## 목표

`fastembed` (ONNX Runtime 기반, native dep 있음)를 `model2vec-rs` (pure Rust, native dep 없음)로 교체한다.  
gluck의 "단일 바이너리, native 의존성 없음" 원칙을 회복하면서 한국어를 포함한 다국어 시맨틱 검색을 유지한다.

## 배경

- `fastembed`는 내부적으로 `ort` (ONNX Runtime C++ 바이너리)를 사용한다.
- CodeBERT+ort를 폐기한 이유와 동일한 문제가 fastembed에도 존재한다.
- `model2vec-rs`는 정적 임베딩(word embedding lookup) 방식으로, ONNX 추론 없이 pure Rust로 동작한다.

## 선택 모델

**`minishlab/M2V_multilingual_output`**

| 항목 | 값 |
|------|----|
| 차원 | 256-dim |
| 언어 | 101개 (한국어 포함) |
| 방식 | 정적 임베딩 (transformer 추론 불필요) |
| 속도 | ~14,600 samples/sec |
| 다운로드 | 첫 `glc index` 실행 시 HF Hub에서 자동 다운로드 (~500MB) |
| 캐시 위치 | `~/.cache/huggingface/hub/` |

fastembed AllMiniLML6V2 (384-dim) 대비 차원이 줄어들지만, 한국어 지원 품질은 향상된다.

## 변경 범위

| 파일 | 변경 |
|------|------|
| `Cargo.toml` | `fastembed = "4"` 제거, `model2vec-rs = "0.2.1"` 추가 |
| `build.rs` | macOS Accelerate 프레임워크 링크 제거 (fastembed 전용) |
| `src/search/embedding.rs` | 전면 재작성 |
| `src/search/mod.rs` | `Meta::vector_dim` 기본값 256 반영, `verify_version()` v3 추가 |
| 인덱스 meta | `version` 2 → 3 |

`vector.rs`, `bm25.rs`, `rrf.rs`, `chunk.rs`, `indexer.rs`, `modal.rs`, `app.rs` 등은 변경 없다.

## 새 `embedding.rs` 구조

`EmbeddingBackend` enum을 제거하고 `StaticModel`을 직접 래핑한다.

```rust
use model2vec_rs::model::StaticModel;

pub const MODEL_NAME: &str = "minishlab/M2V_multilingual_output";
pub const MODEL_DIM: usize = 256;

pub struct EmbeddingModel {
    model: StaticModel,
}

impl EmbeddingModel {
    pub fn new() -> Result<Self, String> {
        let model = StaticModel::from_pretrained(MODEL_NAME, None)
            .map_err(|e| e.to_string())?;
        Ok(Self { model })
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        self.model.encode(texts, None)
            .map_err(|e| e.to_string())
    }

    pub fn dim(&self) -> usize { MODEL_DIM }
}

#[cfg(test)]
impl EmbeddingModel {
    pub fn new_stub(dim: usize) -> Self { ... }
}
```

`embed()` (단건) 메서드는 제거한다. `indexer.rs`가 `embed_batch()`만 사용하므로 불필요하다.

## 모델 다운로드 흐름

```
glc index 실행
  └─ EmbeddingModel::new()
       └─ StaticModel::from_pretrained("minishlab/M2V_multilingual_output")
            ├─ 캐시 존재 → 즉시 로드
            └─ 캐시 없음 → HF Hub 다운로드 (~500MB) + 진행 출력
```

다운로드 후 `~/.cache/huggingface/hub/`에 캐시된다. 이후 실행은 오프라인으로 동작.

## 인덱스 호환성

차원 변경(384 → 256)으로 기존 v2 인덱스는 재빌드가 필요하다.

`meta.toml`의 `version`을 3으로 올리면 기존 `verify_version()` 로직이 자동으로 `incompatible` 상태를 감지한다. 사용자는 TUI 모달에서 "Index format outdated — run `glc index --force`" 안내를 받는다.

```toml
# .glc-index/meta.toml (v3)
version = 3
model_name = "minishlab/M2V_multilingual_output"
vector_dim = 256
head_oid = "..."
```

## 제거되는 것

- `fastembed` 크레이트 (및 `ort`, ONNX Runtime 전이 의존성 전체)
- `build.rs`의 `cargo:rustc-link-lib=framework=Accelerate`
- `EmbeddingBackend` enum
- `embed()` 단건 메서드
- `Cargo.lock`에서 수천 줄의 ONNX 관련 의존성

## 테스트 전략

- `EmbeddingModel::new_stub(dim)` — `#[cfg(test)]`로 유지, 모든 기존 단위 테스트 통과
- `embedding.rs` 단위 테스트: stub 기반 dimension / normalize / deterministic / batch 테스트 유지
- `indexer.rs` 통합 테스트: stub 모델로 전체 파이프라인 검증
- 실제 모델 다운로드가 필요한 테스트는 `#[ignore]`로 분리
