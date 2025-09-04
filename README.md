# SyncBoxV1 - 文件同步工具

`SyncBox` 是一个高效的目录同步工具，支持实时监听文件变化并自动同步，可通过命令行直接使用或通过配置文件管理多个同步任务。

✨ 功能特点：
- ✅ 即时同步：直接指定源目录和目标目录进行一次性同步
- ✅ 任务运行：基于配置文件中的任务定义执行同步操作
- ✅ 实时监听：监控目录变化并自动触发同步，支持防抖机制
- ✅ 排除规则：可配置文件 / 目录排除模式，跳过不需要同步的内容
- ✅ 可通过环境变量控制日志级别，方便调试和问题排查
- ✅ 试运行模式：预览同步操作而不实际修改文件

---

## 安装

```bash
# 从源码编译
git clone https://github.com/yourusername/syncbox.git
cd syncbox
cargo build --release
# 可执行文件将位于 target/release/syncbox

# 从本地路径安装
cargo install --path .
```


## 🚀 使用指南
```bash
# 直接同步两个目录
syncbox sync <源目录> <目标目录>

# 基于配置文件运行指定任务
syncbox run <任务名称> --config <配置文件路径>

# 监听指定任务并自动同步
syncbox watch <任务名称> --config <配置文件路径> --delay <防抖延迟毫秒数>

#  dry-run 模式（仅预览操作）
syncbox sync ./source ./target --dry-run

# 使用哈希校验（更精确但速度较慢）
syncbox sync ./source ./target --checksum
```


### 配置文件格式
```toml
[[sync]]
name = "documents"
source = "/home/user/Documents"
target = "/backup/Documents"
exclude = ["*.tmp", "/temp/"]

[[sync]]
name = "photos"
source = "/home/user/Pictures"
target = "/backup/Pictures"
exclude = [".DS_Store", "*.log"]
```



### 技术架构
* 项目采用模块化设计，主要包含以下组件：
* CLI 模块：处理命令行参数解析
* 配置模块：读取和解析 TOML 配置文件
* 同步核心：实现目录扫描、文件过滤、同步逻辑
* 监听模块：监控文件系统变化并触发同步
* 基础设施：错误处理和日志系统

### 许可证
MIT