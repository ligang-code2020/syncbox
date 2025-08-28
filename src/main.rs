use tokio::time;
use std::time::Duration;
use clap::Parser;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use syncbox::sync;

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

        Command::Watch {
            name,
            config,
            delay,
        } => {
            watch_task(name, config, delay).await?;
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




async fn watch_task(name: String, config_path: PathBuf, delay_ms: u64) -> anyhow::Result<()> {
    // 1. 加载配置文件
    log::info!("Loading config for task: {}", name);
    let config = syncbox::config::Config::from_file(&config_path)
        .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

    // 2. 查找指定名称的任务
    let task = config.find_task(&name)
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

    println!(
        "👀 Watching task '{}' ({} → {})",
        task.name,
        task.source.display(),
        task.target.display()
    );

    // 3. 创建一个异步 channel，用于从文件监听线程向主异步循环传递事件
    //    - unbounded_channel：不限制缓冲区大小，避免事件丢失
    //    - tx: 发送端（在监听回调中使用）
    //    - rx: 接收端（在主循环中使用）
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // 4. 创建文件系统监听器（watcher）
    //    `recommended_watcher` 会根据操作系统自动选择最优后端：
    //    - macOS: FSEvents
    //    - Linux: inotify
    //    - Windows: ReadDirectoryChangesW
    //
    //    回调函数会在后台线程中被调用，所以必须是 'static + Send
    let mut watcher = recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                // 只关心三类事件：修改、创建、删除
                // 忽略元数据变更（如访问时间）、重命名等，避免过度触发
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        // 将事件发送到 channel
                        // 如果接收端已关闭（如程序退出），则忽略错误
                        let _ = tx.send(event);
                    }
                    _ => {
                        // 其他事件（如 Metadata、Access、Other）不处理
                        log::debug!("Ignored event: {:?}", event);
                    }
                }
            }
            Err(error) => {
                // 监听过程中发生错误（如权限不足、路径不存在）
                eprintln!("📁 File watch error: {}", error);
            }
        }
    }).map_err(|e| anyhow::anyhow!("Failed to create file watcher: {}", e))?;

    // 5. 开始监听源目录（递归监听所有子目录）
    watcher.watch(&task.source, RecursiveMode::Recursive)
        .map_err(|e| anyhow::anyhow!("Failed to watch directory '{}': {}", task.source.display(), e))?;

    log::info!("Started watching: {}", task.source.display());

    // 6. 主事件循环：接收文件变化事件并处理
    loop {
        // --- 防抖机制开始 ---
        // 我们希望：用户连续修改文件时，只在“最后一次修改后 delay_ms 毫秒”才同步一次

        // 6.1 等待第一个文件变化事件
        if rx.recv().await.is_none() {
            log::info!("Watcher channel closed, exiting...");
            break; // channel 被关闭，退出循环（通常是程序终止）
        }

        log::info!("Change detected, starting debounce period of {}ms...", delay_ms);

        // 6.2 进入防抖等待状态
        //     使用一个内层循环，持续检查是否有新事件到来
        loop {
            // 尝试在 `delay_ms` 毫秒内接收下一个事件
            // 如果收到新事件，说明用户还在修改，需要“重置”防抖计时器
            match time::timeout(Duration::from_millis(delay_ms), rx.recv()).await {
                Ok(Some(_)) => {
                    // 又有新事件！说明文件还在被修改，重新开始等待
                    log::debug!("Another change detected, restarting debounce timer...");
                    continue; // 继续等待
                }
                Ok(None) => {
                    // channel 被关闭（发送端关闭）
                    log::info!("Watcher channel closed during debounce.");
                    return Ok(()); // 正常退出
                }
                Err(_) => {
                    // timeout 超时！说明在 delay_ms 毫秒内没有新事件
                    // 👉 这正是我们想要的：用户已经“停止”修改文件
                    log::info!("Debounce period ended with no further changes.");
                    break; // 跳出内层循环，准备执行同步
                }
            }
        }
        // --- 防抖机制结束 ---

        // 7. 执行同步操作
        //    使用已有的 sync_directories 函数，支持 exclude 规则
        println!("📁 Detected stable changes → syncing...");
        match sync_directories(&task.source, &task.target, false, &task.exclude).await {
            Ok(()) => {
                println!("✅ Sync completed successfully");
            }
            Err(e) => {
                eprintln!("❌ Sync failed: {}", e);
                // 注意：这里不返回错误，继续监听
                // 因为一次同步失败不应导致监听中断
            }
        }

        // 8. 同步完成，回到外层循环，继续等待下一次变化
    }

    Ok(())
}