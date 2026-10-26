//! One bounded frame from an already checked source; no arbitrary command or output path.
use crate::media_access::CheckedMediaFile;
use std::{path::Path, time::Duration};
pub(crate) fn sample_jpeg(
    ffmpeg: &Path,
    source: &CheckedMediaFile,
    at: u64,
    stop: &dyn Fn() -> bool,
) -> Result<String, String> {
    if at >= 604800000 || stop() {
        return Err("单帧时间无效或提取已停止".into());
    }
    let work = tempfile::tempdir().map_err(|_| "无法创建单帧临时目录")?;
    crate::runtime::tools::ensure_space(work.path(), 4 * 1024 * 1024)?;
    let path = work.path().join("frame.jpg");
    let args = vec![
        "-nostdin".into(),
        "-v".into(),
        "error".into(),
        "-ss".into(),
        format!("{:.3}", at as f64 / 1000.0),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        "scale=1024:1024:force_original_aspect_ratio=decrease".into(),
        "-n".into(),
        path.to_string_lossy().into_owned(),
    ];
    crate::runtime::process::capture(ffmpeg, &args, Duration::from_secs(30), stop, &mut |_| {})
        .map_err(|_| "视频单帧提取失败或已停止")?;
    if stop() {
        return Err("单帧提取已停止".into());
    }
    crate::ai::analyzer::jpeg(&path)
}
