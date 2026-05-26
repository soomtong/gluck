use std::path::Path;

use git2::{DiffOptions, Oid};

use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::search::SearchError;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileChanges {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

pub fn compute_file_changes(
    repo: &GitRepo,
    old_oid: &str,
    new_oid: &str,
) -> Result<FileChanges, SearchError> {
    let r = repo.repository();
    let old = Oid::from_str(old_oid).map_err(|e| SearchError::Git(e.to_string()))?;
    let new = Oid::from_str(new_oid).map_err(|e| SearchError::Git(e.to_string()))?;
    let old_tree = r
        .find_commit(old)
        .and_then(|c| c.tree())
        .map_err(|e| SearchError::Git(e.to_string()))?;
    let new_tree = r
        .find_commit(new)
        .and_then(|c| c.tree())
        .map_err(|e| SearchError::Git(e.to_string()))?;

    let mut opts = DiffOptions::new();
    let diff = r
        .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))
        .map_err(|e| SearchError::Git(e.to_string()))?;

    let mut out = FileChanges::default();
    for delta in diff.deltas() {
        match delta.status() {
            git2::Delta::Added | git2::Delta::Copied => {
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.added.push(p.to_string());
                }
            }
            git2::Delta::Modified => {
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.modified.push(p.to_string());
                }
            }
            git2::Delta::Deleted => {
                if let Some(p) = delta.old_file().path().and_then(Path::to_str) {
                    out.deleted.push(p.to_string());
                }
            }
            git2::Delta::Renamed => {
                if let Some(p) = delta.old_file().path().and_then(Path::to_str) {
                    out.deleted.push(p.to_string());
                }
                if let Some(p) = delta.new_file().path().and_then(Path::to_str) {
                    out.added.push(p.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

pub fn commits_since(
    repo: &GitRepo,
    old_oid: &str,
    new_oid: &str,
) -> Result<Vec<CommitInfo>, SearchError> {
    let r = repo.repository();
    let new = Oid::from_str(new_oid).map_err(|e| SearchError::Git(e.to_string()))?;
    let old = Oid::from_str(old_oid).map_err(|e| SearchError::Git(e.to_string()))?;
    let mut revwalk = r.revwalk().map_err(|e| SearchError::Git(e.to_string()))?;
    revwalk
        .push(new)
        .map_err(|e| SearchError::Git(e.to_string()))?;
    // hide old_oid — old_oid에 도달 가능한 커밋은 결과에서 제외
    revwalk
        .hide(old)
        .map_err(|e| SearchError::Git(e.to_string()))?;
    let mut out = Vec::new();
    for oid in revwalk.flatten() {
        if let Ok(c) = r.find_commit(oid) {
            out.push(CommitInfo::from_git_commit(&c));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    #[test]
    fn test_commits_since_excludes_old_and_includes_new() {
        let (_dir, repo) = init_test_repo();
        let c1 = add_file_commit(&repo, "a.txt", b"1", "first");
        let c2 = add_file_commit(&repo, "b.txt", b"2", "second");
        let c3 = add_file_commit(&repo, "c.txt", b"3", "third");

        let gr = crate::git::repo::GitRepo::open(_dir.path()).unwrap();
        let commits = commits_since(&gr, &c1.to_string(), &c3.to_string()).unwrap();
        // old_oid(c1) 자체는 제외, c2/c3만 포함
        let oids: Vec<String> = commits.iter().map(|c| c.id.to_string()).collect();
        assert!(oids.contains(&c2.to_string()));
        assert!(oids.contains(&c3.to_string()));
        assert!(!oids.contains(&c1.to_string()));
    }

    #[test]
    fn test_added_modified_deleted_classified() {
        let (_dir, repo) = init_test_repo();
        let _c1 = add_file_commit(&repo, "keep.txt", b"v1", "Add keep");
        let c2 = add_file_commit(&repo, "drop.txt", b"x", "Add drop");
        let c3_oid = {
            // c3: modify keep.txt, delete drop.txt, add new.txt
            std::fs::write(_dir.path().join("keep.txt"), b"v2").unwrap();
            std::fs::remove_file(_dir.path().join("drop.txt")).unwrap();
            std::fs::write(_dir.path().join("new.txt"), b"hi").unwrap();
            let mut idx = repo.index().unwrap();
            idx.add_path(std::path::Path::new("keep.txt")).unwrap();
            idx.add_path(std::path::Path::new("new.txt")).unwrap();
            idx.remove_path(std::path::Path::new("drop.txt")).unwrap();
            idx.write().unwrap();
            let tree_oid = idx.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("t", "t@e").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "c3", &tree, &[&head])
                .unwrap()
                .to_string()
        };

        let gr = crate::git::repo::GitRepo::open(_dir.path()).unwrap();
        // c2를 baseline으로 사용 — c1 tree에는 drop.txt가 없어 c1→c3 diff로는 Deleted 감지 불가
        let changes = compute_file_changes(&gr, &c2.to_string(), &c3_oid).unwrap();
        assert!(changes.added.iter().any(|p| p == "new.txt"));
        assert!(changes.modified.iter().any(|p| p == "keep.txt"));
        assert!(changes.deleted.iter().any(|p| p == "drop.txt"));
    }
}
