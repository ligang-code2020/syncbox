mod types;
mod scanner;
mod filter;
mod file_ops;
mod sync_logic;
mod watcher;
mod report;


pub use types::{FileInfo, SyncParameters};
pub use sync_logic::{sync_directories};
pub use watcher::{watch_task};
















