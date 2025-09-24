use super::file_ops::{copy_file, delete_extra_files};
use super::filter::should_sync;
use super::report::{SyncReport, print_report};
use super::scanner::scan_directory; // 确保使用的是我们刚优化过的并行版本
use super::types::{FileInfo, SyncParameters};
use crate::utils::create_progress_bar;
use chrono::Utc;
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;
use tracing::{debug, warn};

// ==============================================
// 模块 4：同步逻辑（SyncLogic）
// ==============================================

pub struct SyncOptions {
    pub dry_run: bool,
    pub excludes: Vec<String>,
    pub checksum: bool,
    pub delete_extra: bool,
    pub delete_excludes: Vec<String>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            excludes: vec![],
            checksum: false,
            delete_extra: false,
            delete_excludes: vec![],
        }
    }
}

/// 执行一次完整的目录同步操作。
///
/// 包括扫描源目录、比对目标文件、复制差异文件、可选删除多余文件。
///
/// # 参数
/// * `params` - 同步参数结构体，包含源/目标路径、dry-run、checksum、排除规则等。
///
/// # 返回
/// * `Ok(SyncReport)` - 同步操作报告，包含成功、失败、删除等统计信息。
/// * `Err(anyhow::Error)` - 扫描、复制或删除过程中发生致命错误。
///
/// # 流程
/// 1. 扫描源目录。
/// 2. 构建同步队列（需复制的文件）。
/// 3.（可选）删除目标端多余文件。
/// 4. 执行文件复制（带进度条）。
/// 5. 生成并打印报告。
pub async fn sync_directories(params: &SyncParameters) -> anyhow::Result<SyncReport> {
    let options = SyncOptions {
        dry_run: params.dry_run,
        excludes: params.excludes.clone(),
        checksum: params.checksum,
        delete_extra: params.delete_extra,
        delete_excludes: params.delete_excludes.clone(),
    };

    let mut report = SyncReport::default(); // 初始化报告
    println!("当前时间戳1: {}", Utc::now().timestamp());

    // 1. 扫描源目录获取所有文件
    let source_files = scan_directory(&params.source, &options.excludes, options.checksum)
        .map_err(|e| anyhow::anyhow!("Failed to scan source directory -> {}", e))?;
    println!("当前时间戳2: {}", Utc::now().timestamp());

    // 2.预扫描目标目录，构建缓存
    let target_cache: HashMap<String, FileInfo> = if params.target.exists() {
        match scan_directory(&params.target, &options.excludes, options.checksum) {
            Ok(target_files) => target_files
                .into_iter()
                .filter_map(|info| {
                    let relative = info
                        .path
                        .strip_prefix(&params.target)
                        .map(|p| p.to_string_lossy().to_string())
                        .ok();
                    relative.map(|rel| (rel, info))
                })
                .collect(),
            Err(e) => {
                warn!(error = ?e, "Failed to scan target directory, proceeding with empty cache");
                HashMap::new()
            }
        }
    } else {
        debug!("Target directory does not exist, skipping target scan");
        HashMap::new()
    };

    // 2. 预扫描：筛选出需要同步的文件，并计算总大小
    let mut sync_queue = Vec::new();
    let mut total_sync_size: u64 = 0;

    for source_info in &source_files {
        let relative = source_info
            .path
            .strip_prefix(&params.source)
            .expect("File not under source root");

        let relative_str = relative.to_string_lossy().to_string();
        let target_path = params.target.join(relative);

        let target_info = target_cache.get(&relative_str);

        // 判断是否需要同步，只将需要同步的文件加入队列
        if should_sync(source_info, target_info, options.checksum) {
            sync_queue.push((source_info.clone(), target_path));
            total_sync_size += source_info.size;
        }
    }

    if options.delete_extra {
        let (deleted, would_delete, delete_errors) = delete_extra_files(
            &params.source,
            &params.target,
            options.dry_run,
            &options.excludes,
            &options.delete_excludes,
        )
        .await?;

        report.deleted = deleted;
        report.would_delete = would_delete;
        report.delete_errors = delete_errors;
    }

    // 检查是否有需要同步的文件
    if sync_queue.is_empty()
        && (!options.delete_extra || report.would_delete.is_empty() || report.deleted.is_empty())
    {
        // 没有文件需要同步，直接返回
        print_report(
            true,
            &report,
            options.dry_run,
            options.delete_extra,
            source_files.len(),
            total_sync_size,
            params.detail,
        );
        return Ok(report);
    }

    // 4. 处理同步队列
    let mut processed_size = 0;

    if options.dry_run {
        // Dry-run 模式：列出所有将被同步的文件
        for (source_info, _target_path) in &sync_queue {
            report.copied.push(source_info.path.clone());
        }
    } else {
        // 正常模式：初始化进度条
        let pb = create_progress_bar(total_sync_size);

        // 原子计数器，用于并发更新进度
        let processed_bytes = Arc::new(AtomicU64::new(0));

        // 控制最大并发数（可根据系统调整）
        let semaphore = Arc::new(Semaphore::new(8));

        // 创建异步任务集合
        let mut tasks = FuturesUnordered::new();

        for (source_info, target_path) in &sync_queue {
            let source_path = source_info.path.clone();
            let target_path_clone = target_path.clone();
            let size = source_info.size;
            let pb_clone = pb.clone();
            let processed_bytes_clone = processed_bytes.clone();
            let semaphore_clone = semaphore.clone();
            let source_display = source_path.display().to_string();
            let target_display = target_path_clone.display().to_string();

            let task = tokio::spawn(async move {
                // 获取信号量许可（控制并发）
                let _permit = semaphore_clone.acquire().await.unwrap();

                let result = copy_file(&source_path, &target_path_clone, false).await;

                // 无论成功失败，都更新进度
                let current = processed_bytes_clone.fetch_add(size, Ordering::Relaxed) + size;
                pb_clone.set_position(current);

                (
                    result,
                    source_path,
                    target_path_clone,
                    source_display,
                    target_display,
                )
            });

            tasks.push(task);
        }

        // 等待所有任务完成
        while let Some(result) = tasks.next().await {
            match result {
                Ok((Ok(()), source_path, target_path, source_display, target_display)) => {
                    report.copied.push(source_path);
                    debug!(
                        source = %source_display,
                        target = %target_display,
                        "File copied"
                    );
                }
                Ok((Err(e), source_path, target_path, source_display, target_display)) => {
                    warn!(
                        error = ?e,
                        source = %source_display,
                        target = %target_display,
                        "Failed to copy file"
                    );
                    report.errors.push((target_path, e.to_string()));
                }
                Err(join_error) => {
                    // 任务 panic（理论上不应发生）
                    warn!(error = ?join_error, "Copy task panicked");
                    report.errors.push((PathBuf::new(), join_error.to_string()));
                }
            }
        }

        pb.finish_with_message("File sync completed");
    }

    if report.errors.len() > 0 {
        warn!(count = report.errors.len(), "Some files failed to copy");
        anyhow::bail!("Failed to copy {} files", report.errors.len());
    }

    // 5. 统一输出整合后的结果
    print_report(
        false,
        &report,
        options.dry_run,
        options.delete_extra, // 新增：是否启用删除功能
        source_files.len(),
        total_sync_size,
        params.detail,
    );

    Ok(report)
}
