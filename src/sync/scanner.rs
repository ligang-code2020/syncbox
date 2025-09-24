use std::fs;
use super::types::{FileInfo}; 
use crate::infra::error::SyncError; 
use std::path::Path;
use tracing::{debug, warn};
use super::filter::{should_exclude};

/// 递归遍历目录，返回所有文件和目录的 `FileInfo` 列表
///
/// # 参数
/// - `path`: 要扫描的目录路径
///
/// # 返回
/// - `Ok(Vec<FileInfo>)`: 扫描到的文件信息列表
/// - `Err(std::io::Error)`: 扫描过程中发生的 I/O 错误
///
/// # 注意
/// 此函数会跳过无法访问的文件或目录，并记录警告日志。
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