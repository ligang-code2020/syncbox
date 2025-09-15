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
        /// 源目录路径
        source: PathBuf,
        /// 目标目录路径
        target: PathBuf,
        /// 试运行模式（不实际写入）
        #[arg(long)]
        dry_run: bool,
        /// 使用校验和比较文件内容（而非仅修改时间/大小）
        #[arg(long)]
        checksum: bool,
        /// 删除目标目录中源目录不存在的文件
        #[arg(long)]
        delete: bool,
        /// 排除同步的文件或目录（支持通配符，可多次指定）
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,
        /// 排除删除的文件或目录（即使 delete=true 也不会删这些，可多次指定）
        #[arg(long, value_name = "PATTERN", alias = "delete-ignore")]
        delete_exclude: Vec<String>,
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

        #[arg(long)]
        checksum: bool,
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

        #[arg(long)]
        checksum: bool,
    },
}
