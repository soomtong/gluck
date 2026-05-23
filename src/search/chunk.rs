use tree_sitter::{Language, Node, Parser};

const MAX_WHOLE_FILE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Impl,
    Other,
}

#[derive(Debug, Clone)]
pub enum Chunk {
    CommitMessage {
        oid: String,
        title: String,
        body: String,
        author_time: i64,
    },
    WholeFile {
        commit_oid: String,
        path: String,
        content: String,
    },
    Symbol {
        commit_oid: String,
        path: String,
        symbol_name: String,
        kind: SymbolKind,
        line_start: u32,
        line_end: u32,
        content: String,
    },
}

impl Chunk {
    pub fn embed_text(&self) -> String {
        match self {
            Chunk::CommitMessage { title, body, .. } => {
                if body.is_empty() {
                    title.clone()
                } else {
                    format!("{}\n{}", title, body)
                }
            }
            Chunk::WholeFile { path, content, .. } => {
                let end = content.floor_char_boundary(content.len().min(2048));
                format!("{}\n{}", path, &content[..end])
            }
            Chunk::Symbol {
                path,
                symbol_name,
                content,
                ..
            } => {
                format!("{} {}\n{}", path, symbol_name, content)
            }
        }
    }

    pub fn bm25_title(&self) -> &str {
        match self {
            Chunk::CommitMessage { title, .. } => title,
            Chunk::WholeFile { path, .. } => path,
            Chunk::Symbol { symbol_name, .. } => symbol_name,
        }
    }

    pub fn bm25_body(&self) -> &str {
        match self {
            Chunk::CommitMessage { body, .. } => body,
            Chunk::WholeFile { content, .. } => content,
            Chunk::Symbol { content, .. } => content,
        }
    }

    pub fn commit_oid(&self) -> &str {
        match self {
            Chunk::CommitMessage { oid, .. } => oid,
            Chunk::WholeFile { commit_oid, .. } => commit_oid,
            Chunk::Symbol { commit_oid, .. } => commit_oid,
        }
    }
}

pub fn split_file(commit_oid: &str, path: &str, content: &str) -> Vec<Chunk> {
    if content.len() < MAX_WHOLE_FILE_BYTES {
        return vec![Chunk::WholeFile {
            commit_oid: commit_oid.to_string(),
            path: path.to_string(),
            content: content.to_string(),
        }];
    }

    let lang = detect_tree_sitter_language(path);
    if let Some(lang) = lang {
        let symbols = extract_symbols(commit_oid, path, content, lang);
        if !symbols.is_empty() {
            return symbols;
        }
    }

    vec![Chunk::WholeFile {
        commit_oid: commit_oid.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    }]
}

fn detect_tree_sitter_language(path: &str) -> Option<Language> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;
    match ext {
        "rs" => {
            let raw_fn = tree_sitter_rust::LANGUAGE.into_raw();
            let raw_ptr = unsafe { raw_fn() };
            let lang = unsafe { Language::from_raw(raw_ptr as *const _) };
            Some(lang)
        }
        _ => None,
    }
}

fn extract_symbols(
    commit_oid: &str,
    path: &str,
    content: &str,
    lang: Language,
) -> Vec<Chunk> {
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return vec![];
    }
    let tree = match parser.parse(content.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };

    let mut chunks = Vec::new();
    let root = tree.root_node();
    collect_symbols(root, content, commit_oid, path, &mut chunks);
    chunks
}

fn collect_symbols(
    node: Node,
    source: &str,
    commit_oid: &str,
    path: &str,
    out: &mut Vec<Chunk>,
) {
    let kind = node.kind();
    let sym_kind = match kind {
        "function_item" => Some(SymbolKind::Function),
        "impl_item" => Some(SymbolKind::Impl),
        "struct_item" => Some(SymbolKind::Struct),
        _ => None,
    };

    if let Some(sym_kind) = sym_kind {
        let name = find_name_child(&node, source).unwrap_or_else(|| kind.to_string());
        let start_row = node.start_position().row as u32;
        let end_row = node.end_position().row as u32;
        if end_row - start_row >= 2 {
            let byte_start = node.start_byte();
            let byte_end = node.end_byte().min(source.len());
            let content = source[byte_start..byte_end].to_string();
            out.push(Chunk::Symbol {
                commit_oid: commit_oid.to_string(),
                path: path.to_string(),
                symbol_name: name,
                kind: sym_kind,
                line_start: start_row,
                line_end: end_row,
                content,
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(child, source, commit_oid, path, out);
    }
}

fn find_name_child(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            let s = child.utf8_text(source.as_bytes()).ok()?;
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_file_whole_chunk() {
        let chunks = split_file("abc", "foo.rs", "fn hi() {}");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], Chunk::WholeFile { .. }));
    }

    #[test]
    fn test_commit_embed_text_no_body() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: String::new(),
            author_time: 0,
        };
        assert_eq!(c.embed_text(), "Fix bug");
    }

    #[test]
    fn test_commit_embed_text_with_body() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: "details".into(),
            author_time: 0,
        };
        assert!(c.embed_text().contains("Fix bug"));
        assert!(c.embed_text().contains("details"));
    }

    #[test]
    fn test_large_rust_file_produces_symbols() {
        let big_fn = format!(
            "fn foo() {{\n{}\n}}\nfn bar() {{\n{}\n}}",
            "    let x = 1;\n".repeat(200),
            "    let y = 2;\n".repeat(200),
        );
        let chunks = split_file("abc", "src/lib.rs", &big_fn);
        let has_symbol = chunks.iter().any(|c| matches!(c, Chunk::Symbol { .. }));
        assert!(has_symbol, "expected symbol chunks for large Rust file");
    }
}
