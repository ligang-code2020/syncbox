use std::fs;
use std::os::unix::fs::PermissionsExt;
use syncbox::sync::{SyncParameters, sync_directories};
use tempfile::TempDir;

#[tokio::test]
async fn test_source_file_permission_denied() {
    let temp_dir = TempDir::new().unwrap();
    let source = temp_dir.path().join("source");
    let target = temp_dir.path().join("target");

    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();

    let file = source.join("secret.txt");
    fs::write(&file, "top secret").unwrap();

    // 设置源文件为 000，无人可读
    let no_perm = fs::Permissions::from_mode(0o000);
    fs::set_permissions(&file, no_perm).unwrap();

    let params = SyncParameters {
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
    assert!(format!("{:?}", result.unwrap_err()).contains("Permission denied"));
}
