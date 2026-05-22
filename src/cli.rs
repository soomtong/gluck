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
