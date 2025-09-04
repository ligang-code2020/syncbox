use clap::Parser;
use syncbox::{cli, infra, sync};
use tracing::info;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    infra::logging::init_logger(); // 初始化日志
    // 后续所有 tracing 日志都可用
    tracing::info!("SyncBox 启动");
    tracing::debug!("这是 debug 日志，只有 RUST_LOG=debug 时才显示");

    let args = cli::Args::parse();
    match args.command {
        cli::Command::Sync {
            source,
            target,
            dry_run,
            checksum,
        } => {
            info!(
                "Sync: copying file {} → {}",
                source.display(),
                target.display()
            );

            let options = sync::SyncOptions {
                dry_run,
                excludes: vec![],
                checksum, // 新增
            };
            sync::sync_directories(&source, &target, &options).await?;
        }

        cli::Command::Run {
            name,
            config,
            dry_run,
            checksum, // 新增
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

            let options = sync::SyncOptions {
                dry_run,
                excludes: task.exclude.clone(),
                checksum, // 新增
            };

            // 3. 执行同步
            sync::sync_directories(&task.source, &task.target, &options).await?;
        }

        cli::Command::Watch {
            name,
            config,
            delay, ..
        } => {
            info!("Watching task: {}", name);

            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            sync::watch_task(&task, delay).await?;
        }
    }

    Ok(())
}
