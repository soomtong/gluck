# Chunking Implementation Plan

## Status

- **Type:** Implementation plan, follow-up to v2 design spec
- **Implements:** `Chunk` type system defined in `docs/superpowers/specs/2026-05-23-semantic-search-design-v2.md` §Components.4 (Chunking)
- **Predecessor for:** `model2vec-integration-plan`, `tantivy-korean-tokenizer-plan`, `indexer-pipeline-plan`

## Context

v2 spec은 `Chunk` enum 세 가지 variant (`CommitMessage` / `WholeFile` / `Symbol`)를 정의했다. 본 plan은 그 타입을 실제 코드로 구현하고, **tree-sitter를 활용한 함수 단위 코드 청킹**과 **fallback 전략**을 확립한다. 이 단계가 먼저 완료되어야 임베딩·인덱싱·검색 코드 path가 결정되므로 v2 후속 작업 중 *가장 먼저* 진행한다.

핵심 원칙:

1. **gluck의 기존 tree-sitter 인프라 재사용** — `src/highlight/engine.rs`의 언어 감지 로직과 의존성을 공유. 단, *highlight*와 *chunking*은 다른 tree-sitter API 계층을 쓰므로 (HighlightConfiguration vs Parser/Query) 코드는 분리.
2. **Sum type + parse-don't-validate** — 미지원 언어, 이진 파일, 너무 큰 파일 등 모든 edge case가 `Chunk` enum의 어떤 variant에 해당하는지 *컴파일 타임에* 명확.
3. **UTF-8 안전** — `4fcbce6` 커밋(CommitIndex char boundary 패닉)과 같은 함정을 청킹에서 반복하지 않기 위해 byte offset이 아니라 char/line offset 기반 API 우선.

---

## 지원 언어 (MVP scope)

| 언어 | 청킹 지원 | 추론 근거 |
|---|---|---|
| Rust | ✅ tree-sitter | 이미 의존성 있음, gluck 자체 코드 |
| Python | ✅ tree-sitter | 흔함, 청킹 패턴 단순 |
| JavaScript | ✅ tree-sitter | 흔함 |
| TypeScript | ✅ tree-sitter | 흔함 |
| TSX | ✅ tree-sitter | React/Vue 코드에서 TS보다 흔할 수 있음, 별도 grammar |
| Go | ✅ tree-sitter | 흔함 |
| 그 외 (C/C++/Java/...) | ⚠️ fallback | fixed-size 또는 WholeFile |
| Markdown | ⚠️ WholeFile | 청킹 의미 약함 |
| 이진 | ❌ skip | `is_binary_blob()` |

C/C++/Java 등 추가는 Phase 2 — 의존성 늘리기 전에 MVP로 실용성 확인.

---

## 변경 순서 (의존성 기준)

### Step 1: tree-sitter 언어 의존성 추가 — `Cargo.toml`

~~~toml
[dependencies]
# 기존
tree-sitter = "0.22"
tree-sitter-highlight = "0.22"
tree-sitter-rust = "0.23"
tree-sitter-markdown-fork = "0.7.3"

# 신규 (청킹용)
tree-sitter-python = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-go = "0.23"
~~~

**버전 정렬:** `tree-sitter` 0.22와 호환되는 0.23.x 언어 파서들. 각 crate의 `Cargo.toml`에서 ABI 호환성 확인 필요. 만약 충돌하면 0.22.x 라인으로 통일.

**바이너리 크기 영향:** 각 언어 파서 ~200KB-500KB 컴파일된 크기. 다섯 개 추가 시 약 2MB. 단일 바이너리 정체성에 영향 미미.

**수정 파일:** `Cargo.toml`

---

### Step 2: 언어 식별 공유 — `src/lang.rs` (신규)

현재 `detect_language()`는 `src/highlight/engine.rs` 내부에서 `&str` → `String`을 반환. 청킹과 highlight가 공유해야 하므로 외부 모듈로 추출하고 타입을 강화한다.

~~~rust
// src/lang.rs

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    C,
    Cpp,
    Java,
    Bash,
    Toml,
    Json,
    Markdown,
    Html,
    Css,
    /// 확장자는 알지만 청킹/하이라이팅 지원 없음
    Unknown(String),
}

impl Language {
    pub fn from_path(path: &str) -> Self {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" | "mjs" => Self::JavaScript,
            "ts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "go" => Self::Go,
            "c" | "h" => Self::C,
            "cpp" | "cc" | "cxx" | "hpp" => Self::Cpp,
            "java" => Self::Java,
            "sh" | "bash" => Self::Bash,
            "toml" => Self::Toml,
            "json" => Self::Json,
            "md" => Self::Markdown,
            "html" => Self::Html,
            "css" => Self::Css,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// highlight 엔진에서 쓰던 String 표현 (호환성)
    pub fn as_str(&self) -> &str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Bash => "bash",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Css => "css",
            Self::Unknown(s) => s.as_str(),
        }
    }

    /// tree-sitter 기반 symbol 추출 지원 여부
    pub fn supports_symbol_chunking(&self) -> bool {
        matches!(
            self,
            Self::Rust | Self::Python | Self::JavaScript | Self::TypeScript | Self::Tsx | Self::Go
        )
    }
}
~~~

`Unknown(String)` 채택 — `Box::leak`은 누수가 누적되진 않지만 *읽는 사람이 한 번 멈추는 비용*이 커서 제거. 대가로 `Language: Copy`를 잃고 `Clone`이 됨. `Chunk`가 어차피 owned `String` 필드들을 들고 있어 `language` 한 필드의 clone 비용은 무시 가능.

**수정 파일:**
- 신규: `src/lang.rs`
- 수정: `src/lib.rs` — `pub mod lang;`
- 수정: `src/highlight/engine.rs` — 자체 `detect_language()` 제거, `Language::from_path(path).as_str()` 호출로 교체

**호환성 검증:** `cargo test`로 highlight 테스트 통과 확인 (그대로 동작해야 함).

---

### Step 3: `Chunk` 타입 정의 — `src/search/chunk/mod.rs` (신규)

~~~rust
// src/search/chunk/mod.rs

pub mod symbol;
pub mod file;
pub mod commit;

pub use symbol::{SymbolKind, SymbolSpan};

/// 검색 인덱스의 단위. SearchDocument의 페이로드가 됨.
#[derive(Debug, Clone)]
pub enum Chunk {
    /// 커밋 메시지 한 건.
    CommitMessage {
        oid: String,         // git OID, 20 hex bytes
        title: String,       // first line of commit message
        body: String,        // rest of message (may be empty)
        author_time: i64,    // Unix timestamp seconds
    },
    /// 파일 전체.
    /// - 작은 파일 (< 4KB)
    /// - tree-sitter 미지원 언어
    /// - 청킹 실패로 fallback
    WholeFile {
        commit_oid: String,
        path: String,
        language: crate::lang::Language,
        content: String,     // UTF-8 (lossy if originally non-UTF-8)
    },
    /// tree-sitter로 추출한 코드 심볼.
    Symbol {
        commit_oid: String,
        path: String,
        language: crate::lang::Language,
        kind: SymbolKind,
        name: String,                // 함수/구조체 이름 (e.g., "main", "Iterator")
        line_start: u32,             // 1-indexed, inclusive
        line_end: u32,               // 1-indexed, inclusive
        content: String,             // 해당 심볼의 소스 텍스트
    },
}

impl Chunk {
    /// 임베딩 모델에 입력할 텍스트.
    /// commit: title + "\n" + body
    /// file:   path + "\n" + content (path 시그널이 검색에 도움)
    /// symbol: name + "\n" + content
    pub fn embed_text(&self) -> String {
        match self {
            Self::CommitMessage { title, body, .. } => {
                if body.is_empty() {
                    title.clone()
                } else {
                    format!("{}\n{}", title, body)
                }
            }
            Self::WholeFile { path, content, .. } => format!("{}\n{}", path, content),
            Self::Symbol { name, content, .. } => format!("{}\n{}", name, content),
        }
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::CommitMessage { .. } => "commit",
            Self::WholeFile { .. } => "file",
            Self::Symbol { .. } => "symbol",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChunkError {
    #[error("tree-sitter parser failed for {language}: {message}")]
    Parse { language: &'static str, message: String },
    #[error("query compilation failed for {language}: {0}")]
    Query(String, &'static str),
    #[error("source contains invalid UTF-8")]
    InvalidUtf8,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
~~~

**중요한 디자인 결정:**

1. **`Chunk`가 `commit_oid`/`path`를 *복제*해서 보유.** 한 파일이 수십~수백 개 심볼로 쪼개지면 `commit_oid` 같은 짧은 string이 그만큼 중복된다. 메모리 효율을 따지면 `Arc<str>`/`InternedString` 도입할 수도 있지만, gluck 규모(<100K chunks)에서는 단순함이 이긴다. **`String` 채택**.
2. **`SearchDocument`라는 이름 안 씀.** v1 spec의 `SearchDocument`와 이 plan의 `Chunk`는 같은 것. v2부터 *Chunk*로 통일 (semble과 용어 일치).
3. **`content`에 source 텍스트를 통째로 보유.** 인덱싱이 끝나면 `Chunk`는 버려지므로 lifetime 복잡도를 안 만든다 (`&str` 안 쓰고 `String` 사용).

**수정 파일:**
- 신규: `src/search/chunk/mod.rs`, `src/search/chunk/symbol.rs`, `src/search/chunk/file.rs`, `src/search/chunk/commit.rs`
- 수정: `src/lib.rs` — `pub mod search;`
- 신규: `src/search/mod.rs` — `pub mod chunk;`

---

### Step 4: Symbol 추출 — `src/search/chunk/symbol.rs`

언어별 tree-sitter 쿼리로 상위 레벨 심볼을 뽑는다. *Nested function은 청크화 안 함* — 청크는 위 두께만, top-level만.

~~~rust
// src/search/chunk/symbol.rs

use crate::lang::Language;
use crate::search::chunk::ChunkError;
use tree_sitter::{Language as TsLanguage, Parser, Query, QueryCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Class,
    Module,
    TypeAlias,
}

#[derive(Debug, Clone)]
pub struct SymbolSpan {
    pub kind: SymbolKind,
    pub name: String,
    pub line_start: u32,    // 1-indexed
    pub line_end: u32,      // 1-indexed
    pub byte_start: usize,
    pub byte_end: usize,
}

pub fn extract_symbols(source: &str, language: Language) -> Result<Vec<SymbolSpan>, ChunkError> {
    let (ts_lang, query_src) = match language {
        Language::Rust       => (rust_language(),       RUST_QUERY),
        Language::Python     => (python_language(),     PYTHON_QUERY),
        Language::JavaScript => (javascript_language(), JAVASCRIPT_QUERY),
        Language::TypeScript => (typescript_language(), TYPESCRIPT_QUERY),
        Language::Tsx        => (tsx_language(),        TYPESCRIPT_QUERY),
        Language::Go         => (go_language(),         GO_QUERY),
        _ => return Ok(Vec::new()),    // 미지원 → 빈 결과
    };

    let mut parser = Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| ChunkError::Parse {
            language: language.as_str_static(),
            message: e.to_string(),
        })?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ChunkError::Parse {
            language: language.as_str_static(),
            message: "parse returned None".into(),
        })?;

    let query = Query::new(&ts_lang, query_src)
        .map_err(|e| ChunkError::Query(e.to_string(), language.as_str_static()))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut spans = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(span) = build_symbol_span(m, &query, source) {
            spans.push(span);
        }
    }
    Ok(spans)
}

fn build_symbol_span(
    m: &tree_sitter::QueryMatch,
    query: &Query,
    source: &str,
) -> Option<SymbolSpan> {
    // 쿼리는 두 capture를 가짐: @symbol (전체 노드), @name (식별자)
    let mut symbol_node = None;
    let mut name_node = None;
    let mut kind = SymbolKind::Function;     // 쿼리 capture 이름으로 분기
    for cap in m.captures {
        let cap_name = &query.capture_names()[cap.index as usize];
        match cap_name.as_str() {
            "symbol.function" => { symbol_node = Some(cap.node); kind = SymbolKind::Function; }
            "symbol.method"   => { symbol_node = Some(cap.node); kind = SymbolKind::Method; }
            "symbol.struct"   => { symbol_node = Some(cap.node); kind = SymbolKind::Struct; }
            "symbol.enum"     => { symbol_node = Some(cap.node); kind = SymbolKind::Enum; }
            "symbol.trait"    => { symbol_node = Some(cap.node); kind = SymbolKind::Trait; }
            "symbol.class"    => { symbol_node = Some(cap.node); kind = SymbolKind::Class; }
            "name"            => { name_node = Some(cap.node); }
            _ => {}
        }
    }
    let symbol_node = symbol_node?;
    let name_node = name_node?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();
    Some(SymbolSpan {
        kind,
        name,
        line_start: (symbol_node.start_position().row + 1) as u32,
        line_end:   (symbol_node.end_position().row + 1) as u32,
        byte_start: symbol_node.start_byte(),
        byte_end:   symbol_node.end_byte(),
    })
}

// --- 언어별 binding (highlight 모듈과 같은 패턴) ---
fn rust_language() -> TsLanguage {
    let raw_fn = tree_sitter_rust::LANGUAGE.into_raw();
    let raw_ptr = unsafe { raw_fn() };
    unsafe { TsLanguage::from_raw(raw_ptr as *const _) }
}
fn python_language() -> TsLanguage     { tree_sitter_python::LANGUAGE.into() }
fn javascript_language() -> TsLanguage { tree_sitter_javascript::LANGUAGE.into() }
fn typescript_language() -> TsLanguage { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() }
fn tsx_language() -> TsLanguage        { tree_sitter_typescript::LANGUAGE_TSX.into() }
fn go_language() -> TsLanguage         { tree_sitter_go::LANGUAGE.into() }

// --- 쿼리들 ---
// 핵심 원칙: top-level 심볼만. nested function/method 제외.
// `((source_file (xxx) @symbol))` 형태로 root 직속 자식만 매칭.

const RUST_QUERY: &str = r#"
((source_file
   (function_item name: (identifier) @name) @symbol.function))

((source_file
   (struct_item name: (type_identifier) @name) @symbol.struct))

((source_file
   (enum_item name: (type_identifier) @name) @symbol.enum))

((source_file
   (trait_item name: (type_identifier) @name) @symbol.trait))

; impl 자체는 청크화하지 않고, 그 내부 메서드만 추출
((source_file
   (impl_item
     (declaration_list
       (function_item name: (identifier) @name) @symbol.method))))
"#;

const PYTHON_QUERY: &str = r#"
((module
   (function_definition name: (identifier) @name) @symbol.function))

((module
   (class_definition name: (identifier) @name) @symbol.class))
"#;

const JAVASCRIPT_QUERY: &str = r#"
((program
   (function_declaration name: (identifier) @name) @symbol.function))

((program
   (class_declaration name: (identifier) @name) @symbol.class))

; const foo = () => {...}, const foo = function() {...}
((program
   (lexical_declaration
     (variable_declarator
       name: (identifier) @name
       value: [(arrow_function) (function_expression)]) ) @symbol.function))
"#;

const TYPESCRIPT_QUERY: &str = r#"
((program
   (function_declaration name: (identifier) @name) @symbol.function))

((program
   (class_declaration name: (type_identifier) @name) @symbol.class))

((program
   (interface_declaration name: (type_identifier) @name) @symbol.struct))

((program
   (type_alias_declaration name: (type_identifier) @name) @symbol.struct))
"#;

const GO_QUERY: &str = r#"
((source_file
   (function_declaration name: (identifier) @name) @symbol.function))

((source_file
   (method_declaration name: (field_identifier) @name) @symbol.method))

((source_file
   (type_declaration
     (type_spec name: (type_identifier) @name)) @symbol.struct))
"#;
~~~

**쿼리 작성 원칙:**

- `((source_file (xxx)))` 형태로 root의 직접 자식만 매칭 → nested 함수/메서드 자동 제외
- `@name` capture는 *식별자 노드*만, `@symbol.xxx` capture는 *전체 노드* (line range 계산용)
- 각 언어의 root node 이름이 다름: Rust/Go = `source_file`, Python = `module`, JS/TS = `program`

**Rust `impl_item` 처리:**

`impl Foo`는 컨테이너일 뿐이므로 청크화하지 않고, 내부 `function_item`만 `Method` variant로 추출한다. 큰 impl 블록(100줄+)을 통째로 청크화하면 검색 단위가 너무 거칠다 — 한 메서드를 찾고 싶은데 impl 전체가 결과로 나오는 상황을 피한다. 쿼리에서 `(impl_item (declaration_list (function_item ...)))`로 한 단계 더 들어감으로써 nested function 제외 원칙(`(source_file (...))`)의 *예외*가 되지만, impl은 scope가 아니라 컨테이너이므로 이 예외는 정당화된다.

**수정 파일:** `src/search/chunk/symbol.rs` (신규)

---

### Step 5: File → Chunks 파이프라인 — `src/search/chunk/file.rs`

파일 하나가 들어오면 적절한 `Chunk` variant들로 변환.

~~~rust
// src/search/chunk/file.rs

use crate::lang::Language;
use crate::search::chunk::{Chunk, ChunkError};
use crate::search::chunk::symbol::extract_symbols;

/// 파일 크기 임계값 — 미만은 WholeFile, 이상은 Symbol 시도
const WHOLE_FILE_THRESHOLD: usize = 8 * 1024;     // 8 KB

/// 한 파일을 청크들로 변환.
///
/// - 이진 파일은 호출 전에 거른다 (`is_binary_blob`).
/// - source는 lossy UTF-8 변환 후 들어와야 한다.
pub fn file_to_chunks(
    commit_oid: &str,
    path: &str,
    source: &str,
) -> Result<Vec<Chunk>, ChunkError> {
    let language = Language::from_path(path);

    // 1. 작은 파일은 통째로
    if source.len() < WHOLE_FILE_THRESHOLD {
        return Ok(vec![Chunk::WholeFile {
            commit_oid: commit_oid.to_string(),
            path: path.to_string(),
            language,
            content: source.to_string(),
        }]);
    }

    // 2. tree-sitter 지원 언어 → Symbol 시도. 하나라도 잡히면 Symbol 청크들 사용.
    if language.supports_symbol_chunking() {
        let spans = extract_symbols(source, language.clone())?;
        if !spans.is_empty() {
            let chunks = spans
                .into_iter()
                .filter_map(|s| span_to_chunk(commit_oid, path, language.clone(), source, s))
                .collect();
            return Ok(chunks);
        }
    }

    // 3. fallback: WholeFile (큰 파일이지만 심볼 추출 0건)
    Ok(vec![Chunk::WholeFile {
        commit_oid: commit_oid.to_string(),
        path: path.to_string(),
        language,
        content: source.to_string(),
    }])
}

fn span_to_chunk(
    commit_oid: &str,
    path: &str,
    language: Language,
    source: &str,
    span: crate::search::chunk::SymbolSpan,
) -> Option<Chunk> {
    // UTF-8 boundary 안전한 슬라이싱
    let content = source.get(span.byte_start..span.byte_end)?.to_string();
    Some(Chunk::Symbol {
        commit_oid: commit_oid.to_string(),
        path: path.to_string(),
        language,
        kind: span.kind,
        name: span.name,
        line_start: span.line_start,
        line_end: span.line_end,
        content,
    })
}
~~~

**왜 coverage 휴리스틱을 안 쓰는가:**

Rust 파일은 `use`/매크로/상수가 많아 함수만 잡으면 coverage가 30% 이하인 게 *정상*이다. 50% 컷오프는 정상 파일을 fallback으로 떨어뜨려 검색 granularity를 거꾸로 떨어뜨린다. 빠진 부분(예: use 블록)이 검색 품질에 정말 영향을 주면 그때 Symbol+WholeFile 동시 인덱싱으로 갈 수 있지만, MVP는 단순함을 선택 — `!spans.is_empty()` 하나로 충분.

**UTF-8 안전성:**

`source.get(byte_start..byte_end)`는 *char boundary가 아닌 byte 범위*면 `None`을 반환 → 패닉 안 함. tree-sitter는 byte offset 단위로 동작하지만 정상적으로 파싱된 source에서는 항상 char boundary에 떨어진다. 만약 떨어지지 않으면 (이론상 일어나선 안 되지만) 해당 심볼을 skip — `4fcbce6` 종류의 패닉을 원천 방지.

**수정 파일:** `src/search/chunk/file.rs` (신규)

---

### Step 6: Commit Message → Chunk — `src/search/chunk/commit.rs`

~~~rust
// src/search/chunk/commit.rs

use crate::search::chunk::Chunk;
use git2::Commit;

pub fn commit_to_chunk(commit: &Commit) -> Chunk {
    let full_msg = commit.message().unwrap_or("");
    let (title, body) = split_title_body(full_msg);
    Chunk::CommitMessage {
        oid: commit.id().to_string(),
        title,
        body,
        author_time: commit.author().when().seconds(),
    }
}

fn split_title_body(message: &str) -> (String, String) {
    let mut lines = message.lines();
    let title = lines.next().unwrap_or("").to_string();
    // 빈 줄 하나 건너뛰고 나머지를 body로
    let rest: Vec<&str> = lines.collect();
    let body = match rest.split_first() {
        Some((first, rest)) if first.is_empty() => rest.join("\n"),
        _ => rest.join("\n"),
    };
    (title, body)
}
~~~

빈 메시지 처리: `title == ""` 인 경우도 chunk 자체는 만들어진다 (`oid`로 검색은 됨, 메시지 검색에서는 무력). 인덱서 단계에서 거를지 본 plan에서는 결정 안 함 — 인덱서 plan에서 결정.

**수정 파일:** `src/search/chunk/commit.rs` (신규)

---

### Step 7: 테스트

#### 7.1 단위 테스트 — Symbol 추출 (`symbol.rs` 안)

각 언어별로 known input → 예상 spans 검증.

~~~rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Language;

    #[test]
    fn rust_extracts_top_level_functions() {
        let src = r#"
fn outer() {
    fn inner() {}
}
fn another() {}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let names: Vec<_> = spans.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["outer", "another"]);   // inner 제외
    }

    #[test]
    fn rust_extracts_struct_and_impl_methods() {
        let src = r#"
struct Foo { x: i32 }
impl Foo {
    fn bar(&self) {}
    fn baz(&self) {}
}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let kinds_names: Vec<_> = spans.iter().map(|s| (s.kind, s.name.as_str())).collect();
        assert!(kinds_names.contains(&(SymbolKind::Struct, "Foo")));
        // impl 자체는 청크 안 함, 내부 메서드만 추출
        assert!(kinds_names.contains(&(SymbolKind::Method, "bar")));
        assert!(kinds_names.contains(&(SymbolKind::Method, "baz")));
    }

    #[test]
    fn python_top_level_only() {
        let src = r#"
def outer():
    def inner():
        pass

class Bar:
    def method(self):
        pass
"#;
        let spans = extract_symbols(src, Language::Python).unwrap();
        let names: Vec<_> = spans.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"Bar"));
        assert!(!names.contains(&"inner"));
        assert!(!names.contains(&"method"));    // 클래스 메서드는 별도 plan
    }

    #[test]
    fn unsupported_language_returns_empty() {
        let spans = extract_symbols("// some text", Language::C).unwrap();
        assert!(spans.is_empty());
    }
}
~~~

#### 7.2 단위 테스트 — File 파이프라인 (`file.rs` 안)

~~~rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_file_becomes_whole_file() {
        let chunks = file_to_chunks("oid", "tiny.rs", "fn main(){}").unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], Chunk::WholeFile { .. }));
    }

    #[test]
    fn large_rust_file_becomes_symbols() {
        let src = format!(
            "// padding\n{}\nfn one() {{}}\nfn two() {{}}\nfn three() {{}}\n",
            "// ".repeat(4_000)
        );
        let chunks = file_to_chunks("oid", "big.rs", &src).unwrap();
        // 큰 파일이고 심볼이 있으므로 Symbol variant들
        assert!(chunks.iter().all(|c| matches!(c, Chunk::Symbol { .. })));
        assert!(chunks.len() >= 3);
    }

    #[test]
    fn utf8_safe_slicing() {
        // 한글이 섞인 큰 파일 — byte 슬라이싱이 char boundary를 침범하지 않는지
        let body = "// 한국어 주석이 잔뜩 들어간 큰 파일 ".repeat(200);
        let src = format!("{}\nfn foo() {{}}\n", body);
        let chunks = file_to_chunks("oid", "korean.rs", &src).unwrap();
        // panic 없이 동작하면 통과
        assert!(!chunks.is_empty());
    }
}
~~~

#### 7.3 Integration 테스트 — gluck 자체 코드에 적용

~~~rust
// tests/chunk_integration.rs
use gluck::lang::Language;
use gluck::search::chunk::file_to_chunks;

#[test]
fn chunk_gluck_own_main() {
    let src = std::fs::read_to_string("src/main.rs").unwrap();
    let chunks = file_to_chunks("HEAD", "src/main.rs", &src).unwrap();
    assert!(!chunks.is_empty());
    // main.rs는 최소 `fn main()` 하나는 있어야 함
    // (작은 파일이면 WholeFile, 크면 Symbol)
}

#[test]
fn chunk_gluck_highlight_engine() {
    // 253줄의 큰 파일 → Symbol 청킹 트리거
    let src = std::fs::read_to_string("src/highlight/engine.rs").unwrap();
    let chunks = file_to_chunks("HEAD", "src/highlight/engine.rs", &src).unwrap();
    assert!(chunks.len() > 1, "expected multiple symbol chunks");
    let symbol_chunks: Vec<_> = chunks.iter()
        .filter(|c| matches!(c, gluck::search::chunk::Chunk::Symbol { .. }))
        .collect();
    assert!(!symbol_chunks.is_empty());
}
~~~

#### 7.4 검증 단계

1. `cargo build` — 모든 신규 모듈 컴파일
2. `cargo test` — 위 모든 테스트 통과
3. `cargo clippy` — 경고 없음
4. `cargo test highlight` — 기존 highlight 테스트 *여전히* 통과 (Language refactor 회귀 검증)

---

## 향후 변경 영향도

본 plan이 끝나면 다음 plan들이 `Chunk`를 입력으로 받을 수 있게 된다:

| 후속 plan | `Chunk`를 어떻게 쓰는가 |
|---|---|
| model2vec-integration | `chunk.embed_text()` → 256-dim vector |
| tantivy-korean-tokenizer | `chunk.kind_str() / title / body / path` → Tantivy doc |
| indexer-pipeline | commit walker + tree walker → `Vec<Chunk>` → indexers |
| search-modal-ui | `Chunk` variant 기반으로 결과 그룹화 (Commits / Files & Symbols) |

`Chunk` enum이 안정되면 위 plan들은 *서로 독립적으로* 진행 가능.

---

## Resolved Decisions

구현 전 검토에서 다음 결정이 확정되었다:

1. **`Unknown(String)` 채택** — `Box::leak` 패턴이 코드에 남기는 인지 비용 제거. `Language: Copy`를 잃고 `Clone`이 됨. `Chunk`가 어차피 owned `String` 필드들을 들고 있어 `language` 한 필드의 clone 비용은 무시 가능.
2. **WholeFile 임계값 8KB** — 일반적인 Rust/Python 모듈 분포가 5-15KB. 4KB는 너무 보수적이라 *대부분의 실제 파일이* Symbol 청킹 경로로 빠짐.
3. **Coverage 휴리스틱 제거** — Rust는 `use`/매크로 비중이 커서 함수 coverage가 30% 이하인 게 정상. 50% 컷오프는 정상 파일을 fallback으로 떨어뜨린다. `!spans.is_empty()` 하나로 단순화. 빠진 부분이 검색 품질에 영향을 준다면 Phase 2에서 Symbol+WholeFile 동시 인덱싱 검토.
4. **`Language::Tsx` 별도 분리** — `.tsx`는 다른 grammar이고 React/Vue 코드에서 흔하다. 잘못된 grammar로 파싱하면 함수 추출이 *조용히* 실패. 비용은 enum variant 한 줄 + match arm 한 줄. 의존성 추가 없음 (`tree-sitter-typescript` crate 하나에 둘 다 포함).
5. **`impl` 컨테이너 청크화 안 함, 내부 메서드만 추출** — 큰 impl 블록은 검색 단위로 너무 거칠다. `SymbolKind::Impl` 제거, 메서드는 `SymbolKind::Method`로. 쿼리에서 `(impl_item (declaration_list (function_item ...)))`로 한 단계 더 내려감.

---

## Remaining Open Questions

실측 데이터 확보 후 조정 후보:

- **WholeFile 임계값 8KB의 실제 분포 적합성** — gluck 자체 코드 + 대상 repo 몇 개에서 *Symbol 청크 수 vs 평균 청크 크기*를 보고 조정.
- **Symbol + WholeFile 동시 인덱싱** — 검색 품질이 부족하다는 신호(예: use 블록 검색이 안 됨)가 나오면 그때 도입 검토.

---

## Design Notes

이 plan의 우아함은 **`Chunk` enum이 청킹/임베딩/인덱싱/UI 모든 후속 단계의 *공통 인터페이스*가 된다**는 점에 있다. v2 spec에서 그린 단방향 파이프라인 — `Chunk` → `(doc_id, embedding)` → `VectorIndex`/`Tantivy` — 의 가장 왼쪽 변환을 본 plan이 담당.

또한 tree-sitter를 *highlight 용도와 chunking 용도로 동시에 활용*하는 패턴은 영택님이 좋아하시는 **"같은 도구를 여러 layer가 공유한다"** 원칙 — Gleam 도메인 + Elixir 인프라가 같은 BEAM을 공유하는 패턴과 같은 정신 — 의 작은 사례다. tree-sitter는 gluck 안에서 *파싱이라는 단일 추상화* 위에 두 가지 view (visual / semantic)를 띄우는 역할을 한다.

마지막으로, `extract_symbols`가 `Result<Vec<SymbolSpan>, ChunkError>`를 반환하고 *언어가 미지원이면 빈 `Vec`을 반환* (에러가 아닌 success-with-empty)하는 디자인은 ["parse don't validate"](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/) 정신과 일치한다. 호출자는 "심볼이 추출됐다 / 안 됐다"를 *타입이 아니라 길이로* 본다 — 추가 분기 없이 `if !spans.is_empty()` 한 줄로 fallback 로직이 깔끔하게 표현된다.
