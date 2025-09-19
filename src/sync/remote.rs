// ==============================================
// 新增模块：远程路径解析器
// 作用：判断一个字符串是否为远程路径，并解析为结构体
// ==============================================

use anyhow::Result;
use std::path::Path;
use tokio::process::Command;
use crate::utils::{create_progress_bar, format_file_size};

#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteFile {
    /// 相对于目标目录的路径，如 "subdir/file.txt"
    pub path: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（Unix 时间戳，秒）
    pub mtime: i64,
}

impl RemoteFile {
    /// 判断是否与本地文件“相同”（大小 + 修改时间）
    pub fn is_same_as_local(&self, local_path: &std::path::Path) -> bool {
        if let Ok(metadata) = std::fs::metadata(local_path) {
            let local_size = metadata.len();
            let local_mtime = metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            return self.size == local_size && self.mtime == local_mtime;
        }
        false
    }
}

async fn create_remote_directory(remote: &RemoteTarget) -> Result<()> {
    let cmd = format!(
        "mkdir -p {}",
        shell_escape::escape(remote.path.clone().into())
    );

    let output = Command::new("ssh")
        .arg("-i")  // 添加密钥参数
        .arg("~/.ssh/id_rsa")  // 你的密钥路径
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-p")
        .arg(remote.port.to_string())
        .arg(format!("{}@{}", remote.user, remote.host))
        .arg(&cmd)
        .output()
        .await?;



    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ 创建远程目录失败:\n{}", stderr));
    }
    Ok(())
}

pub async fn scan_remote_files(remote: &RemoteTarget) -> Result<Vec<RemoteFile>> {
    // 先尝试创建远程目录
    create_remote_directory(remote).await?;

    let remote_dir = &remote.path;

    // 构造 find 命令
    // -type f: 只找文件
    // -printf: 输出格式：相对路径\t大小\t修改时间（秒）\n
    let find_cmd = format!(
        "cd {} && find . -type f ! -name '.*' -printf '%P\\t%s\\t%T@\\n'",
        shell_escape::escape(remote_dir.into())
    );

    let output = Command::new("ssh")
        .arg("-i")  // 添加密钥参数
        .arg("~/.ssh/id_rsa")  // 你的密钥路径
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-p")
        .arg(remote.port.to_string())
        .arg(format!("{}@{}", remote.user, remote.host))
        .arg(&find_cmd)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 如果目录不存在，返回空列表（不是错误）
        if stderr.contains("No such file or directory") {
            return Ok(vec![]);
        }
        return Err(anyhow::anyhow!("❌ 扫描远程目录失败:\n{}", stderr));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in output_str.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            continue; // 跳过格式错误的行
        }

        let path = parts[0].to_string();
        let size = parts[1].parse::<u64>().unwrap_or(0);
        let mtime = parts[2]
            .split('.')
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        files.push(RemoteFile { path, size, mtime });
    }

    Ok(files)
}

pub fn parse_remote_target(target: &str) -> Option<RemoteTarget> {
    if !target.contains('@') || target.split(':').count() < 2 {
        return None;
    }

    let at_pos = target.find('@')?;
    let user = target[..at_pos].to_string();
    let rest = &target[at_pos + 1..];  // 格式："host:port:/path" 或 "host:/path"

    // 分割主机（含端口）和路径
    let (host_port_part, path) = {
        let colon_pos = rest.rfind(':')?;
        (&rest[..colon_pos], rest[colon_pos + 1..].to_string())
    };

    // 分割主机和端口（支持 "host:port" 或直接 "host"）
    let (host, port) = if host_port_part.contains(':') {
        let parts: Vec<&str> = host_port_part.splitn(2, ':').collect();
        (parts[0].to_string(), parts[1].parse().ok()?)  // 解析端口
    } else {
        (host_port_part.to_string(), 22)  // 默认端口22
    };

    Some(RemoteTarget { user, host, port, path })
}

pub async fn upload_file(
    local_path: &Path,
    remote: &RemoteTarget,
    dry_run: bool,
) -> Result<()> {
    let file_size = std::fs::metadata(local_path)?.len();

    if dry_run {
        println!(
            "INFO 📤 [模拟] 上传文件到远程: user={} host={} path={}",
            remote.user, remote.host, remote.path
        );
        return Ok(());
    }

    let remote_str = format!("{}@{}:{}", remote.user, remote.host, remote.path);

    println!("📤 正在上传: {} → {}", local_path.display(), remote_str);

    // 创建进度条
    let pb = create_progress_bar(file_size);
    pb.set_message(format!("上传 {}", local_path.file_name().unwrap_or_default().to_string_lossy()));


    let output = Command::new("scp")
        .arg("-i")  // 添加密钥参数
        .arg("~/.ssh/id_rsa")  // 你的密钥路径
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-P")  // 注意：之前的-p已修正为-P（大写）
        .arg(remote.port.to_string())
        .arg(local_path)
        .arg(&remote_str)
        .output()
        .await?;

    pb.finish_with_message(format!("上传完成 {}", format_file_size(file_size)));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ scp 上传失败:\n{}", stderr));
    }

    println!(
        "✅ 上传成功: {} → {}@{}:{}",
        local_path.display(),
        remote.user,
        remote.host,
        remote.path
    );

    Ok(())
}


pub async fn download_file(
    remote: &RemoteTarget,
    local_path: &Path,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "INFO 📥 [模拟] 从远程下载文件: user={} host={} path={} → {}",
            remote.user, remote.host, remote.path, local_path.display()
        );
        return Ok(());
    }

    let remote_str = format!("{}@{}:{}", remote.user, remote.host, remote.path);
    println!("📥 正在下载: {} → {}", remote_str, local_path.display());

    // 创建本地目录（如果不存在）
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output = Command::new("scp")
        .arg("-i")  // 添加密钥参数
        .arg("~/.ssh/id_rsa")  // 你的密钥路径
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-P")  // 注意：之前的-p已修正为-P（大写）
        .arg(remote.port.to_string())
        .arg(&remote_str)
        .arg(local_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ scp 下载失败:\n{}", stderr));
    }

    println!(
        "✅ 下载成功: {}@{}:{} → {}",
        remote.user, remote.host, remote.path, local_path.display()
    );
    Ok(())
}


pub async fn delete_remote_file(
    remote: &RemoteTarget,
    file_path: &str, // 远程文件的相对路径
    dry_run: bool,
) -> Result<()> {
    let full_remote_path = format!("{}/{}", remote.path, file_path);
    if dry_run {
        println!(
            "INFO 🗑️ [模拟] 删除远程文件: user={} host={} path={}",
            remote.user, remote.host, full_remote_path
        );
        return Ok(());
    }

    println!(
        "🗑️ 正在删除远程文件: {}@{}:{}",
        remote.user, remote.host, full_remote_path
    );

    // 构造 ssh 删除命令（使用 rm -f 避免文件不存在时报错）
    let cmd = format!(
        "rm -f {}",
        shell_escape::escape(full_remote_path.clone().into())
    );

    let output = Command::new("ssh")
        .arg("-i")  // 添加密钥参数
        .arg("~/.ssh/id_rsa")  // 你的密钥路径
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-p")
        .arg(remote.port.to_string())
        .arg(format!("{}@{}", remote.user, remote.host))
        .arg(&cmd)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ 删除远程文件失败:\n{}", stderr));
    }

    println!(
        "✅ 删除成功: {}@{}:{}",
        remote.user, remote.host, &full_remote_path
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remote_target() {
        let target = "admin@192.168.0.121:/Users/admin/Desktop/lgtest";
        let parsed = parse_remote_target(target);
        assert!(parsed.is_some(), "目标路径解析失败");
        let remote = parsed.unwrap();
        assert_eq!(remote.user, "admin");
        assert_eq!(remote.host, "192.168.0.121");
        assert_eq!(remote.path, "/Users/admin/Desktop/lgtest");
    }

    // #[test]
    // fn test_parse_local_path() {
    //     assert!(parse_remote_target("./local/path").is_none());
    //     assert!(parse_remote_target("/absolute/path").is_none());
    //     assert!(parse_remote_target("not@valid").is_none());
    //     assert!(parse_remote_target("user@host:without/slash").is_none());
    // }
}