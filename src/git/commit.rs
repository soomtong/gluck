use crate::git::repo::GitRepo;
use anyhow::Result;
use git2::Oid;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub author: String,
    pub date: SystemTime,
    pub message: String,
}

impl CommitInfo {
    pub fn from_git_commit(commit: &git2::Commit) -> Self {
        let id = commit.id();
        let short_id = format!("{}", &id.to_string()[..7.min(id.to_string().len())]);
        let author = commit.author().to_string();
        let date = UNIX_EPOCH + Duration::from_secs(commit.time().seconds() as u64);
        let message = commit.message().unwrap_or("").trim().to_string();
        Self {
            id,
            short_id,
            author,
            date,
            message,
        }
    }
}

pub fn list_commits(repo: &GitRepo) -> Result<Vec<CommitInfo>> {
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

pub fn search_commits(commits: &[CommitInfo], query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    commits
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.message.to_lowercase().contains(&q)
                || c.author.to_lowercase().contains(&q)
                || c.short_id.starts_with(&q)
        })
        .map(|(i, _)| i)
        .collect()
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
    }

    #[test]
    fn test_search_by_message() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"a", "Add auth module");
        add_file_commit(&repo, "b.txt", b"b", "Fix login bug");
        add_file_commit(&repo, "c.txt", b"c", "Update README");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let results = search_commits(&commits, "auth");
        assert_eq!(results, vec![2]);
    }

    #[test]
    fn test_search_by_hash_prefix() {
        let (dir, repo) = init_test_repo();
        let oid = add_file_commit(&repo, "a.txt", b"a", "First");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let short = format!("{}", &oid.to_string()[..4]);
        let results = search_commits(&commits, &short);
        assert_eq!(results.len(), 1);
    }
}