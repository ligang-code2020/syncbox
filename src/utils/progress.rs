use indicatif::{ProgressBar, ProgressStyle, ProgressState};
use std::fmt::Write;
use crate::utils::format::format_file_size;

/// 创建统一风格的进度条
pub fn create_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:50.cyan/blue}] {bytes_human}/{total_bytes_human} ({eta})"
        )
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
            })
            .with_key("bytes_human", |state: &ProgressState, w: &mut dyn Write| {
                write!(w, "{}", format_file_size(state.pos() as u64)).unwrap()
            })
            .with_key("total_bytes_human", |state: &ProgressState, w: &mut dyn Write| {
                write!(w, "{}", format_file_size(state.len().unwrap() as u64)).unwrap()
            })
            .progress_chars("#>-")
    );
    pb
}