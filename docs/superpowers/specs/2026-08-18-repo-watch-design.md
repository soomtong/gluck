# Repo Watch — 실행 중 커밋/ref 변경 감지 및 자동 갱신

날짜: 2026-08-18
상태: 승인됨

## 목적

glc TUI 실행 중 외부에서 repo가 변경되면(새 커밋, branch 전환, rebase, pull 등)
앱을 재시작하지 않아도 커밋 목록과 diff에 반영되도록 한다.

## 범위

- **포함**: HEAD/refs 변경 감지 (새 커밋, branch 전환, rebase, pull).
- **제외 (비목표)**:
  - 워킹 디렉토리(커밋되지 않은 변경) 감지 — 현재 앱에 워킹 트리 UI가 없음.
    필요 시 별도 작업으로 진행.
  - 시맨틱 검색 인덱스(`.glc-index`) 자동 재인덱싱 — 기존대로 다음
    `glc index` 실행 시 `head_oid` 불일치로 증분 갱신된다.

## 감지 메커니즘: 주기적 폴링 (5초)

- `notify` crate 대신 폴링을 선택. notify를 쓰더라도 메인 루프의
  블로킹 해제가 동일하게 필요하고, 의존성 추가 + lock 파일 이벤트
  debounce 비용 대비 이득이 수백 ms의 반응성뿐이다.
- `main.rs::run_app`의 비인덱싱 분기를 `event::read()` 블로킹에서
  `event::poll(Duration::from_secs(1))`로 변경. 키 입력은 즉시 반응하고,
  타임아웃 시에만 루프가 한 바퀴 돈다.
- `App`에 상태 추가:
  - `last_head: Option<(git2::Oid, String)>` — HEAD oid + ref shorthand
  - `last_head_check: Instant`
- 루프 iteration마다 `HEAD_POLL_INTERVAL`(상수, 5초) 경과 시에만
  `repo.head()`를 resolve해 튜플을 비교한다. libgit2 head resolve는
  저렴하므로 5초 주기 부담 없음. 폴링 주기 config 옵션은 YAGNI로 보류.
- ref 이름도 비교하므로 oid가 같고 branch만 바뀐 경우도 감지된다.

## 갱신 동작: 모드별 혼합

- **Pick 모드**: 변경 감지 즉시 갱신.
  - `CommitStore`를 새로 빌드하고 첫 배치(200)를 로드.
  - 활성 검색 필터가 있으면 배치 로드 후와 동일하게 `update_filter()` 재실행.
  - 선택 위치는 기존 선택 커밋의 oid를 새 목록에서 찾아 복원.
    없으면(rebase 등) 기존 인덱스를 목록 길이로 클램프.
- **View/Diff 모드**: `repo_changed: bool` 플래그만 세우고 footer에
  "repo changed" 표시. 보던 내용은 유지한다. 어떤 경로로든 모드가
  Pick으로 전환되는 시점에 위 Pick 갱신을 적용하고 플래그를 해제한다.
- **캐시**: diff/tree 캐시는 커밋 oid 기반이고 커밋은 불변이므로
  비우지 않는다. 히스토리가 재작성되면 옛 항목은 LRU에서 자연 퇴출된다.

## 구조

로직을 `App` 메서드 두 개로 분리하고 `main.rs` 루프는 타이머 판단 +
호출만 담당한다:

- `App::check_repo_changed(&mut self) -> bool` — HEAD 튜플 비교, 상태 갱신.
- `App::apply_repo_refresh(&mut self)` — store 재빌드, 필터 재실행,
  선택 복원, 플래그 해제.

터미널 없이 `init_test_repo()` / `add_file_commit()` 헬퍼로 테스트 가능하다.

## 에러 처리

- `repo.head()` 실패(unborn HEAD, `.git` 삭제 등) 시 조용히 skip하고
  다음 tick에 재시도한다. 앱을 종료시키지 않는다.

## 테스트

- 새 커밋 추가 → `check_repo_changed()`가 true, 갱신 후 목록에 반영.
- 선택 커밋 oid가 새 목록에서 보존되는지.
- HEAD 변화 없으면 no-op(false)인지.
- rebase(옛 oid 소실) 시 선택 인덱스 클램프되는지.
