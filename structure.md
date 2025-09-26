```plaintext
syncbox/
├── src/
│   ├── main.rs                 # 程序入口，解析命令行参数并分发任务
│   ├── lib.rs                  # 模块导出声明
│   ├── cli/
│   │   └── mod.rs              # 命令行参数解析（基于clap）
│   ├── config/
│   │   └── mod.rs              # 配置文件解析（TOML格式）
│   ├── infra/
│   │   ├── mod.rs              # 基础设施模块导出
│   │   └── logging.rs          # 日志初始化配置
│   ├── sync/
│   │   ├── mod.rs              # 同步模块导出
│   │   ├── types.rs            # 同步相关数据结构定义（FileInfo、SyncParameters等）
│   │   ├── scanner.rs          # 目录扫描逻辑
│   │   ├── filter.rs           # 路径过滤逻辑（排除规则）
│   │   ├── file_ops.rs         # 文件操作（复制、删除、哈希计算）
│   │   ├── sync_logic.rs       # 核心同步逻辑
│   │   ├── watcher.rs          # 目录监听逻辑
│   │   └── report.rs           # 同步结果报告生成
│   └── utils/
│       ├── mod.rs              # 工具函数导出
│       ├── format.rs           # 格式化工具（文件大小格式化）
│       └── progress.rs         # 进度条工具
```