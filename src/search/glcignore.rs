//! `.glcignore` — gitignore-syntax exclusions applied when indexing HEAD files.
//!
//! Read from the repository work tree root (uncommitted edits apply). Commit
//! message docs are never filtered. The file content's git blob oid is stored
//! in `meta.toml` so that editing it forces a full rebuild.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

pub const FILE_NAME: &str = ".glcignore";

/// Written by `glc ignore`. Only files tracked at HEAD are indexed, so this
/// targets committed noise rather than what `.gitignore` already hides.
pub const DEFAULT_TEMPLATE: &str = "\
# glc semantic-search index exclusions (gitignore syntax).
# Only files tracked at HEAD are indexed; .gitignore'd files are already skipped.
# Commit messages are always indexed. Edits trigger a full rebuild on `glc index`.

# Vendored / third-party code
vendor/
third_party/
external/

# Lockfiles
Cargo.lock
package-lock.json
yarn.lock
pnpm-lock.yaml
bun.lock
bun.lockb
go.sum
poetry.lock
uv.lock
Gemfile.lock
composer.lock

# Build output, generated and minified assets
dist/
build/
out/
*.min.js
*.min.css
*.map
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Created,
    Overwritten,
    /// File already exists and `force` was not set; nothing written.
    Exists,
}

/// Writes [`DEFAULT_TEMPLATE`] to `<root>/.glcignore`. An existing file is
/// kept unless `force` is set.
pub fn write_default(root: &Path, force: bool) -> std::io::Result<WriteOutcome> {
    let path = root.join(FILE_NAME);
    let existed = path.exists();
    if existed && !force {
        return Ok(WriteOutcome::Exists);
    }
    std::fs::write(&path, DEFAULT_TEMPLATE)?;
    Ok(if existed {
        WriteOutcome::Overwritten
    } else {
        WriteOutcome::Created
    })
}

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
    fn default_template_patterns() {
        let g = ignore(DEFAULT_TEMPLATE);
        assert!(g.is_ignored("vendor/lib/a.c"));
        assert!(g.is_ignored("web/package-lock.json"));
        assert!(g.is_ignored("static/app.min.js"));
        assert!(!g.is_ignored("src/main.rs"));
        assert!(!g.is_ignored("README.md"));
    }

    #[test]
    fn write_default_keeps_existing_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(
            write_default(dir.path(), false).unwrap(),
            WriteOutcome::Created
        );
        std::fs::write(&path, "custom/\n").unwrap();
        assert_eq!(
            write_default(dir.path(), false).unwrap(),
            WriteOutcome::Exists
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "custom/\n");
        assert_eq!(
            write_default(dir.path(), true).unwrap(),
            WriteOutcome::Overwritten
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_TEMPLATE);
    }

    #[test]
    fn hash_tracks_content() {
        assert_eq!(ignore("a/\n").hash(), ignore("a/\n").hash());
        assert_ne!(ignore("a/\n").hash(), ignore("b/\n").hash());
        assert!(ignore("").hash().is_some());
    }
}
