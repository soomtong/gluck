use clap::{Parser, Subcommand};

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

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build semantic search index for this repository
    Index {
        /// Force full reindex even if index is current
        #[arg(long)]
        force: bool,

        /// Number of documents per embedding batch
        #[arg(long, default_value = "64")]
        batch_size: usize,

        /// Maximum file size to index in bytes
        #[arg(long, default_value = "1000000")]
        max_file_bytes: usize,
    },
}
