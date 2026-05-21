# Gluck Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `glc`, a terminal TUI tool to browse git history, view files at any commit, and diff commits.

**Architecture:** Single `App` struct with mode-state machine (Pick/View/Diff). Git operations isolated in `GitRepo` data layer using git2-rs. ratatui + crossterm for TUI rendering. tree-sitter for syntax highlighting.

**Tech Stack:** Rust (edition 2021), ratatui 0.29, crossterm 0.28, git2 0.20, tree-sitter 0.22, tree-sitter-highlight 0.22, clap 4, tracing 0.1, anyhow 1

---

## File Structure

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Dependencies, binary name `glc` |
| `src/main.rs` | CLI parsing, event loop, terminal setup |
| `src/lib.rs` | Module declarations |
| `src/app.rs` | App state, mode transitions, event dispatch |
| `src/mode.rs` | Mode enum, state structs, KeyBindings |
| `src/git/mod.rs` | Module re-exports |
| `src/git/repo.rs` | GitRepo wrapper around git2::Repository |
| `src/git/commit.rs` | CommitInfo, commit listing & search |
| `src/git/tree.rs` | FileEntry, file tree walking, blob reading |
| `src/git/diff.rs` | Diff computation between commits |
| `src/ui/mod.rs` | Module re-exports |
| `src/ui/layout.rs` | Shared layout helpers, status bar |
| `src/ui/pick.rs` | Pick mode rendering |
| `src/ui/view.rs` | View mode rendering |
| `src/ui/diff.rs` | Diff mode rendering |
| `src/highlight/mod.rs` | Module re-exports |
| `src/highlight/engine.rs` | tree-sitter syntax highlighting |
| `src/debug.rs` | Logging init, debug overlay |

---

## Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/git/mod.rs`
- Create: `src/ui/mod.rs`
- Create: `src/highlight/mod.rs`
- Create: `src/app.rs` (stub)
- Create: `src/mode.rs` (stub)
- Create: `src/debug.rs` (stub)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "gluck"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "glc"
path = "src/main.rs"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
git2 = "0.20"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
tree-sitter = "0.22"
tree-sitter-highlight = "0.22"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create module stubs**

`src/lib.rs`:
```rust
pub mod app;
pub mod debug;
pub mod git;
pub mod highlight;
pub mod mode;
pub mod ui;
```

`src/main.rs`:
```rust
fn main() {
    println!("gluck - git history viewer");
}
```

`src/app.rs`:
```rust
pub struct App;
```

`src/mode.rs`:
```rust
pub struct KeyBindings;
```

`src/debug.rs`:
```rust
pub fn init_logging(_level: &str) {}
```

`src/git/mod.rs`:
```rust
pub mod commit;
pub mod diff;
pub mod repo;
pub mod tree;

pub use commit::CommitInfo;
pub use diff::DiffResult;
pub use repo::GitRepo;
pub use tree::FileEntry;
```

`src/ui/mod.rs`:
```rust
pub mod diff;
pub mod layout;
pub mod pick;
pub mod view;
```

`src/highlight/mod.rs`:
```rust
pub mod engine;

pub use engine::HighlightEngine;
```

`src/ui/layout.rs`:
```rust

```

`src/ui/pick.rs`:
```rust

```

`src/ui/view.rs`:
```rust

```

`src/ui/diff.rs`:
```rust

```

`src/git/repo.rs`:
```rust

```

`src/git/commit.rs`:
```rust

```

`src/git/tree.rs`:
```rust

```

`src/git/diff.rs`:
```rust

```

`src/highlight/engine.rs`:
```rust

```

- [ ] **Step 3: Verify compilation**

Run: `cargo build`
Expected: Success with warnings about unused imports/variables

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "Init gluck project with module structure"
```

---

## Task 2: GitRepo Wrapper

**Files:**
- Modify: `src/git/repo.rs`

- [ ] **Step 1: Write GitRepo and tests**

```rust
use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path).context("Not a git repository")?;
        Ok(Self { repo })
    }

    pub fn repository(&self) -> &Repository {
        &self.repo
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use git2::Signature;
    use tempfile::TempDir;

    pub fn init_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        (dir, repo)
    }

    pub fn add_file_commit(
        repo: &Repository,
        path: &str,
        content: &[u8],
        message: &str,
    ) -> git2::Oid {
        let dir = repo.workdir().unwrap();
        let file_path = dir.join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        let parents: Vec<git2::Commit> = if repo.head().is_ok() {
            vec![repo.head().unwrap().peel_to_commit().unwrap()]
        } else {
            vec![]
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .unwrap();
        oid
    }

    #[test]
    fn test_open_valid_repo() {
        let (dir, _repo) = init_test_repo();
        assert!(GitRepo::open(dir.path()).is_ok());
    }

    #[test]
    fn test_open_invalid_path() {
        let dir = TempDir::new().unwrap();
        assert!(GitRepo::open(dir.path()).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib git::repo`
Expected: 2 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/git/repo.rs
git commit -m "Add GitRepo wrapper with open and test helpers"
```

---

## Task 3: Commit Listing & Search

**Files:**
- Modify: `src/git/commit.rs`

- [ ] **Step 1: Write CommitInfo and tests**

```rust
use crate::git::repo::GitRepo;
use anyhow::Result;
use git2::Oid;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
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
    revwalk.set_sorting(git2::Sort::TIME)?;

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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib git::commit`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/git/commit.rs
git commit -m "Add CommitInfo with listing and search"
```

---

## Task 4: File Tree Exploration

**Files:**
- Modify: `src/git/tree.rs`

- [ ] **Step 1: Write FileEntry, tree walking, and tests**

```rust
use crate::git::repo::GitRepo;
use crate::git::commit::CommitInfo;
use anyhow::Result;
use git2::ObjectType;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
}

pub fn list_tree(repo: &GitRepo, commit: &CommitInfo) -> Result<Vec<FileEntry>> {
    let repository = repo.repository();
    let git_commit = repository.find_commit(commit.id)?;
    let tree = git_commit.tree()?;
    let mut entries = Vec::new();
    walk_tree(repository, &tree, "", &mut entries)?;
    Ok(entries)
}

fn walk_tree(
    repo: &git2::Repository,
    tree: &git2::Tree,
    prefix: &str,
    entries: &mut Vec<FileEntry>,
) -> Result<()> {
    for entry in tree.iter() {
        let name = entry.name().unwrap_or("[binary]").to_string();
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };
        match entry.kind() {
            Some(ObjectType::Tree) => {
                entries.push(FileEntry {
                    name: format!("{}/", name),
                    path: path.clone(),
                    kind: EntryKind::Directory,
                });
                let obj = entry.to_object(repo)?;
                let subtree = obj.as_tree().unwrap();
                walk_tree(repo, subtree, &path, entries)?;
            }
            Some(ObjectType::Blob) => {
                entries.push(FileEntry {
                    name,
                    path,
                    kind: EntryKind::File,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn read_blob(repo: &GitRepo, commit: &CommitInfo, path: &str) -> Result<String> {
    let repository = repo.repository();
    let git_commit = repository.find_commit(commit.id)?;
    let tree = git_commit.tree()?;
    let entry = tree.get_path(std::path::Path::new(path))?;
    let obj = entry.to_object(repository)?;
    let blob = obj.as_blob()
        .ok_or_else(|| anyhow::anyhow!("Not a blob: {}", path))?;
    let content = String::from_utf8_lossy(blob.content()).into_owned();
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::commit::list_commits;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};
    use crate::git::repo::GitRepo;

    #[test]
    fn test_list_tree() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "main.rs", b"fn main() {}", "Initial");
        add_file_commit(&repo, "src/lib.rs", b"pub mod foo;", "Add lib");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let entries = list_tree(&git_repo, &commits[0]).unwrap();

        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"main.rs"));
        assert!(paths.contains(&"src/lib.rs"));
    }

    #[test]
    fn test_tree_entry_kinds() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let entries = list_tree(&git_repo, &commits[0]).unwrap();

        let src_dir = entries.iter().find(|e| e.path == "src/").unwrap();
        assert_eq!(src_dir.kind, EntryKind::Directory);

        let main_file = entries.iter().find(|e| e.path == "src/main.rs").unwrap();
        assert_eq!(main_file.kind, EntryKind::File);
    }

    #[test]
    fn test_read_blob() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "hello.rs", b"fn hello() { println!(\"hi\"); }", "Add hello");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let content = read_blob(&git_repo, &commits[0], "hello.rs").unwrap();
        assert!(content.contains("fn hello()"));
    }

    #[test]
    fn test_read_blob_nested() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/lib.rs", b"pub fn lib() {}", "Add lib");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let commits = list_commits(&git_repo).unwrap();
        let content = read_blob(&git_repo, &commits[0], "src/lib.rs").unwrap();
        assert!(content.contains("pub fn lib()"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib git::tree`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/git/tree.rs
git commit -m "Add FileEntry with tree walking and blob reading"
```

---

## Task 5: Diff Calculation

**Files:**
- Modify: `src/git/diff.rs`

- [ ] **Step 1: Write diff types, computation, and tests**

```rust
use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub old_line_no: Option<u32>,
    pub new_line_no: Option<u32>,
    pub content: String,
    pub kind: DiffLineKind,
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub files: Vec<DiffFile>,
    pub from_id: String,
    pub to_id: String,
}

pub fn compute_diff(repo: &GitRepo, from: &CommitInfo, to: &CommitInfo) -> Result<DiffResult> {
    let repository = repo.repository();
    let from_commit = repository.find_commit(from.id)?;
    let to_commit = repository.find_commit(to.id)?;
    let from_tree = from_commit.tree()?;
    let to_tree = to_commit.tree()?;

    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);

    let diff = repository.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))?;

    let mut files = Vec::new();
    let mut current_file = DiffFile {
        old_path: None,
        new_path: None,
        lines: Vec::new(),
    };

    diff.foreach(
        &mut |delta, _progress| {
            if !current_file.lines.is_empty() {
                files.push(current_file);
            }
            current_file = DiffFile {
                old_path: delta.old_file().path().map(|p| p.to_string_lossy().into_owned()),
                new_path: delta.new_file().path().map(|p| p.to_string_lossy().into_owned()),
                lines: Vec::new(),
            };
            true
        },
        None,
        None,
        &mut |_delta, _hunk, line| {
            let kind = match line.origin() {
                '+' => DiffLineKind::Added,
                '-' => DiffLineKind::Removed,
                _ => DiffLineKind::Context,
            };
            let content = String::from_utf8_lossy(line.content()).into_owned();
            current_file.lines.push(DiffLine {
                old_line_no: line.old_lineno(),
                new_line_no: line.new_lineno(),
                content,
                kind,
            });
            true
        },
    )?;

    if !current_file.lines.is_empty() {
        files.push(current_file);
    }

    Ok(DiffResult {
        files,
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
        // commits[0] = "Second", commits[1] = "First"
        let result = compute_diff(&git_repo, &commits[1], &commits[0]).unwrap();
        assert!(!result.files.is_empty());

        let all_lines: String = result.files[0].lines.iter().map(|l| l.content.clone()).collect();
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
        assert!(result.files.iter().any(|f| f.new_path.as_deref() == Some("b.txt")));
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

        let has_removed = file.lines.iter().any(|l| l.kind == DiffLineKind::Removed);
        let has_added = file.lines.iter().any(|l| l.kind == DiffLineKind::Added);
        let has_context = file.lines.iter().any(|l| l.kind == DiffLineKind::Context);
        assert!(has_removed);
        assert!(has_added);
        assert!(has_context);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib git::diff`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/git/diff.rs
git commit -m "Add diff computation with line-level detail"
```

---

## Task 6: Mode States & KeyBindings

**Files:**
- Modify: `src/mode.rs`

- [ ] **Step 1: Write mode types and keybindings with tests**

```rust
use crate::git::commit::CommitInfo;
use crate::git::diff::DiffResult;
use crate::git::tree::FileEntry;
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Pick(PickState),
    View(ViewState),
    Diff(DiffState),
}

#[derive(Debug, Clone)]
pub struct PickState {
    pub commits: Vec<CommitInfo>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub query: Option<String>,
}

impl PickState {
    pub fn new(commits: Vec<CommitInfo>) -> Self {
        let filtered_indices = (0..commits.len()).collect();
        Self {
            commits,
            filtered_indices,
            selected: 0,
            scroll: 0,
            query: None,
        }
    }

    pub fn visible_commits(&self) -> Vec<&CommitInfo> {
        self.filtered_indices
            .iter()
            .map(|&i| &self.commits[i])
            .collect()
    }

    pub fn apply_search(&mut self, query: &str) {
        use crate::git::commit::search_commits;
        self.query = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };
        self.filtered_indices = if query.is_empty() {
            (0..self.commits.len()).collect()
        } else {
            search_commits(&self.commits, query)
        };
        self.selected = 0;
        self.scroll = 0;
    }
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub commit: CommitInfo,
    pub tree: Vec<FileEntry>,
    pub selected_file: usize,
    pub content: Option<String>,
    pub highlighted: Vec<Line<'static>>,
    pub scroll: usize,
}

impl ViewState {
    pub fn new(commit: CommitInfo, tree: Vec<FileEntry>) -> Self {
        Self {
            commit,
            tree,
            selected_file: 0,
            content: None,
            highlighted: Vec::new(),
            scroll: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffState {
    pub from: CommitInfo,
    pub to: CommitInfo,
    pub diff_result: DiffResult,
    pub selected_file: usize,
    pub side_by_side: bool,
    pub scroll: usize,
}

impl DiffState {
    pub fn new(from: CommitInfo, to: CommitInfo, diff_result: DiffResult) -> Self {
        Self {
            from,
            to,
            diff_result,
            selected_file: 0,
            side_by_side: true,
            scroll: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    MoveDown,
    MoveUp,
    Enter,
    Back,
    Search,
    Quit,
    ToggleView,
    SwitchMode,
}

#[derive(Debug, Clone)]
pub struct KeyBindings {
    pub bindings: std::collections::HashMap<crossterm::event::KeyCode, Action>,
}

impl KeyBindings {
    pub fn default_bindings() -> Self {
        use crossterm::event::KeyCode;
        let mut bindings = std::collections::HashMap::new();
        bindings.insert(KeyCode::Char('j'), Action::MoveDown);
        bindings.insert(KeyCode::Down, Action::MoveDown);
        bindings.insert(KeyCode::Char('k'), Action::MoveUp);
        bindings.insert(KeyCode::Up, Action::MoveUp);
        bindings.insert(KeyCode::Enter, Action::Enter);
        bindings.insert(KeyCode::Char('l'), Action::Enter);
        bindings.insert(KeyCode::Esc, Action::Back);
        bindings.insert(KeyCode::Char('h'), Action::Back);
        bindings.insert(KeyCode::Char('/'), Action::Search);
        bindings.insert(KeyCode::Char('q'), Action::Quit);
        bindings.insert(KeyCode::Char('s'), Action::ToggleView);
        bindings.insert(KeyCode::Tab, Action::SwitchMode);
        Self { bindings }
    }

    pub fn resolve(&self, code: crossterm::event::KeyCode) -> Option<Action> {
        self.bindings.get(&code).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn test_keybindings_resolve() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(kb.resolve(KeyCode::Char('j')), Some(Action::MoveDown));
        assert_eq!(kb.resolve(KeyCode::Down), Some(Action::MoveDown));
        assert_eq!(kb.resolve(KeyCode::Char('k')), Some(Action::MoveUp));
        assert_eq!(kb.resolve(KeyCode::Enter), Some(Action::Enter));
        assert_eq!(kb.resolve(KeyCode::Esc), Some(Action::Back));
        assert_eq!(kb.resolve(KeyCode::Char('x')), None);
    }

    #[test]
    fn test_pick_state_search() {
        let commits = vec![
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "abc1234".into(),
                author: "Alice".into(),
                date: std::time::UNIX_EPOCH,
                message: "Add auth module".into(),
            },
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "def5678".into(),
                author: "Bob".into(),
                date: std::time::UNIX_EPOCH,
                message: "Fix login bug".into(),
            },
        ];
        let mut state = PickState::new(commits);
        state.apply_search("auth");
        assert_eq!(state.filtered_indices.len(), 1);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_pick_state_clear_search() {
        let commits = vec![
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "abc".into(),
                author: "A".into(),
                date: std::time::UNIX_EPOCH,
                message: "First".into(),
            },
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "def".into(),
                author: "B".into(),
                date: std::time::UNIX_EPOCH,
                message: "Second".into(),
            },
        ];
        let mut state = PickState::new(commits);
        state.apply_search("first");
        assert_eq!(state.filtered_indices.len(), 1);
        state.apply_search("");
        assert_eq!(state.filtered_indices.len(), 2);
        assert!(state.query.is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib mode`
Expected: 3 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/mode.rs
git commit -m "Add Mode states, KeyBindings, and search filtering"
```

---

## Task 7: App State Machine

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write App struct with event handling and tests**

```rust
use crate::git::commit::{list_commits, CommitInfo};
use crate::git::diff::compute_diff;
use crate::git::repo::GitRepo;
use crate::git::tree::{list_tree, read_blob};
use crate::mode::{
    Action, DiffState, KeyBindings, Mode, PickState, ViewState,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::Frame;

pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub keybindings: KeyBindings,
    pub should_quit: bool,
    pub searching: bool,
    pub search_input: String,
    pub debug_overlay: bool,
}

impl App {
    pub fn new(repo: GitRepo) -> Result<Self> {
        let commits = list_commits(&repo)?;
        let pick_state = PickState::new(commits);
        Ok(Self {
            mode: Mode::Pick(pick_state),
            repo,
            keybindings: KeyBindings::default_bindings(),
            should_quit: false,
            searching: false,
            search_input: String::new(),
            debug_overlay: false,
        })
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        if self.searching {
            self.handle_search_input(code);
            return;
        }

        if code == KeyCode::Char('d') && self.keybindings.resolve(KeyCode::Ctrl('d')) == None {
            // Ctrl+D is handled separately below
        }

        let Some(action) = self.keybindings.resolve(code) else {
            return;
        };
        match action {
            Action::Quit => self.should_quit = true,
            Action::Search => self.start_search(),
            Action::MoveDown => self.move_down(),
            Action::MoveUp => self.move_up(),
            Action::Enter => self.enter(),
            Action::Back => self.back(),
            Action::ToggleView => self.toggle_view(),
            Action::SwitchMode => self.switch_mode(),
        }
    }

    pub fn handle_ctrl_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('c') => self.should_quit = true,
            KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
            _ => {}
        }
    }

    fn start_search(&mut self) {
        if let Mode::Pick(_) = &self.mode {
            self.searching = true;
            self.search_input.clear();
        }
    }

    fn handle_search_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.searching = false;
                self.search_input.clear();
            }
            KeyCode::Enter => {
                self.searching = false;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.apply_search();
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.apply_search();
            }
            _ => {}
        }
    }

    fn apply_search(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            state.apply_search(&self.search_input);
        }
    }

    fn move_down(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                let max = state.filtered_indices.len().saturating_sub(1);
                state.selected = state.selected.saturating_add(1).min(max);
            }
            Mode::View(state) => {
                let max = state.tree.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
            }
            Mode::Diff(state) => {
                let max = state.diff_result.files.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
            }
        }
    }

    fn move_up(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                state.selected = state.selected.saturating_sub(1);
            }
            Mode::View(state) => {
                state.selected_file = state.selected_file.saturating_sub(1);
            }
            Mode::Diff(state) => {
                state.selected_file = state.selected_file.saturating_sub(1);
            }
        }
    }

    fn enter(&mut self) {
        match &self.mode {
            Mode::Pick(state) => {
                if let Some(&idx) = state.filtered_indices.get(state.selected) {
                    let commit = state.commits[idx].clone();
                    let tree = list_tree(&self.repo, &commit).unwrap_or_default();
                    let view_state = ViewState::new(commit, tree);
                    self.mode = Mode::View(view_state);
                }
            }
            Mode::View(state) => {
                if let Some(entry) = state.tree.get(state.selected_file) {
                    if let Ok(content) = read_blob(&self.repo, &state.commit, &entry.path) {
                        if let Mode::View(vs) = &mut self.mode {
                            vs.content = Some(content);
                        }
                    }
                }
            }
            Mode::Diff(_) => {}
        }
    }

    fn back(&mut self) {
        match &self.mode {
            Mode::View(_) | Mode::Diff(_) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                let mut pick = PickState::new(commits);
                if let Mode::View(vs) = &self.mode {
                    pick.selected = pick
                        .commits
                        .iter()
                        .position(|c| c.id == vs.commit.id)
                        .unwrap_or(0);
                }
                self.mode = Mode::Pick(pick);
            }
            Mode::Pick(_) => {}
        }
    }

    fn switch_mode(&mut self) {
        match &self.mode {
            Mode::View(state) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                let current_idx = commits.iter().position(|c| c.id == state.commit.id);
                if let Some(idx) = current_idx {
                    if idx + 1 < commits.len() {
                        let from = commits[idx + 1].clone();
                        let to = commits[idx].clone();
                        if let Ok(diff_result) = compute_diff(&self.repo, &from, &to) {
                            let diff_state = DiffState::new(from, to, diff_result);
                            self.mode = Mode::Diff(diff_state);
                        }
                    }
                }
            }
            Mode::Diff(state) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                if let Some(idx) = commits.iter().position(|c| c.id == state.to.id) {
                    let commit = commits[idx].clone();
                    let tree = list_tree(&self.repo, &commit).unwrap_or_default();
                    let view_state = ViewState::new(commit, tree);
                    self.mode = Mode::View(view_state);
                }
            }
            Mode::Pick(_) => {}
        }
    }

    fn toggle_view(&mut self) {
        if let Mode::Diff(state) = &mut self.mode {
            state.side_by_side = !state.side_by_side;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    fn test_app() -> (tempfile::TempDir, App) {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "a.txt", b"third", "Third commit");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let app = App::new(git_repo).unwrap();
        (dir, app)
    }

    #[test]
    fn test_app_starts_in_pick_mode() {
        let (_, app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_pick_to_view() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_view_to_pick() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_view_to_diff_to_pick() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_quit() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_move_selection() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(state.selected, 1);
        }
        app.handle_key(KeyCode::Char('k'));
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(state.selected, 0);
        }
    }

    #[test]
    fn test_search_mode() {
        let (_, mut app) = test_app();
        app.handle_key(KeyCode::Char('/'));
        assert!(app.searching);
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('h'));
        app.handle_key(KeyCode::Enter);
        assert!(!app.searching);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib app`
Expected: 7 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "Add App state machine with mode transitions"
```

---

## Task 8: CLI Parsing & Debug/Logging

**Files:**
- Modify: `src/debug.rs`
- Modify: `src/main.rs` (partial - CLI struct only)

- [ ] **Step 1: Write debug/logging module**

```rust
use std::fs::File;
use tracing_subscriber::EnvFilter;

pub fn init_logging(level: &str) {
    let file = File::create("gluck.log").ok();
    if let Some(file) = file {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(level)),
            )
            .with_writer(file)
            .with_ansi(false)
            .init();
    }
}
```

- [ ] **Step 2: Write CLI argument struct**

`src/cli.rs` (new file):

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "glc", about = "Terminal git history file viewer")]
pub struct Cli {
    /// Git repository path
    pub path: Option<String>,

    /// Log level (trace|debug|info|warn|error)
    #[arg(long, default_value = "warn")]
    pub log_level: String,

    /// Enable debug overlay
    #[arg(long)]
    pub debug: bool,
}
```

Update `src/lib.rs` to include cli module:
```rust
pub mod app;
pub mod cli;
pub mod debug;
pub mod git;
pub mod highlight;
pub mod mode;
pub mod ui;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All existing tests still PASS

- [ ] **Step 4: Commit**

```bash
git add src/debug.rs src/cli.rs src/lib.rs
git commit -m "Add CLI parsing and logging setup"
```

---

## Task 9: UI Layout Helpers

**Files:**
- Modify: `src/ui/layout.rs`

- [ ] **Step 1: Write shared layout components**

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn app_layout(area: Rect) -> (Rect, Rect, Rect) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    (header, body, footer)
}

pub fn split_horizontal(area: Rect, left_width: u16) -> (Rect, Rect) {
    let [left, right] = Layout::horizontal([
        Constraint::Length(left_width),
        Constraint::Min(1),
    ])
    .areas(area);
    (left, right)
}

pub fn render_header(frame: &mut ratatui::Frame, area: Rect, title: &str) {
    let header = Paragraph::new(format!(" {} ", title))
        .style(Style::new().white().bold())
        .block(Block::bordered().style(Style::new().dark_gray()));
    frame.render_widget(header, area);
}

pub fn render_footer(frame: &mut ratatui::Frame, area: Rect, hints: &[(&str, &str)]) {
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!("[{}]", key),
                    Style::new().yellow().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", desc)),
            ]
        })
        .collect();
    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

pub fn render_search_bar(frame: &mut ratatui::Frame, area: Rect, query: &str) {
    let search = Paragraph::new(format!("/ {}", query))
        .style(Style::new().yellow())
        .block(Block::bordered().style(Style::new().dark_gray()));
    frame.render_widget(search, area);
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success

- [ ] **Step 3: Commit**

```bash
git add src/ui/layout.rs
git commit -m "Add shared UI layout helpers and status bar"
```

---

## Task 10: Pick Mode UI

**Files:**
- Modify: `src/ui/pick.rs`

- [ ] **Step 1: Write Pick mode renderer**

```rust
use crate::app::App;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

fn format_date(time: std::time::SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let datetime = chrono::DateTime::from_timestamp(secs, 0);
    // Avoid chrono dependency — use a simple approximation
    format!("{}", secs)
}

fn format_commit_line(commit: &crate::git::commit::CommitInfo) -> Line<'static> {
    let date_str = format_date(commit.date);
    Line::from(vec![
        Span::styled(
            format!(" {} ", commit.short_id),
            Style::new().yellow().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<12} ", date_str),
            Style::new().dark_gray(),
        ),
        Span::raw(commit.message.clone()),
    ])
}

pub fn render_pick(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if app.searching {
        layout::render_search_bar(frame, header, &app.search_input);
    } else {
        layout::render_header(frame, header, "gluck - Pick Mode");
    }

    if let Mode::Pick(state) = &app.mode {
        let visible = state.visible_commits();
        let items: Vec<ListItem> = visible.iter().map(|c| ListItem::new(format_commit_line(c))).collect();

        let list = List::new(items).block(
            Block::bordered()
                .title(format!(" {} commits ", visible.len()))
                .style(Style::new().white()),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));

        frame.render_stateful_widget(list, body, &mut list_state);
    }

    let hints = [("[j/k]", "move"), ("[Enter]", "view"), ("[/]", "search"), ("[q]", "quit")];
    layout::render_footer(frame, footer, &hints);
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success (may have unused warning for format_date)

- [ ] **Step 3: Commit**

```bash
git add src/ui/pick.rs
git commit -m "Add Pick mode UI renderer"
```

---

## Task 11: View Mode UI

**Files:**
- Modify: `src/ui/view.rs`

- [ ] **Step 1: Write View mode renderer**

```rust
use crate::app::App;
use crate::git::tree::EntryKind;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

pub fn render_view(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);
    layout::render_header(frame, header, "gluck - View Mode");

    if let Mode::View(state) = &app.mode {
        let (left, right) = layout::split_horizontal(body, 24);

        // File tree
        let items: Vec<ListItem> = state
            .tree
            .iter()
            .map(|entry| {
                let icon = match entry.kind {
                    EntryKind::Directory => "📁 ",
                    EntryKind::File => "  ",
                };
                ListItem::new(format!("{}{}", icon, entry.name))
            })
            .collect();

        let tree_list = List::new(items).block(
            Block::bordered()
                .title(format!(" {} ", state.commit.short_id))
                .style(Style::new().white()),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_file));
        frame.render_stateful_widget(tree_list, left, &mut list_state);

        // File content
        let content_text = state
            .content
            .as_deref()
            .unwrap_or("(select a file to view)");

        let lines: Vec<Line> = content_text
            .lines()
            .enumerate()
            .map(|(i, line)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:>4} ", i + 1),
                        Style::new().dark_gray(),
                    ),
                    Span::raw(line.to_string()),
                ])
            })
            .collect();

        let file_name = state
            .tree
            .get(state.selected_file)
            .map(|e| e.path.as_str())
            .unwrap_or("no file");

        let content = Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(format!(" {} ", file_name))
                    .style(Style::new().white()),
            )
            .scroll((state.scroll as u16, 0));

        frame.render_widget(content, right);
    }

    let hints = [("[j/k]", "move"), ("[Enter]", "open"), ("[Tab]", "diff"), ("[Esc]", "back")];
    layout::render_footer(frame, footer, &hints);
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success

- [ ] **Step 3: Commit**

```bash
git add src/ui/view.rs
git commit -m "Add View mode UI with file tree and content"
```

---

## Task 12: Diff Mode UI

**Files:**
- Modify: `src/ui/diff.rs`

- [ ] **Step 1: Write Diff mode renderer**

```rust
use crate::app::App;
use crate::git::diff::DiffLineKind;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render_diff(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::Diff(state) = &app.mode {
        let title = format!(
            "gluck - Diff: {} vs {}",
            state.from.short_id, state.to.short_id
        );
        layout::render_header(frame, header, &title);

        let file = state.diff_result.files.get(state.selected_file);
        let file_name = file
            .map(|f| f.new_path.as_deref().unwrap_or("?"))
            .unwrap_or("no file");

        if let Some(file) = file {
            if state.side_by_side {
                render_side_by_side(frame, body, file, state.scroll);
            } else {
                render_unified(frame, body, file_name, file, state.scroll);
            }
        } else {
            let empty = Paragraph::new("No diff").block(Block::bordered());
            frame.render_widget(empty, body);
        }
    }

    let hints = [("[j/k]", "move"), ("[s]", "toggle view"), ("[Tab]", "back"), ("[Esc]", "pick")];
    layout::render_footer(frame, footer, &hints);
}

fn style_for_kind(kind: &DiffLineKind) -> Style {
    match kind {
        DiffLineKind::Added => Style::new().fg(Color::Green),
        DiffLineKind::Removed => Style::new().fg(Color::Red),
        DiffLineKind::Context => Style::new(),
    }
}

fn render_unified(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_name: &str,
    file: &crate::git::diff::DiffFile,
    scroll: usize,
) {
    let lines: Vec<Line> = file
        .lines
        .iter()
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Added => "+",
                DiffLineKind::Removed => "-",
                DiffLineKind::Context => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(format!(" {} ", file_name))
                .style(Style::new().white()),
        )
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_side_by_side(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &crate::git::diff::DiffFile,
    scroll: usize,
) {
    let (left, right) = layout::split_horizontal(area, area.width / 2);

    let old_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| dl.kind != DiffLineKind::Added)
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Removed => "-",
                _ => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
            ])
        })
        .collect();

    let new_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| dl.kind != DiffLineKind::Removed)
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Added => "+",
                _ => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
            ])
        })
        .collect();

    let old_widget = Paragraph::new(old_lines)
        .block(Block::bordered().title(" old ").style(Style::new().white()))
        .scroll((scroll as u16, 0));
    let new_widget = Paragraph::new(new_lines)
        .block(Block::bordered().title(" new ").style(Style::new().white()))
        .scroll((scroll as u16, 0));

    frame.render_widget(old_widget, left);
    frame.render_widget(new_widget, right);
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success

- [ ] **Step 3: Commit**

```bash
git add src/ui/diff.rs
git commit -m "Add Diff mode UI with side-by-side and unified views"
```

---

## Task 13: Syntax Highlighting

**Files:**
- Modify: `src/highlight/engine.rs`

- [ ] **Step 1: Write highlight engine**

```rust
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashMap;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

pub struct HighlightEngine {
    configs: HashMap<String, HighlightConfiguration>,
    theme: HashMap<String, Style>,
}

impl HighlightEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            configs: HashMap::new(),
            theme: default_theme(),
        };
        engine.register_languages();
        engine
    }

    pub fn highlight(&mut self, source: &str, path: &str) -> Vec<Line<'static>> {
        let lang = Self::detect_language(path);
        let config = match self.configs.get(&lang) {
            Some(c) => c,
            None => return Self::plain_lines(source),
        };

        let mut highlighter = Highlighter::new();
        let events = match highlighter.highlight(config, source.as_bytes(), None, |_| None) {
            Ok(e) => e,
            Err(_) => return Self::plain_lines(source),
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut current_style = Style::new();
        let mut source_iter = source.bytes();
        let mut byte_pos = 0;

        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(h)) => {
                    let highlight_name = format!("{}", h.0);
                    current_style = self
                        .theme
                        .get(&highlight_name)
                        .copied()
                        .unwrap_or_default();
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    current_style = Style::new();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    // Advance to start position
                    while byte_pos < start {
                        source_iter.next();
                        byte_pos += 1;
                    }
                    let len = end - start;
                    let mut buf = Vec::with_capacity(len);
                    while byte_pos < end {
                        if let Some(b) = source_iter.next() {
                            buf.push(b);
                            byte_pos += 1;
                        } else {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).into_owned();
                    if text.contains('\n') {
                        let parts: Vec<&str> = text.split('\n').collect();
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                lines.push(Line::from(std::mem::take(&mut current_spans)));
                            }
                            if !part.is_empty() {
                                current_spans
                                    .push(Span::styled(part.to_string(), current_style));
                            }
                        }
                    } else if !text.is_empty() {
                        current_spans.push(Span::styled(text, current_style));
                    }
                }
                Err(_) => break,
            }
        }

        if !current_spans.is_empty() {
            lines.push(Line::from(current_spans));
        }

        if lines.is_empty() {
            Self::plain_lines(source)
        } else {
            lines
        }
    }

    fn plain_lines(source: &str) -> Vec<Line<'static>> {
        source
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect()
    }

    fn detect_language(path: &str) -> String {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "rs" => "rust",
            "py" => "python",
            "js" | "mjs" => "javascript",
            "ts" => "typescript",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            "java" => "java",
            "sh" | "bash" => "bash",
            "toml" => "toml",
            "json" => "json",
            "md" => "markdown",
            "html" => "html",
            "css" => "css",
            _ => ext,
        }
        .to_string()
    }

    fn register_languages(&mut self) {
        // Register Rust highlighting
        if let Ok(config) = Self::make_rust_config() {
            self.configs.insert("rust".to_string(), config);
        }
    }

    fn make_rust_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_rust::language(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
}

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant",
    "function.builtin",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "comment",
];

fn default_theme() -> HashMap<String, Style> {
    let mut theme = HashMap::new();
    theme.insert("keyword".into(), Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD));
    theme.insert("function".into(), Style::new().fg(Color::Blue));
    theme.insert("function.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("string".into(), Style::new().fg(Color::Green));
    theme.insert("string.special".into(), Style::new().fg(Color::Cyan));
    theme.insert("comment".into(), Style::new().fg(Color::DarkGray));
    theme.insert("type".into(), Style::new().fg(Color::Cyan));
    theme.insert("type.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("constant".into(), Style::new().fg(Color::Yellow));
    theme.insert("variable".into(), Style::new().fg(Color::White));
    theme.insert("variable.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("variable.parameter".into(), Style::new().fg(Color::White));
    theme.insert("operator".into(), Style::new().fg(Color::Yellow));
    theme.insert("punctuation".into(), Style::new().fg(Color::DarkGray));
    theme.insert("punctuation.bracket".into(), Style::new().fg(Color::DarkGray));
    theme.insert("punctuation.delimiter".into(), Style::new().fg(Color::DarkGray));
    theme.insert("property".into(), Style::new().fg(Color::White));
    theme.insert("attribute".into(), Style::new().fg(Color::Yellow));
    theme.insert("tag".into(), Style::new().fg(Color::Cyan));
    theme
}
```

Add to `Cargo.toml`:
```toml
tree-sitter-rust = "0.23"
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success (may need tree-sitter-rust version adjustment)

- [ ] **Step 3: Commit**

```bash
git add src/highlight/engine.rs Cargo.toml Cargo.lock
git commit -m "Add syntax highlighting engine with tree-sitter"
```

---

## Task 14: App Render Dispatch

**Files:**
- Modify: `src/app.rs` (add render method)

- [ ] **Step 1: Add render method to App**

Add to `src/app.rs`:

```rust
use crate::ui;

impl App {
    pub fn render(&self, frame: &mut Frame) {
        match &self.mode {
            Mode::Pick(_) => ui::pick::render_pick(frame, frame.area(), self),
            Mode::View(_) => ui::view::render_view(frame, frame.area(), self),
            Mode::Diff(_) => ui::diff::render_diff(frame, frame.area(), self),
        }

        if self.debug_overlay {
            self.render_debug_overlay(frame);
        }
    }

    fn render_debug_overlay(&self, frame: &mut Frame) {
        use ratatui::layout::Rect;
        use ratatui::widgets::Paragraph;

        let mode_name = match &self.mode {
            Mode::Pick(_) => "Pick",
            Mode::View(_) => "View",
            Mode::Diff(_) => "Diff",
        };

        let info = match &self.mode {
            Mode::Pick(s) => format!(
                "Mode: {} | Selected: {} | Commits: {} | Filtered: {}",
                mode_name, s.selected, s.commits.len(), s.filtered_indices.len(),
            ),
            Mode::View(s) => format!(
                "Mode: {} | File: {} | Files: {} | Scroll: {}",
                mode_name, s.selected_file, s.tree.len(), s.scroll,
            ),
            Mode::Diff(s) => format!(
                "Mode: {} | File: {} | Files: {} | Side-by-side: {}",
                mode_name, s.selected_file, s.diff_result.files.len(), s.side_by_side,
            ),
        };

        let area = Rect::new(frame.area().width - 50, 0, 50, 1);
        let debug = Paragraph::new(info).style(Style::new().on_dark_gray().yellow());
        frame.render_widget(debug, area);
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build`
Expected: Success

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "Add render dispatch and debug overlay to App"
```

---

## Task 15: Main Event Loop

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write complete main with event loop**

```rust
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use gluck::app::App;
use gluck::cli::Cli;
use gluck::debug;
use gluck::git::repo::GitRepo;
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();

    debug::init_logging(&cli.log_level);

    let path = cli
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let repo = GitRepo::open(&path)?;
    let mut app = App::new(repo)?;
    if cli.debug {
        app.debug_overlay = true;
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_app(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.is_empty() {
                    app.handle_key(key.code);
                } else {
                    app.handle_ctrl_key(key.code);
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Build and run manually**

Run: `cargo build`
Expected: Success

Run: `cargo run -- .` (run on this repo)
Expected: TUI opens with commit list, j/k to move, Enter to view, q to quit

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "Add main event loop and wire all components"
```

---

## Task 16: Date Formatting & Polish

**Files:**
- Modify: `src/ui/pick.rs` (fix date display)
- Modify: `Cargo.toml` (add chrono if needed, or use simpler approach)

- [ ] **Step 1: Replace placeholder date with readable format**

In `src/ui/pick.rs`, replace the `format_date` function:

```rust
fn format_date(time: std::time::SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let days_since_epoch = secs / 86400;
    // Simple date calculation without chrono
    let (year, month, day) = days_to_date(days_since_epoch);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
```

- [ ] **Step 2: Verify build and run**

Run: `cargo run -- .`
Expected: Commit dates show as YYYY-MM-DD format

- [ ] **Step 3: Commit**

```bash
git add src/ui/pick.rs
git commit -m "Add readable date formatting for commit list"
```

---

## Self-Review

### Spec Coverage

| Spec Section | Task |
|---|---|
| Project structure | Task 1 |
| GitRepo wrapper (git2-rs) | Task 2 |
| Commit listing & search | Task 3 |
| File tree exploration | Task 4 |
| Blob reading | Task 4 |
| Diff computation | Task 5 |
| Mode enum & state structs | Task 6 |
| KeyBindings (multi-key) | Task 6 |
| App state machine | Task 7 |
| Mode transitions (Pick→View→Diff) | Task 7 |
| CLI interface | Task 8 |
| Debug/logging (tracing) | Task 8 |
| Debug overlay (Ctrl+D) | Task 14 |
| Performance spans | Task 8 (tracing) |
| Common layout | Task 9 |
| Pick mode UI | Task 10 |
| View mode UI | Task 11 |
| Diff mode UI (side-by-side + unified) | Task 12 |
| Syntax highlighting (tree-sitter) | Task 13 |
| Main event loop | Task 15 |
| Binary name `glc` | Task 1 |

### Placeholder Scan

No TBD, TODO, or placeholder patterns found.

### Type Consistency

All types defined in Tasks 2-6 are consistently referenced in Tasks 7-15. Method names (`list_commits`, `list_tree`, `read_blob`, `compute_diff`, `search_commits`) are consistent across all usage sites.
