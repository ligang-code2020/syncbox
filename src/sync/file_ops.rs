use std::fs;
use std::path::{Path, PathBuf};
use super::scanner::{scan_directory};
use super::filter::should_exclude;

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