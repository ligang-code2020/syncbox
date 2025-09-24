use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use crate::{cli, config};
use super::file_ops::compute_blake3_hash;

#[derive(Debug, Clone)]
pub struct FileInfo {
    // 文件目录
    pub path: PathBuf,
    // 系统时间
    pub mtime: SystemTime,
    // 文件大小
    pub size: u64,
    // 存储 BLAKE3 哈希值
    pub blake3_hash: Option<[u8; 32]>,
}

impl FileInfo {
    /// 从路径创建 FileInfo
    pub fn from_path(path: &Path, compute_hash: bool) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let blake3_hash = if compute_hash && metadata.is_file() {
            Some(compute_blake3_hash(path)?)
        } else {
            None
        };
        Ok(FileInfo {
            path: path.to_path_buf(),
            mtime: metadata.modified()?,
            size: metadata.len(),
            blake3_hash,
        })
    }

    /// 默认策略：比较两个文件的修改时间和大小
    pub fn is_newer_than(&self, target: &Self) -> bool {
        self.mtime > target.mtime || self.size != target.size
    }

    /// 增强策略：比较两个文件是否内容相同（用于哈希模式）
    pub fn content_eq(&self, other: &Self) -> bool {
        self.size == other.size && self.blake3_hash == other.blake3_hash
    }
}

// 定义统一的同步参数结构
#[derive(Debug, Clone)]
pub struct SyncParameters {
    /// 源目录
    pub source: PathBuf,
    /// 目标目录
    pub target: PathBuf,
    /// 试运行模式
    pub dry_run: bool,
    /// 是否使用校验和比较
    pub checksum: bool,
    /// 排除同步规则列表
    pub excludes: Vec<String>,
    /// 是否删除目标额外文件
    pub delete_extra: bool,
    /// 排除目标目录删除列表
    pub delete_excludes: Vec<String>,
    /// 是否显示详细操作列表
    pub detail: bool,
}

// 实现从不同来源转换为统一参数
impl From<&cli::Command> for SyncParameters {
    fn from(cmd: &cli::Command) -> Self {
        match cmd {
            cli::Command::Sync {
                source,
                target,
                dry_run,
                checksum,
                delete,
                exclude,
                delete_exclude,
                detail,
            } => Self {
                source: source.clone(),
                target: target.clone(),
                dry_run: *dry_run,
                checksum: *checksum,
                excludes: exclude.clone(),
                delete_extra: *delete,
                delete_excludes: delete_exclude.clone(),
                detail: *detail,
            },
            cli::Command::Run {
                name: _,
                config: _,
                dry_run,
                checksum,
                detail,
            } => {
                // 这里只是占位，实际会在加载配置后覆盖
                Self {
                    source: PathBuf::new(),
                    target: PathBuf::new(),
                    dry_run: *dry_run,
                    checksum: *checksum,
                    excludes: Vec::new(),
                    delete_extra: false,
                    delete_excludes: Vec::new(),
                    detail: *detail,
                }
            }
            cli::Command::Watch {
                name: _,
                config: _,
                delay: _,
                checksum,
                dry_run,
                detail,
            } => Self {
                source: PathBuf::new(),
                target: PathBuf::new(),
                dry_run: *dry_run,
                checksum: *checksum,
                excludes: Vec::new(),
                delete_extra: false,
                delete_excludes: Vec::new(),
                detail: *detail,
            }
        }
    }
}

// 从配置任务转换
impl From<&config::SyncTask> for SyncParameters {
    fn from(task: &config::SyncTask) -> Self {
        Self {
            source: task.source.clone(),
            target: task.target.clone(),
            dry_run: false,  // 由命令行参数决定
            checksum: false, // 由命令行参数决定
            excludes: task.exclude.clone(),
            delete_extra: task.delete_extra,
            delete_excludes: task.delete_extra_exclude.clone(),
            detail: false,
        }
    }
}