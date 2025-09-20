use clap::Parser;
use syncbox::utils::{ check_sshpass, test_ssh_keypair};
use syncbox::{cli, infra, sync};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    check_sshpass()?;

    let args = cli::Args::parse();
    infra::logging::init_logger();

    match args.command {
        // ============ SYNC 模式 ============
        cli::Command::Sync {
            source,
            target,
            dry_run,
            checksum,
            delete,
            exclude,
            delete_exclude,
            detail,
            password,
        } => {
            let remote = match sync::remote::parse_remote_target(&target) {
                Some(remote) => remote,
                None => {
                    return Err(anyhow::anyhow!("无效的远程目标格式: {}", target));
                }
            };

            // 优先从环境变量获取密码，CLI 参数作为备选
            let ssh_password = std::env::var("SYNCBOX_SSH_PASSWORD").ok().or(password);
            let mut final_password = ssh_password.clone();


            if final_password.is_none() {
                eprintln!("\n未检测到SSH密码（环境变量或命令参数）");

                // 询问用户是否已配置免密
                let mut input = String::new();
                eprint!("你是否已配置SSH免密登录？(y/n) ");
                std::io::stdin().read_line(&mut input).map_err(|e| {
                    anyhow::anyhow!("读取输入失败: {}", e)
                })?;

                let input = input.trim().to_lowercase();
                if input == "y" || input == "yes" {
                    eprintln!("正在验证免密登录...");
                    if !test_ssh_keypair(&remote.user, &remote.host, remote.port) {
                        // 增加手动验证提示
                        eprintln!("\n❌ 免密登录验证失败！");
                        eprintln!("请手动验证免密是否生效：");
                        eprintln!("  ssh -p {} {}@{} exit", remote.port, remote.user, remote.host);
                        eprintln!("如果上述命令需要输入密码，则免密配置确实未生效");
                        eprintln!("配置方法：");
                        eprintln!("  1. 生成密钥对：ssh-keygen");
                        eprintln!("  2. 上传公钥：ssh-copy-id -p {} {}@{}", remote.port, remote.user, remote.host);
                        return Err(anyhow::anyhow!("免密配置无效"));
                    }
                    eprintln!("✅ 免密登录验证通过");
                } else {
                    // 用户确认未配置免密，提示配置方法
                    eprintln!("\n请先配置免密登录或提供密码：");
                    eprintln!("方法1（推荐）：配置免密");
                    eprintln!("  ssh-copy-id -p {} {}@{}", remote.port, remote.user, remote.host);
                    eprintln!("方法2：使用环境变量提供密码");
                    eprintln!("  export SYNCBOX_SSH_PASSWORD=你的密码");
                    eprintln!("方法3：命令行指定密码");
                    eprintln!("  --password 你的密码");
                    return Err(anyhow::anyhow!("未配置免密且未提供密码"));
                }
            }


            let params = sync::SyncParameters {
                source: source.clone(),
                target: target.clone(),
                dry_run,
                checksum,
                excludes: exclude.clone(),
                delete_extra: delete,
                delete_excludes: delete_exclude.clone(),
                detail,
                ssh_password,
            };

            info!("Sync: copying file {} → {}", source.display(), target);
            sync::sync_directories(&params).await?;
        }

        // ============ RUN TASK 模式 ============
        cli::Command::Run {
            name,
            config,
            dry_run,
            checksum, // 新增
            detail,
            password,
        } => {
            let ssh_password = std::env::var("SYNCBOX_SSH_PASSWORD").ok().or(password);

            // 1. 加载配置文件
            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            // 2. 查找任务
            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            info!(
                "Run: copying file {} → {}",
                &task.source.display(),
                &task.target
            );

            let mut params = sync::SyncParameters::from(task);
            params.dry_run = dry_run;
            params.checksum = checksum;
            params.detail = detail;
            params.ssh_password = ssh_password;

            // 调用统一核心逻辑
            sync::sync_directories(&params).await?;
        }

        // ============ WATCH 模式 ============
        cli::Command::Watch {
            name,
            config,
            dry_run,
            delay,
            checksum,
            detail,
            password,
        } => {
            let ssh_password = std::env::var("SYNCBOX_SSH_PASSWORD").ok().or(password);

            let config = syncbox::config::Config::from_file(&config)
                .map_err(|e| anyhow::anyhow!("Config error: {}", e))?;

            let task = config
                .find_task(&name)
                .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in config", name))?;

            info!(
                "Watch: copying file {} → {}",
                &task.source.display(),
                &task.target
            );

            let mut params = sync::SyncParameters::from(task);
            params.checksum = checksum; // 新增此行
            params.detail = detail;
            params.dry_run = dry_run;
            params.ssh_password = ssh_password;
            sync::watch_task(&params, delay).await?;
        }
    }

    Ok(())
}
