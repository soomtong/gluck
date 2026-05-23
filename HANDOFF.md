# HANDOFF: gluck 시맨틱 검색 — PR 및 릴리즈 대기

## 현재 브랜치

`semantic-v2` — 구현 완료, main 머지 대기

## 남은 작업

- [ ] **E2E 검증**: `glc index` 실행 + `S` 키 TUI 동작 확인
- [ ] **PR**: `semantic-v2` → `main` 머지
- [ ] **릴리즈**: `git tag v0.6.0` + `git push origin main v0.6.0`
  - GitHub Actions가 빌드 아티팩트, GitHub Release, Homebrew tap 자동 처리

## 핵심 파일

- `src/search/` — BM25 + Vector + RRF 전체 파이프라인
- `src/search/modal.rs` — SemanticSearchModal 상태머신
- `src/ui/search_modal.rs` — ratatui 오버레이 렌더러
- `src/app.rs` — App 통합 (search_engine, search_modal 필드)
- `docs/plans/2026-05-23-semantic-search-design-v2.md` — 설계 문서

## 다음 에이전트에게

1. `cargo build --release && glc index` — 패닉 없이 완료되는지 확인
2. TUI에서 `S` 키 → 검색 모달 → 결과 선택 → mode 전환까지 확인
3. `semantic-v2` → `main` PR 생성 후 머지
4. `release` 스킬로 `v0.6.0` 태그 + 푸시
