use thiserror::Error;
use std::path::PathBuf;

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
}