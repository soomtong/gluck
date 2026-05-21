use crate::git::commit::CommitInfo;
use crate::git::repo::{GitError, GitRepo};
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    Context {
        old_line_no: u32,
        new_line_no: u32,
        content: String,
    },
    Added {
        line_no: u32,
        content: String,
    },
    Removed {
        line_no: u32,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileChange {
    Added { path: String },
    Deleted { path: String },
    Modified { old_path: String, new_path: String },
}

impl FileChange {
    pub fn path(&self) -> &str {
        match self {
            FileChange::Added { path } => path,
            FileChange::Deleted { path } => path,
            FileChange::Modified { new_path, .. } => new_path,
        }
    }

    pub fn old_path(&self) -> Option<&str> {
        match self {
            FileChange::Added { .. } => None,
            FileChange::Deleted { path } => Some(path),
            FileChange::Modified { old_path, .. } => Some(old_path),
        }
    }

    pub fn new_path(&self) -> Option<&str> {
        match self {
            FileChange::Added { path } => Some(path),
            FileChange::Deleted { .. } => None,
            FileChange::Modified { new_path, .. } => Some(new_path),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffFile {
    pub change: Option<FileChange>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffResult {
    pub files: Vec<DiffFile>,
    pub from_id: String,
    pub to_id: String,
}

pub fn compute_diff(repo: &GitRepo, from: &CommitInfo, to: &CommitInfo) -> Result<DiffResult, GitError> {
    let repository = repo.repository();
    let from_commit = repository.find_commit(from.id)?;
    let to_commit = repository.find_commit(to.id)?;
    let from_tree = from_commit.tree()?;
    let to_tree = to_commit.tree()?;

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);

    let diff = repository.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))?;

    let files: RefCell<Vec<DiffFile>> = RefCell::new(Vec::new());
    let current_file_index: RefCell<Option<usize>> = RefCell::new(None);

    diff.foreach(
        &mut |delta: git2::DiffDelta<'_>, _: f32| {
            *current_file_index.borrow_mut() = Some(files.borrow().len());
            let change = match (delta.old_file().path(), delta.new_file().path()) {
                (None, Some(new)) => Some(FileChange::Added {
                    path: new.to_string_lossy().into_owned(),
                }),
                (Some(old), None) => Some(FileChange::Deleted {
                    path: old.to_string_lossy().into_owned(),
                }),
                (Some(old), Some(new)) => Some(FileChange::Modified {
                    old_path: old.to_string_lossy().into_owned(),
                    new_path: new.to_string_lossy().into_owned(),
                }),
                (None, None) => None,
            };
            files.borrow_mut().push(DiffFile {
                change,
                lines: Vec::new(),
            });
            true
        },
        None,
        None,
        Some(&mut |_: git2::DiffDelta<'_>, _: Option<git2::DiffHunk<'_>>, line: git2::DiffLine<'_>| {
            if let Some(idx) = *current_file_index.borrow() {
                let content = String::from_utf8_lossy(line.content()).into_owned();
                let diff_line = match line.origin() {
                    '+' => DiffLine::Added {
                        line_no: line.new_lineno().unwrap_or(0),
                        content,
                    },
                    '-' => DiffLine::Removed {
                        line_no: line.old_lineno().unwrap_or(0),
                        content,
                    },
                    _ => DiffLine::Context {
                        old_line_no: line.old_lineno().unwrap_or(0),
                        new_line_no: line.new_lineno().unwrap_or(0),
                        content,
                    },
                };
                files.borrow_mut()[idx].lines.push(diff_line);
            }
            true
        }),
    )?;

    Ok(DiffResult {
        files: files.into_inner(),
        from_id: from.short_id.clone(),
        to_id: to.short_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::list_commits;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    #[test]
    fn test_diff_no_changes() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"hello", "First");
        add_file_commit(&repo, "b.txt", b"world", "Second");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let result = compute_diff(&git_repo, &commits[0], &commits[0]).unwrap();
        assert!(result.files.is_empty());
    }

    #[test]
    fn test_diff_with_changes() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let result = compute_diff(&git_repo, &commits[1], &commits[0]).unwrap();
        assert!(!result.files.is_empty());

        let all_lines: String = result.files[0].lines.iter().map(|l| match l {
            DiffLine::Context { content, .. }
            | DiffLine::Added { content, .. }
            | DiffLine::Removed { content, .. } => content.clone(),
        }).collect();
        assert!(all_lines.contains("second"));
    }

    #[test]
    fn test_diff_added_file() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "b.txt", b"second", "Add b.txt");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let result = compute_diff(&git_repo, &commits[1], &commits[0]).unwrap();
        assert!(result.files.iter().any(|f| f.change.as_ref().map(|c| c.new_path()) == Some(Some("b.txt"))));
    }

    #[test]
    fn test_diff_line_kinds() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"line1\nline2\nline3\n", "First");
        add_file_commit(&repo, "a.txt", b"line1\nmodified\nline3\n", "Modify");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let result = compute_diff(&git_repo, &commits[1], &commits[0]).unwrap();
        let file = &result.files[0];

        let has_removed = file.lines.iter().any(|l| matches!(l, DiffLine::Removed { .. }));
        let has_added = file.lines.iter().any(|l| matches!(l, DiffLine::Added { .. }));
        let has_context = file.lines.iter().any(|l| matches!(l, DiffLine::Context { .. }));
        assert!(has_removed);
        assert!(has_added);
        assert!(has_context);
    }
}
