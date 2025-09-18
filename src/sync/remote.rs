// ==============================================
// 新增模块：远程路径解析器
// 作用：判断一个字符串是否为远程路径，并解析为结构体
// ==============================================

use anyhow::Result;
use ssh2::Session;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;



#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub user: String,
    pub host: String,
    pub path: String,
}

/// 解析 "user@host:/remote/path" 格式（保持不变）
pub fn parse_remote_target(target: &str) -> Option<RemoteTarget> {
    if !target.contains('@') || !target.contains(':') {
        return None;
    }

    let at_pos = target.find('@')?;
    let colon_pos = target.rfind(':')?;

    if at_pos >= colon_pos {
        return None;
    }

    let user = target[..at_pos].to_string();
    let host = target[at_pos + 1..colon_pos].to_string();
    let path = target[colon_pos + 1..].to_string();

    Some(RemoteTarget { user, host, path })
}

/// 上传本地文件到远程服务器 —— 使用 ssh2（同步，但用 spawn_blocking 包装）
pub async fn upload_file(
    local_path: &Path,
    remote: &RemoteTarget,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!(
            "INFO 📤 [模拟] 上传文件到远程: user={} host={} path={}",
            remote.user, remote.host, remote.path
        );
        return Ok(());
    }

    // 在 tokio 线程池中运行同步代码
    tokio::task::spawn_blocking(move || {
        // 建立 TCP 连接
        let tcp = TcpStream::connect(format!("{}:22", remote.host))?;

        // 初始化 SSH 会话
        let mut sess = Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        // 使用本地 SSH 密钥认证（默认 ~/.ssh/id_rsa）
        sess.userauth_agent(&remote.user)?;

        // 如果密钥认证失败，可以改用密码（取消注释）：
        // sess.userauth_password(&remote.user, "your_password")?;

        if !sess.authenticated() {
            return Err(anyhow::anyhow!("SSH 认证失败"));
        }

        // 启动 SFTP
        let sftp = sess.sftp()?;

        // 读取本地文件
        let mut file = File::open(local_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // 创建远程文件并写入
        let mut remote_file = sftp.create(&remote.path)?;
        remote_file.write_all(&buffer)?;

        println!(
            "✅ 上传成功: {} → {}@{}:{}",
            local_path.display(),
            remote.user,
            remote.host,
            remote.path
        );

        Ok::<(), anyhow::Error>(())
    })
        .await??; // 注意：两个 ?，第一个解包 JoinHandle，第二个解包内部 Result

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remote_target() {
        let target = "alice@server1:/backups/photos";
        let parsed = parse_remote_target(target).unwrap();
        assert_eq!(parsed.user, "alice");
        assert_eq!(parsed.host, "server1");
        assert_eq!(parsed.path, "/backups/photos");
    }

    #[test]
    fn test_parse_local_path() {
        assert!(parse_remote_target("./local/path").is_none());
        assert!(parse_remote_target("/absolute/path").is_none());
        assert!(parse_remote_target("not@valid").is_none());
        assert!(parse_remote_target("user@host:without/slash").is_none());
    }
}