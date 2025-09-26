use std::fs;
use std::os::unix::fs::PermissionsExt;
use syncbox::sync::{ SyncParameters};
use syncbox::sync::sync_directories;
use tempfile::TempDir;

#[tokio::test]
async fn test_target_directory_not_writable() {
    let temp_dir = TempDir::new().unwrap();
    let source = temp_dir.path().join("source");
    let target = temp_dir.path().join("target");

    // 创建源文件
    fs::create_dir(&source).unwrap();
    fs::write(source.join("test.txt"), "hello").unwrap();

    // 创建目标目录
    fs::create_dir(&target).unwrap();

    // 设置目标目录为只读（其他用户不可写）
    let readonly_perm = fs::Permissions::from_mode(0o555); // r-xr-xr-x
    fs::set_permissions(&target, readonly_perm).unwrap();

    let params = SyncParameters{
        source,
        target,
        dry_run: false,
        checksum: false,
        excludes: vec![],
        delete_extra: false,
        delete_excludes: vec![],
        detail: false,
    };

    // 尝试同步
    let result = sync_directories(&params).await;

    // 验证：应该失败
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(format!("{:?}", err).contains("Only readable directory"));
}
