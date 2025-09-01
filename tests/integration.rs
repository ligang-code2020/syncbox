

mod integration {
    // 包含所有集成测试模块
    mod sync_success; // 成功同步
    mod target_not_writable; // 文件只读

    mod file_permission_denied; // 权限不足

    mod source_not_found; // 源文件没有找到
}
