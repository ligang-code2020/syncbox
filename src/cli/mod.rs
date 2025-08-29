use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "syncbox")]
#[command(about = "Sync files between directories", long_about = None)]
pub struct Args {
    /// Subcommand
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Sync a directory to another location
    Sync {
        /// Source directory
        source: PathBuf,

        /// Target directory
        target: PathBuf,

        /// Perform a dry run without making changes
        #[arg(long)]
        dry_run: bool,
    },
    Run {
        /// Name of the task to run (from config)
        name: String,

        /// Config file path (optional, default: ./syncbox.toml)
        #[arg(long, default_value = "syncbox.toml")]
        config: PathBuf,

        /// Perform a dry run
        #[arg(long)]
        dry_run: bool,
    },

    Watch {
        /// Name of the task to watch
        name: String,

        /// Config file path (default: syncbox.toml)
        #[arg(long, default_value = "syncbox.toml")]
        config: PathBuf,

        /// Watch delay in milliseconds (default: 500ms)
        #[arg(long, default_value = "500")]
        delay: u64,
    },
}
