use indicatif::{ProgressBar, ProgressStyle, ProgressState};
use std::fmt::Write;
use crate::utils::format::format_file_size;

pub fn create_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {bytes}/{total_bytes} [{bar:40.cyan/blue}] {bytes_per_sec} ({elapsed_precise} / {eta})"
        )
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                if state.pos() >= state.len().unwrap_or(0) {
                    write!(w, "0.0s").unwrap();
                } else {
                    write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap();
                }
            })
            .with_key("bytes", |state: &ProgressState, w: &mut dyn Write| {
                write!(w, "{}", format_file_size(state.pos())).unwrap();
            })
            .with_key("total_bytes", |state: &ProgressState, w: &mut dyn Write| {
                write!(w, "{}", format_file_size(state.len().unwrap_or(0))).unwrap();
            })
            .with_key("bytes_per_sec", |state: &ProgressState, w: &mut dyn Write| {
                let rate = state.per_sec();
                if rate.is_nan() || rate <= 0.0 {
                    write!(w, "-").unwrap();
                } else if rate < 1024.0 {
                    write!(w, "{:.1} B/s", rate).unwrap();
                } else if rate < 1024.0 * 1024.0 {
                    write!(w, "{:.1} KB/s", rate / 1024.0).unwrap();
                } else if rate < 1024.0 * 1024.0 * 1024.0 {
                    write!(w, "{:.1} MB/s", rate / (1024.0 * 1024.0)).unwrap();
                } else {
                    write!(w, "{:.1} GB/s", rate / (1024.0 * 1024.0 * 1024.0)).unwrap();
                }
            })
            .progress_chars("#>-")
    );
    pb
}