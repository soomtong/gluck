# Turbovec Integration Plan

## Context

`docs/superpowers/specs/2026-05-22-semantic-search-design.md`에 정의된 시맨틱 검색의 vector backend를 brute-force f32 matrix에서 **turbovec (TurboQuant 기반)** 으로 교체한다.

핵심 동기:

- gluck의 단일 바이너리 정체성 유지 — turbovec은 pure Rust 크레이트, native dep 없음 (`ort`와 달리)
- 메모리 footprint 8배 축소 (4-bit 양자화), 디스크 footprint도 동일
- FAISS FastScan 대비 빠른 SIMD 검색 (NEON / AVX-512BW)
- **Data-oblivious 양자화** → codebook training 없음, 인덱싱 파이프라인이 단순해짐
- IdMapIndex 기반 incremental indexing 경로를 처음부터 확보

이는 알고리즘 변경이 아니라 **vector storage + scoring 백엔드 교체**다. RRF, Tantivy BM25, 모달 UI, embedding 모델 선택 등 다른 컴포넌트는 그대로.

---

## Architectural Decisions

### D1. turbovec를 default backend로, 사용자 선택지 노출 안 함

MVP 정신에 따라 `[search.vector]` config는 추가하지 않는다. 추후 dev flag `--vector-backend brute_force`만 필요 시 도입.

### D2. 4-bit quantization 고정

| 옵션 | 압축률 | Recall (R@1, d≥768) | 비고 |
|---|---|---|---|
| 2-bit | 16x | 0–3pt 손실 | gluck 규모에서 over-engineering |
| **4-bit** | **8x** | **≈0pt 손실** | **default** |

gluck의 인덱스 규모(수천 ~ 수만 chunks)에서는 압축률보다 quality가 중요. 4-bit이면 R@1이 사실상 손실 없음.

### D3. `IdMapIndex` 사용 (`TurboQuantIndex` 아님)

향후 incremental indexing 시 stable `u64` doc_id가 필요. 처음부터 `IdMapIndex`로 가면 Phase 2 마이그레이션 비용 0.

### D4. `doc_id` space는 Tantivy와 공유

- 인덱싱 시 단조 증가 `u64` 카운터 사용
- Tantivy `id` 필드(stored TEXT)에 `doc_id.to_string()` 저장
- turbovec `IdMapIndex`에 같은 `doc_id`로 `add_with_ids` 호출
- BM25 결과 → `doc_id` 파싱 → vector 결과와 RRF fusion에서 직접 join

### D5. L2-normalize는 인덱싱 시점

turbovec은 hypersphere 위 direction을 가정. embedding 모델 출력이 normalized가 아니면 (jina-embeddings-v3, gte-multilingual-base 모두 unnormalized) 인덱싱 직전에 L2-normalize. 쿼리 임베딩도 동일.

### D6. `meta.toml.version` bump: 1 → 2

vector backend 변경으로 기존 인덱스 호환성 깨짐. version mismatch 시 `glc index --force` 유도.

---

## 변경 순서 (의존성 기준)

### Step 1: 의존성 추가 및 `VectorIndex` 추상화 — `Cargo.toml`, `src/search/vector.rs`

**Cargo.toml 추가:**

~~~toml
[dependencies]
turbovec = "0.x"  # 최신 stable로 pin (format 안정성 위해 정확한 버전 명시)
~~~

**새 타입 (`src/search/vector.rs`):**

~~~rust
use std::path::Path;
use turbovec::IdMapIndex;

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("turbovec error: {0}")]
    Turbovec(String),
}

pub struct VectorIndex {
    inner: IdMapIndex,
    dim: usize,
}

impl VectorIndex {
    pub const BIT_WIDTH: usize = 4;

    pub fn new(dim: usize) -> Self {
        Self {
            inner: IdMapIndex::new(dim, Self::BIT_WIDTH),
            dim,
        }
    }

    pub fn add(&mut self, vectors: &[f32], ids: &[u64]) -> Result<(), VectorError> {
        if vectors.len() != ids.len() * self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: ids.len() * self.dim,
                actual: vectors.len(),
            });
        }
        let normalized = l2_normalize_batch(vectors, self.dim);
        self.inner.add_with_ids(&normalized, ids);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        let normalized = l2_normalize(query);
        let (scores, ids) = self.inner.search(&normalized, k);
        ids.into_iter().zip(scores.into_iter()).collect()
    }

    pub fn write(&self, path: &Path) -> Result<(), VectorError> {
        self.inner
            .write(path.to_str().expect("non-utf8 path"))
            .map_err(|e| VectorError::Turbovec(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, VectorError> {
        let inner = IdMapIndex::load(path.to_str().expect("non-utf8 path"))
            .map_err(|e| VectorError::Turbovec(e.to_string()))?;
        let dim = inner.dim();
        Ok(Self { inner, dim })
    }
}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / norm).collect()
}

fn l2_normalize_batch(vectors: &[f32], dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(vectors.len());
    for chunk in vectors.chunks(dim) {
        out.extend(l2_normalize(chunk));
    }
    out
}
~~~

**수정 파일:** `Cargo.toml`, `src/search/vector.rs` (신규)

---

### Step 2: 저장 레이아웃 및 `meta.toml` 스키마 변경 — `src/search/indexer.rs`, `src/search/mod.rs`

기존 `.glc-index/vectors/embeddings.bin` + `doc_ids.bin` → turbovec native format `.glc-index/vectors/index.tvim` 단일 파일.

**meta.toml 변경:**

~~~toml
version = 2  # was 1
head_oid = "..."
doc_count = 187
indexed_at = 2026-05-23T...
model_name = "jina-embeddings-v3"
vector_dim = 1024
vector_backend = "turboquant_4bit"  # NEW
~~~

`Meta` 구조체에 `vector_backend: String` 필드 추가, `verify_version()` 메서드는 version != 2면 `SearchError::IncompatibleIndex` 반환.

**수정 파일:** `src/search/mod.rs` (Meta 구조체), `src/search/indexer.rs`

---

### Step 3: 인덱싱 파이프라인 통합 — `src/search/indexer.rs`

**새 흐름:**

~~~rust
let mut vector_index = VectorIndex::new(model.dim());
let mut next_doc_id: u64 = 0;

// 배치 단위로 임베딩 + add (embedding 모델 호출 효율을 위해)
for batch in chunks.chunks(config.batch_size) {
    let texts: Vec<_> = batch.iter().map(|c| c.embed_text()).collect();
    let embeddings = model.embed_batch(&texts)?;  // shape: [batch_size * dim]

    let ids: Vec<u64> = (0..batch.len() as u64)
        .map(|i| next_doc_id + i)
        .collect();

    // Tantivy 측 인덱싱
    for (chunk, &doc_id) in batch.iter().zip(ids.iter()) {
        tantivy_writer.add_document(doc!(
            id => doc_id.to_string(),
            kind => chunk.kind(),
            title => chunk.title(),
            body => chunk.body(),
            path => chunk.path(),
            commit_oid => chunk.commit_oid(),
        ))?;
    }

    // turbovec 측 인덱싱 (배치 add)
    vector_index.add(&embeddings, &ids)?;

    next_doc_id += batch.len() as u64;
    progress.advance(batch.len());
}

tantivy_writer.commit()?;
vector_index.write(&index_root.join("vectors/index.tvim"))?;
write_meta(&Meta {
    version: 2,
    doc_count: next_doc_id,
    vector_backend: "turboquant_4bit".into(),
    ..base_meta
})?;
~~~

**수정 파일:** `src/search/indexer.rs`

---

### Step 4: 검색 파이프라인 통합 — `src/search/mod.rs`, `src/search/rrf.rs`

**`SearchEngine::open`:**

~~~rust
pub struct SearchEngine {
    bm25: Bm25Index,
    vectors: VectorIndex,
    model: EmbeddingModel,
    config: SearchConfig,
}

impl SearchEngine {
    pub fn open(index_root: &Path) -> Result<Self, SearchError> {
        let meta = read_meta(index_root)?;
        meta.verify_version()?;  // version != 2 → IncompatibleIndex
        let bm25 = Bm25Index::open(&index_root.join("bm25"))?;
        let vectors = VectorIndex::load(&index_root.join("vectors/index.tvim"))?;
        let model = EmbeddingModel::load(&meta.model_name)?;
        Ok(Self { bm25, vectors, model, config: SearchConfig::from(&meta) })
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        let bm25_hits = self.bm25.search(query, self.config.bm25_top_k)?;  // Vec<(u64, f32)>
        let query_emb = self.model.embed(query)?;
        let vec_hits = self.vectors.search(&query_emb, self.config.vector_top_k);
        let fused = rrf_fuse(&bm25_hits, &vec_hits, self.config.rrf_k, self.config.result_limit);
        self.hydrate(fused)  // doc_id → SearchResult (path/title/score)
    }
}
~~~

**`rrf_fuse` 시그니처 변경 — `u64` doc_id 기반:**

~~~rust
pub fn rrf_fuse(
    bm25: &[(u64, f32)],
    vec: &[(u64, f32)],
    k: f32,
    limit: usize,
) -> Vec<(u64, f32)> {
    let mut scores: std::collections::HashMap<u64, f32> = std::collections::HashMap::new();
    for (rank, (id, _)) in bm25.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
    }
    let mut out: Vec<_> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}
~~~

**`Bm25Index::search` 반환을 `Vec<(u64, f32)>`로 통일** — Tantivy의 stored `id` 필드를 `u64::parse`로 파싱해서 반환. fusion 단계에서 ID 변환 분기 제거.

**수정 파일:** `src/search/mod.rs`, `src/search/bm25.rs`, `src/search/rrf.rs`

---

### Step 5: 마이그레이션 처리 — `src/cli.rs`, `src/search/modal.rs`

**`glc index` 동작:**

- `.glc-index/`가 이미 존재하고 `meta.version == 2`이며 `head_oid` 일치 → no-op + 메시지
- 존재하지만 `meta.version < 2`이거나 누락 → "Index format upgraded. Re-indexing required." 안내 후 `--force`인 경우에만 재구축
- 존재하지 않음 → 신규 구축

**CLI 플래그:**

~~~rust
enum Commands {
    Index {
        path: Option<PathBuf>,
        #[arg(long, default_value = "32")]
        batch_size: usize,
        #[arg(long, default_value = "1048576")]
        max_file_size: usize,
        /// Force rebuild even if index exists and is current
        #[arg(long)]
        force: bool,
    },
}
~~~

**TUI 모달 분기:**

- `SearchError::IncompatibleIndex` → "Index format outdated. Run `glc index --force` to rebuild." 표시
- 기존 "No index found" 모달과 동일한 UX 레이어 재사용

**수정 파일:** `src/cli.rs`, `src/search/modal.rs`, `src/search/mod.rs` (SearchError variant 추가)

---

### Step 6: 검증

각 Step 완료 후:

1. `cargo build` — 컴파일 확인
2. `cargo test` — 신규 단위 테스트 (아래 표 참고) + 기존 통과
3. **End-to-end 검증** (gluck 자체 repo로):
   - `glc index` → `.glc-index/` 생성, `vectors/index.tvim` 존재 확인
   - `glc` → `S` → "에러 처리" / "git history" 입력 → top-5 결과 sanity check
   - `glc index --force` 재실행 → 결정성 검증 (같은 머신에서 byte-identical 인덱스)
4. **메모리/디스크 footprint 측정** (선택):
   - `du -sh .glc-index/vectors/` → 예상치(약 `doc_count × dim / 2` bytes + rotation matrix 약 `4 × dim²` bytes)와 비교
   - 1024-dim, 1000 docs 기준: vectors ~512KB + rotation 4MB ≈ 4.5MB
5. `cargo clippy` — 경고 없는지 확인

---

## 새 단위 테스트

| 모듈 | 테스트 |
|---|---|
| `VectorIndex::add/search` | normalize 정확성, k=1 self-query 검증, 빈 인덱스 검색 |
| `VectorIndex::write/load` | 직렬화 round-trip, dim 보존, ID 보존 |
| `rrf_fuse` | u64 doc_id 기반 fusion 정확성, 한쪽이 비어 있을 때, 중복 ID 처리 |
| `Meta::verify_version` | version 1/2/누락 모두 적절한 에러 |
| `Bm25Index::search` | `Vec<(u64, f32)>` 반환, parse 실패 시 에러 |
| `Indexer` | 통합 테스트: `init_test_repo()` → 인덱스 → search → top-k 검증 |

---

## Scope 외 (deferred)

- **Incremental indexing**: `IdMapIndex::remove(doc_id)` 기반 점진적 업데이트는 향후 별도 plan
- **2-bit / 8-bit 옵션화**: `BIT_WIDTH = 4` hard-coded
- **사용자 config 노출**: backend 선택지를 config.toml에 추가하지 않음
- **HNSW / IVF 등 ANN 결합**: turbovec의 brute-force scoring으로 gluck 규모에서는 충분
- **Rotation matrix 외부화**: 현재 turbovec format에 포함되어 있으므로 별도 관리 불필요

---

## 디자인 메모

이 통합의 우아함은 turbovec의 **data-oblivious 양자화** 특성이 gluck의 **per-repo, full-reindex** 패턴과 자연스럽게 맞물린다는 점이다. PQ 계열이라면 codebook training이 인덱싱 파이프라인의 별도 단계로 들어가야 하고, drift 추적 메타데이터(언제 학습됐는지, 어느 데이터로 학습됐는지)도 필요해진다. TurboQuant는 random rotation + Lloyd-Max scalar quantization으로 끝나므로, 인덱싱 파이프라인의 mental model이 단순해진다 — *embedding이 들어가면 압축된 바이트가 나온다, 그게 전부다.*

`Chunk` → `(doc_id: u64, embedding: Vec<f32>)` → `VectorIndex::add` → `index.tvim` 라는 단방향 데이터 흐름이 `swift-spinning-whale.md`의 "타입이 강제하는 파이프라인" 정신과 일관된다. `VectorIndex`는 외부에서 보면 `add → search → write/load`라는 네 가지 동작만 가진 작은 surface, 내부에서는 양자화·압축·SIMD 스코어링이 캡슐화되어 있다.

또한 `Bm25Index`와 `VectorIndex`가 같은 `Vec<(u64, f32)>` 형태로 결과를 반환하게 통일함으로써, RRF fusion은 "두 ranked list를 받아 합친다"는 추상화에 충실해진다. 둘 중 하나를 다른 quantizer(예: 향후 RaBitQ, ScaNN-style asymmetric quantization)로 교체해도 fusion 코드는 그대로다.
