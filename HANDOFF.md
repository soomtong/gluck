# HANDOFF: 시맨틱 검색 품질 최적화 (후보 1~3 완료)

## 목표
`glc` 시맨틱 검색(BM25 + turbovec 벡터 + RRF)의 품질 개선. 전체 계획과 단계별 실측 결과는 `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md`에 있음 (먼저 읽을 것).

## 현재 브랜치
`main`. 이번 세션 커밋:
- `75c15dd` .glcignore에 기본 제외 목록과 HANDOFF.md 추가
- `d632bf9` typo 쿼리 교정 추가 (INDEX_VERSION 10, `typo_mode`)
- `fdb0841` negative 쿼리 "no strong match" 표시 (`weak_gap`)
- `5b25fb9` glc에 -V/--version 옵션 추가
- `38e90dd` 검색 결과 제목이 camelCase 분해된 채 저장되던 문제 수정 (INDEX_VERSION 11)

## 완료된 작업
- [x] `.glcignore` 정리: `glc ignore` 기본 목록 병합, `/HANDOFF.md` 제외(typo 쿼리 원문을 인용해 측정을 오염시키고 있었음. report-8, report-9는 오염된 측정)
- [x] 후보 3 typo 교정: 어휘(title + path_terms + `body_words`)에 없는 단어를 편집거리 1의 타이핑 실수 유형으로만 교정해 BM25 쿼리에 덧붙임. typo MRR 0.420 → 0.559, 전체 MRR 0.494 → 0.517. spec `docs/superpowers/specs/2026-10-01-search-quality-typo-correction-design.md`
- [x] 후보 2 negative 표시: `SearchEngine::search_scored` → `SearchOutcome { results, weak }`. 규칙: 단어 매칭 없음 AND 벡터 top1−top10평균 < `weak_gap`(0.055). 결과는 그대로 두고 모달 제목에 `· no strong match`를 표시. 리포트는 표시가 붙은 negative를 PASS로 판정하고 positive 오경보 수를 출력. negative 33.3% → 88.9%, 오경보 0, 순위 지표 변화 없음
- [x] tmux로 TUI 확인: negative 쿼리에는 표시가 붙고 정상 쿼리에는 붙지 않음
- [x] cargo test 382 통과, clippy `--all-targets -D warnings` 통과, 수정 파일 rustfmt

## 현재 기준선
`docs/reports/report-2026-10-01-14.md` (HEAD 5b25fb9 기준 인덱스, 630 docs, INDEX_VERSION 11):
MRR 0.508, R@5 0.648, R@10 0.694, NDCG@10 0.545, negative 88.9%, 표시가 붙은 positive 4/54(오경보 0), commit MRR 0.278.
같은 HEAD에서 제목 버그 수정 전 값: report-13 (MRR 0.498). report-12 이전과는 HEAD가 달라 직접 비교 불가.

## 시도했으나 실패한 접근
- 이전 세션: 벡터 후보 확대, exact rerank, 고정 vec_min_score, fusion 튜닝, 커밋 경로 stem, 후보 2 절대 임계값
- 후보 3: FuzzyTermQuery, title/path_terms만 어휘로 쓰기, 편집거리 2, 임베딩 텍스트 교정(typo_mode 2/3)

## 남은 작업 / 아이디어
- [ ] commit 카테고리(MRR 0.278): 분석 완료, 사용자 결정 대기. 병목은 한영 교차 의미 매칭(정답이 vector 165~517위). 선택지: (a) 더 강한 다국어 임베딩 모델, (b) 색인 시 커밋 메시지 번역·동의어 확장. 계획서 "commit 카테고리 분석" 절 참고. `examples/commit_probe.rs`로 재측정
- [ ] paraphrase(0.343), korean(0.556) 개선 여지
- [ ] `weak_gap` 양쪽 여유가 0.006뿐. 커밋이 쌓인 뒤 리포트의 "answered in top 10" 수치(오경보)를 다시 확인
- [ ] 남은 negative 실패 1개: `spring boot dependency injection` (word hit 1건)
- [ ] 다음 릴리즈 때 Windows 빌드 확인. INDEX_VERSION 11이라 사용자 인덱스가 full rebuild됨. README/site에 typo 교정과 no strong match 표시 문서화

## 핵심 파일
- `src/search/mod.rs` — `SearchEngine::search_scored`, `SearchOutcome`, 교정 적용 지점
- `src/search/typo.rs` — 교정 규칙
- `src/search/params.rs` — `typo_mode`, `weak_gap`
- `src/search/report/{metrics,render,perf}.rs` — `evaluate_outcome`, `weak_positive_summary`
- `src/search/modal_state.rs`, `src/ui/search_modal.rs`, `src/app.rs`(search worker) — UI 표시
- `examples/neg_probe.rs`(fix_hits 열 추가), `examples/typo_probe.rs`, `examples/commit_probe.rs`
- `tests/fixtures/search_queries.toml` — 63개 정답 세트 (튜닝에 맞춰 수정 금지)

## 다음 에이전트에게
1. 측정: `cargo build --release && ./target/release/glc index --force && ./target/release/glc report --out docs/reports/report-YYYY-MM-DD-N.md`. 빠른 비교는 `--warmup 0 --iters 1 --param key=value`. 설치된 `/opt/homebrew/bin/glc`(v0.14.0)로 색인하면 INDEX_VERSION 9가 되므로 `./target/release/glc`를 사용한다.
2. 평가 쿼리를 인용하는 추적 파일은 `.glcignore`에 넣는다 (`docs/**/*search-quality*` 패턴에 맞는 이름이면 자동 제외).
3. 변화가 쿼리 1~2개 수준이면 채택하지 말 것.
4. 커밋 메시지는 한국어, 접두사 없음, 80자 이내. 수정한 파일만 `rustfmt`, CI는 `cargo clippy --all-targets -- -D warnings`.
