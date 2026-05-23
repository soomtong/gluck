use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "glc", about = "Terminal git history file viewer")]
pub struct Cli {
    /// Git repository path (TUI 모드)
    pub path: Option<String>,

    /// Log level (trace|debug|info|warn|error)
    #[arg(long, default_value = "warn", global = true)]
    pub log_level: String,

    /// Enable debug overlay
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build search index for semantic search
    Index {
        /// Repository path (default: current directory)
        #[arg(default_value = ".")]
        repo_path: PathBuf,

        /// Batch size for embedding generation
        #[arg(long, default_value = "32")]
        batch_size: usize,

        /// Max file size to index in bytes
        #[arg(long, default_value = "1048576")]
        max_file_size: usize,

        /// Force rebuild even if index is current
        #[arg(long)]
        force: bool,
    },
}
