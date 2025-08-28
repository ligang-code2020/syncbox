use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
            log::debug!("Skipped (excluded): {}", path.display());
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

// 判断一个路径是否应该被排除
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
