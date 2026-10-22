use std::path::{Path, PathBuf};
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub ready: bool,
    pub message: String,
}
#[cfg(test)]
mod tests {
    #[test]
    fn download_space_and_tool_integrity_checks() {
        assert!(super::verify_digest("mpv", "tampered").is_err());
        assert!(super::check_available_space(200, 100).is_ok());
        assert!(super::check_available_space(99, 100)
            .unwrap_err()
            .contains("磁盘空间不足"));
        assert!(super::verify_digest("yt-dlp", "tampered").is_err());
        assert!(super::verify_digest(
            "yt-dlp",
            "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a"
        )
        .is_ok());
    }
}

pub fn locate(resource: Option<&Path>, name: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(debug_assertions)]
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/media-tools"));
    if let Some(root) = resource {
        roots.insert(0, root.join("resources/media-tools"));
        roots.insert(0, root.join("media-tools"));
    }
    roots
        .into_iter()
        .map(|root| root.join(format!("{name}.exe")))
        .find(|p| p.is_file())
}
pub fn require(resource: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    let path =
        locate(resource, name).ok_or_else(|| format!("工具缺失：{name}，请修复安装下载组件"))?;
    if matches!(name, "yt-dlp" | "deno" | "mpv") {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut file = std::fs::File::open(&path).map_err(|_| "工具无法读取")?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|_| "工具校验失败")?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        verify_digest(name, &format!("{:x}", hasher.finalize()))?;
    }
    Ok(path)
}
fn verify_digest(name: &str, digest: &str) -> Result<(), String> {
    let expected = match name {
        "yt-dlp" => "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a".to_owned(),
        "deno" => "e020f3e232bd16e33768dee528e5983349c962952051ced0a5d58ad42f5d9b33".to_owned(),
        "mpv" => {
            let manifest: serde_json::Value =
                serde_json::from_str(include_str!("../../resources/learning-components.json"))
                    .map_err(|_| "内核校验清单损坏")?;
            manifest["files"]
                .as_array()
                .and_then(|files| files.iter().find(|f| f["path"] == "media-tools/mpv.exe"))
                .and_then(|file| file["sha256"].as_str())
                .ok_or("内核校验清单缺少mpv摘要")?
                .to_owned()
        }
        _ => return Ok(()),
    };
    if digest == expected {
        Ok(())
    } else {
        Err(format!("工具校验失败：{name}，请修复安装；未执行此工具"))
    }
}
pub fn check_available_space(available: u64, required: u64) -> Result<(), String> {
    if available < required {
        Err("磁盘空间不足：释放空间后恢复任务".into())
    } else {
        Ok(())
    }
}
pub fn ensure_space(folder: &Path, required: u64) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let path = folder
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut available = 0;
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err("权限错误：无法读取下载目录的磁盘空间".into());
        }
        check_available_space(available, required)
    }
    #[cfg(not(windows))]
    {
        let _ = (folder, required);
        Err("下载功能当前仅支持 Windows".into())
    }
}
pub fn inspect(resource: Option<&Path>) -> Vec<ToolStatus> {
    ["yt-dlp", "ffmpeg", "ffprobe", "deno"]
        .into_iter()
        .map(|name| {
            let checked = require(resource, name);
            let failure = checked.as_ref().err().cloned();
            let path = checked.ok();
            let version = path
                .as_ref()
                .and_then(|p| {
                    crate::runtime::process::capture(
                        p,
                        &[if matches!(name, "ffmpeg" | "ffprobe") {
                            "-version"
                        } else {
                            "--version"
                        }
                        .into()],
                        std::time::Duration::from_secs(5),
                        &|| false,
                        &mut |_| {},
                    )
                    .ok()
                })
                .and_then(|s| s.lines().next().map(str::to_string));
            ToolStatus {
                name: name.into(),
                ready: version.is_some(),
                message: failure.unwrap_or_else(|| {
                    if version.is_some() {
                        "可用".into()
                    } else {
                        "工具启动失败".into()
                    }
                }),
                path,
                version,
            }
        })
        .collect()
}
