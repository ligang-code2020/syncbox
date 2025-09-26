use std::fmt::Write;
use std::path::PathBuf;
use chrono::Local;
use num_format::{Locale, ToFormattedString};
use tracing::{info, warn};
use crate::utils::format_file_size;

// ==============================================
// 模块 6：输出结果报告
// 统一输出同步、删除结果
// ==============================================

/// 同步操作的结果报告
#[derive(Debug, Default)]
pub struct SyncReport {
    pub copied: Vec<PathBuf>,                  // 成功复制的文件
    pub errors: Vec<(PathBuf, String)>,        // 同步错误
    pub deleted: Vec<PathBuf>,                 // 成功删除的文件
    pub would_delete: Vec<PathBuf>,            // dry-run模式下待删除的文件
    pub delete_errors: Vec<(PathBuf, String)>, // 删除错误
}

/// 生成并打印同步操作的详细报告。
///
/// 统一输出同步和删除操作的结果，支持详细模式和 dry-run 模式。
///
/// # 参数
/// * `is_latest` - 是否为“无变更”状态（用于打印特定提示）。
/// * `report` - 同步操作结果报告。
/// * `dry_run` - 是否为试运行模式。
/// * `delete_extra` - 是否启用了删除多余文件功能。
/// * `total_source_files` - 源目录文件总数。
/// * `total_sync_size` - 待同步/已同步文件总大小（字节）。
/// * `detail` - 是否显示详细文件列表。
///
/// # 输出
/// 打印到日志（`tracing::info!`）和/或标准输出，包含：
/// - 同步文件数/大小
/// - 错误数（如有）
/// - 删除文件数（如有）
/// - 详细文件列表（如启用）
pub fn print_report(
    is_latest: bool,
    report: &SyncReport,
    dry_run: bool,
    delete_extra: bool,
    total_source_files: usize,
    total_sync_size: u64,
    detail: bool,
) {
    if is_latest {
        warn!("未发现待同步的文件");
        return;
    }
    let mut output = String::new();

    // 1. 基础同步信息
    writeln!(
        output,
        "{}\n源文件总数：{}，{}同步文件数: {} ({})",
        if dry_run {
            "试运行模式"
        } else {
            "同步成功！"
        },
        total_source_files.to_formatted_string(&Locale::en),
        if dry_run { "待" } else { "" },
        report.copied.len().to_formatted_string(&Locale::en),
        format_file_size(total_sync_size)
    )
        .unwrap();

    if detail && !report.copied.is_empty() {
        writeln!(output, "{}同步的文件：", if dry_run { "待" } else { "" }).unwrap();
        for path in &report.copied {
            writeln!(output, "  - {}", path.display()).unwrap();
        }
    }

    // 3. 同步错误信息
    if !dry_run && !report.errors.is_empty() {
        writeln!(
            output,
            "同步错误数: {}",
            report.errors.len().to_formatted_string(&Locale::en)
        )
            .unwrap();
    }

    if detail && !report.errors.is_empty() {
        writeln!(output, "同步错误详情：").unwrap();
        for (path, err) in &report.errors {
            writeln!(output, "  - {}: {}", path.display(), err).unwrap();
        }
    }

    // 4. 删除信息（仅当启用删除且有数据时显示）
    if delete_extra {
        let has_delete_data = if dry_run {
            !report.would_delete.is_empty() // 试运行：待删除不为空
        } else {
            !report.deleted.is_empty() || !report.delete_errors.is_empty() // 正常模式：已删除或删除错误不为空
        };

        if has_delete_data {
            if dry_run {
                writeln!(
                    output,
                    "待删除文件数: {}",
                    report.would_delete.len().to_formatted_string(&Locale::en)
                )
                    .unwrap();
                if detail && !report.would_delete.is_empty() {
                    writeln!(output, "待删除的文件：").unwrap();
                    for path in &report.would_delete {
                        writeln!(output, "  - {}", path.display()).unwrap();
                    }
                }
            } else {
                writeln!(
                    output,
                    "已删除文件数: {}",
                    report.deleted.len().to_formatted_string(&Locale::en)
                )
                    .unwrap();
                if detail && !report.deleted.is_empty() {
                    writeln!(output, "已删除的文件：").unwrap();
                    for path in &report.deleted {
                        writeln!(output, "  - {}", path.display()).unwrap();
                    }
                }

                if !report.delete_errors.is_empty() {
                    writeln!(
                        output,
                        "删除错误数: {}",
                        report.delete_errors.len().to_formatted_string(&Locale::en)
                    )
                        .unwrap();
                    if detail {
                        writeln!(output, "删除错误详情：").unwrap();
                        for (path, err) in &report.delete_errors {
                            writeln!(output, "  - {}: {}", path.display(), err).unwrap();
                        }
                    }
                }
            }
        }
    }

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    info!("[{}] {}", timestamp, output);
}