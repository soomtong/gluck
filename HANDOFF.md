# HANDOFF: 시맨틱 검색 품질 최적화 (후보 3 채택, 후보 2 남음)

## 목표
`glc` 시맨틱 검색(BM25 + turbovec 벡터 + RRF)의 품질 개선. 전체 계획과 단계별 실측 결과는 `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md`에 있음 (먼저 읽을 것).

## 현재 브랜치
`main` (HEAD 0b0e66f, v0.14.0). 이번 세션 변경은 아직 커밋하지 않음 (아래 "미커밋 변경" 참고).

## 이번 세션 결정 (사용자 확인 완료)
- 이 repo `.glcignore`에 `glc ignore` 기본 목록 병합 → 완료
- 후보 3(typo)을 후보 2(negative)보다 먼저 → 완료
- 후보 2 동작: 결과를 비우지 않고 "강한 매칭 없음" 표시만. 리포트 negative 판정은 "low-confidence 표시도 pass"로 변경 예정

## 완료된 작업
- [x] `.glcignore`: `DEFAULT_TEMPLATE` 병합(vendor/ 19개, Cargo.lock 제외), `/HANDOFF.md` 추가. HANDOFF.md가 typo 쿼리 원문을 인용해 오타 단어를 "알려진 단어"로 만들고 BM25 bigram도 오염시키고 있었음. report-8, report-9는 이 오염을 포함
- [x] 후보 3 typo 교정: `src/search/typo.rs`, BM25 `body_words` 필드(검색에는 안 쓰고 어휘 수집 전용), `SearchEngine.vocab`, `SearchParams.typo_mode`(기본 1 = BM25 쿼리에 교정 단어 덧붙이기). `INDEX_VERSION` 10. 의존성 `strsim` 추가(clap이 이미 쓰던 crate)
- [x] 측정: report-10(typo_mode=0 기준선) MRR 0.494 → report-11(채택) 0.517, R@5 0.630 → 0.648, typo MRR 0.420 → 0.559. typo가 아닌 쿼리와 negative의 순위는 완전히 같음
- [x] spec `docs/superpowers/specs/2026-10-01-search-quality-typo-correction-design.md`, 계획서 "후보 3" 절, AGENTS.md 갱신
- [x] `examples/typo_probe.rs`: fixture 쿼리별 교정 결과 출력
- [x] cargo test 380 통과, clippy `--all-targets -D warnings` 통과, 수정 파일 rustfmt 완료

## 미커밋 변경
`.glcignore`, `Cargo.toml`/`Cargo.lock`(strsim), `src/search/{typo.rs,bm25.rs,mod.rs,params.rs}`, `examples/typo_probe.rs`, `AGENTS.md`, 계획서, spec, `docs/reports/report-2026-10-01-{9,10,11}.md`, 이 파일.
커밋 메시지 예: "typo 쿼리 교정 추가: 어휘에 없는 단어를 편집거리 1로 교정 (INDEX_VERSION 10)". `.glcignore` 병합과 기준선 재측정은 별도 커밋으로 나눠도 됨.

## 시도했으나 실패한 접근
- 이전 세션: 벡터 후보 확대, exact rerank, 고정 vec_min_score, fusion 튜닝, 커밋 경로 stem, 후보 2 절대 임계값
- 후보 3: tantivy FuzzyTermQuery(상수 점수라 검토 단계에서 제외), title/path_terms만 어휘로 쓰기(오교정으로 negative 22.2%), 편집거리 2(다른 실제 단어로만 교정됨), 임베딩 텍스트 교정(typo_mode 2/3, 효과 작음)

## 남은 작업
- [ ] 이번 세션 변경 커밋 (사용자 확인 후)
- [ ] 후보 2 (negative "강한 매칭 없음" 표시): 후보 3을 적용했으니 `examples/neg_probe.rs`로 규칙 `word_hits == 0 AND vec_gap < 0.05`를 다시 측정. typo 교정 뒤에는 `is_binray_blob` 등의 word_hits가 바뀌었을 수 있음. 판정에 교정된 쿼리를 쓸지 결정 필요. 리포트 negative 판정을 "표시 붙으면 pass"로 바꾸고, UI(`ui/search_modal.rs`)에 표시 추가
- [ ] 남은 typo 실패: #31 `reciprical rank fussion`(정답 `rrf.rs` 본문에 "reciprocal" 없음), #33 `is_binray_blob`(교정은 되지만 순위 밖). 1~2쿼리 수준이라 개별 튜닝은 하지 말 것
- [ ] 다음 릴리즈 때 Windows 빌드 확인 (INDEX_VERSION 10 → 사용자 인덱스 full rebuild 발생)

## 핵심 파일
- `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md` — 계획 + 단계별 결과
- `docs/reports/report-2026-10-01-11.md` — 현재 기준선 (HEAD 0b0e66f, 597 docs, typo_mode=1): MRR 0.517, R@5 0.648, R@10 0.657, NDCG@10 0.543, negative 33.3%, commit MRR 0.222
- `docs/reports/report-2026-10-01-10.md` — 같은 인덱스, typo_mode=0
- `src/search/typo.rs` — `query_words`, `correct`, `is_typing_slip`, `is_plural_of`
- `src/search/bm25.rs` — `body_words` 필드, `word_vocab()`
- `src/search/mod.rs` — `SearchEngine.vocab`, 교정 적용 지점(`search`)
- `examples/neg_probe.rs`, `examples/typo_probe.rs`
- `tests/fixtures/search_queries.toml` — 63개 정답 세트 (튜닝에 맞춰 수정 금지)

## 다음 에이전트에게
1. 미커밋 변경이 있으면 커밋할지 사용자에게 먼저 확인한다.
2. 측정: `cargo build --release && ./target/release/glc index --force && ./target/release/glc report --out docs/reports/report-YYYY-MM-DD-N.md`. 빠른 비교는 `--warmup 0 --iters 1 --param key=value`. 설치된 `/opt/homebrew/bin/glc`(v0.14.0)로 색인하면 INDEX_VERSION 9가 되므로 반드시 `./target/release/glc`를 사용한다.
3. 평가 쿼리를 인용하는 추적 파일을 새로 만들면 `.glcignore`에 추가한다 (`docs/**/*search-quality*` 패턴에 맞는 이름이면 자동 제외).
4. 변화가 쿼리 1~2개 수준이면 채택하지 말 것.
5. 커밋 메시지는 한국어, 접두사 없음, 80자 이내. 수정한 파일만 `rustfmt`, CI는 `cargo clippy --all-targets -- -D warnings`.
