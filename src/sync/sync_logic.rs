use chrono::Utc;
use tracing::{debug, warn};
use crate::utils::create_progress_bar;
use super::file_ops::{copy_file, delete_extra_files};
use super::filter::should_sync;
use super::report::{print_report, SyncReport};
pub use super::scanner::{scan_directory};
use super::types::{FileInfo, SyncParameters};

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

/// 执行一次完整的目录同步
///
/// # 策略
/// 1. 扫描源目录
/// 2. 扫描目标目录
/// 3. 复制新/更新的文件
/// 4. （可选）删除目标目录中多余的文件
///
/// # 参数
/// - `source`: 源目录
/// - `target`: 目标目录
/// - `dry_run`: 是否为试运行（不实际修改文件）
/// - `excludes`: 排除规则
/// - `delete_extra`: 是否删除目标目录中多余的文件
///
/// # 返回
/// - `Ok(SyncReport)`: 同步结果报告
/// - `Err(_)`: 致命错误（如源目录不存在）

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

    // 2. 预扫描：筛选出需要同步的文件，并计算总大小
    let mut sync_queue = Vec::new();
    let mut total_sync_size: u64 = 0;

    for source_info in &source_files {
        let relative = source_info
            .path
            .strip_prefix(&params.source)
            .expect("File not under source root");
        let target_path = params.target.join(relative);
        let target_info = if target_path.exists() {
            FileInfo::from_path(&target_path, options.checksum).ok()
        } else {
            None
        };

        // 判断是否需要同步，只将需要同步的文件加入队列
        if should_sync(source_info, target_info.as_ref(), options.checksum) {
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
        && (!options.delete_extra
        || report.would_delete.is_empty()
        || report.deleted.is_empty())
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

        for (source_info, target_path) in &sync_queue {
            match copy_file(&source_info.path, target_path, options.dry_run).await {
                Ok(()) => {
                    report.copied.push(source_info.path.clone());
                    processed_size += source_info.size;
                    pb.set_position(processed_size);
                    debug!(
                            source = %source_info.path.display(),
                            target = %target_path.display(),
                            "File copied"
                        );
                }
                Err(e) => {
                    warn!(
                            error = ?e,
                            source = %source_info.path.display(),
                            target = %target_path.display(),
                            "Failed to copy file"
                        );
                    report.errors.push((target_path.clone(), e.to_string()));
                    processed_size += source_info.size;
                    pb.set_position(processed_size);
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