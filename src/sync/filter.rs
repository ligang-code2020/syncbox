use std::path::Path;
use super::types::{FileInfo};
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