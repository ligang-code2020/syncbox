use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

// 我们使用 TOML 作为配置文件格式
// 这个结构体表示整个 syncbox.toml 文件的内容
#[derive(Deserialize, Debug)]
pub struct Config {
    // sync 是一个数组，可以定义多个同步任务
    pub sync: Vec<SyncTask>,
}

// 每个同步任务的定义
#[derive(Deserialize, Debug)]
pub struct SyncTask {
    // 任务名称，比如 "my-docs"
    pub name: String,

    // 源目录路径
    pub source: PathBuf,

    // 目标目录路径
    pub target: PathBuf,

    // 可选：要排除的文件或目录（支持 glob 模式）
    #[serde(default)]
    pub exclude: Vec<String>,

    #[serde(default = "default_delete_extra")]
    pub delete_extra: bool,
}

fn default_delete_extra() -> bool {
    false
}

impl Config {
    // 从文件路径加载配置
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        // 1. 检查文件是否存在
        if !path.exists() {
            anyhow::bail!("Config file not found: {}", path.display());
        }

        // 2. 读取文件内容
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

        // 3. 解析 TOML
        let config: Config = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

        // 4. 返回配置
        Ok(config)
    }

    // 根据名称查找一个同步任务
    pub fn find_task(&self, name: &str) -> Option<&SyncTask> {
        self.sync.iter().find(|task| task.name == name)
    }
}
