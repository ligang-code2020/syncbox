use std::fs;
use syncbox::sync::SyncOptions;
use syncbox::sync::sync_directories;
use tempfile::TempDir;

#[tokio::test]
async fn test_sync_success() {
    // 1. 创建临时目录
    let temp_dir = TempDir::new().unwrap();
    let source = temp_dir.path().join("source");
    let target = temp_dir.path().join("target");

    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();

    // 2. 创建源文件
    let source_file = source.join("hello.txt");
    let content = "Hello, SyncBox!";
    fs::write(&source_file, content).unwrap();

    // 3. 执行同步
    let result = sync_directories(
        &source,
        &target,
        &SyncOptions {
            dry_run: false,
            excludes: vec![],
            checksum: true,
            delete_extra: false,
            delete_excludes: vec![],
        },
    )
    .await;

    // 4. 验证：同步应该成功
    assert!(result.is_ok(), "Sync failed: {:?}", result);

    // 5. 验证：目标文件存在
    let target_file = target.join("hello.txt");
    assert!(target_file.exists(), "Target file does not exist");

    // 6. 验证：文件内容正确
    let target_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(target_content, content, "File content mismatch");

    // 7. 验证：mtime 应该被保留（如果 syncbox 支持）
    // let source_meta = source_file.metadata().unwrap();
    // let target_meta = target_file.metadata().unwrap();
    // assert_eq!(source_meta.modified().unwrap(), target_meta.modified().unwrap());
}
