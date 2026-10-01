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

#### Phase 2 결과 (2026-10-01, 완료)

- 벡터 후보 확대 (`max(limit*5, 50)`): 기각. R@5 0.760→0.720, negative pass 40%→20%. 벡터 하위권 노이즈가 RRF에 섞임. `candidate_limit` 유지.
- path 필터 allowlist (`2bbf8ad`): 적용. 경로 쿼리 7개 수동 비교에서 이전 0건이던 쿼리 2개 포함 모두 결과를 채움. path 없는 쿼리는 기준선과 동일.
- exact rerank: 보류. 4-bit vs exact (620 chunks, fixture 30개) 평균 overlap@10 0.950 / @20 0.953 / @50 0.964, exact top-1은 30/30 동일, 점수 오차 평균 0.0037. 정답 순위 변화 7건 모두 1~5칸이고 RRF 입력 범위(top 20/40) 경계를 넘는 경우 없음.
- 관찰: 벡터가 정답을 놓치는 원인은 양자화가 아니라 임베딩/chunk 텍스트. positive 25개 중 7개는 exact로도 top 50 밖이고, 벡터 top-10의 약 51%가 Commit 문서. Phase 3에서 다룰 대상.

### Phase 3. 노이즈 억제 / fusion 튜닝

1. 벡터 hit 최소 유사도 threshold (exact cosine 기준). negative 쿼리 5개와 positive 25개의 정답 hit 점수 분포를 리포트에 덤프해 값 결정.
2. RRF `k=60`, 가중치 grid 탐색 (`k ∈ {20, 40, 60}`, `w_vec ∈ {1.0, 1.5, 2.0}`)을 `glc report`에 숨은 옵션으로 넣어 비교. 과적합 방지를 위해 fixture 쿼리를 카테고리당 5개 → 8개 이상으로 확장한 뒤 결정.
3. typo: BM25에 identifier 전용 필드(심볼명 snake/camel 분해 후 ngram_3) 또는 tantivy `FuzzyTermQuery`(distance 1)를 영문 단일 토큰에만 적용하는 방안 비교. 범위가 커서 Phase 1~2 결과 본 뒤 별도 spec으로 분리.

#### Phase 3 결과 (2026-10-01, 1차)

준비:
- fixture 스키마: `commit` 카테고리 추가, Commit 정답은 `{ kind = "Commit", title = "<커밋 제목>" }` (path 없음). 기존 fixture는 Commit 정답이 없어서 Commit 비중을 낮추면 커밋 검색이 망가져도 점수가 오르는 편향이 있었음.
- fixture 63개로 확장 (positive 6개 카테고리 × 9, negative 9). 정답은 엔진을 돌리지 않고 코드/`git log`만 보고 작성.
- `SearchParams` (`src/search/params.rs`) + `glc report --param key=value` (숨김). 기본값은 기존 동작과 per-query 결과 동일 확인.
- 확장 기준선: `report-2026-10-01-5.md` — MRR 0.463, R@5 0.593, R@10 0.639, NDCG@10 0.498, negative pass 33.3%. commit 카테고리 MRR 0.250으로 가장 약함.

sweep (`--warmup 0 --iters 1`, 쿼리 1개 MRR 변화 ≈ 집계 0.0185):

| config | MRR | R@5 | R@10 | commit MRR | neg pass |
|---|---|---|---|---|---|
| baseline | 0.463 | 0.593 | 0.639 | 0.250 | 33.3% |
| vec_min_score 0.25 / 0.30 / 0.35 | 0.459 / 0.454 / 0.449 | 0.574 | 0.620 / 0.620 / 0.602 | 0.250 | 33.3% |
| vec_commit_penalty 0.03 / 0.08 / 0.15 | 0.473 / 0.471 / 0.497 | 0.611 / 0.593 / 0.593 | 0.657 / 0.639 / 0.639 | 0.250 / 0.222 / 0.222 | 33.3 / 22.2 / 22.2% |
| rrf_k 10 / 20 / 40 | 0.474 / 0.463 / 0.463 | 0.593 | 0.639 | 0.250 | 33.3% |
| w_vec 0.5 (=0.75) / 1.5 | 0.475 / 0.437 | 0.556 / 0.537 | 0.657 / 0.556 | 0.222 / 0.259 | 55.6 / 22.2% |
| w_vec_korean 1.0 / 2.0 | 0.463 | 0.593 | 0.639 | 0.250 | 33.3% |
| korean_anchor 0 / 1 | 0.365 / 0.457 | 0.491 / 0.574 | 0.537 / 0.620 | 0.139 / 0.250 | 33.3% |
| 결과에서 vendor/ 제외 (임시 실험) | 0.463 | 0.593 | 0.639 | 0.250 | 33.3% |
| 결과에서 vendor/, docs/, site/ 제외 (임시 실험) | 0.481 | 0.611 | 0.676 | 0.250 | 11.1% |

결론:
- 고정 vector 하한은 기각. negative 쿼리 vector top 점수(0.33~0.36)가 한국어 정답 쿼리 top 점수(~0.26)보다 높아서 분리 불가.
- fusion 파라미터는 모두 쿼리 1~2개 수준의 변화이고 지표 간 trade-off가 있음 → 기본값 유지 (과적합 회피). korean_anchor=3은 유효함이 재확인됨.
- 병목은 fusion이 아니라 검색 소스 쪽:
  - commit: 맞힌 2개는 제목 단어를 그대로 쓴 쿼리. 회상형/언어 교차(한국어 쿼리 ↔ 영어 커밋) 쿼리는 전부 실패. 커밋 embed text가 제목+본문뿐이라 신호가 약함.
  - negative: RRF는 항상 무언가를 반환하므로 fusion 단계에서 막을 수 없음. 관련성 판단(BM25 매칭 품질 + vector 점수의 상대 분포)이 필요.
  - typo: 기존 계획대로 별도 spec.

다음 후보 (사용자 결정 필요):
1. 커밋 embed text 보강: 변경된 파일 경로(및 diff stat)를 붙여 임베딩. 인덱싱 시 커밋별 diff 계산 비용 발생, `INDEX_VERSION` 증가.
2. negative 대응: "결과 없음" 판단 기준 설계 (예: BM25 hit 없음 + vector top 점수가 쿼리별 분포 대비 낮음).
3. typo spec (fuzzy BM25 / identifier 필드).

#### tantivy 0.26.1 → 0.26.2 검토 (2026-10-01, 완료)

변경사항 (0.26.2는 버그 수정만 포함, 인덱스 포맷 변경 없음):
- term aggregation doc count overflow, nested aggregation buffer flush 누락 — aggregation은 사용하지 않으므로 무관.
- `BufferedUnionScorer::seek_danger` override 비활성화 ([#3086](https://github.com/quickwit-oss/tantivy/issues/3086)) — `Must` intersection 안에 AND 자식을 가진 union이 있을 때 한쪽 term만 가진 문서가 매칭되던 버그. 우리 기본 쿼리는 `Should`만 쓰므로 영향 없고, 사용자가 `+a +b` / `AND` 문법을 직접 쓸 때만 해당. 비용 없이 받는 정합성 수정.
- 0.27.0은 CHANGELOG에만 있고 미배포. breaking change는 `set_fast(&str)`뿐이고 우리 코드는 `set_fast`를 쓰지 않음.
- `Cargo.lock`은 tantivy 항목만 수정 (`cargo update -p tantivy`가 windows-sys/darling 참조까지 다시 풀어서 되돌림).

계측 중 발견: BM25 인덱스 재빌드가 비결정적이었음.
- 기본 `Index::writer()`는 멀티스레드라 문서가 세그먼트 3개에 무작위로 나뉨 → 동점 BM25 점수의 순서(DocAddress)가 빌드마다 바뀜 → 같은 HEAD·같은 버전에서 재빌드만 해도 MRR이 0.443~0.459로 흔들림. 같은 인덱스로 검색을 반복하면 결과는 동일함.
- turbovec 파일 해시도 빌드마다 다르지만 저장 시 nonce 때문이고 검색 결과에는 영향 없음.
- 수정: `writer_with_num_threads(1, ..)`. 3회 재빌드 결과 완전 일치. 인덱싱 시간 1.31s → 1.49s (625 docs, 임베딩 포함).
- 결정적 조건에서 0.26.1과 0.26.2의 54개 쿼리 결과는 완전히 같음.
- 영향: Phase 1~2처럼 재빌드를 사이에 둔 비교에는 ±0.016 MRR 수준의 노이즈가 섞여 있었음. Phase 3 sweep은 같은 인덱스에서 `--param`만 바꿨으므로 유효함.

새 기준선 `report-2026-10-01-6.md` (HEAD 51c4fd2, 625 docs, 결정적 빌드): MRR 0.450, R@5 0.574, R@10 0.620, NDCG@10 0.483, negative 33.3%, commit MRR 0.250.

#### 후보 1: 커밋 embed text에 변경 경로 추가 (2026-10-01, 채택)

구현: `Chunk::CommitMessage.paths` — 첫 부모와의 name-only tree diff(`chunk::changed_paths`, root는 빈 tree 기준), 최대 `MAX_COMMIT_PATHS`=20개. 임베딩 텍스트에만 공백 구분으로 덧붙이고 BM25 문서는 그대로. `INDEX_VERSION` 9. 인덱싱 1.49s → 1.69s.

같은 HEAD(5d15db5, 627 docs)에서 비교:

| 변형 | MRR | R@5 | R@10 | NDCG@10 | commit MRR | negative |
|---|---|---|---|---|---|---|
| 경로 없음 (기준) | 0.450 | 0.583 | 0.620 | 0.484 | 0.250 | 33.3% |
| 전체 경로, 상한 5 | 0.478 | 0.583 | 0.602 | 0.502 | 0.222 | 33.3% |
| 전체 경로, 상한 20 (채택) | 0.479 | 0.583 | 0.602 | 0.502 | 0.222 | 33.3% |
| 파일 stem만, 상한 5 | 0.447 | 0.565 | 0.602 | 0.478 | 0.222 | 33.3% |
| 파일 stem만, 상한 20 | 0.449 | 0.565 | 0.602 | 0.479 | 0.222 | 33.3% |

결론:
- 목표였던 commit 카테고리는 개선되지 않음 (#54 rank 4 → 밖). 쿼리의 "layout"이 다른 커밋의 `src/ui/layout.rs` 경로와 맞물려 그 커밋이 vector 1위로 올라옴. 정답 커밋은 `src/app.rs` 한 줄 변경이라 경로 신호가 약함.
- 대신 파일 쿼리 9개가 개선 (#1, #10, #11, #13, #14, #15, #28, #29, #43), 악화는 #54 하나. `diagnose`로 보면 경로를 가진 커밋 문서가 vector 상위권에 들어오면서 `src/app.rs`의 노이즈 symbol들을 밀어내 정답 파일이 올라가는 간접 효과.
- 1~2쿼리 수준이 아닌 여러 카테고리에 걸친 일관된 변화라 채택. commit 카테고리의 회상형/한영 교차 쿼리는 여전히 미해결.
- `report-2026-10-01-7.md`가 새 기준선.

#### 측정 오염 제거: `.glcignore` (2026-10-01, 완료)

후보 2를 보려고 negative 쿼리를 `diagnose`하다 발견: 인덱스에 `docs/reports/*.md`, `tests/fixtures/search_queries.toml`, 평가 세트를 인용한 설계 문서와 학습 가이드가 들어 있었음. 이 문서들에 fixture 쿼리 문장과 정답 경로가 그대로 있어서 "django orm migrations" 같은 negative도 BM25에서 리포트 파일과 정확히 매칭됨. 리포트를 만들 때마다 오염 문서가 하나씩 늘어 측정마다 코퍼스가 조금씩 달라지는 문제도 있었음.

- 일반 기능으로 `.glcignore` 추가 (repo 루트, gitignore 문법, `ignore` crate). HEAD 파일에만 적용, 커밋 문서는 제외 대상 아님. 내용의 blob oid를 `meta.toml` `ignore_hash`에 저장해 바뀌면 full rebuild.
- 이 저장소의 `.glcignore`: `docs/reports/`, `tests/fixtures/`, `docs/**/*search-quality*`, `docs/**/*fixture-category-matrix*`, 학습 가이드. 기준은 "평가 세트를 인용하는 문서"로, 식별자 쿼리에 정상 매칭되는 문서(`repo-watch` 계획의 `head_info` 등)는 남김.
- 결과 (`report-2026-10-01-8.md`, 609 docs): MRR 0.479 → 0.487, R@5 0.583 → 0.611, R@10 0.602 → 0.620, NDCG@10 0.502 → 0.512. typo가 가장 크게 오름 (0.292 → 0.383; 오타 쿼리의 bigram이 리포트 속 원문 쿼리와 겹치던 노이즈가 사라짐). natural 3개는 소폭 하락. negative는 33.3% 그대로.
- 이전 기준선들(report-5~7)은 오염된 코퍼스 기준이라 report-8 이후와 직접 비교하지 말 것.

## 4. 리스크

- `statrs`/`rand_chacha`가 정확 버전 pin이라 다른 의존성과 충돌 시 resolver 실패 가능 → `cargo update -p turbovec` 단계에서 확인.
- Windows 릴리스 빌드: BLAS 제거로 오히려 단순해지지만, x86 `x86-64-v2` baseline + 런타임 dispatch로 바뀐 점을 릴리스 워크플로에서 한 번 확인.
- calibrate 샘플링에 RNG가 들어가면 인덱스 재현성이 깨짐 → 고정 seed 사용.

## 5. 문서 갱신

- `CLAUDE.md`: `INDEX_VERSION` 표기가 "currently 5"로 남아 있음 (실제 7, 이번 작업 후 8). BLAS 항목 제거/수정.
