use clap::Parser;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use syncbox::sync; // 引入我们写的模块

/// A simple file synchronization tool
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志系统
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info) // 默认显示 Info 及以上
        .init();
    let args = Args::parse();
    match args.command {
        Command::Sync {
            source,
            target,
            dry_run,
        } => {
            log::info!("Syncing from {} to {}", source.display(), target.display());
            sync_directories(&source, &target, dry_run, &[]).await?;
        }

        Command::Run {
            name,
            config,
            dry_run,
        } => {
            log::info!("Running task: {}", name);

            // 1. 加载配置文件
            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            // 2. 查找任务
            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            // 3. 执行同步
            log::info!(
                "Task '{}' found: {} → {}",
                task.name,
                task.source.display(),
                task.target.display()
            );
            sync_directories(&task.source, &task.target, dry_run, &task.exclude).await?;
        }
    }

    Ok(())
}

async fn sync_directories(
    source: &PathBuf,
    target: &PathBuf,
    dry_run: bool,
    exclude: &[String],
) -> anyhow::Result<()> {
    // 1. 扫描源目录
    let source_files = sync::scan_directory(&source, exclude)
        .map_err(|e| anyhow::anyhow!("Failed to scan source: {}", e))?;

    println!("📊 Found {} files in source", source_files.len());

    let mut copied = 0;
    let mut skipped = 0;
    let mut failed_to_copy = 0;

    // 2. 遍历每个源文件
    for source_info in &source_files {
        // 计算目标路径：/src/a/b.txt → /dst/a/b.txt
        let relative = source_info
            .path
            .strip_prefix(&source)
            .expect("File not under source root");
        let target_path = target.join(relative);

        // 获取目标文件信息（如果存在）
        let target_info = if target_path.exists() {
            sync::FileInfo::from_path(&target_path).ok()
        } else {
            None
        };

        // 判断是否需要同步
        if sync::should_sync(source_info, target_info.as_ref()) {
            match sync::copy_file(&source_info.path, &target_path, dry_run) {
                Ok(()) => {
                    if !dry_run {
                        copied += 1;
                    }
                }
                Err(e) => {
                    // 👇 用户必须看到这个！
                    eprintln!("❌ Failed to copy '{}': {}", source_info.path.display(), e);
                    failed_to_copy += 1;
                }
            }
        } else {
            skipped += 1;
        }
    }

    println!("✅ Sync complete!");
    println!("   Copied: {}", copied);
    println!("   Skipped (unchanged): {}", skipped);
    if dry_run {
        println!("   (Dry run mode)");
    }
    if failed_to_copy > 0 {
        eprintln!("❌ Failed to copy {} files.", failed_to_copy);
    }

    // 如果有复制失败，我们也可以考虑返回错误（可选）
    if failed_to_copy > 0 {
        anyhow::bail!("Failed to copy {} files", failed_to_copy);
    }

    Ok(())
}
