use crate::git::repo::{GitError, GitRepo};
use crate::git::commit::CommitInfo;
use git2::ObjectType;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
}

pub fn list_tree(repo: &GitRepo, commit: &CommitInfo) -> Result<Vec<FileEntry>, GitError> {
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
) -> Result<(), GitError> {
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

pub fn is_binary_blob(repo: &GitRepo, commit: &CommitInfo, path: &str) -> Result<bool, GitError> {
    let repository = repo.repository();
    let git_commit = repository.find_commit(commit.id)?;
    let tree = git_commit.tree()?;
    let entry = tree.get_path(std::path::Path::new(path))?;
    let obj = entry.to_object(repository)?;
    let blob = obj.as_blob()
        .ok_or_else(|| GitError::BlobReadFailed(format!("Not a blob: {}", path)))?;
    Ok(blob.is_binary())
}

pub fn read_blob(repo: &GitRepo, commit: &CommitInfo, path: &str) -> Result<String, GitError> {
    let repository = repo.repository();
    let git_commit = repository.find_commit(commit.id)?;
    let tree = git_commit.tree()?;
    let entry = tree.get_path(std::path::Path::new(path))?;
    let obj = entry.to_object(repository)?;
    let blob = obj.as_blob()
        .ok_or_else(|| GitError::BlobReadFailed(format!("Not a blob: {}", path)))?;
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

        let src_dir = entries.iter().find(|e| e.path == "src").unwrap();
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