mod commit;
mod file;
mod symbol;

pub use commit::{changed_paths, commit_to_chunk};
pub use file::split_file;
pub use symbol::{extract_symbols, SymbolKind, SymbolSpan};

use thiserror::Error;

#[derive(Debug, Clone)]
pub enum Chunk {
    CommitMessage {
        oid: String,
        title: String,
        body: String,
        author_time: i64,
        /// Paths changed relative to the first parent (capped). Embedded only.
        paths: Vec<String>,
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
            Chunk::CommitMessage {
                title, body, paths, ..
            } => {
                let mut text = title.clone();
                if !body.is_empty() {
                    text.push('\n');
                    text.push_str(body);
                }
                if !paths.is_empty() {
                    text.push('\n');
                    text.push_str(&paths.join(" "));
                }
                text
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

    pub fn bm25_title(&self) -> String {
        match self {
            Chunk::CommitMessage { title, .. } => title.clone(),
            Chunk::WholeFile { path, .. } => path.clone(),
            Chunk::Symbol {
                path, symbol_name, ..
            } => format!("{}::{}", path, symbol_name),
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

#[derive(Debug, Error)]
pub enum ChunkError {
    #[error("tree-sitter parse failed for {language}: {message}")]
    Parse {
        language: &'static str,
        message: String,
    },
    #[error("tree-sitter query failed for {language}: {message}")]
    Query {
        language: &'static str,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_bm25_title_includes_path_and_name() {
        let c = Chunk::Symbol {
            commit_oid: "abc".into(),
            path: "src/search/error.rs".into(),
            symbol_name: "handle_io_error".into(),
            kind: SymbolKind::Function,
            line_start: 1,
            line_end: 5,
            content: "fn handle_io_error() {}".into(),
        };
        let title = c.bm25_title();
        assert!(
            title.contains("src/search/error.rs"),
            "bm25_title must include path"
        );
        assert!(
            title.contains("handle_io_error"),
            "bm25_title must include symbol name"
        );
    }

    #[test]
    fn commit_embed_text_no_body() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: String::new(),
            author_time: 0,
            paths: vec![],
        };
        assert_eq!(c.embed_text(), "Fix bug");
    }

    #[test]
    fn commit_embed_text_with_body() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: "details".into(),
            author_time: 0,
            paths: vec![],
        };
        assert!(c.embed_text().contains("Fix bug"));
        assert!(c.embed_text().contains("details"));
    }

    #[test]
    fn commit_embed_text_appends_paths() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: String::new(),
            author_time: 0,
            paths: vec!["src/ui/view.rs".into(), "src/app.rs".into()],
        };
        assert_eq!(c.embed_text(), "Fix bug\nsrc/ui/view.rs src/app.rs");
    }
}
