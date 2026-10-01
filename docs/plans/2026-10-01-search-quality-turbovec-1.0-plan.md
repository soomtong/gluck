# 검색 품질 최적화 계획 — turbovec 1.0 전환 중심

- 작성일: 2026-10-01
- 기준: `turbovec 0.8.0` (Cargo.lock), 최신 `1.0.0` (crates.io)
- 마지막 품질 리포트: `docs/reports/report-2026-05-27-5.md` (MRR 0.563, R@5 0.760, R@10 0.860, NDCG@10 0.688, negative pass 80%)

## 1. turbovec 0.8.0 → 1.0.0 변경사항 검토

두 버전의 crate 소스를 받아 직접 diff 비교함 (`/tmp/tv-review`).

### 1.1 우리 코드에 영향 있는 변경

| 항목 | 0.8.0 | 1.0.0 | gluck 영향 |
|---|---|---|---|
| 파일 포맷 | TVIM v3 (flat snapshot) | v7 컨테이너 (superblock + 헤더 슬롯 2개 + block unit). `load()`는 v7만 읽고 구버전은 `legacy_format_error` | 기존 `.glc-index/vectors/index.tvim` 로드 실패 → `INDEX_VERSION` 7 → 8로 올려 전체 재빌드를 강제해야 함 |
| TQ+ calibration | 첫 `add` 때 자동 fit, 이후 고정 | 명시적 `calibrate(sample)` 호출. 호출 안 하면 `CalibrationState::Uncalibrated` (plain TurboQuant) | 그대로 올리면 조용히 품질 하락 (upstream 수치: R@10 평균 -2.5pp, 최대 -8.7pp). 반드시 calibrate 추가 |
| calibration 기준 | 5/95% quantile 고정 | codebook 기반 확률 레벨 (4-bit ≈ 0.996). 4-bit 기준 recall 개선 | calibrate만 하면 자동 적용 |
| 의존성 | `ndarray 0.17` + BLAS(`cblas-sys`), `faer`, build.rs가 openblas/Accelerate 링크 | `rayon`, `rand`, `rand_chacha =0.3.1`, `statrs =0.17.1`만 남음. build.rs 제거 | `blas-src`/`openblas-src`가 오직 turbovec 0.8 때문에 필요했음 (`cargo tree -i cblas-sys` 확인, model2vec의 ndarray 0.15는 BLAS 미사용). 제거 가능 → CI `libopenblas-dev`/`-lopenblas` 정리 |
| MSRV | 1.70 | 1.89 (x86 AVX-512 intrinsic 안정화 기준) | 로컬 1.97.1, CI stable이면 문제 없음 |
| 영속화 | `write`/`load` | `write`/`load` + `sync(path)` 증분 저장 (fsync 1회, crash-safe) | 증분 업데이트 경로에서 전체 재작성 대신 `sync` 사용 가능 |
| 검색 API | `search`, `search_with_allowlist` | 동일 + `try_search*` (패닉 대신 `SearchError`: dim 불일치, NaN/범위 밖 값) | `try_search`로 교체하면 비정상 쿼리 벡터에서 패닉 방지 |
| 경고 출력 | 없음 | `set_warning_hook` (기본 stderr). durability shortfall 경고 | TUI alternate screen 오염 방지를 위해 `tracing`으로 포워딩하는 hook 설치 |
| `prepare()` | 패킹만 | id→slot 맵까지 미리 빌드 | 엔진 로드 스레드에서 `prepare()` 호출하면 첫 검색/remove 지연 제거 |

### 1.2 성능 관련 (품질 무관, 참고)

- 4-bit 검색 커널이 NEON SDOT/SMMLA, AVX-512 VNNI로 교체되어 FAISS 대비 평균 3.4~3.5배 (0.8은 12~20% 우위). 현재 p50 0.17ms라 체감 차이는 작음.
- `remove`는 O(1) swap-and-pop 유지.
- `add`는 packed row를 버리므로 add 후 첫 검색에서 재패킹 — 인덱서 끝에서 `prepare()` 한 번이면 충분.

## 2. 현재 검색 파이프라인의 품질 약점

`src/search/mod.rs::search` 기준.

1. 벡터 후보가 너무 적음: `candidate_limit = limit * 2` (path 필터 시 `limit*4`). 벡터는 flat scan이라 후보를 늘려도 비용이 거의 없는데 RRF 입력이 얇음.
2. path 필터가 후처리: 상위 후보를 뽑은 뒤 `apply_path_filter`로 걸러서, 해당 경로 문서가 후보 밖이면 누락. turbovec `search_with_allowlist`로 경로 매칭 id만 대상으로 검색하면 해결.
3. 양자화 오차 보정 없음: 4-bit 점수로만 순위 결정. 문서 수가 수백~수천 규모라 float32 원본(256-dim × 4B × N, 447 docs ≈ 450KB)을 같이 저장해 상위 후보를 exact cosine으로 rerank할 여유가 충분함.
4. 벡터 점수 하한 없음: negative 쿼리 `django orm migrations`가 rank 8/10에 코드 결과를 노출. 벡터 hit에 최소 유사도 threshold 필요.
5. typo 카테고리 취약 (MRR 0.329, `reciprical rank fussion`, `revwalk topolgical` 미검출): BM25가 `ngram_2_2`라 일부 흡수하지만 identifier 오타는 놓침.
6. 리포트 버그: per-query NDCG@10이 1.062 (`tantivvy delete_trm`)로 1을 넘음 — 중복 relevant hit 이중 계산 의심 (`src/search/report/metrics.rs`).

## 3. 실행 계획

각 단계 후 `glc index --force && glc report --out docs/reports/report-YYYY-MM-DD-N.md`로 측정, 이전 리포트와 비교. 단계별 커밋.

### Phase 0. 기준선 재측정

- 현재 HEAD에서 리포트 생성 (5월 이후 코드 변경 반영). 이후 모든 비교의 기준.
- `metrics.rs` NDCG > 1 버그 먼저 수정 (측정 도구가 틀리면 이후 비교 무의미).

### Phase 1. turbovec 1.0 전환 (동작 동등 + calibration 보존)

1. `Cargo.toml`: `turbovec = "1.0"`. `blas-src`/`openblas-src` 제거 후 macOS/Linux 빌드 확인. CI의 `libopenblas-dev` 설치와 `RUSTFLAGS: -C link-arg=-lopenblas` 제거. 링크 에러가 나면 원인 확인 후 되돌림.
2. `vector.rs`
   - `VectorIndex::calibrate(&[Vec<f32>])` 추가: 정규화된 벡터에서 최대 1024개 uniform random 샘플(문서 수가 적으면 전체) → `inner.calibrate`.
   - `search` → `try_search` 기반으로 바꾸고 에러는 빈 결과 + `tracing::warn`.
   - `load` 후 `prepare()` 호출.
   - `calibration_state()` 노출 (리포트 Index 섹션에 표기).
3. `indexer.rs`
   - 전체 빌드: `vector.add` 전에 전 코퍼스로 `calibrate`.
   - 증분 업데이트: 로드된 인덱스의 calibration이 파일에 함께 저장되므로 재calibrate하지 않음 (4-bit는 refit마다 재인코딩 손실). `save`(전체 write) 대신 `sync` 사용 검토 — `write`로 만든 파일은 unclaimed snapshot이라 첫 `sync`가 claim함.
4. `INDEX_VERSION` 7 → 8. 구버전 인덱스는 기존 경로대로 full rebuild.
5. `turbovec::set_warning_hook`으로 경고를 `tracing`에 연결 (`main.rs` 초기화).
6. 테스트: 기존 `vector.rs` 테스트 유지 + calibrate 후 `calibration_state() == Calibrated`, save/load 후에도 유지, 0.8 포맷 파일 로드 시 에러 → rebuild 경로.
7. 기대 결과: 품질 지표 ±노이즈 이내 (0.8도 첫 add에서 전체 코퍼스로 calibrate했으므로). 떨어지면 calibrate 누락/샘플링 문제.

#### Phase 1 결과 (2026-10-01, 완료)

- 적용: `turbovec = "1.0"`, `blas-src`/`openblas-src` 제거 (Cargo.lock 약 640줄 감소), CI OpenBLAS 단계 제거, `VectorIndex::calibrate` (균등 간격 결정적 샘플, 최대 `RECOMMENDED_CALIBRATION_ROWS`=1000, `MIN_CALIBRATION_ROWS` 미만이면 생략), `save`를 `sync` 기반으로 변경, `try_search`, load 후 `prepare()`, warning hook → `tracing`, `INDEX_VERSION` 8.
- 샘플링은 RNG 대신 균등 stride로 해서 의존성 추가 없이 재현성 확보.
- 테스트: 단위 354개 통과, `--ignored` e2e 3개(전체/증분 인덱싱, report) 통과, clippy `-D warnings` 통과.
- 실측 (612 docs, 같은 HEAD `04882e7`):

| 지표 | 0.8 (`report-2026-10-01-1`) | 1.0 (`report-2026-10-01-2`) |
|---|---|---|
| MRR | 0.610 | 0.607 |
| Recall@5 | 0.760 | 0.760 |
| Recall@10 | 0.780 | 0.820 |
| typo MRR | 0.250 | 0.270 |
| negative pass | 20% | 40% |
| p50 / QPS | 0.13ms / 7961 | 0.13ms / 8027 |

- 결론: 품질과 성능 모두 동등하거나 약간 개선됨. NDCG@10은 리포트 버그(값이 1을 넘음)로 비교에서 제외 — Phase 0 항목이라 아직 수정하지 않음.
- 미검증: Linux/Windows에서 BLAS 없이 빌드되는지는 CI에서 확인해야 함 (로컬은 macOS만).

#### Phase 0 보완 (2026-10-01, 완료)

- NDCG/Recall 버그 수정 (`ad601cc`): 정답 항목별로 첫 매칭 순위만 인정. 이전 리포트의 NDCG는 비교 불가.
- RRF 동점 비결정성 수정: `HashMap` 순회 순서에 따라 동점 결과 순서가 실행마다 바뀌었음. 개별 리스트 최고 순위 → id 순으로 tie-break. 리포트 3회 연속 per-query 결과 동일 확인.
- Phase 2 이후 비교 기준선: `report-2026-10-01-4.md` (MRR 0.607, R@5 0.760, R@10 0.820, NDCG@10 0.641, negative pass 40%).

### Phase 2. 벡터 recall 확보

1. 벡터 후보 수 분리: `vec_candidates = max(limit * 5, 50)`. BM25 후보는 현행 유지. RRF 출력은 `candidate_limit`로 자름.
2. path 필터 allowlist화: `doc_store`에서 경로 prefix 매칭 id 집합 생성 → `search_with_allowlist`. BM25는 기존 QueryParser `path:` 처리 유지. `apply_path_filter`는 BM25 쪽 안전망으로만 남김.
3. exact rerank: `vectors/raw.f32`에 id 순서대로 float32 원본 저장 (meta에 경로/개수 기록). 벡터 상위 `vec_candidates`를 exact cosine으로 재정렬 후 RRF 투입. 증분 업데이트 시 삭제/추가 반영 필요 — id→offset 맵을 같이 저장하거나 compaction 시점에 재작성.
   - 진행 전 계측: 리포트에 "4-bit top-10 vs exact top-10 overlap" 지표를 추가해 양자화 오차가 실제로 순위를 흔드는지 먼저 확인. overlap이 0.95 이상이면 3번은 보류.

### Phase 3. 노이즈 억제 / fusion 튜닝

1. 벡터 hit 최소 유사도 threshold (exact cosine 기준). negative 쿼리 5개와 positive 25개의 정답 hit 점수 분포를 리포트에 덤프해 값 결정.
2. RRF `k=60`, 가중치 grid 탐색 (`k ∈ {20, 40, 60}`, `w_vec ∈ {1.0, 1.5, 2.0}`)을 `glc report`에 숨은 옵션으로 넣어 비교. 과적합 방지를 위해 fixture 쿼리를 카테고리당 5개 → 8개 이상으로 확장한 뒤 결정.
3. typo: BM25에 identifier 전용 필드(심볼명 snake/camel 분해 후 ngram_3) 또는 tantivy `FuzzyTermQuery`(distance 1)를 영문 단일 토큰에만 적용하는 방안 비교. 범위가 커서 Phase 1~2 결과 본 뒤 별도 spec으로 분리.

## 4. 리스크

- `statrs`/`rand_chacha`가 정확 버전 pin이라 다른 의존성과 충돌 시 resolver 실패 가능 → `cargo update -p turbovec` 단계에서 확인.
- Windows 릴리스 빌드: BLAS 제거로 오히려 단순해지지만, x86 `x86-64-v2` baseline + 런타임 dispatch로 바뀐 점을 릴리스 워크플로에서 한 번 확인.
- calibrate 샘플링에 RNG가 들어가면 인덱스 재현성이 깨짐 → 고정 seed 사용.

## 5. 문서 갱신

- `CLAUDE.md`: `INDEX_VERSION` 표기가 "currently 5"로 남아 있음 (실제 7, 이번 작업 후 8). BLAS 항목 제거/수정.
