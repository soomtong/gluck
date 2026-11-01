use crate::git::repo::{GitError, GitRepo};
use git2::Oid;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub author: String,
    pub date: SystemTime,
    pub message: String,
    /// Tag names (without `refs/tags/`) pointing at this commit, sorted.
    /// Only `CommitStore` fills this; other constructors leave it empty.
    pub tags: Vec<String>,
}

impl CommitInfo {
    pub fn from_git_commit(commit: &git2::Commit) -> Self {
        let id = commit.id();
        let short_id = id.to_string()[..7.min(id.to_string().len())].to_string();
        let author = commit.author().to_string();
        let date = UNIX_EPOCH + Duration::from_secs(commit.time().seconds() as u64);
        let message = commit.message().unwrap_or("").trim().to_string();
        Self {
            id,
            short_id,
            author,
            date,
            message,
            tags: Vec::new(),
        }
    }
}

/// Map every tagged commit to its tag names (annotated tags are peeled to
/// the commit they point at). Unreadable or non-commit tags are skipped.
pub fn tag_map(repository: &git2::Repository) -> HashMap<Oid, Vec<String>> {
    let mut map: HashMap<Oid, Vec<String>> = HashMap::new();
    let Ok(refs) = repository.references_glob("refs/tags/*") else {
        return map;
    };
    for reference in refs.flatten() {
        let Ok(name) = reference.shorthand() else {
            continue;
        };
        if let Ok(commit) = reference.peel_to_commit() {
            map.entry(commit.id()).or_default().push(name.to_string());
        }
    }
    for tags in map.values_mut() {
        tags.sort();
    }
    map
}

pub fn list_commits(repo: &GitRepo) -> Result<Vec<CommitInfo>, GitError> {
    let repository = repo.repository();
    let mut revwalk = repository.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repository.find_commit(oid)?;
        commits.push(CommitInfo::from_git_commit(&commit));
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    #[test]
    fn test_list_commits() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "c.txt", b"third", "Third commit");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].message, "Third commit");
        assert_eq!(commits[2].message, "First commit");
    }

    #[test]
    fn test_commit_info_fields() {
        let (dir, repo) = init_test_repo();
        let oid = add_file_commit(&repo, "a.txt", b"hello", "Test message");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let c = &commits[0];
        assert_eq!(c.id, oid);
        assert!(c.short_id.len() <= 7);
        assert!(c.author.contains("Test"));
        assert_eq!(c.message, "Test message");
        assert!(c.tags.is_empty());
    }

    #[test]
    fn test_tag_map_lightweight_and_annotated() {
        let (dir, repo) = init_test_repo();
        let first = add_file_commit(&repo, "a.txt", b"a", "first");
        let second = add_file_commit(&repo, "b.txt", b"b", "second");
        let obj = repo.find_object(second, None).unwrap();
        repo.tag_lightweight("v0.2.0", &obj, false).unwrap();
        let sig = repo.signature().unwrap();
        repo.tag("release", &obj, &sig, "annotated", false).unwrap();

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let map = tag_map(git_repo.repository());
        assert_eq!(map.get(&second).unwrap(), &["release", "v0.2.0"]);
        assert!(!map.contains_key(&first));
    }
}
