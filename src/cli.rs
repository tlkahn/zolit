use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "zolit", version, about = "Export Zotero annotations into Lit-flavored markdown")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to Zotero SQLite database
    #[arg(long, global = true, env = "ZOLIT_DB")]
    pub db: Option<PathBuf>,

    /// Directory containing companion markdown files
    #[arg(long, global = true, env = "ZOLIT_MD_DIR")]
    pub md_dir: Option<PathBuf>,

    /// Output directory (copy files here instead of modifying in-place)
    #[arg(long, global = true)]
    pub output_dir: Option<PathBuf>,

    /// Fuzzy match threshold (0.0–1.0)
    #[arg(long, global = true, default_value = "0.4")]
    pub threshold: f64,

    /// Filter markdown files by glob pattern
    #[arg(long, global = true)]
    pub filter: Option<String>,

    /// Show what would change without writing files
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Use LLM fallback for unmatched annotations (requires 'llm' feature)
    #[cfg(feature = "llm")]
    #[arg(long, global = true)]
    pub llm_fallback: bool,

    /// LLM model name (e.g. "gpt-4o-mini")
    #[cfg(feature = "llm")]
    #[arg(long, global = true, default_value = "gpt-4o-mini")]
    pub llm_model: String,

    /// LLM API base URL
    #[cfg(feature = "llm")]
    #[arg(long, global = true, default_value = "https://api.openai.com/v1", env = "LLM_BASE_URL")]
    pub llm_base_url: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Sync Zotero annotations into companion markdown files
    Sync,

    /// List annotated PDFs found in the Zotero database
    List,

    /// Show sync status: matched/unmatched/pending counts
    Status,
}
