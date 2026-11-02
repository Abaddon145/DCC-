use std::path::{Path, PathBuf};

/// Read only library-owned durable learning material, never runtime/grant paths.
pub fn learning_file(base: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.contains(':') || relative.chars().any(char::is_control) {
        return Err("学习资料路径不安全".into());
    }
    if !relative.starts_with("learning-data/") {
        return Err("不是学习资料路径".into());
    }
    crate::media_access::reject_links(base)?;
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = crate::media::safe_relative(&base, relative)?;
    crate::media_access::reject_links(&path)?;
    let path = path.canonicalize().map_err(|_| "学习资料缺失")?;
    if !path.starts_with(base.join("learning-data")) || !path.is_file() {
        return Err("学习资料不在当前素材库内".into());
    }
    Ok(path)
}
