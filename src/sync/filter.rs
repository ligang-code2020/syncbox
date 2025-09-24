use super::types::{FileInfo};
use std::path::{Path};
use std::sync::Arc;
use glob::Pattern;
use tracing::debug;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use parking_lot::Mutex; // 轻量级锁，比 std::sync::Mutex 快

// 缓存：key = 排除规则的排序后拼接字符串，value = 编译好的 matcher
static EXCLUDE_MATCHER_CACHE: Lazy<Mutex<HashMap<String, Arc<ExcludeMatcher>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// ==============================================
// 模块 2：过滤器（Filter）
// 负责判断文件是否应被排除或同步
// ==============================================

#[derive(Debug, Clone)]
pub struct ExcludeMatcher {
    patterns: Vec<Pattern>
}

impl ExcludeMatcher {
    pub fn new(exclude_patterns: &[String]) -> anyhow::Result<Self> {
        let mut compiled = Vec::with_capacity(exclude_patterns.len());
        for pat in exclude_patterns {
            let glob_pat = Pattern::new(pat)
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern '{}': {}", pat, e))?;
            compiled.push(glob_pat);
        }

        debug!(patterns = ?exclude_patterns, "Compiled {} exclude patterns", compiled.len());

        Ok(Self {
            patterns: compiled
        })
    }

    /// 检查 path 是否应被排除
    /// path: 绝对路径
    /// root: 源根目录（用于计算相对路径）
    pub fn is_excluded(&self, path: &Path, root: &Path) -> bool {
        // 1. 检查是否是系统文件（保持原有逻辑）
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name == ".DS_Store" || file_name.starts_with("._") {
                return true;
            }
        }

        // 2. 计算相对路径（只计算一次）
        let relative = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => return false, // 不在根目录下？不推荐，但安全处理
        };

        let relative_str = relative.to_string_lossy();

        // 3. 使用预编译的 glob pattern 快速匹配
        for pattern in &self.patterns {
            if pattern.matches_path(relative) || pattern.matches(&relative_str) {
                return true;
            }
        }

        false
    }
}



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
    if exclude_patterns.is_empty() {
        return false;
    }

    // 生成缓存 key：排序 + 拼接，确保相同规则集命中同一缓存
    let mut key_parts = exclude_patterns.to_vec();
    key_parts.sort_unstable();
    let cache_key = key_parts.join("|");

    // 从缓存获取或创建 matcher
    let matcher = {
        let mut cache = EXCLUDE_MATCHER_CACHE.lock();
        cache.entry(cache_key.clone()).or_insert_with(|| {
            match ExcludeMatcher::new(&exclude_patterns) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    tracing::error!(error = ?e, patterns = ?exclude_patterns, "Failed to compile exclude patterns");
                    // 如果编译失败，退化为不排斥（安全策略）
                    Arc::new(ExcludeMatcher { patterns: vec![] })
                }
            }
        }).clone()
    };

    matcher.is_excluded(path, root)
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