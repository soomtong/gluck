# HANDOFF: 시맨틱 검색 품질 최적화 (turbovec 1.0 전환 이후 Phase 3 진행 중)

## 목표
`glc` 시맨틱 검색(BM25 + turbovec 벡터 + RRF)의 품질 개선. 전체 계획과 단계별 실측 결과는 `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md`에 있음 (먼저 읽을 것).

## 현재 브랜치
`main` — origin보다 6커밋 앞섬, push 안 함. working tree clean (이 파일만 untracked).

- `4eb0d09` turbovec 1.0 전환 (명시적 TQ+ calibration, sync 저장, BLAS 제거, INDEX_VERSION 8)
- `ad601cc` 리포트 NDCG/Recall이 정답당 한 번만 계산되도록 수정
- `022312a` RRF 동점 순서를 결정적으로 고정
- `2bbf8ad` path 필터 쿼리의 벡터 검색을 allowlist로 한정
- `7316a82` 계획서에 Phase 2 결과 기록
- `51c4fd2` 리포트 commit 카테고리 + `--param` 오버라이드, fixture 63개로 확장

## 완료된 작업
- [x] Phase 1: turbovec 0.8 → 1.0. 품질/성능 동등 확인
- [x] Phase 0 보완: NDCG > 1 버그 수정, RRF 동점 비결정성 수정 (리포트 3회 동일 확인)
- [x] Phase 2: path 필터 allowlist 적용. 4-bit vs exact 계측 (overlap@10 0.95, top-1 30/30 동일)으로 exact rerank 보류
- [x] Phase 3 준비: Commit 정답 지원(`{ kind = "Commit", title = "..." }`, path 없음), fixture 63개 (positive 6카테고리 × 9 + negative 9), `SearchParams` + `glc report --param key=value` (숨김)
- [x] tantivy 0.26.1 → 0.26.2 (결과 동일). BM25 writer 1스레드로 고정해 인덱스 재빌드 비결정성 제거 (재빌드마다 MRR ±0.016 흔들리던 문제)
- [x] Phase 3 sweep: 하한/commit penalty/rrf_k/w_vec/korean 가중치/anchor 모두 비교. 기본값 유지 결정

## 시도했으나 실패한 접근
- 벡터 후보 확대 (`max(limit*5, 50)`): R@5 0.760→0.720, negative 40%→20%. 하위권 노이즈가 RRF에 섞임
- exact float32 rerank: 양자화 오차가 순위를 거의 안 바꿈 (계측 결과). 인덱스 8배 비용 대비 이득 없음
- 고정 vector 유사도 하한 (`vec_min_score`): negative top 점수(0.33~0.36) > 한국어 정답 top 점수(~0.26)라 분리 불가
- fusion 파라미터 튜닝: 모든 변화가 쿼리 1~2개 수준(MRR 1쿼리 ≈ 0.0185)이고 지표 간 trade-off → 과적합 우려로 미적용
- 결과에서 vendor/, docs/, site/ 제외 (임시 실험, 코드 되돌림): 의미 있는 이득 없음

## 남은 작업 (사용자가 다음 방향 선택 대기 중)
- [ ] 후보 1: 커밋 embed text 보강 — 변경 파일 경로(및 diff stat)를 커밋 임베딩 텍스트에 추가. commit 카테고리 MRR 0.250 개선이 목표. INDEX_VERSION 9 필요
- [ ] 후보 2: negative 대응 — "결과 없음" 판단 기준 설계 (BM25 매칭 없음 + vector 점수가 낮을 때 등). 정답을 숨길 위험 있음
- [ ] 후보 3: typo spec (fuzzy BM25 / identifier 전용 필드) — `docs/superpowers/specs/`에 별도 spec으로
- [ ] push 후 CI에서 Linux/Windows BLAS 없는 빌드 확인 (로컬은 macOS만 검증)

## 핵심 파일
- `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md` — 계획 + Phase별 결과 표
- `docs/reports/report-2026-10-01-6.md` — 현재 기준선 (63 fixtures, HEAD 51c4fd2, 결정적 빌드): MRR 0.450, R@5 0.574, R@10 0.620, NDCG@10 0.483, negative pass 33.3%, commit MRR 0.250
- `src/search/mod.rs:166` — `SearchEngine::search` (BM25/vector/RRF 흐름), `:259` `adjust_vec_hits`, `:112` `path_doc_ids`, `:297` `INDEX_VERSION`
- `src/search/params.rs` — 튜닝 파라미터와 기본값 (기본값 = 기존 동작)
- `src/search/chunk/mod.rs:36` — `Chunk::embed_text` (후보 1 수정 지점: `CommitMessage`는 title+body뿐)
- `src/search/indexer.rs:206` — 전체 빌드에서 `calibrate` 후 `add`; `:367` `collect_commits`
- `src/search/vector.rs:40` — `calibrate` (stride 샘플, 최대 1000행), `:100` `save` (= turbovec `sync`)
- `src/search/report/metrics.rs:72` — 정답 매칭, `:103` 정답당 1회 credit
- `tests/fixtures/search_queries.toml` — 63개 정답 세트 (엔진 결과를 보지 않고 작성됨 — 튜닝에 맞춰 수정 금지)

## 다음 에이전트에게
1. 계획서의 "Phase 3 결과" 섹션을 읽고, 사용자에게 후보 1~3 중 선택을 확인한다 (사용자는 후보 1을 아직 승인하지 않음).
2. 측정은 `cargo build --release && ./target/release/glc report --out docs/reports/report-YYYY-MM-DD-N.md`. 빠른 비교는 `--warmup 0 --iters 1 --param key=value`. 인덱스 형식/임베딩 텍스트를 바꾸면 `glc index --force` 먼저.
3. 비교 기준은 `report-2026-10-01-6.md` (코퍼스가 바뀌면 같은 HEAD에서 기준선부터 다시 측정). 변화가 쿼리 1~2개 수준이면 채택하지 말 것.
4. 커밋 메시지는 한국어, 접두사 없음, 80자 이내. 수정한 파일만 `rustfmt`, CI는 `cargo clippy --all-targets -- -D warnings`.
