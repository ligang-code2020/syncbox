use std::time::Duration;
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use tracing::{debug, error, info};
use crate::infra::error::SyncError;
use super::sync_logic::sync_directories;
use super::report::SyncReport;
use super::types::SyncParameters;

// ==============================================
// 模块 5：监听器（Watcher）
// 文件系统监听，实时同步
// ==============================================


/// 启动文件系统监听器，实时同步源目录变更到目标目录。
///
/// 使用 `notify` crate 监听文件事件，支持防抖（debounce）机制。
///
/// # 参数
/// * `params` - 同步参数（源/目标路径、排除规则等）。
/// * `delay_ms` - 防抖延迟时间（毫秒），连续修改后等待此时间再触发同步。
///
/// # 返回
/// * `Ok(SyncReport)` - 累计所有同步操作的报告。
/// * `Err(SyncError)` - 监听器创建或同步过程中发生错误。
///
/// # 注意
/// - 通常不启用 `dry_run` 和 `checksum`（由调用方决定）。
/// - 监听 `Create`, `Modify`, `Remove` 事件。
/// - 使用异步通道与主循环通信。
pub async fn watch_task(
    params: &SyncParameters,
    delay_ms: u64,
) -> anyhow::Result<SyncReport, SyncError> {
    // let options = SyncOptions {
    //     dry_run: false, // watch 模式通常不是 dry_run
    //     excludes: params.excludes.clone(),
    //     delete_extra: params.delete_extra,
    //     checksum: false,
    //     delete_excludes: params.delete_excludes.clone(),
    // };

    let mut total_report = SyncReport::default(); // 累计所有同步的报告

    // 3. 创建一个异步 channel，用于从文件监听线程向主异步循环传递事件
    //    - unbounded_channel：不限制缓冲区大小，避免事件丢失
    //    - tx: 发送端（在监听回调中使用）
    //    - rx: 接收端（在主循环中使用）
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // 4. 创建文件系统监听器（watcher）
    //    `recommended_watcher` 会根据操作系统自动选择最优后端：
    //    - macOS: FSEvents
    //    - Linux: inotify
    //    - Windows: ReadDirectoryChangesW
    //
    //    回调函数会在后台线程中被调用，所以必须是 'static + Send
    let mut watcher =
        recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // 只关心三类事件：修改、创建、删除
                    // 忽略元数据变更（如访问时间）、重命名等，避免过度触发
                    match event.kind {
                        // 只处理文件内容修改和创建事件
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                            let _ = tx.send(event);
                        }
                        _ => {
                            debug!(event = ?event, "Ignored file system event");
                        }
                    }
                }
                Err(error) => {
                    // 监听过程中发生错误（如权限不足、路径不存在）
                    error!("📁 File watch error: {}", error)
                }
            }
        })
            .map_err(|e| anyhow::anyhow!("Failed to create file watcher: {}", e))?;

    // 5. 开始监听源目录（递归监听所有子目录）
    watcher
        .watch(&params.source, RecursiveMode::Recursive)
        .map_err(|e| {
            anyhow::anyhow!(
                    "Failed to watch directory '{}': {}",
                    params.source.display(),
                    e
                )
        })?;

    info!(
            "Started watching: {} → {}",
            params.source.display(),
            params.target.display()
        );

    // 6. 主事件循环：接收文件变化事件并处理
    loop {
        // --- 防抖机制开始 ---
        // 我们希望：用户连续修改文件时，只在“最后一次修改后 delay_ms 毫秒”才同步一次

        // 6.1 等待第一个文件变化事件
        if rx.recv().await.is_none() {
            info!("Watcher channel closed, exiting...");
            break; // channel 被关闭，退出循环（通常是程序终止）
        }

        debug!(
                "Change detected, starting debounce period of {}ms...",
                delay_ms
            );

        // 6.2 进入防抖等待状态
        //     使用一个内层循环，持续检查是否有新事件到来
        loop {
            // 尝试在 `delay_ms` 毫秒内接收下一个事件
            // 如果收到新事件，说明用户还在修改，需要“重置”防抖计时器
            match tokio::time::timeout(Duration::from_millis(delay_ms), rx.recv()).await {
                Ok(Some(_)) => {
                    // 又有新事件！说明文件还在被修改，重新开始等待
                    debug!("Another change detected, restarting debounce timer...");
                    continue; // 继续等待
                }
                Ok(None) => {
                    // channel 被关闭（发送端关闭）
                    info!("Watcher channel closed during debounce.");
                    return Ok(total_report); // 正常退出
                }
                Err(_) => {
                    // timeout 超时！说明在 delay_ms 毫秒内没有新事件
                    // 👉 这正是我们想要的：用户已经“停止”修改文件
                    debug!("Debounce period ended with no further changes.");
                    break; // 跳出内层循环，准备执行同步
                }
            }
        }
        // --- 防抖机制结束 ---

        // 7. 执行同步操作
        debug!("📁 Detected stable changes → syncing...");
        match sync_directories(&params).await {
            Ok(report) => {
                debug!("✅ Sync completed successfully");
                total_report.copied.extend(report.copied);
                total_report.errors.extend(report.errors);
            }
            Err(e) => {
                error!(
                        error = ?e,
                        source = %params.source.display(),
                        target = %params.target.display(),
                        "Sync failed during watch"
                    );
                total_report
                    .errors
                    .push((params.source.clone(), e.to_string()));
            }
        }

        // 8. 同步完成，回到外层循环，继续等待下一次变化
    }

    Ok(total_report)
}