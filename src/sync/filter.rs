use std::path::Path;
use super::types::{FileInfo};

// ==============================================
// 模块 2：过滤器（Filter）
// 负责判断文件是否应被排除或同步
// ==============================================


/// 判断指定路径是否应被排除在同步之外。
///
/// 根据排除规则列表和默认系统文件名进行匹配。
///
/// # 参数
/// * `path` - 待检查的绝对路径。
/// * `root` - 源目录根路径，用于计算相对路径。
/// * `exclude_patterns` - 用户定义的排除规则列表。
///
/// # 返回
/// * `true` - 该路径应被排除。
/// * `false` - 该路径应被包含。
///
/// # 规则说明
/// - 以 `/` 开头：匹配相对路径起始。
/// - 以 `/` 结尾：匹配目录。
/// - 含 `*`：视为通配符（转换为正则 `.*`）。
/// - 默认排除 macOS 系统文件。
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


/// 判断源文件是否需要同步到目标位置。
///
/// 根据是否启用校验和模式，选择不同比较策略。
///
/// # 参数
/// * `source_info` - 源文件信息。
/// * `target_info` - 目标文件信息（若不存在则为 `None`）。
/// * `checksum` - 是否启用内容哈希校验模式。
///
/// # 返回
/// * `true` - 需要同步（目标不存在、内容不同或时间更新）。
/// * `false` - 无需同步（目标存在且内容/时间一致）。
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