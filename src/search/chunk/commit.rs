use std::time::SystemTime;

use crate::git::commit::CommitInfo;
use crate::search::chunk::Chunk;

/// Upper bound on changed paths attached to a commit chunk. Bulk commits
/// (vendoring, renames) would otherwise drown the message in path noise.
pub const MAX_COMMIT_PATHS: usize = 20;

pub fn commit_to_chunk(info: &CommitInfo, paths: Vec<String>) -> Chunk {
    let (title, body) = split_title_body(&info.message);
    let author_time = info
        .date
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Chunk::CommitMessage {
        oid: info.id.to_string(),
        title,
        body,
        author_time,
        paths,
    }
}

/// Paths changed by `commit` relative to its first parent (empty tree for a
/// root commit). Name-only tree diff — no content is read. Capped at
/// [`MAX_COMMIT_PATHS`]; errors yield an empty list.
pub fn changed_paths(repo: &git2::Repository, commit: &git2::Commit) -> Vec<String> {
    let Ok(tree) = commit.tree() else {
        return vec![];
    };
    let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    let Ok(diff) = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) else {
        return vec![];
    };
    diff.deltas()
        .filter_map(|d| d.new_file().path().or_else(|| d.old_file().path()))
        .filter_map(|p| p.to_str().map(str::to_string))
        .take(MAX_COMMIT_PATHS)
        .collect()
}

fn split_title_body(msg: &str) -> (String, String) {
    let mut lines = msg.splitn(2, '\n');
    let title = lines.next().unwrap_or("").trim().to_string();
    let body = lines.next().unwrap_or("").trim().to_string();
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_title_and_body() {
        let (t, b) = split_title_body("Fix bug\n\nLong description here.");
        assert_eq!(t, "Fix bug");
        assert!(b.contains("Long description"));
    }

    #[test]
    fn changed_paths_lists_files_touched_by_commit() {
        use crate::git::repo::tests::{add_file_commit, init_test_repo};
        let (_dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"one", "first");
        let oid = add_file_commit(&repo, "b/c.rs", b"two", "second");
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(changed_paths(&repo, &commit), vec!["b/c.rs".to_string()]);
    }

    #[test]
    fn title_only_yields_empty_body() {
        let (t, b) = split_title_body("Single-line message");
        assert_eq!(t, "Single-line message");
        assert!(b.is_empty());
    }
}
