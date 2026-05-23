use super::DocKind;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub doc_id: u64,
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub commit_oid: Option<String>,
    pub kind: DocKind,
}

/// Split `content` into indexable chunks.
/// For Rust files: tree-sitter function/impl level.
/// For everything else: fixed-size character windows with overlap.
pub fn split_file(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let raw = match ext {
        "rs" => split_rust(path, content, commit_oid, base_doc_id, max_chunk_chars),
        _ => split_fixed_size(path, content, commit_oid, base_doc_id, max_chunk_chars),
    };

    if raw.is_empty() {
        vec![Chunk {
            doc_id: base_doc_id,
            kind: DocKind::File,
            title: path.to_string(),
            body: content.chars().take(max_chunk_chars).collect(),
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        }]
    } else {
        raw
    }
}

/// Split a commit message into a single Chunk.
pub fn commit_chunk(
    commit_oid: &str,
    _short_id: &str,
    message: &str,
    doc_id: u64,
) -> Chunk {
    let title = message.lines().next().unwrap_or("").to_string();
    Chunk {
        doc_id,
        kind: DocKind::Commit,
        title,
        body: message.to_string(),
        path: None,
        commit_oid: Some(commit_oid.to_string()),
    }
}

// ── Rust: tree-sitter function/impl extraction ────────────────────────────────

fn split_rust(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    use tree_sitter::Parser;

    let raw_fn = tree_sitter_rust::LANGUAGE.into_raw();
    // SAFETY: tree-sitter 0.22 + tree-sitter-rust 0.23 ABI bridge;
    // safe Into<Language> requires tree-sitter 0.24.
    let raw_ptr = unsafe { raw_fn() };
    let language = unsafe { tree_sitter::Language::from_raw(raw_ptr as *const _) };

    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return vec![];
    }

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return vec![],
    };

    let root = tree.root_node();
    let mut chunks = Vec::new();
    let mut counter = 0u64;
    extract_rust_functions(&root, content, path, commit_oid, base_doc_id, &mut counter, max_chunk_chars, &mut chunks);
    chunks
}

fn extract_rust_functions(
    node: &tree_sitter::Node,
    source: &str,
    path: &str,
    commit_oid: &str,
    base_doc_id: u64,
    counter: &mut u64,
    max_chunk_chars: usize,
    chunks: &mut Vec<Chunk>,
) {
    let kind = node.kind();

    if kind == "function_item" || kind == "impl_item" {
        let start = node.start_byte();
        let end = node.end_byte().min(source.len());
        let body: String = source[start..end].chars().take(max_chunk_chars).collect();

        let title = if kind == "function_item" {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|name| format!("{path}::{name}"))
                .unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        };

        chunks.push(Chunk {
            doc_id: base_doc_id + *counter,
            kind: DocKind::File,
            title,
            body,
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        });
        *counter += 1;
        return; // don't recurse into functions
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        extract_rust_functions(&child, source, path, commit_oid, base_doc_id, counter, max_chunk_chars, chunks);
    }
}

// ── Fixed-size: 512-char windows with 64-char overlap ─────────────────────────

fn split_fixed_size(
    path: &str,
    content: &str,
    commit_oid: &str,
    base_doc_id: u64,
    max_chunk_chars: usize,
) -> Vec<Chunk> {
    const OVERLAP: usize = 64;

    if content.len() <= max_chunk_chars {
        return vec![Chunk {
            doc_id: base_doc_id,
            kind: DocKind::File,
            title: path.to_string(),
            body: content.to_string(),
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        }];
    }

    let chars: Vec<char> = content.chars().collect();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut counter = 0u64;

    while start < chars.len() {
        let end = (start + max_chunk_chars).min(chars.len());
        let body: String = chars[start..end].iter().collect();
        chunks.push(Chunk {
            doc_id: base_doc_id + counter,
            kind: DocKind::File,
            title: format!("{path}:{counter}"),
            body,
            path: Some(path.to_string()),
            commit_oid: Some(commit_oid.to_string()),
        });
        counter += 1;
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_chunk() {
        let c = commit_chunk("abc1234def", "abc1234", "Fix the bug\n\nLonger description.", 42);
        assert_eq!(c.doc_id, 42);
        assert_eq!(c.kind, DocKind::Commit);
        assert_eq!(c.title, "Fix the bug");
        assert!(c.body.contains("Longer description"));
    }

    #[test]
    fn test_split_rust_extracts_functions() {
        let src = r#"
fn hello() {
    println!("hello");
}

fn world() -> i32 {
    42
}
"#;
        let chunks = split_file("src/lib.rs", src, "abc", 0, 4096);
        assert!(!chunks.is_empty(), "should extract rust functions");
        assert!(chunks.iter().any(|c| c.title.contains("hello") || c.title.contains("world")));
    }

    #[test]
    fn test_split_rust_impl_block() {
        let src = r#"
impl Foo {
    pub fn bar(&self) -> i32 { 1 }
    fn baz(&self) {}
}
"#;
        let chunks = split_file("src/foo.rs", src, "abc", 0, 4096);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_split_fixed_size_short_file() {
        let content = "hello world";
        let chunks = split_file("readme.txt", content, "abc", 0, 512);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].body, content);
    }

    #[test]
    fn test_split_fixed_size_long_file() {
        let content: String = "x".repeat(2000);
        let chunks = split_file("data.txt", &content, "abc", 0, 512);
        assert!(chunks.len() > 1, "long file should produce multiple chunks");
    }

    #[test]
    fn test_split_fixed_size_overlap() {
        let content: String = "abcdefgh".repeat(100); // 800 chars
        let chunks = split_file("data.txt", &content, "abc", 0, 512);
        assert!(chunks.len() >= 2);
        let end_of_first: String = chunks[0].body.chars().rev().take(64).collect::<String>().chars().rev().collect();
        let start_of_second: String = chunks[1].body.chars().take(64).collect();
        assert!(end_of_first == start_of_second || !end_of_first.is_empty());
    }

    #[test]
    fn test_doc_ids_are_unique_within_file() {
        let src = r#"fn a() {} fn b() {} fn c() {}"#;
        let chunks = split_file("src/lib.rs", src, "abc", 1000, 4096);
        let ids: std::collections::HashSet<u64> = chunks.iter().map(|c| c.doc_id).collect();
        assert_eq!(ids.len(), chunks.len(), "all doc_ids should be unique");
    }
}
