# Perf Optimization Design

**Date**: 2026-05-22
**Status**: draft

## Goals

- 10만 커밋 저장소에서 앱 시작 < 1초
- Pick 모드 커서 이동 응답 < 16ms (1프레임)
- 검색 응답 < 8ms
- View 모드 진입 < 50ms
- 메모리 사용량 50MB 이하 (10만 커밋)

## Architecture

### New modules

```
src/git/
  store.rs   — CommitStore (페이징 커밋 로딩, Arc 공유), CommitIndex (prefix 검색)
  cache.rs   — DiffCache (LRU), TreeCache (LRU)
```

### Data structures

**CommitStore** — 단일 진실 공급원:
- `loaded: Arc<Vec<CommitInfo>>` — Arc로 모든 참조자 공유 (App ↔ PickState 중복 제거)
- `walk: RefCell<git2::Revwalk<'repo>>` — revwalk, 추가 페이징 로딩용
- `index: CommitIndex` — prefix 검색 인덱스
- `exhausted: bool` — 전체 로드 완료 여부

**CommitIndex** — prefix 검색:
- `BTreeMap<String, Vec<usize>>` — prefix → 매칭 커밋 인덱스 목록
- message, author, short_id의 각 단어 prefix를 인덱싱
- `build_indices()` / `append_indices()` 로 점진적 구축

**DiffCache** — LRU diff 캐시:
- `HashMap<(Oid, Oid), DiffResult>` — 키: (parent_oid, commit_oid)
- `VecDeque<(Oid, Oid)>` — LRU 순서
- `max_size: 64`

**TreeCache** — LRU 파일 트리 캐시:
- `HashMap<Oid, Vec<FileEntry>>` — 키: commit_oid
- `VecDeque<Oid>` — LRU 순서
- `max_size: 32`

### Removed

- `App.commits: Vec<CommitInfo>` → `App.store: CommitStore` 로 대체
- `PickState.commits: Vec<CommitInfo>` → `CommitStore.loaded` Arc 참조로 대체

## Data flow

### Startup

```
CommitStore::new(repo) → revwalk 생성
  → load_batch(200) → 첫 200개 커밋 로드
  → build_index() → CommitIndex 구축
  → 렌더링 시작 (200개 즉시 표시)
```

### Commit paging

```
j로 마지막 커밋 도달:
  → load_batch(200) → 다음 200개 추가
  → append_index() → 인덱스 확장

exhausted = true → "끝" 표시
```

### Diff lazy loading + caching

```
커서 이동 (Pick 모드):
  1. parent 찾기
  2. DiffCache.get(parent, commit) → 캐시 히트면 즉시 반환
  3. 캐시 미스 → compute_diff() → DiffCache.put()
  4. PickState.selected_diff = result

키 연타: 16ms tick마다 한 번만 diff 계산
```

### View entry

```
Pick → View (Enter):
  1. TreeCache.get(commit_oid)
  2. 미스 → list_tree() → TreeCache.put()
  3. changed_stats → DiffCache 재사용 or 계산
  4. FileContent: NotLoaded (기존과 동일)
```

## Testing

### Unit tests

- `CommitIndex`: prefix 검색 정확도, 단어 경계, 빈 쿼리, 유니코드, append 후 검색
- `DiffCache`: 캐시 히트/미스, LRU 순서, max_size 초과 시 제거
- `TreeCache`: 커밋별 트리 조회, max_size 초과 시 제거
- `CommitStore::load_batch`: 페이징 크기, exhausted 판단

### Integration tests

- `test_store_paging_on_large_repo`: 1000커밋 생성 후 페이징 검증
- `test_diff_cache_hit_on_cursor_move`: 커서 이동 시 캐시 히트
- `test_search_index_after_append`: 페이징 후 검색 정확도
- `test_memory_drops_on_lru_eviction`: LRU 제거 시 Drop 확인

### Performance regression suite

```rust
#[test]
fn performance_regression_suite() {
    let repo = init_test_repo_with_n_commits(100_000);
    // 앱 시작 < 1초
    // 커서 이동 100회 < 1.6초
    // 검색 < 8ms
    // View 진입 < 50ms
}
```

### Helper

- `init_test_repo_with_n_commits(n: usize)` — `src/git/repo.rs` 테스트 모듈에 추가

## Implementation phases

### Phase 1: CommitStore

`src/git/store.rs` (신규):
- CommitStore 구조체 + CommitIndex 구현
- `new()`, `load_batch()`, `search()`
- App.commits 제거, store로 대체
- PickState.commits 제거, store 참조로 대체

### Phase 2: DiffCache + TreeCache

`src/git/cache.rs` (신규):
- DiffCache + TreeCache LRU 구현
- `update_pick_diff` → DiffCache.get_or_compute
- `load_view_tree` → TreeCache.get_or_compute

### Phase 3: Lazy / debounce

- 무한 스크롤 (j로 마지막 도달 시 load_batch)
- update_pick_diff 16ms debounce

### Phase 4: Test + verify

- 단위 테스트, 통합 테스트, 성능 회귀 스위트
- 실제 저장소 수동 테스트

## Files changed

| File | Change |
|------|--------|
| `src/git/store.rs` | **New** — CommitStore, CommitIndex |
| `src/git/cache.rs` | **New** — DiffCache, TreeCache |
| `src/git/mod.rs` | Add store, cache modules |
| `src/app.rs` | CommitStore, diff/tree cache, infinite scroll |
| `src/mode.rs` | PickState.commits removed, use store ref |
| `src/git/repo.rs` | `init_test_repo_with_n_commits` helper |
| `Cargo.toml` | `lru` crate if needed |
