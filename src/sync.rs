use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use tracing::{info, debug, warn, error};
use crate::error::SyncError;
use crate::sync;

#[derive(Debug, Clone)]
pub struct FileInfo {
    // 文件目录
    pub path: PathBuf,
    // 系统时间
    pub mtime: SystemTime,
    // 文件大小
    pub size: u64,
}

impl FileInfo {
    /// 从路径创建 FileInfo
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(FileInfo {
            path: path.to_path_buf(),
            mtime: metadata.modified()?,
            size: metadata.len(),
        })
    }

    /// 比较两个文件，source 是否比 target 新
    pub fn is_newer_than(&self, target: &Self) -> bool {
        self.mtime > target.mtime
    }
}

/// 递归遍历目录，返回所有文件的 FileInfo
pub fn scan_directory<P: AsRef<Path>>(
    root: P,
    exclude_patterns: &[String],
) -> std::io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    let root = root.as_ref();

    if !root.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Source directory not found: {}", root.display()),
        ));
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => {
            // 👇 用户需要知道这个！
            eprintln!("❌️  Cannot read directory '{}': {}", root.display(), e);
            return Ok(files);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("⚠️  Cannot read entry in '{}': {}", root.display(), e);
                continue;
            }
        };

        let path = entry.path();

        // 👇 新增：检查是否应该排除
        if should_exclude(&path, root, exclude_patterns) {
            debug!("Skipped (excluded): {}", path.display());
            continue;
        }

        if path.is_dir() {
            match scan_directory(&path, exclude_patterns) {
                Ok(mut sub_files) => files.append(&mut sub_files),
                Err(e) => {
                    eprintln!("⚠️  Cannot scan subdirectory '{}': {}", path.display(), e);
                }
            }
        } else {
            match FileInfo::from_path(&path) {
                Ok(info) => files.push(info),
                Err(e) => {
                    eprintln!("⚠️  Cannot read file '{}': {}", path.display(), e);
                }
            }
        }
    }
    Ok(files)
}

/// 比较源和目标文件，决定是否需要同步
pub fn should_sync(source_info: &FileInfo, target_info: Option<&FileInfo>) -> bool {
    match target_info {
        None => true, // 目标不存在，需要同步
        Some(target) => source_info.is_newer_than(target),
    }
}

/// 复制文件（带目录创建）
pub fn copy_file(source: &Path, target: &Path, dry_run: bool) -> std::io::Result<()> {
    if dry_run {
        println!("💡 Would copy: {} → {}", source.display(), target.display());
        return Ok(());
    }

    // 创建目标目录
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    // 执行复制
    fs::copy(source, target)?;
    Ok(())
}

/// 判断一个路径是否应该被排除
pub fn should_exclude(path: &Path, root: &Path, exclude_patterns: &[String]) -> bool {
    // 我们需要将路径转换为“相对于 root 的路径”
    // 比如：/Users/you/syncbox-test/src/a.tmp → a.tmp
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
            if relative_str.starts_with(&*pattern) || relative_str.contains(&format!("/{}", pattern))
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

    false
}

/// 递归扫描目标目录，找出需要删除的文件
pub async fn delete_extra_files(
    source: &PathBuf,
    target: &PathBuf,
    dry_run: bool,
    exclude: &[String],
) -> anyhow::Result<()> {
    use std::collections::HashSet;

    // 1. 扫描源目录，收集所有文件的相对路径（String）
    let source_files: HashSet<String> = scan_directory(source, exclude)?
        .into_iter()
        .filter_map(|info| {
            info.path
                .strip_prefix(source)
                .ok()
                .map(|rel| rel.to_string_lossy().to_string()) // ✅ 在这里转成 String
        })
        .collect();

    // 2. 递归遍历目标目录
    let mut to_delete = Vec::new();
    scan_target_for_deletion(
        target,
        target,
        &source,
        &source_files,
        exclude,
        &mut to_delete,
    )
        .await?;

    // 3. 执行删除
    for path in &to_delete {
        if dry_run {
            println!("💡 Would delete: {}", path.display());
        } else {
            match tokio::fs::remove_file(path).await {
                Ok(()) => println!("🗑️  Deleted: {}", path.display()),
                Err(e) => eprintln!("❌ Failed to delete '{}': {}", path.display(), e),
            }
        }
    }

    Ok(())
}

async fn scan_target_for_deletion(
    current: &PathBuf,
    target_root: &PathBuf,
    source_root: &PathBuf,
    source_files: &std::collections::HashSet<String>,
    exclude: &[String],
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
                to_delete,
            );
            Box::pin(future).await?;
        } else {
            if let Ok(rel_path) = path.strip_prefix(target_root) {
                let rel_str = rel_path.to_string_lossy().to_string();
                if !source_files.contains(&rel_str)
                    && !sync::should_exclude(&path, source_root, exclude)
                {
                    to_delete.push(path);
                }
            }
        }
    }

    Ok(())
}


pub async fn sync_directories(
    source: &PathBuf,
    target: &PathBuf,
    dry_run: bool,
    exclude: &[String],
    delete_extra: bool,
) -> anyhow::Result<()> {
    // 1. 扫描源目录
    let source_files = scan_directory(&source, exclude)
        .map_err(|e| anyhow::anyhow!("Failed to scan source: {}", e))?;

    println!("📊 Found {} files in source", source_files.len());

    let mut copied = 0;
    let mut skipped = 0;
    let mut failed_to_copy = 0;

    // 2. 遍历每个源文件
    for source_info in &source_files {
        // 计算目标路径：/src/a/b.txt → /dst/a/b.txt
        let relative = source_info
            .path
            .strip_prefix(&source)
            .expect("File not under source root");
        let target_path = target.join(relative);

        // 获取目标文件信息（如果存在）
        let target_info = if target_path.exists() {
            sync::FileInfo::from_path(&target_path).ok()
        } else {
            None
        };

        // 判断是否需要同步
        if sync::should_sync(source_info, target_info.as_ref()) {
            match sync::copy_file(&source_info.path, &target_path, dry_run) {
                Ok(()) => {
                    if !dry_run {
                        copied += 1;
                    }
                }
                Err(e) => {
                    // 👇 用户必须看到这个！
                    eprintln!("❌ Failed to copy '{}': {}", source_info.path.display(), e);
                    failed_to_copy += 1;
                }
            }
        } else {
            skipped += 1;
        }
    }

    println!("✅ Sync complete!");
    println!("   Copied: {}", copied);
    println!("   Skipped (unchanged): {}", skipped);
    if dry_run {
        println!("   (Dry run mode)");
    }
    if failed_to_copy > 0 {
        eprintln!("❌ Failed to copy {} files.", failed_to_copy);
    }

    // 如果有复制失败，我们也可以考虑返回错误（可选）
    if failed_to_copy > 0 {
        anyhow::bail!("Failed to copy {} files", failed_to_copy);
    }

    if delete_extra {
        sync::delete_extra_files(&source, &target, dry_run, exclude).await?;
    }

    Ok(())
}

pub async fn watch_task(name: String, config_path: PathBuf, delay_ms: u64) -> anyhow::Result<()> {
    // 1. 加载配置文件
    info!("Loading config for task: {}", name);
    let config = crate::config::Config::from_file(&config_path)
        .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

    // 2. 查找指定名称的任务
    let task = config
        .find_task(&name)
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

    println!(
        "👀 Watching task '{}' ({} → {})",
        task.name,
        task.source.display(),
        task.target.display()
    );

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
    let mut watcher = recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                // 只关心三类事件：修改、创建、删除
                // 忽略元数据变更（如访问时间）、重命名等，避免过度触发
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        // 将事件发送到 channel
                        // 如果接收端已关闭（如程序退出），则忽略错误
                        let _ = tx.send(event);
                    }
                    _ => {
                        // 其他事件（如 Metadata、Access、Other）不处理
                        debug!("Ignored event: {:?}", event);
                    }
                }
            }
            Err(error) => {
                // 监听过程中发生错误（如权限不足、路径不存在）
                eprintln!("📁 File watch error: {}", error);
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

    info!("Started watching: {}", task.source.display());

    // 6. 主事件循环：接收文件变化事件并处理
    loop {
        // --- 防抖机制开始 ---
        // 我们希望：用户连续修改文件时，只在“最后一次修改后 delay_ms 毫秒”才同步一次

        // 6.1 等待第一个文件变化事件
        if rx.recv().await.is_none() {
            info!("Watcher channel closed, exiting...");
            break; // channel 被关闭，退出循环（通常是程序终止）
        }

        info!(
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
                    info!("Debounce period ended with no further changes.");
                    break; // 跳出内层循环，准备执行同步
                }
            }
        }
        // --- 防抖机制结束 ---

        // 7. 执行同步操作
        println!("📁 Detected stable changes → syncing...");
        match sync::sync_directories(
            &task.source,
            &task.target,
            false,
            &task.exclude,
            task.delete_extra,
        )
            .await
        {
            Ok(()) => {
                println!("✅ Sync completed successfully");
            }
            Err(e) => {
                eprintln!("❌ Sync failed: {}", e);
                // 注意：这里不返回错误，继续监听
                // 因为一次同步失败不应导致监听中断
            }
        }

        // 8. 同步完成，回到外层循环，继续等待下一次变化
    }

    Ok(())
}