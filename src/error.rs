use thiserror::Error;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("源目录不存在: {0}")]
    SourceNotFound(PathBuf),

    #[error("目标目录不可写: {0}")]
    TargetNotWritable(PathBuf),

    #[error("文件被占用或权限不足: {0}")]
    PermissionDenied(PathBuf),

    #[error("路径遍历错误: {0}")]
    WalkError(#[from] walkdir::Error),

    #[error("I/O 错误: {0}")]
    IoError(#[from] std::io::Error),
}