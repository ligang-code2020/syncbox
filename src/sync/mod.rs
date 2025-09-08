use crate::infra::error::SyncError;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use notify::{Event, EventKind, RecursiveMode, Watcher, recommended_watcher};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

// ==============================================
// 公共类型定义（对外暴露）
// ==============================================

/// 表示一个文件或目录的元信息
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

/// 同步操作的结果报告
#[derive(Debug, Default)]
pub struct SyncReport {
    pub copied: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub errors: Vec<(PathBuf, String)>,
}

// ==============================================
// 模块 1：扫描器（Scanner）
// 负责遍历目录，收集文件信息
// ==============================================

mod scanner {
    use super::*;
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
}

// ==============================================
// 模块 2：过滤器（Filter）
// 负责判断文件是否应被排除或同步
// ==============================================
mod filter {
    use super::*;

    /// 判断一个路径是否应该被排除（基于排除规则）
    ///
    /// # 参数
    /// - `path`: 要检查的路径
    /// - `excludes`: 排除规则列表（支持通配符和正则）
    ///
    /// # 返回
    /// - `true`: 应该排除
    /// - `false`: 不排除
    ///
    /// # 规则支持
    /// - `*.tmp` → 所有 .tmp 文件
    /// - `/temp/` → 包含 temp 的路径
    /// todo - 将来可扩展为正则表达式
    pub fn should_exclude(path: &Path, root: &Path, exclude_patterns: &[String]) -> bool {
        // 我们需要将路径转换为“相对于 root 的路径”
        // 比如：/Users/you/syncbox-tests/src/a.tmp → a.tmp
        let relative = match path.strip_prefix(root) {
            Ok(rel) => rel,
            Err(_) => return false, // 无法计算相对路径，不排除
        };

        // 将相对路径转成字符串
        let relative_str = relative.to_string_lossy();

        // 检查每个排除规则
        for pattern in exclude_patterns {
            // 简单实现：支持后缀匹配（.tmp）和目录匹配（Secret/）
            if pattern.starts_with('/') {
                // 如果规则以 / 开头，匹配完整路径（从 root 开始）
                if relative_str.starts_with(&pattern[1..]) {
                    return true;
                }
            } else if pattern.ends_with('/') {
                // 如果规则以 / 结尾，匹配目录
                if relative_str.starts_with(&*pattern)
                    || relative_str.contains(&format!("/{}", pattern))
                {
                    return true;
                }
            } else {
                // 否则，匹配后缀（如 .tmp）
                if relative_str.ends_with(pattern) {
                    return true;
                }
            }
        }

        // 排除默认系统文件
        if let Some(name) = relative.file_name().and_then(|s| s.to_str()) {
            matches!(
                name,
                ".DS_Store" | ".fseventsd" | ".Trashes" | ".Spotlight-V100" | ".TemporaryItems"
            ) || name.starts_with("._") // AppleDouble 文件
        } else {
            false
        }
    }

    /// 比较源文件和目标文件，决定是否需要同步
    ///
    /// # 策略
    /// - 目标文件不存在 → 需要同步
    /// - 源文件更新 → 需要同步
    /// - 源文件更大 → 需要同步（防截断）
    ///
    /// # 返回
    /// - `true`: 需要同步
    /// - `false`: 无需同步

    pub fn should_sync(
        source_info: &FileInfo,
        target_info: Option<&FileInfo>,
        checksum: bool,
    ) -> bool {
        match target_info {
            None => true, // 目标不存在，需要同步
            Some(target) => {
                if checksum {
                    // 哈希模式：比较大小和哈希值
                    !source_info.content_eq(target)
                } else {
                    // 默认模式：比较 mtime 和 size
                    source_info.is_newer_than(target)
                }
            }
        }
    }
}

// ==============================================
// 模块 3：文件操作（FileOps）
// 负责实际的文件复制、删除等操作
// ==============================================

mod file_ops {
    use super::*;
    /// 复制文件（自动创建目标目录）
    ///
    /// # 参数
    /// - `src`: 源文件路径
    /// - `dst`: 目标文件路径
    ///
    /// # 行为
    /// 1. 确保目标目录存在（自动创建）
    /// 2. 执行文件复制
    ///
    /// # 注意
    /// 使用 `tokio::fs::copy`，保留元信息（如修改时间）。
    pub async fn copy_file(source: &Path, target: &Path, dry_run: bool) -> std::io::Result<()> {
        if dry_run {
            return Ok(());
        }

        // 创建目标目录
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 执行复制
        tokio::fs::copy(source, target).await?;
        Ok(())
    }

    pub fn compute_blake3_hash(path: &Path) -> std::io::Result<[u8; 32]> {
        let mut file = fs::File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut file, &mut hasher)?;
        Ok(hasher.finalize().into())
    }
}

// ==============================================
// 模块 4：同步逻辑（SyncLogic）
// ==============================================

mod sync_logic {
    use super::*;

    pub struct SyncOptions {
        pub dry_run: bool,
        pub excludes: Vec<String>,
        pub checksum: bool,
    }

    impl Default for SyncOptions {
        fn default() -> Self {
            Self {
                dry_run: false,
                excludes: vec![],
                checksum: false,
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
    ///
    /// # 返回
    /// - `Ok(SyncReport)`: 同步结果报告
    /// - `Err(_)`: 致命错误（如源目录不存在）

    pub async fn sync_directories(
        source: &PathBuf,
        target: &PathBuf,
        options: &SyncOptions,
    ) -> anyhow::Result<()> {
        // 1. 扫描源目录获取所有文件
        let source_files = scan_directory(source, &options.excludes, options.checksum)
            .map_err(|e| anyhow::anyhow!("Failed to scan source directory -> {}", e))?;



        // 2. 预扫描：筛选出需要同步的文件，并计算总大小
        let mut sync_queue = Vec::new();
        let mut total_sync_size: u64 = 0;

        for source_info in &source_files {
            let relative = source_info
                .path
                .strip_prefix(source)
                .expect("File not under source root");
            let target_path = target.join(relative);
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

        // 检查是否有需要同步的文件
        if sync_queue.is_empty() {
            // 没有文件需要同步，直接提示并返回
            debug!("✅无需同步，已经是最新的了");
            return Ok(());
        }

        // 4. 处理同步队列
        let mut copied = 0;
        let mut failed_to_copy = 0;
        let mut processed_size = 0;

        if options.dry_run {
            // Dry-run 模式：列出所有将被同步的文件
            info!("📋 Dry run mode - files to be synchronized:");
            for (source_info, target_path) in &sync_queue {
                info!(
                    "→ {} → {}",
                    source_info.path.display(),
                    target_path.display()
                );
            }
        } else {
            // 正常模式：初始化进度条
            let pb = ProgressBar::new(total_sync_size);
            pb.set_style(ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:50.cyan/blue}] {bytes}/{total_bytes} ({eta})"
            )?
                .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                    write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
                })
                .progress_chars("#>-"));

            for (source_info, target_path) in &sync_queue {
                match copy_file(&source_info.path, &target_path, options.dry_run).await {
                    Ok(()) => {
                        copied += 1;
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
                        failed_to_copy += 1;
                        processed_size += source_info.size;
                        pb.set_position(processed_size);
                    }
                }
            }

            pb.finish_with_message("File sync completed");
        }

        // 5. 统计信息（跳过的文件=总文件数-待同步文件数）
        let skipped = source_files.len() - sync_queue.len();
        info!(
            total = source_files.len(),
            to_sync = sync_queue.len(),
            copied = if options.dry_run { 0 } else { copied },
            skipped,
            failed = failed_to_copy,
            dry_run = options.dry_run,
            "Sync completed"
        );

        if failed_to_copy > 0 {
            warn!(count = failed_to_copy, "Some files failed to copy");
            anyhow::bail!("Failed to copy {} files", failed_to_copy);
        }
        Ok(())
    }
}

// ==============================================
// 模块 5：监听器（Watcher）
// 文件系统监听，实时同步
// ==============================================

mod watcher {
    use super::*;

    /// 启动文件监听任务
    ///
    /// # 行为
    /// 监听源目录变化，一旦有文件修改/创建，立即触发同步
    ///
    /// # 参数
    /// - `name`: 任务名
    /// - `delay_ms`: 延时操作
    ///
    /// # 注意
    /// 此函数会阻塞运行，直到监听被中断。
    pub async fn watch_task(task: &crate::config::SyncTask, delay_ms: u64) -> anyhow::Result<()> {
        let options = SyncOptions {
            dry_run: false, // watch 模式通常不是 dry_run
            excludes: task.exclude.clone(),
            checksum: false,
        };

        // 3. 创建一个异步 channel，用于从文件监听线程向主异步循环传递事件
        //    - unbounded_channel：不限制缓冲区大小，避免事件丢失
        //    - tx: 发送端（在监听回调中使用）
        //    - rx: 接收端（在主循环中使用）
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // 4. 创建文件系统监听器（watcher）
        //    `recommended_watcher` 会根据操作系统自动选择最优后端：
        //    - macOS: FSEvents
        //    - Linux: inotify
        //    - Windows: ReadDirectoryChangesW
        //
        //    回调函数会在后台线程中被调用，所以必须是 'static + Send
        let mut watcher =
            recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // 只关心二类事件：修改、创建
                        // 忽略元数据变更（如访问时间）、重命名等，避免过度触发
                        match event.kind {
                            // 只处理文件内容修改和创建事件
                            EventKind::Create(_) => {
                                let _ = tx.send(event);
                            }
                            EventKind::Modify(modify_kind) => {
                                // 仅处理文件内容数据修改，忽略元数据、权限等变更
                                if matches!(modify_kind, notify::event::ModifyKind::Data(_)) {
                                    let _ = tx.send(event);
                                } else {
                                    debug!(event = ?event, "Ignored non-data modify event");
                                }
                            }
                            // 明确忽略删除相关事件（包括可能的元数据变更）
                            EventKind::Remove(_) => {
                                debug!(event = ?event, "Ignored file removal event");
                            }
                            _ => {
                                debug!(event = ?event, "Ignored file system event");
                            }
                        }
                    }
                    Err(error) => {
                        // 监听过程中发生错误（如权限不足、路径不存在）
                        error!("📁 File watch error: {}", error)
                    }
                }
            })
            .map_err(|e| anyhow::anyhow!("Failed to create file watcher: {}", e))?;

        // 5. 开始监听源目录（递归监听所有子目录）
        watcher
            .watch(&task.source, RecursiveMode::Recursive)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to watch directory '{}': {}",
                    task.source.display(),
                    e
                )
            })?;

        info!(
            "Started watching: {} → {}",
            task.source.display(),
            task.target.display()
        );

        // 6. 主事件循环：接收文件变化事件并处理
        loop {
            // --- 防抖机制开始 ---
            // 我们希望：用户连续修改文件时，只在“最后一次修改后 delay_ms 毫秒”才同步一次

            // 6.1 等待第一个文件变化事件
            if rx.recv().await.is_none() {
                info!("Watcher channel closed, exiting...");
                break; // channel 被关闭，退出循环（通常是程序终止）
            }

            debug!(
                "Change detected, starting debounce period of {}ms...",
                delay_ms
            );

            // 6.2 进入防抖等待状态
            //     使用一个内层循环，持续检查是否有新事件到来
            loop {
                // 尝试在 `delay_ms` 毫秒内接收下一个事件
                // 如果收到新事件，说明用户还在修改，需要“重置”防抖计时器
                match tokio::time::timeout(Duration::from_millis(delay_ms), rx.recv()).await {
                    Ok(Some(_)) => {
                        // 又有新事件！说明文件还在被修改，重新开始等待
                        debug!("Another change detected, restarting debounce timer...");
                        continue; // 继续等待
                    }
                    Ok(None) => {
                        // channel 被关闭（发送端关闭）
                        info!("Watcher channel closed during debounce.");
                        return Ok(()); // 正常退出
                    }
                    Err(_) => {
                        // timeout 超时！说明在 delay_ms 毫秒内没有新事件
                        // 👉 这正是我们想要的：用户已经“停止”修改文件
                        debug!("Debounce period ended with no further changes.");
                        break; // 跳出内层循环，准备执行同步
                    }
                }
            }
            // --- 防抖机制结束 ---

            // 7. 执行同步操作
            debug!("📁 Detected stable changes → syncing...");
            match sync_directories(&task.source, &task.target, &options).await {
                Ok(()) => {
                    debug!("✅ Sync completed successfully");
                }
                Err(e) => {
                    error!(
                        error = ?e,
                        source = %task.source.display(),
                        target = %task.target.display(),
                        "Sync failed during watch"
                    );
                }
            }

            // 8. 同步完成，回到外层循环，继续等待下一次变化
        }

        Ok(())
    }
}

// ==============================================
// 公共接口导出（供 main.rs 调用）
// ==============================================

use crate::sync::file_ops::compute_blake3_hash;
pub use file_ops::copy_file;
pub use filter::{should_exclude, should_sync};
pub use scanner::scan_directory;
pub use sync_logic::{SyncOptions, sync_directories};
pub use watcher::watch_task;
