// ==============================================
// 新增模块：远程路径解析器
// 作用：判断一个字符串是否为远程路径，并解析为结构体
// ==============================================

use anyhow::{Result, anyhow};
use ssh2::Session;
use std::path::Path;
use std::net::TcpStream;
use std::io::prelude::*;

#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub user: String,
    pub host: String,
    pub path: String,
}

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

    // 建立 TCP 连接
    let tcp = TcpStream::connect(format!("{}:22", remote.host))?;

    // 创建 SSH session
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;

    // 认证（这里使用密码认证，你也可以使用密钥认证）
    // 注意：实际使用时应该从安全的地方获取密码，而不是硬编码
    session.userauth_password(&remote.user, "your_password_here")?;

    if !session.authenticated() {
        return Err(anyhow!("SSH authentication failed"));
    }

    // 创建 SFTP session
    let sftp = session.sftp()?;

    // 读取本地文件内容
    let content = tokio::fs::read(local_path).await?;

    // 创建远程文件并写入内容
    let mut remote_file = sftp.create(&remote.path)?;
    remote_file.write_all(&content)?;
    remote_file.flush()?;

    println!(
        "✅ 上传成功: {} → {}@{}:{}",
        local_path.display(),
        remote.user,
        remote.host,
        remote.path
    );

    // 关闭连接
    drop(sftp);
    session.disconnect(None, "Normal shutdown", None)?;

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