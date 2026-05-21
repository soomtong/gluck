pub mod commit;
pub mod diff;
pub mod repo;
pub mod tree;

pub use commit::CommitInfo;
pub use diff::DiffResult;
pub use repo::{GitError, GitRepo};
pub use tree::FileEntry;