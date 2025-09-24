use walkdir::WalkDir;
use rayon::prelude::*;
use super::types::FileInfo;
use super::filter::{ExcludeMatcher};
use crate::infra::error::SyncError;
use std::path::Path;
use tracing::warn;

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
    let root = root.as_ref();

    // 1. 检查目录是否存在
    if !root.exists() {
        return Err(SyncError::SourceNotFound(root.to_path_buf()));
    }

    // 2. 预编译排除规则
    let matcher = ExcludeMatcher::new(exclude_patterns)
        .map_err(|e| SyncError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?;

    // 2. 使用 WalkDir 非递归方式收集所有文件路径（过滤排除项）
    let entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(|e| {
            let entry = match e {
                Ok(entry) => entry,
                Err(e) => {
                    warn!(error = ?e, "Failed to read directory entry");
                    return None;
                }
            };
            let path = entry.path();
            // 跳过目录（我们只关心文件）
            if !path.is_file() {
                return None;
            }
            // 👇 使用预编译 matcher 快速判断
            if matcher.is_excluded(path, root) {
                return None;
            }
            Some(path.to_path_buf())
        })
        .collect();

    // 3. 并行处理每个文件路径，构建 FileInfo
    let files: Vec<_> = entries
        .par_iter() // 👈 使用 rayon 并行迭代
        .filter_map(|path| {
            match FileInfo::from_path(path, compute_hash) {
                Ok(info) => Some(info),
                Err(e) => {
                    warn!(
                        error = ?e,
                        path = %path.display(),
                        "Failed to read file metadata"
                    );
                    None
                }
            }
        })
        .collect();

    Ok(files)
}