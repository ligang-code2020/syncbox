use clap::Parser;
use std::path::PathBuf;
use syncbox::sync;
use tracing::{info};

#[derive(Parser)]
#[command(name = "syncbox")]
#[command(about = "Sync files between directories", long_about = None)]
struct Args {
    /// Subcommand
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger(); // 初始化日志
    // 后续所有 tracing 日志都可用
    tracing::info!("SyncBox 启动");
    tracing::debug!("这是 debug 日志，只有 RUST_LOG=debug 时才显示");

    let args = Args::parse();
    match args.command {
        Command::Sync {
            source,
            target,
            dry_run,
        } => {
            info!(
                "Sync: copying file {} → {}",
                source.display(),
                target.display()
            );
            sync::sync_directories(&source, &target, dry_run, &[], false).await?;
        }

        Command::Run {
            name,
            config,
            dry_run,
        } => {
            info!("Running task: {}", name);

            // 1. 加载配置文件
            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            // 2. 查找任务
            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            info!(
                "Run: copying file {} → {}",
                &task.source.display(),
                &task.target.display()
            );

            // 3. 执行同步
            sync::sync_directories(
                &task.source,
                &task.target,
                dry_run,
                &task.exclude,
                task.delete_extra,
            )
            .await?;
        }

        Command::Watch {
            name,
            config,
            delay,
        } => {
            sync::watch_task(name, config, delay).await?;
        }
    }

    Ok(())
}

use tracing_subscriber::{EnvFilter, fmt};

pub fn init_logger() {
    // 从 RUST_LOG 环境变量读取日志级别
    // 默认 info，可设置 RUST_LOG=debug 查看详细日志
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt().with_env_filter(filter).init();
}
