// ==============================================
// 新增模块：远程路径解析器
// 作用：判断一个字符串是否为远程路径，并解析为结构体
// ==============================================

use crate::utils::{create_progress_bar, format_file_size};
use anyhow::{Result, anyhow};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tokio::process::Command;
use tracing::{error, info, warn};

/// 检查系统是否安装了 sshpass
fn check_sshpass_installed() -> bool {
    StdCommand::new("which")
        .arg("sshpass")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn test_ssh_keypair(remote_user: &str, remote_host: &str, remote_port: u16) -> bool {
    // 构建SSH命令，增加更多容错参数
    let output = std::process::Command::new("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new") // 自动接受新主机密钥
        .arg("-o")
        .arg("ConnectTimeout=5") // 延长超时时间
        .arg("-o")
        .arg("BatchMode=yes") // 非交互模式
        .arg("-o")
        .arg("PasswordAuthentication=no") // 禁用密码认证（强制密钥）
        .arg("-p")
        .arg(remote_port.to_string())
        .arg(format!("{}@{}", remote_user, remote_host))
        .arg("exit 0")
        .output();

    match output {
        Ok(output) => {
            // 同时检查退出码和错误输出（避免因其他错误导致误判）
            output.status.success() && output.stderr.is_empty()
        }
        Err(_) => false,
    }
}

/// 构建带认证的SSH命令
fn build_ssh_command(
    remote: &RemoteTarget,
    password: Option<&str>,
) -> Result<Command, anyhow::Error> {
    // 根据认证方式构建基础命令
    let mut command = if let Some(pwd) = password {
        // 密码认证：使用sshpass
        if !check_sshpass_installed() {
            return Err(anyhow::anyhow!(
                "需要安装 sshpass 以使用密码认证，请执行：\n\
            - Ubuntu/Debian: sudo apt install sshpass\n\
            - macOS: brew install sshpass\n\
            - Windows: 通过 WSL 或 Cygwin 安装",
            ));
        }
        let mut cmd = Command::new("sshpass");
        cmd.arg("-p").arg(pwd).arg("ssh");
        cmd
    } else {
        // 密钥认证或系统默认认证
        Command::new("ssh")
    };

    // 添加SSH密钥参数（如果指定）
    if let Some(key_path) = &remote.ssh_key_path {
        command.arg("-i").arg(key_path);
    }

    // 添加通用SSH选项
    command
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-p")
        .arg(remote.port.to_string())
        .arg(format!("{}@{}", remote.user, remote.host));

    Ok(command)
}

#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub ssh_key_path: Option<String>,
}

impl RemoteTarget {
    /// 解析目标字符串并处理认证（通用入口）
    pub fn resolve_and_auth(
        target: &str,
        password: Option<String>,
    ) -> Result<(Self, Option<String>)> {
        // 解析远程目标
        let remote = parse_remote_target(target)
            .ok_or_else(|| anyhow::anyhow!("无效的远程目标格式: {}", target))?;

        // 处理认证逻辑（复用之前实现的 handle_auth）
        let ssh_password = remote.handle_auth(password)?;

        Ok((remote, ssh_password))
    }

    /// 处理远程目标的认证逻辑（密码获取、免密验证）
    pub fn handle_auth(&self, password: Option<String>) -> Result<Option<String>> {
        // 优先从环境变量获取密码
        let ssh_password = std::env::var("SYNCBOX_SSH_PASSWORD").ok().or(password);

        if ssh_password.is_none() {
            // 无密码时验证免密登录
            self.verify_keypair_auth()?;
        }

        Ok(ssh_password)
    }

    /// 验证免密登录
    fn verify_keypair_auth(&self) -> Result<()> {
        warn!("未检测到SSH密码（环境变量或命令参数）");
        warn!("你是否已配置SSH免密登录？(y/n)");
        io::stdout().flush()?; // 确保提示语及时输出

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input == "y" || input == "yes" {
            if !test_ssh_keypair(&self.user, &self.host, self.port) {
                error!("免密登录验证失败！\n\
                        请手动验证免密是否生效 \n\
                        - ssh -p {} {}@{}", self.port, self.user, self.host
                );
                return Err(anyhow!("免密配置无效"));
            }
            info!("免密登录验证通过");
        } else {
            warn!(
                "请先配置免密登录或提供密码：\n\
                 - 方法1（推荐）：手动配置ssh免密\n\
                 - 方法2：使用环境变量提供密码：export SYNCBOX_SSH_PASSWORD=你的密码\n\
                 - 方法3：命令行指定密码：--password 你的密码"
            );
            return Err(anyhow!("未配置免密且未提供密码"));
        }

        Ok(())
    }
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
            let local_mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            return self.size == local_size && self.mtime == local_mtime;
        }
        false
    }
}

async fn create_remote_directory(remote: &RemoteTarget, password: Option<&str>) -> Result<()> {
    let cmd = format!(
        "mkdir -p {}",
        shell_escape::escape(remote.path.clone().into())
    );

    // 使用新的命令构建函数
    let mut command = build_ssh_command(remote, password)?;
    let output = command.arg(&cmd).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ 创建远程目录失败:\n{}", stderr));
    }
    Ok(())
}

pub async fn scan_remote_files(
    remote: &RemoteTarget,
    password: Option<&str>,
) -> Result<Vec<RemoteFile>> {
    // 先尝试创建远程目录
    create_remote_directory(remote, password).await?;
    let remote_dir = &remote.path;

    // 构造 find 命令
    let find_cmd = format!(
        "cd {} && find . -type f ! -name '.*' -printf '%P\\t%s\\t%T@\\n'",
        shell_escape::escape(remote_dir.into())
    );

    // 使用新的命令构建函数
    let mut command = build_ssh_command(remote, password)?;
    let output = command.arg(&find_cmd).output().await?;

    // 后续代码保持不变...
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
    // 分离密钥参数（格式：?key=~/path/to/key）
    let (target_part, ssh_key_path) = if target.contains("?key=") {
        let parts: Vec<&str> = target.splitn(2, "?key=").collect();
        (parts[0], Some(parts[1].to_string()))
    } else {
        (target, None) // 与 if 分支保持相同缩进
    };

    // 必须包含 @ 符号（用户名分隔符）
    let at_pos = target_part.find('@')?;
    let user = target_part[..at_pos].to_string();
    let rest = &target_part[at_pos + 1..]; // 剩余部分：host[:port]:path

    // 必须包含至少一个 : 作为 host 和 path 的分隔符
    let colon_pos = rest.rfind(':')?;
    let host_port_part = &rest[..colon_pos];
    let path = rest[colon_pos + 1..].to_string();

    // 解析主机和端口（格式：host 或 host:port）
    let (host, port) = if host_port_part.contains(':') {
        let hp_parts: Vec<&str> = host_port_part.splitn(2, ':').collect();
        if hp_parts.len() != 2 {
            return None; // 无效的端口格式
        }
        let port_num = hp_parts[1].parse().ok()?; // 端口必须是数字
        (hp_parts[0].to_string(), port_num)
    } else {
        (host_port_part.to_string(), 22) // 默认 SSH 端口
    };

    Some(RemoteTarget {
        user,
        host,
        port,
        path,
        ssh_key_path, // 新增：添加密钥路径
    })
}

pub async fn upload_file(
    local_path: &Path,
    remote: &RemoteTarget,
    dry_run: bool,
    ssh_password: Option<&str>,
) -> Result<()> {
    // 1. 验证本地文件是否存在
    if !local_path.exists() {
        return Err(anyhow!("❌ 本地文件不存在: {}", local_path.display()));
    }
    if !local_path.is_file() {
        return Err(anyhow!("❌ 不是文件: {}", local_path.display()));
    }

    // 2. 获取文件大小（用于进度条）
    let file_size = std::fs::metadata(local_path)
        .map_err(|e| anyhow!("❌ 无法获取文件信息: {}", e))?
        .len();

    // 3. 处理模拟运行
    if dry_run {
        println!(
            "📤 [模拟] 上传: {} → {}@{}:{}:{}",
            local_path.display(),
            remote.user,
            remote.host,
            remote.port,
            remote.path
        );
        return Ok(());
    }

    // 4. 构建远程目标路径字符串
    let remote_path = format!("{}@{}:{}", remote.user, remote.host, remote.path);
    println!(
        "📤 开始上传: {} ({})",
        local_path.display(),
        format_file_size(file_size)
    );

    // 5. 创建进度条
    let pb = create_progress_bar(file_size);
    pb.set_message(format!(
        "上传中: {}",
        local_path.file_name().unwrap_or_default().to_string_lossy()
    ));

    // 6. 构建scp命令（根据认证方式选择）
    let mut command = if let Some(pwd) = ssh_password {
        // 密码认证：使用sshpass包裹scp
        if !check_sshpass_installed() {
            return Err(anyhow::anyhow!(
                "需要安装 sshpass 以使用密码认证，请执行：\n\
            - Ubuntu/Debian: sudo apt install sshpass\n\
            - macOS: brew install sshpass\n\
            - Windows: 通过 WSL 或 Cygwin 安装",
            ));
        }
        let mut cmd = Command::new("sshpass");
        cmd.arg("-p").arg(pwd).arg("scp"); // 传递密码
        cmd
    } else {
        // 密钥认证或系统默认认证
        Command::new("scp")
    };

    // 7. 添加通用参数
    command
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new") // 自动接受新主机密钥
        .arg("-o")
        .arg("ConnectTimeout=10") // 10秒连接超时
        .arg("-P") // scp端口参数（大写P）
        .arg(remote.port.to_string());

    // 8. 添加SSH密钥参数（如果指定）
    if let Some(key_path) = &remote.ssh_key_path {
        command.arg("-i").arg(key_path);
    }

    // 9. 添加源文件和目标路径
    command.arg(local_path).arg(&remote_path);

    // 10. 执行上传命令
    let output = command
        .output()
        .await
        .map_err(|e| anyhow!("❌ 上传命令执行失败: {}", e))?;

    // 11. 更新进度条（完成）
    pb.finish_with_message(format!("上传完成: {}", format_file_size(file_size)));

    // 12. 处理命令执行结果
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 常见错误提示优化
        let error_msg = if stderr.contains("Permission denied") {
            "权限不足，请检查远程目录权限或认证信息".to_string()
        } else if stderr.contains("Connection refused") {
            format!(
                "连接被拒绝，请检查主机{}和端口{}是否可用",
                remote.host, remote.port
            )
        } else if stderr.contains("No such file or directory") {
            "远程目录不存在，请先创建目录".to_string()
        } else {
            stderr.to_string()
        };
        return Err(anyhow!("❌ 上传失败: {}", error_msg));
    }

    // 13. 上传成功
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
    password: Option<&str>, // 新增密码参数
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "INFO 📥 [模拟] 从远程下载文件: user={} host={} path={} → {}",
            remote.user,
            remote.host,
            remote.path,
            local_path.display()
        );
        return Ok(());
    }

    let remote_str = format!("{}@{}:{}", remote.user, remote.host, remote.path);
    println!("📥 正在下载: {} → {}", remote_str, local_path.display());

    // 创建本地目录（如果不存在）
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 构建scp命令
    let mut command = if let Some(pwd) = password {
        // 密码认证：使用sshpass
        if !check_sshpass_installed() {
            return Err(anyhow::anyhow!(
                "需要安装 sshpass 以使用密码认证，请执行：\n\
            - Ubuntu/Debian: sudo apt install sshpass\n\
            - macOS: brew install sshpass\n\
            - Windows: 通过 WSL 或 Cygwin 安装",
            ));
        }
        let mut cmd = Command::new("sshpass");
        cmd.arg("-p").arg(pwd).arg("scp");
        cmd
    } else {
        // 密钥认证
        Command::new("scp")
    };

    // 添加SSH密钥参数（如果指定）
    if let Some(key_path) = &remote.ssh_key_path {
        command.arg("-i").arg(key_path);
    }

    // 添加其他参数
    let output = command
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-P") // 注意：scp端口参数是大写P
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
        remote.user,
        remote.host,
        remote.path,
        local_path.display()
    );
    Ok(())
}

pub async fn delete_remote_file(
    remote: &RemoteTarget,
    path: &str,
    password: Option<&str>,
) -> Result<()> {
    let full_path = format!(
        "{}/{}",
        remote.path,
        shell_escape::escape(path.into())
    );
    let cmd = format!("rm -f {}", full_path);

    let mut command = build_ssh_command(remote, password)?;
    let output = command.arg(&cmd).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("❌ 删除远程文件失败: {} - {}", path, stderr));
    }
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
