//! Fixture TOML 로드.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::search::DocKind;
use crate::search::report::ReportError;

#[derive(Debug, Deserialize)]
pub struct FixtureSet {
    #[serde(default, rename = "query")]
    pub queries: Vec<FixtureQuery>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub expected: Vec<ExpectedHit>,
}

#[derive(Debug, Deserialize)]
pub struct ExpectedHit {
    pub path: String,
    #[serde(default)]
    pub kind: Option<DocKind>,
    #[serde(default)]
    pub title: Option<String>,
}

pub fn load(path: &Path) -> Result<FixtureSet, ReportError> {
    if !path.exists() {
        return Err(ReportError::FixturesMissing(path.to_path_buf()));
    }
    let s = std::fs::read_to_string(path)?;
    let set: FixtureSet = toml::from_str(&s)?;
    if set.queries.is_empty() {
        return Err(ReportError::EmptyFixtures);
    }
    for (i, q) in set.queries.iter().enumerate() {
        if q.expected.is_empty() {
            return Err(ReportError::EmptyExpected(i));
        }
    }
    Ok(set)
}

// 디버깅용: PathBuf 인자를 받는 thin wrapper
#[allow(clippy::ptr_arg)]
pub fn load_path(p: &PathBuf) -> Result<FixtureSet, ReportError> {
    load(p.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let p = dir.path().join("q.toml");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn loads_minimal_query() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "hello"
expected = [{ path = "src/main.rs" }]
"#,
        );
        let set = load(&p).unwrap();
        assert_eq!(set.queries.len(), 1);
        assert_eq!(set.queries[0].text, "hello");
        assert_eq!(set.queries[0].expected[0].path, "src/main.rs");
        assert!(set.queries[0].expected[0].kind.is_none());
        assert!(set.queries[0].expected[0].title.is_none());
    }

    #[test]
    fn parses_kind_and_title() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "delete_term"
expected = [
    { path = "src/search/bm25.rs", kind = "Symbol", title = "delete_doc" },
]
"#,
        );
        let set = load(&p).unwrap();
        let e = &set.queries[0].expected[0];
        assert_eq!(e.kind, Some(DocKind::Symbol));
        assert_eq!(e.title.as_deref(), Some("delete_doc"));
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.toml");
        match load(&missing) {
            Err(ReportError::FixturesMissing(p)) => assert_eq!(p, missing),
            other => panic!("expected FixturesMissing, got {other:?}"),
        }
    }

    #[test]
    fn rejects_zero_queries() {
        let dir = tempdir().unwrap();
        let p = write(&dir, "");
        match load(&p) {
            Err(ReportError::EmptyFixtures) => {}
            other => panic!("expected EmptyFixtures, got {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_expected() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "hello"
expected = []
"#,
        );
        match load(&p) {
            Err(ReportError::EmptyExpected(i)) => assert_eq!(i, 0),
            other => panic!("expected EmptyExpected, got {other:?}"),
        }
    }
}
