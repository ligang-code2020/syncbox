# SyncBox - 文件同步工具

`SyncBox` 是一个高效的目录同步工具，支持实时监听文件变化并自动同步，可通过命令行直接使用或通过配置文件管理多个同步任务。

✨ 功能特点：
- ✅ 即时同步：直接指定源目录和目标目录进行一次性同步
- ✅ 任务运行：基于配置文件中的任务定义执行同步操作
- ✅ 实时监听：监控目录变化并自动触发同步，支持防抖机制
- ✅ 排除规则：可配置文件 / 目录排除模式，跳过不需要同步的内容
- ✅ 安全操作：默认不删除额外文件，支持试运行模式预览操作
- ✅ 校验选项：支持基于文件修改时间的快速同步和哈希校验的精确同步
- ✅ 详细日志：可通过环境变量控制日志级别，方便调试和问题排查


---

## 📦 安装方式

### 方式 1：直接下载预编译二进制（推荐，无需 Rust 环境）
从 GitHub Releases 下载对应平台的压缩包，解压后即可使用

```bash
# 以 Linux 为例，下载并解压
curl -L https://github.com/ligang-code2020/syncbox/releases/download/v0.1.0/syncbox-linux-x86_64.tar.gz -o syncbox-linux.tar.gz
tar -zxvf syncbox-linux.tar.gz

# 赋予执行权限并全局安装
chmod +x syncbox
sudo mv syncbox /usr/local/bin/
```

### 方式 2：通过 Cargo 安装（Rust 开发者，直接拉取 crates.io 版本）
`cargo install syncbox `

### 方式 3：从源码编译（需 Rust 环境，适合自定义修改）
若需基于源码二次开发或验证最新代码，可通过 Git 克隆仓库后编译：

```bash
# 1. 克隆源码仓库
git clone https://github.com/ligang-code2020/syncbox.git
cd syncbox

# 2. 编译 Release 版本（优化编译，生成的二进制体积更小、运行更快）
cargo build --release

# 3. 编译产物路径：target/release/syncbox（可直接执行或手动移动到 PATH 目录）
# 手动全局安装示例：
sudo mv target/release/syncbox /usr/local/bin/
```

## 🚀 使用指南
### 直接同步两个目录
```bash
# 直接同步两个目录
# 基础用法
syncbox sync <源目录> <目标目录>

# 示例
syncbox sync ./source ./target
```

### 基于配置文件运行指定任务
```bash
syncbox run <任务名称> --config <配置文件路径>

# 示例
syncbox run documents --config ./syncbox.toml
```

### 监听指定任务并自动同步
```bash
syncbox watch <任务名称> --config <配置文件路径> --delay <防抖延迟毫秒数>

# 示例：监听 photos 任务，防抖延迟 1000ms
syncbox watch task --config ./syncbox.toml --delay 1000
```

### 高级选项
```bash
# 试运行模式（仅预览操作，不实际修改文件）
syncbox sync ./source ./target --dry-run

# 使用哈希校验（更精确但速度较慢）
syncbox sync ./source ./target --checksum

# 同步时删除目标目录中源目录不存在的文件
syncbox sync ./source ./target --delete

# 同步时删除目标目录中源目录不存在的文件
syncbox sync ./source ./target --delete --delete-exclude "important.txt"  # 即使--delete生效，也不删除important.txt

# 排除指定模式的文件
syncbox sync ./source ./target 
--exclude "*.tmp" # 匹配所有的.tmp结尾的文件
--exclude "temp" # 匹配temp文件夹所有文件

# 显示详细操作列表
syncbox sync ./source ./target --detail

# 显示详细操作列表
syncbox sync ./source ./target --detail
```
#### 🔍 排除规则说明
同步工具通过 `--exclude` 参数参数指定需要排除的文件或目录，规则如下：
1. 基础匹配逻辑 
   * 支持通配符 * 匹配任意字符（如 *.log 匹配所有 .log 后缀文件）
   * 路径匹配基于相对路径（相对于源目录的路径）
2. 目录排除
   * 不带斜杠 secret：匹配名为 secret 的文件或目录（包含子目录中的 secret）
   * 带斜杠 secret/：仅匹配路径中包含 secret/ 的目录（如 secret/file.txt 或 sub/secret/file.txt）
3. 绝对路径前缀匹配
   * 以 / 开头的模式（如 /temp/）：匹配从源目录根开始的路径（仅匹配 temp/ 目录下的内容，不匹配子目录中的 temp/）
4. 系统默认排除
   * 自动排除常见系统文件：.DS_Store、.fseventsd、.Trashes、.Spotlight-V100、.TemporaryItems 及 ._ 开头的文件


### 配置文件格式
```toml
[[sync]]
name = "task"
source = "/xx/xx/xx/syncbox/source"
target = "/xx/xx/xx/syncbox/target"
exclude = [".log"]
delete_extra = true
delete_extra_exclude = [".DS_Store"]
```

#### 配置项说明
* name：任务名称（唯一标识）
* source：源目录路径
* target：目标目录路径
* exclude：同步时需要排除的文件 / 目录模式列表
* delete_extra：是否删除目标目录中源目录不存在的文件（默认 false）
* delete_extra_exclude：即使 delete_extra 为 true 也不删除的文件 / 目录模式列表


### 技术架构
* 项目采用模块化设计，主要包含以下组件：
* CLI 模块：处理命令行参数解析
* 配置模块：读取和解析 TOML 配置文件
* 同步核心：实现目录扫描、文件过滤、同步逻辑
* 监听模块：监控文件系统变化并触发同步
* 基础设施：错误处理和日志系统

### 许可证
本项目采用 MIT 许可证，详情参见 `LICENSE` 文件。