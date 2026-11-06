# Linux amd64 릴리스 바이너리 제공 계획

## 결론

소스 수정 없이 릴리스 파이프라인과 문서만 바꾸면 된다. 유일한 결정 사항은 glibc 하한이며, `ubuntu-22.04` 러너 빌드(하한 2.35)로 정했다.

## 검증 결과 (x86_64, Ubuntu glibc 2.39)

- `cargo build --release --locked`: 성공, 경고 0, 약 6분 30초
- `cargo test --locked`: 499 통과 / 0 실패 / 3 ignored
- 산출물: 59MB, 동적 의존성은 `libstdc++`, `libz`, `libgcc_s`, `libm`, `libc`뿐 (openssl 없음)
- 24.04에서 빌드하면 `GLIBC_2.39`를 요구한다 → 오래된 배포판에서 실행 불가

## Blocking 요소

| 항목 | 상태 |
|---|---|
| glibc 하한 | 결정: `ubuntu-22.04` 러너 (2.35). RHEL 9(2.34)는 제외, 소스 빌드 안내 |
| TLS / openssl | 문제 없음 (rustls + ring) |
| SIMD / SIGILL | 문제 없음. turbovec이 `is_x86_feature_detected!`로 런타임 분기. `-C target-cpu=native` 금지 |
| Rust 버전 | turbovec MSRV 1.89, CI는 stable |

## 변경 범위

1. `.github/workflows/release.yml`
   - 매트릭스에 `x86_64-unknown-linux-gnu` / `ubuntu-22.04` / `linux-x86_64` 추가
   - `workflow_dispatch` 추가: 빌드만 수행. `release` job은 `push`(태그)에서만 실행
2. `README.md`: Linux 바이너리 설치 절 추가, glibc 요구 버전 명시
3. Homebrew 포뮬러는 이번 범위에서 제외 (macOS aarch64 전용 유지)
4. `site/`는 Cargo 항목이 이미 Linux를 포함하므로 변경 없음

## 검증 순서

1. Actions에서 `Release` 워크플로를 수동 실행(workflow_dispatch)하고 `glc-linux-x86_64` 아티팩트를 받는다.
2. `ubuntu:22.04`, `debian:12` 컨테이너에서 `glc --help`와 임시 git 저장소 smoke test, `glc index`를 실행한다.
3. 가능하면 AVX2 없음 / AVX-512 있음 CPU에서 `glc index`를 확인한다.
4. 정식 `v*` 태그를 푸시한다.

## 미검증 / 범위 밖

- `ubuntu-22.04` 러너 실빌드와 러너 이미지 폐지 일정
- musl 정적 빌드, cargo-zigbuild (glibc 하한을 더 낮춰야 할 때의 대안)
- TUI 대화형 실행, 다른 CPU에서의 실측
- Linux aarch64, Linuxbrew 포뮬러
- 바이너리 크기 축소(`[profile.release]` strip/lto)는 모든 타깃에 영향을 주므로 별도 작업
