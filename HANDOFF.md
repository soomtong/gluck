# HANDOFF: 시맨틱 검색 품질 최적화 (후보 1 완료, 후보 2 분석 완료·결정 대기)

## 목표
`glc` 시맨틱 검색(BM25 + turbovec 벡터 + RRF)의 품질 개선. 전체 계획과 단계별 실측 결과는 `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md`에 있음 (먼저 읽을 것).

## 현재 브랜치
`main` — v0.14.0 릴리즈 완료 (tantivy 0.26.2, 커밋 경로 임베딩, `.glcignore`, `glc ignore` 포함).
`f6dc23e` 기준 CI(Lint/Test/Check) 통과.

이번 세션 커밋:
- `5d15db5` tantivy 0.26.2 적용, BM25 writer 1스레드로 고정해 재빌드 결정성 확보
- `d811501` calibration 저장 테스트가 SIMD 커널별 동점 차이에 흔들리지 않도록 수정 (Linux CI 실패 원인)
- `cabbe81` 커밋 임베딩 텍스트에 변경 파일 경로 추가 (INDEX_VERSION 9)
- `bf01a88` .glcignore로 인덱싱 제외 지원, 평가 산출물을 인덱스에서 제외
- `f6dc23e` negative 쿼리 판별 신호 probe 예제 (`examples/neg_probe.rs`)
- `9d3fa22` glc ignore 명령 추가: 기본 제외 목록으로 .glcignore 생성

## 완료된 작업
- [x] Phase 1~3 (이전 세션): turbovec 1.0, RRF 동점 결정성, path allowlist, fusion sweep → 기본값 유지
- [x] tantivy 0.26.1 → 0.26.2: 결정적 조건에서 결과 완전 동일. 0.27.0은 미배포
- [x] BM25 writer 1스레드: 멀티스레드 writer가 세그먼트를 무작위 분할 → 동점 순서가 재빌드마다 바뀌어 MRR ±0.016 흔들리던 문제 제거
- [x] 후보 1: 커밋 임베딩에 변경 경로(첫 부모 name-only diff, 최대 20개). 파일 쿼리 9개 개선/1개 악화로 채택. 목표였던 commit 카테고리는 개선 못 함 (0.250 → 0.222)
- [x] `.glcignore` (repo 루트, gitignore 문법, `ignore` crate). HEAD 파일에만 적용, 해시를 `meta.toml` `ignore_hash`에 저장해 변경 시 full rebuild
- [x] 측정 오염 제거: 리포트/fixture/평가 세트 인용 문서가 인덱스에 들어가 negative까지 BM25 정확 매칭되던 문제. 이 repo `.glcignore`로 제외
- [x] `glc ignore [--force]`: 기본 제외 목록(vendor/third_party/external, lockfile, dist/build/out, *.min.js/css, *.map)으로 `.glcignore` 생성. 기존 파일은 `--force` 없이는 유지
- [x] README / site / AGENTS.md 문서화 (`.glcignore`, `glc ignore`, 커밋 경로 임베딩, 인덱싱 전략 수치 정정)
- [x] CI에서 Linux BLAS 없는 빌드 확인 (Windows는 release 워크플로에서만 빌드 — 다음 태그 때 확인)

## 시도했으나 실패한 접근
- 벡터 후보 확대, exact float32 rerank, 고정 vec_min_score, fusion 파라미터 튜닝 (이전 세션, 계획서 참고)
- 후보 1 변형: 파일 stem만 붙이기 (MRR 0.447~0.449, 이득 없음), 경로 상한 5 vs 20 (차이 없음)
- 후보 2 절대 임계값 규칙 (BM25 원점수 / cosine 절댓값): negative "spring boot"와 정답 "탭 문자 버그" 커밋의 단어 점수가 6.36으로 같음. 스케일이 코퍼스마다 달라 이식 불가

## 남은 작업
- [ ] 기준선 재측정 (사용자 지시로 다음 세션에): 이 repo `.glcignore`에 `glc ignore` 기본 목록(특히 `Cargo.lock`)을 합칠지 사용자 확인 → 합치면 `glc index` 후 `glc report --out docs/reports/report-2026-10-01-9.md`(또는 당일 날짜)로 새 기준선. 결과를 계획서에 기록
- [ ] 후보 2 (negative) — 사용자 결정 대기 2가지:
  1. 순서: 후보 3(typo/fuzzy)을 먼저 할지. 최선 규칙이 숨기는 positive 5개 중 `is_binray_blob`, `reciprical rank fussion`이 후보 3의 대상이라 충돌. 권장: 3 먼저
  2. 동작: 결과를 비울지 vs "강한 매칭 없음" 표시만 붙일지. 권장: 표시 (정답 손실 없음). 표시 방식이면 리포트 negative 판정을 "low-confidence 표시도 pass"로 바꿔야 함
  - 최선 후보 규칙: `word_hits == 0` (쿼리 단어가 title/path_terms에 없음) AND `vec_gap < 0.05` (vector top1 − top10 평균) → negative 6/9, 현재 맞히는 정답 손실 0
- [ ] 후보 3: typo spec (fuzzy BM25 / identifier 전용 필드) — `docs/superpowers/specs/`에 별도 spec으로

## 핵심 파일
- `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md` — 계획 + 단계별 결과 (tantivy, 후보 1, `.glcignore` 섹션 추가됨)
- `docs/reports/report-2026-10-01-8.md` — 현재 기준선 (오염 제거 후, HEAD cabbe81, 609 docs): MRR 0.487, R@5 0.611, R@10 0.620, NDCG@10 0.512, negative 33.3%, commit MRR 0.222. report-5~7은 오염된 코퍼스라 비교 금지
- `.glcignore` — 이 repo의 평가 산출물 제외 목록
- `src/search/glcignore.rs` — `GlcIgnore`, `DEFAULT_TEMPLATE`, `write_default`
- `src/search/chunk/commit.rs` — `changed_paths`, `MAX_COMMIT_PATHS`
- `src/search/indexer.rs` — `commit_chunks`, `.glcignore` 적용 (full/incremental), `ignore_hash` 비교
- `src/search/mod.rs` — `SearchEngine::search`, `IndexMeta.ignore_hash`, `INDEX_VERSION` 9
- `examples/neg_probe.rs` — 쿼리별 word_hits / word_top / ngram_top / vec1 / vec_gap 출력 (`cargo run --release --example neg_probe`)
- `tests/fixtures/search_queries.toml` — 63개 정답 세트 (튜닝에 맞춰 수정 금지)

## 다음 에이전트에게
1. 먼저 기준선 재측정 항목을 사용자와 확인하고 진행한다.
2. 그다음 후보 2의 두 가지 결정을 사용자에게 받는다 (위 권장안 제시).
3. 측정: `cargo build --release && ./target/release/glc index --force && ./target/release/glc report --out docs/reports/report-YYYY-MM-DD-N.md`. 빠른 비교는 `--warmup 0 --iters 1`. 리포트 파일은 `.glcignore`로 제외되므로 추가해도 코퍼스가 안 바뀜. 단 커밋이 늘면 커밋 문서 수가 바뀌므로 비교는 같은 HEAD에서.
4. 변화가 쿼리 1~2개 수준이면 채택하지 말 것.
5. 커밋 메시지는 한국어, 접두사 없음, 80자 이내. 수정한 파일만 `rustfmt`, CI는 `cargo clippy --all-targets -- -D warnings` (examples 포함).
