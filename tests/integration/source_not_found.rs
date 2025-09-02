use std::path::Path;
use tempfile::TempDir;
use syncbox::sync::SyncOptions;
use syncbox::sync::sync_directories;

#[tokio::test]
async fn test_source_directory_not_found() {
    // let source = Path::new("/this/path/does/not/exist/abc123");
    // let target = Path::new("/tmp/should-not-be-created");

    let temp_dir = TempDir::new().unwrap();
    let source = temp_dir.path().join("/this/path/does/not/exist/abc123");
    let target = temp_dir.path().join("/tmp/should-not-be-created");

    let result = sync_directories(
        &source,
        &target,
        &SyncOptions {
            dry_run: false,
            excludes: vec![],
        },
    )
    .await;

    // 验证：应该失败
    assert!(result.is_err());

    // 可选：验证错误类型（如果你的 SyncError 有 Io 变体）
    let err = result.unwrap_err();
    assert!(format!("{:?}", err).contains("No such file or directory"));
    // 或更精确地匹配（取决于你的 error 类型设计）
}
