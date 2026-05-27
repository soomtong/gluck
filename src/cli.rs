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

    /// Generate search quality + performance report
    Report {
        /// Fixture TOML path
        #[arg(long, default_value = "tests/fixtures/search_queries.toml")]
        fixtures: String,

        /// Markdown output path (stdout is always shown)
        #[arg(long)]
        out: Option<String>,

        /// Warmup iterations per query
        #[arg(long, default_value = "3")]
        warmup: usize,

        /// Measurement iterations per query
        #[arg(long, default_value = "10")]
        iters: usize,

        /// top-k for search() (k=10 covers NDCG@10/Recall@10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Diagnose a single query: dump BM25 tokens, raw BM25/Vector/RRF rankings
    Diagnose {
        /// Query text to analyze (e.g. "검색 인덱스 빌드")
        query: String,

        /// Number of top hits per stage
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}
