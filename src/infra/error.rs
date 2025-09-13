use thiserror::Error;
use std::path::PathBuf;
use notify::Error as NotifyError; // 引入notify库的错误类型

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Source directory not found: {0}")]
    SourceNotFound(PathBuf),

    #[error("Target directory is not writable: {0}")]
    TargetNotWritable(PathBuf),

    #[error("File is in use or permission denied: {0}")]
    PermissionDenied(PathBuf),

    #[error("Directory traversal error: {0}")]
    WalkError(#[from] walkdir::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    // 新增：文件监听错误（notify库的错误）
    #[error("File watcher error: {0}")]
    WatchError(#[from] NotifyError),

    // 新增：同步逻辑中的通用错误（包装anyhow::Error）
    #[error("Sync logic error: {0}")]
    SyncLogicError(String),

    // 新增：删除额外文件时的错误
    #[error("Failed to delete extra files: {0}")]
    DeleteError(String),
}

// 为anyhow::Error实现转换（需在SyncError所在模块中）
impl From<anyhow::Error> for SyncError {
    fn from(err: anyhow::Error) -> Self {
        SyncError::SyncLogicError(err.to_string())
    }
}