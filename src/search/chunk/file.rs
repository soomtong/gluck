use super::symbol::{extract_symbols, SymbolSpan};
use super::Chunk;
use crate::lang::Language;

/// 파일 크기 임계값 — 미만은 WholeFile, 이상은 Symbol 시도.
const WHOLE_FILE_THRESHOLD: usize = 8 * 1024; // 8 KB

/// 너무 짧은 심볼은 청크화 가치가 없다 — 1줄짜리 setter 등을 제외.
const MIN_SYMBOL_LINES: u32 = 2;

pub fn split_file(commit_oid: &str, path: &str, content: &str) -> Vec<Chunk> {
    if content.len() < WHOLE_FILE_THRESHOLD {
        return vec![Chunk::WholeFile {
            commit_oid: commit_oid.to_string(),
            path: path.to_string(),
            content: content.to_string(),
        }];
    }

    let language = Language::from_path(path);
    if let Some(lang) = language.filter(|l| l.supports_symbol_chunking()) {
        if let Ok(spans) = extract_symbols(content, lang) {
            if !spans.is_empty() {
                let chunks: Vec<Chunk> = spans
                    .into_iter()
                    .filter_map(|s| span_to_chunk(commit_oid, path, content, s))
                    .collect();
                if !chunks.is_empty() {
                    return chunks;
                }
            }
        }
    }

    vec![Chunk::WholeFile {
        commit_oid: commit_oid.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    }]
}

fn span_to_chunk(commit_oid: &str, path: &str, source: &str, span: SymbolSpan) -> Option<Chunk> {
    if span.line_end.saturating_sub(span.line_start) < MIN_SYMBOL_LINES.saturating_sub(1) {
        return None;
    }
    // UTF-8 boundary 안전한 슬라이싱 — char boundary가 아니면 None (panic 안 함)
    let content = source.get(span.byte_start..span.byte_end)?.to_string();
    Some(Chunk::Symbol {
        commit_oid: commit_oid.to_string(),
        path: path.to_string(),
        symbol_name: span.name,
        kind: span.kind,
        line_start: span.line_start,
        line_end: span.line_end,
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::chunk::SymbolKind;

    #[test]
    fn small_file_becomes_whole_file() {
        let chunks = split_file("oid", "tiny.rs", "fn main(){}");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], Chunk::WholeFile { .. }));
    }

    #[test]
    fn large_rust_file_produces_symbols() {
        let big_fn = format!(
            "fn foo() {{\n{}\n}}\nfn bar() {{\n{}\n}}",
            "    let x = 1;\n".repeat(500),
            "    let y = 2;\n".repeat(500),
        );
        let chunks = split_file("abc", "src/lib.rs", &big_fn);
        assert!(chunks.iter().any(|c| matches!(c, Chunk::Symbol { .. })));
    }

    #[test]
    fn utf8_safe_slicing_does_not_panic() {
        let body = "// 한국어 주석이 잔뜩 들어간 큰 파일\n".repeat(400);
        let src = format!("{}\nfn foo() {{\n    let x = 1;\n}}\n", body);
        assert!(src.len() > 8 * 1024);
        // panic 없이 동작하면 통과
        let chunks = split_file("oid", "korean.rs", &src);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn unsupported_language_large_file_falls_back_to_whole_file() {
        // .java는 supports_symbol_chunking == false. 큰 파일이지만 WholeFile.
        let src = "class Foo { int x; }\n".repeat(500);
        assert!(src.len() > 8 * 1024);
        let chunks = split_file("oid", "Big.java", &src);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], Chunk::WholeFile { .. }));
    }

    #[test]
    fn symbol_kind_preserved() {
        // Method/Function/Struct 분리 확인용
        let big_fn = format!(
            "struct Foo {{ x: i32 }}\nimpl Foo {{\n    fn bar(&self) {{\n{}\n}}\n}}\nfn top() {{\n{}\n}}",
            "        let _ = 1;\n".repeat(300),
            "    let _ = 1;\n".repeat(300),
        );
        let chunks = split_file("oid", "big.rs", &big_fn);
        let kinds: Vec<SymbolKind> = chunks
            .iter()
            .filter_map(|c| match c {
                Chunk::Symbol { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&SymbolKind::Method));
        assert!(kinds.contains(&SymbolKind::Function));
    }
}
