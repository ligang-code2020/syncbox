use std::process::Command;

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