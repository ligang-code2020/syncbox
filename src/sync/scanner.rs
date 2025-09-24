use std::fs;
use super::types::{FileInfo};
use crate::infra::error::SyncError;
use std::path::Path;
use tracing::{debug, warn};
use super::filter::{should_exclude};

// ==============================================
// 模块 1：扫描器（Scanner）
// 负责遍历目录，收集文件信息
// ==============================================


/// 递归扫描指定根目录，收集所有非排除文件的元信息。
///
/// 遍历目录树，跳过符合排除规则的路径，并可选择是否计算文件哈希。
///
/// # 参数
/// * `root` - 要扫描的根目录路径。
/// * `exclude_patterns` - 排除规则字符串列表（支持通配符、目录匹配）。
/// * `compute_hash` - 是否为每个文件计算 BLAKE3 内容哈希。
///
/// # 返回
/// * `Ok(Vec<FileInfo>)` - 扫描到的所有文件信息列表。
/// * `Err(SyncError)` - 目录不存在、无权限或 I/O 错误。
///
/// # 注意
/// - 遇到无法读取的子目录或文件时记录警告并跳过，不中断整体扫描。
/// - 默认排除系统文件（如 `.DS_Store`, `._*` 等）。
pub fn scan_directory<P: AsRef<Path>>(
    root: P,
    exclude_patterns: &[String],
    compute_hash: bool,
) -> Result<Vec<FileInfo>, SyncError> {
    let mut files = Vec::new();
    let root = root.as_ref();

    // 1. 检查目录是否存在
    if !root.exists() {
        return Err(SyncError::SourceNotFound(root.to_path_buf()));
    }

    // 2. 读取目录
    let entries = fs::read_dir(root).map_err(|e| {
        debug!(
                error = ?e,
                path = %root.display(),
                "Failed to read directory"
            );
        SyncError::IoError(e)
    })?;

    // 3. 遍历条目
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                warn!(
                        error = ?e,
                        dir = %root.display(),
                        "Failed to read directory entry"
                    );
                continue;
            }
        };

        let path = entry.path();

        // 4. 检查是否排除
        if should_exclude(&path, root, exclude_patterns) {
            debug!(path = %path.display(), "Skipped (excluded)");
            continue;
        }

        if path.is_dir() {
            match scan_directory(&path, exclude_patterns, compute_hash) {
                Ok(mut sub_files) => {
                    files.append(&mut sub_files);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        } else {
            match FileInfo::from_path(&path, compute_hash) {
                Ok(info) => files.push(info),
                Err(e) => {
                    warn!(
                            error = ?e,
                            path = %path.display(),
                            "Failed to read file metadata"
                        );
                }
            }
        }
    }

    Ok(files)
}