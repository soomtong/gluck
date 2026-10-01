//! `.glcignore` — gitignore-syntax exclusions applied when indexing HEAD files.
//!
//! Read from the repository work tree root (uncommitted edits apply). Commit
//! message docs are never filtered. The file content's git blob oid is stored
//! in `meta.toml` so that editing it forces a full rebuild.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

pub const FILE_NAME: &str = ".glcignore";

pub struct GlcIgnore {
    matcher: Option<Gitignore>,
    hash: Option<String>,
}

impl GlcIgnore {
    /// Loads `<root>/.glcignore`. A missing or unreadable file yields a matcher
    /// that ignores nothing. Invalid lines are skipped.
    pub fn load(root: &Path) -> Self {
        let Ok(content) = std::fs::read_to_string(root.join(FILE_NAME)) else {
            return Self::empty();
        };
        Self::from_str(root, &content)
    }

    fn from_str(root: &Path, content: &str) -> Self {
        let mut builder = GitignoreBuilder::new(root);
        for line in content.lines() {
            let _ = builder.add_line(None, line);
        }
        let hash = git2::Oid::hash_object(git2::ObjectType::Blob, content.as_bytes())
            .ok()
            .map(|oid| oid.to_string());
        Self {
            matcher: builder.build().ok(),
            hash,
        }
    }

    pub fn empty() -> Self {
        Self {
            matcher: None,
            hash: None,
        }
    }

    /// Blob oid of the file content; `None` when there is no `.glcignore`.
    pub fn hash(&self) -> Option<&str> {
        self.hash.as_deref()
    }

    /// `path` is repo-relative with `/` separators (as stored in git trees).
    /// Parent directory patterns (`docs/reports/`) also exclude nested files.
    pub fn is_ignored(&self, path: &str) -> bool {
        self.matcher
            .as_ref()
            .is_some_and(|m| m.matched_path_or_any_parents(path, false).is_ignore())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore(content: &str) -> GlcIgnore {
        GlcIgnore::from_str(Path::new("/repo"), content)
    }

    #[test]
    fn directory_pattern_excludes_nested_files() {
        let g = ignore("docs/reports/\n");
        assert!(g.is_ignored("docs/reports/report-1.md"));
        assert!(!g.is_ignored("docs/plans/plan.md"));
    }

    #[test]
    fn glob_comment_and_negation() {
        let g = ignore("# eval artifacts\n*.toml\n!Cargo.toml\n");
        assert!(g.is_ignored("tests/fixtures/search_queries.toml"));
        assert!(!g.is_ignored("Cargo.toml"));
        assert!(!g.is_ignored("src/main.rs"));
    }

    #[test]
    fn missing_file_ignores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let g = GlcIgnore::load(dir.path());
        assert!(g.hash().is_none());
        assert!(!g.is_ignored("anything.rs"));
    }

    #[test]
    fn hash_tracks_content() {
        assert_eq!(ignore("a/\n").hash(), ignore("a/\n").hash());
        assert_ne!(ignore("a/\n").hash(), ignore("b/\n").hash());
        assert!(ignore("").hash().is_some());
    }
}
