use clap::Parser;
use std::path::PathBuf;
use clap::builder::ValueParser;

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
    Sync {
        /// 源目录路径
        source: PathBuf,
        /// 目标目录路径
        #[arg(value_parser = ValueParser::string())]
        target: String,
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
        /// 显示详细操作列表（哪些文件被同步/删除）
        #[arg(long)]
        detail: bool,
    },
    Run {
        /// 任务名
        name: String,
        /// 配置文件路径 (默认: ./syncbox.toml)
        #[arg(long, default_value = "syncbox.toml")]
        config: PathBuf,
        /// 试运行模式（不实际写入）
        #[arg(long)]
        dry_run: bool,
        /// 使用校验和比较文件内容（而非仅修改时间/大小）
        #[arg(long)]
        checksum: bool,
        /// 显示详细操作列表（哪些文件被同步/删除）
        #[arg(long)]
        detail: bool,
    },

    Watch {
        /// 任务名
        name: String,
        /// 配置文件路径 (默认: ./syncbox.toml)
        #[arg(long, default_value = "syncbox.toml")]
        config: PathBuf,
        /// 监听延时 (默认: 500ms)
        #[arg(long, default_value = "500")]
        delay: u64,
        /// 试运行模式（不实际写入）
        #[arg(long)]
        dry_run: bool,
        /// 使用校验和比较文件内容（而非仅修改时间/大小）
        #[arg(long)]
        checksum: bool,
        /// 显示详细操作列表（哪些文件被同步/删除）
        #[arg(long)]
        detail: bool,
    },
}
