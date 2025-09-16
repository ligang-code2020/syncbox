use clap::Parser;
use syncbox::{cli, infra, sync};
use tracing::{info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    infra::logging::init_logger();
    info!("SyncBox Starting...");

    let args = cli::Args::parse();
    match args.command {
        // ============ SYNC 模式 ============
        cli::Command::Sync {
            source,
            target,
            dry_run,
            checksum,
            delete,
            exclude,
            delete_exclude,
            detail
        } => {
            let params = sync::SyncParameters {
                source: source.clone(),
                target: target.clone(),
                dry_run,
                checksum,
                excludes: exclude.clone(),
                delete_extra: delete,
                delete_excludes: delete_exclude.clone(),
                detail
            };

            info!(
                "Sync: copying file {} → {}",
                source.display(),
                target.display()
            );
            sync::sync_directories(&params).await?;
        }

        // ============ RUN TASK 模式 ============
        cli::Command::Run {
            name,
            config,
            dry_run,
            checksum, // 新增
            detail
        } => {

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

            let mut params = sync::SyncParameters::from(task);
            params.dry_run = dry_run;
            params.checksum = checksum;
            params.detail = detail;

            // 调用统一核心逻辑
            sync::sync_directories(&params).await?;
        }

        // ============ WATCH 模式 ============
        cli::Command::Watch {
            name,
            config,
            dry_run,
            delay,
            checksum,
            detail
        } => {



            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            info!(
                "Watch: copying file {} → {}",
                &task.source.display(),
                &task.target.display()
            );

            let mut params = sync::SyncParameters::from(task);
            params.checksum = checksum; // 新增此行
            params.detail = detail;
            params.dry_run = dry_run;
            sync::watch_task(&params, delay).await?;
        }
    }

    Ok(())
}
