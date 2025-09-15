use crate::infra::error::SyncError;
use crate::utils::{create_progress_bar, format_file_size};
use crate::{cli, config};
use notify::{Event, EventKind, RecursiveMode, Watcher, recommended_watcher};
use num_format::{Locale, ToFormattedString};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};
use chrono::Local;

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
                // 将通配符 * 转换为正则的 .*，支持 *.log 匹配所有 .log 后缀文件
                let regex_pattern = pattern.replace('*', ".*");
                if let Ok(regex) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
                    if regex.is_match(&relative_str) {
                        return true;
                    }
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

    /// 删除文件（安全删除，记录错误）
    ///
    /// # 参数
    /// - `path`: 要删除的文件路径
    ///
    /// # 注意
    /// 不会 panic，错误会返回或记录日志。

    pub async fn delete_extra_files(
        source: &PathBuf,
        target: &PathBuf,
        dry_run: bool,
        exclude: &[String],
        delete_exclude: &[String],
    ) -> anyhow::Result<(Vec<PathBuf>, Vec<PathBuf>, Vec<(PathBuf, String)>)> {
        use std::collections::HashSet;

        // 1. 扫描源目录，收集所有文件的相对路径（String）
        let source_files: HashSet<String> = scan_directory(source, exclude, false)?
            .into_iter()
            .filter_map(|info| {
                info.path
                    .strip_prefix(source)
                    .ok()
                    .map(|rel| rel.to_string_lossy().to_string())
            })
            .collect();

        // // 2. 递归遍历目标目录
        let mut to_delete = Vec::new();
        scan_target_for_deletion(
            target,
            target,
            &source,
            &source_files,
            exclude,
            delete_exclude,
            &mut to_delete,
        )
        .await?;

        // 收集删除结果
        let mut deleted = Vec::new();
        let mut would_delete = Vec::new();
        let mut delete_errors = Vec::new();

        // 3. 执行删除
        for path in &to_delete {
            if dry_run {
                would_delete.push(path.clone());
            } else {
                match tokio::fs::remove_file(path).await {
                    Ok(()) => {
                        deleted.push(path.clone());
                        would_delete.push(path.clone());
                    }
                    Err(e) => {
                        delete_errors.push((path.clone(), e.to_string()));
                    }
                }
            }
        }

        Ok((deleted, would_delete, delete_errors))
    }

    /// 递归扫描目标目录，找出需要删除的文件（源目录中没有）
    ///
    /// # 参数
    /// - `source_files`: 源目录的文件列表
    /// - `target_root`: 目标根目录
    /// - `excludes`: 排除规则
    ///
    /// # 返回
    /// - `Ok(Vec<PathBuf>)`: 可以安全删除的文件列表

    pub async fn scan_target_for_deletion(
        current: &PathBuf,
        target_root: &PathBuf,
        source_root: &PathBuf,
        source_files: &std::collections::HashSet<String>,
        exclude: &[String],
        delete_exclude: &[String],
        to_delete: &mut Vec<PathBuf>,
    ) -> std::io::Result<()> {
        let mut dir = tokio::fs::read_dir(current).await?;

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                // ✅ 使用 Box::pin 包装递归调用，引入间接层
                let future = scan_target_for_deletion(
                    &path,
                    target_root,
                    source_root,
                    source_files,
                    exclude,
                    delete_exclude,
                    to_delete,
                );
                Box::pin(future).await?;
            } else {
                if let Ok(rel_path) = path.strip_prefix(target_root) {
                    let rel_str = rel_path.to_string_lossy().to_string();
                    if !source_files.contains(&rel_str)
                        && !should_exclude(&path, source_root, exclude)
                        && !should_exclude(&path, target_root, delete_exclude)
                    // 应用删除排除
                    {
                        to_delete.push(path);
                    }
                }
            }
        }

        Ok(())
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

        // 1. 扫描源目录获取所有文件
        let source_files = scan_directory(&params.source, &options.excludes, options.checksum)
            .map_err(|e| anyhow::anyhow!("Failed to scan source directory -> {}", e))?;

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
                        // failed_to_copy += 1;
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
    pub async fn watch_task(
        params: &SyncParameters,
        delay_ms: u64,
    ) -> anyhow::Result<SyncReport, SyncError> {
        // let options = SyncOptions {
        //     dry_run: false, // watch 模式通常不是 dry_run
        //     excludes: params.excludes.clone(),
        //     delete_extra: params.delete_extra,
        //     checksum: false,
        //     delete_excludes: params.delete_excludes.clone(),
        // };

        let mut total_report = SyncReport::default(); // 累计所有同步的报告

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
                        // 只关心三类事件：修改、创建、删除
                        // 忽略元数据变更（如访问时间）、重命名等，避免过度触发
                        match event.kind {
                            // 只处理文件内容修改和创建事件
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                                let _ = tx.send(event);
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
            .watch(&params.source, RecursiveMode::Recursive)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to watch directory '{}': {}",
                    params.source.display(),
                    e
                )
            })?;

        info!(
            "Started watching: {} → {}",
            params.source.display(),
            params.target.display()
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
                        return Ok(total_report); // 正常退出
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
            match sync_directories(&params).await {
                Ok(report) => {
                    debug!("✅ Sync completed successfully");
                    total_report.copied.extend(report.copied);
                    total_report.errors.extend(report.errors);
                }
                Err(e) => {
                    error!(
                        error = ?e,
                        source = %params.source.display(),
                        target = %params.target.display(),
                        "Sync failed during watch"
                    );
                    total_report
                        .errors
                        .push((params.source.clone(), e.to_string()));
                }
            }

            // 8. 同步完成，回到外层循环，继续等待下一次变化
        }

        Ok(total_report)
    }
}

// ==============================================
// 模块 6：输出结果报告
// 统一输出同步、删除结果
// ==============================================
mod report {
    use super::*;
    use std::fmt::Write;

    /// 同步操作的结果报告
    #[derive(Debug, Default)]
    pub struct SyncReport {
        pub copied: Vec<PathBuf>,                  // 成功复制的文件
        pub errors: Vec<(PathBuf, String)>,        // 同步错误
        pub deleted: Vec<PathBuf>,                 // 成功删除的文件
        pub would_delete: Vec<PathBuf>,            // dry-run模式下待删除的文件
        pub delete_errors: Vec<(PathBuf, String)>, // 删除错误
    }

    /// 统一打印同步和删除的结果
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
}

// ==============================================
// 公共接口导出（供 main.rs 调用）
// ==============================================

use crate::sync::file_ops::compute_blake3_hash;
pub use file_ops::{copy_file, delete_extra_files};
pub use filter::{should_exclude, should_sync};
pub use report::{SyncReport, print_report};
pub use scanner::scan_directory;
pub use sync_logic::{SyncOptions, sync_directories};
pub use watcher::watch_task;
