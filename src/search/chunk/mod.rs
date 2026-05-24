mod commit;
mod file;
mod symbol;

pub use commit::commit_to_chunk;
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
    fn commit_embed_text_no_body() {
        let c = Chunk::CommitMessage {
            oid: "abc".into(),
            title: "Fix bug".into(),
            body: String::new(),
            author_time: 0,
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
        };
        assert!(c.embed_text().contains("Fix bug"));
        assert!(c.embed_text().contains("details"));
    }
}
