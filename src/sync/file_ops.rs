use std::fs;
use std::path::{Path, PathBuf};
use super::scanner::{scan_directory};
use super::filter::should_exclude;

// ==============================================
// 模块 3：文件操作（FileOps）
// 负责实际的文件复制、删除等操作
// ==============================================

/// 异步复制文件从源路径到目标路径。
///
/// 自动创建目标路径的父目录。支持 dry-run 模式。
///
/// # 参数
/// * `source` - 源文件路径。
/// * `target` - 目标文件路径。
/// * `dry_run` - 若为 `true`，仅模拟操作，不实际复制。
///
/// # 返回
/// * `Ok(())` - 复制成功或 dry-run 模式。
/// * `Err(std::io::Error)` - 创建目录或复制文件失败。

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

/// 计算指定文件的 BLAKE3 哈希值。
///
/// # 参数
/// * `path` - 要计算哈希的文件路径。
///
/// # 返回
/// * `Ok([u8; 32])` - 成功计算的 32 字节哈希值。
/// * `Err(std::io::Error)` - 文件打开或读取失败。
pub fn compute_blake3_hash(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().into())
}


/// 扫描目标目录，找出并删除“多余”文件（即源目录中不存在的文件）。
///
/// 支持排除规则和 dry-run 模式。
///
/// # 参数
/// * `source` - 源目录路径。
/// * `target` - 目标目录路径。
/// * `dry_run` - 是否仅模拟删除。
/// * `exclude` - 同步排除规则（也用于判断是否“多余”）。
/// * `delete_exclude` - 删除操作的额外排除规则。
///
/// # 返回
/// * `Ok((deleted, would_delete, delete_errors))`
///   - `deleted`: 实际删除的文件列表。
///   - `would_delete`: dry-run 模式下拟删除的文件列表。
///   - `delete_errors`: 删除失败的文件及错误信息。
/// * `Err(anyhow::Error)` - 扫描或删除过程中发生错误。
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


/// 递归遍历目标目录，收集待删除的“多余”文件路径。
///
/// 内部辅助函数，不对外暴露。
///
/// # 参数
/// * `current` - 当前遍历的目录路径。
/// * `target_root` - 目标目录根路径（用于计算相对路径）。
/// * `source_root` - 源目录根路径（用于排除判断）。
/// * `source_files` - 源目录中所有文件的相对路径集合。
/// * `exclude` - 同步排除规则。
/// * `delete_exclude` - 删除排除规则。
/// * `to_delete` - 输出参数，收集待删除路径。
///
/// # 返回
/// * `Ok(())` - 遍历完成。
/// * `Err(std::io::Error)` - 读取目录失败。
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