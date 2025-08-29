# SyncBox 📦

`SyncBox` 是一个轻量、可靠的文件同步工具，支持单向增量同步、排除规则和可选的删除同步。适用于备份、部署、开发同步等场景。

✨ 特性：
- ✅ 单次同步 & 持续监听模式
- ✅ 增量同步（仅复制新增/修改的文件）
- ✅ 支持 glob 风格排除规则（`.tmp`, `Secret/` 等）
- ✅ 可选：同步删除目标目录中多余的文件
- ✅ 干运行模式（`--dry-run`），预演操作
- ✅ TOML 配置，支持多任务管理

---

## 🚀 快速开始

### 1. 安装

```bash
# 假设你已发布到 crates.io
cargo install syncbox

# 或从源码构建
git clone https://github.com/yourname/syncbox.git
cd syncbox
cargo install --path .