//! Fixture TOML 로드.

use std::path::Path;

use serde::Deserialize;

use crate::search::report::ReportError;
use crate::search::DocKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    ExactIdentifier,
    NaturalLanguage,
    Korean,
    Typo,
    Paraphrase,
    Negative,
}

#[derive(Debug, Deserialize)]
pub struct FixtureSet {
    #[serde(default, rename = "query")]
    pub queries: Vec<FixtureQuery>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureQuery {
    pub text: String,
    pub category: Category,
    #[serde(default)]
    pub expected: Vec<ExpectedHit>,
    #[serde(default)]
    pub forbidden: Vec<ForbiddenRule>,
}

#[derive(Debug, Deserialize)]
pub struct ExpectedHit {
    pub path: String,
    #[serde(default)]
    pub kind: Option<DocKind>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ForbiddenRule {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

fn validate_forbidden_rule(rule: &ForbiddenRule) -> Result<(), String> {
    match (&rule.path, &rule.path_prefix) {
        (None, None) => Err("must specify either 'path' or 'path_prefix'".into()),
        (Some(_), Some(_)) => Err("cannot specify both 'path' and 'path_prefix'".into()),
        _ => Ok(()),
    }
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
        match q.category {
            Category::Negative => {
                if !q.expected.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "negative queries must not have 'expected' entries".into(),
                    });
                }
                if q.forbidden.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "negative queries must have at least one 'forbidden' rule".into(),
                    });
                }
                for (ri, rule) in q.forbidden.iter().enumerate() {
                    validate_forbidden_rule(rule).map_err(|reason| {
                        ReportError::InvalidForbiddenRule {
                            query_index: i,
                            rule_index: ri,
                            reason,
                        }
                    })?;
                }
            }
            _ => {
                if q.expected.is_empty() {
                    return Err(ReportError::EmptyExpected(i));
                }
                if !q.forbidden.is_empty() {
                    return Err(ReportError::InvalidNegativeQuery {
                        index: i,
                        reason: "positive queries must not have 'forbidden' entries".into(),
                    });
                }
            }
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
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
category = "exact_identifier"
text = "hello"
expected = [{ path = "src/main.rs" }]
"#,
        );
        let set = load(&p).unwrap();
        assert_eq!(set.queries.len(), 1);
        assert_eq!(set.queries[0].text, "hello");
        assert_eq!(set.queries[0].category, Category::ExactIdentifier);
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
category = "exact_identifier"
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
    fn fixture_query_requires_category_field() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
text = "test"
expected = [{ path = "src/a.rs" }]
"#,
        );
        match load(&p) {
            Err(ReportError::Toml(e)) if e.contains("missing field `category`") => {}
            other => panic!("expected missing field error, got {:?}", other),
        }
    }

    #[test]
    fn parses_all_category_variants() {
        let cases = [
            ("exact_identifier", Category::ExactIdentifier),
            ("natural_language", Category::NaturalLanguage),
            ("korean", Category::Korean),
            ("typo", Category::Typo),
            ("paraphrase", Category::Paraphrase),
        ];
        for (s, expected) in cases {
            let dir = tempdir().unwrap();
            let p = write(
                &dir,
                &format!(
                    r#"
[[query]]
category = "{s}"
text = "t"
expected = [{{ path = "src/a.rs" }}]
"#
                ),
            );
            let set = load(&p).unwrap();
            assert_eq!(set.queries[0].category, expected);
        }
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
category = "exact_identifier"
text = "hello"
expected = []
"#,
        );
        match load(&p) {
            Err(ReportError::EmptyExpected(i)) => assert_eq!(i, 0),
            other => panic!("expected EmptyExpected, got {other:?}"),
        }
    }

    #[test]
    fn rejects_positive_with_forbidden() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "exact_identifier"
text = "test"
expected = [{ path = "src/a.rs" }]
forbidden = [{ path_prefix = "src/" }]
"#,
        );
        match load(&p) {
            Err(ReportError::InvalidNegativeQuery { .. }) => {}
            other => panic!("expected InvalidNegativeQuery, got {other:?}"),
        }
    }

    #[test]
    fn rejects_negative_with_expected() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "negative"
text = "test"
expected = [{ path = "src/a.rs" }]
forbidden = [{ path_prefix = "src/" }]
"#,
        );
        match load(&p) {
            Err(ReportError::InvalidNegativeQuery { .. }) => {}
            other => panic!("expected InvalidNegativeQuery, got {other:?}"),
        }
    }

    #[test]
    fn rejects_negative_without_forbidden() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "negative"
text = "test"
"#,
        );
        match load(&p) {
            Err(ReportError::InvalidNegativeQuery { .. }) => {}
            other => panic!("expected InvalidNegativeQuery, got {other:?}"),
        }
    }

    #[test]
    fn rejects_forbidden_rule_with_both_path_and_prefix() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "negative"
text = "test"
forbidden = [{ path = "src/a.rs", path_prefix = "src/" }]
"#,
        );
        match load(&p) {
            Err(ReportError::InvalidForbiddenRule { .. }) => {}
            other => panic!("expected InvalidForbiddenRule, got {other:?}"),
        }
    }

    #[test]
    fn rejects_forbidden_rule_with_neither() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "negative"
text = "test"
forbidden = [{}]
"#,
        );
        match load(&p) {
            Err(ReportError::InvalidForbiddenRule { .. }) => {}
            other => panic!("expected InvalidForbiddenRule, got {other:?}"),
        }
    }

    #[test]
    fn loads_negative_query_with_path_prefix() {
        let dir = tempdir().unwrap();
        let p = write(
            &dir,
            r#"
[[query]]
category = "negative"
text = "react component lifecycle"
forbidden = [{ path_prefix = "src/" }]
"#,
        );
        let set = load(&p).unwrap();
        assert_eq!(set.queries.len(), 1);
        assert_eq!(set.queries[0].category, Category::Negative);
        assert!(set.queries[0].expected.is_empty());
        assert_eq!(
            set.queries[0].forbidden[0].path_prefix.as_deref(),
            Some("src/")
        );
    }

    #[test]
    fn loads_project_fixtures() {
        let p = std::path::Path::new("tests/fixtures/search_queries.toml");
        if p.exists() {
            let set = load(p).expect("project fixtures must be valid");
            assert!(
                set.queries.len() >= 7,
                "expected at least 7 queries, got {}",
                set.queries.len()
            );
        }
    }
}
