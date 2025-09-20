use std::process::Command;
use std::time::Duration;

pub fn check_sshpass() -> std::io::Result<()> {
    // 执行 `sshpass --version`，能成功运行就说明已安装
    if let Err(e) = Command::new("sshpass").arg("--version").output() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "未找到 sshpass 工具，请先安装：\n\
            - Ubuntu/Debian: sudo apt install sshpass\n\
            - macOS: brew install sshpass\n\
            - Windows: 通过 WSL 或 Cygwin 安装",
        ));
    }
    Ok(())
}


// 检查是否配置了 SSH 免密登录
pub fn test_ssh_keypair(remote_user: &str, remote_host: &str, remote_port: u16) -> bool {
    // 构建SSH命令，增加更多容错参数
    let output = Command::new("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")  // 自动接受新主机密钥
        .arg("-o")
        .arg("ConnectTimeout=5")                 // 延长超时时间
        .arg("-o")
        .arg("BatchMode=yes")                    // 非交互模式
        .arg("-o")
        .arg("PasswordAuthentication=no")        // 禁用密码认证（强制密钥）
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