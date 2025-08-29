use tracing_subscriber::{EnvFilter, fmt};

pub fn init_logger() {
    // 从 RUST_LOG 环境变量读取日志级别
    // 默认 info，可设置 RUST_LOG=debug 查看详细日志
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt().with_env_filter(filter).init();
}
