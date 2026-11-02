use super::transcription::{
    self, verified_file, verified_local_file, verify_model_file, verify_tool_files,
};
use super::transcription::{WhisperManifest, WhisperModel};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct WhisperPaths {
    pub tools: PathBuf,
    pub models: PathBuf,
}
impl WhisperPaths {
    pub fn new(config: &Path) -> Self {
        Self {
            tools: config.join("learning-tools/whisper-1.9.1"),
            models: config.join("learning-models"),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub bytes: u64,
    pub url: String,
    pub installed: bool,
    pub message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub version: String,
    pub model_revision: String,
    pub tool_ready: bool,
    pub tool_message: String,
    pub models: Vec<ModelStatus>,
}

pub fn inspect_components(config: &Path) -> Result<ComponentStatus, String> {
    let spec = transcription::component_specs()?;
    let paths = WhisperPaths::new(config);
    let tool = verify_tool_files(&paths.tools, &spec, &|| false);
    let tool_message = tool
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| "已通过完整校验".into());
    Ok(ComponentStatus {
        version: spec.whisper_version,
        model_revision: spec.model_revision,
        tool_ready: tool.is_ok(),
        tool_message,
        models: spec
            .models
            .into_iter()
            .map(|model| {
                let verified = verify_model_file(&paths.models, &model, &|| false);
                ModelStatus {
                    id: model.id,
                    label: model.label,
                    bytes: model.bytes,
                    url: model.url,
                    installed: verified.is_ok(),
                    message: verified.err().unwrap_or_else(|| "已通过完整校验".into()),
                }
            })
            .collect(),
    })
}
pub fn import_model(config: &Path, id: &str, source: &Path) -> Result<(), String> {
    let spec = super::transcription::component_specs()?
        .models
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("模型选项无效")?;
    import_model_with_spec(config, source, &spec)
}
pub(super) fn import_model_with_spec(
    config: &Path,
    source: &Path,
    spec: &WhisperModel,
) -> Result<(), String> {
    let (_, mut guard) = verified_local_file(source, Some(spec.bytes), &spec.sha256, &|| false)?;
    transcription::validate_model_header(&mut guard)?;
    let paths = WhisperPaths::new(config);
    let target = paths.models.join(&spec.filename);
    crate::media_access::reject_links(&target)?;
    if target.exists() {
        return verify_model_file(&paths.models, spec, &|| false).map(|_| ());
    }
    safe_directory(&paths.models)?;
    crate::runtime::tools::ensure_space(
        &paths.models,
        spec.bytes.saturating_add(16 * 1024 * 1024),
    )?;
    copy_verified(source, &target, Some(spec.bytes), &spec.sha256)?;
    verify_model_file(&paths.models, spec, &|| false).map(|_| ())
}
pub fn install_tools(config: &Path, source: &Path) -> Result<(), String> {
    install_with_manifest(config, source, &super::transcription::component_specs()?)
}
pub(super) fn install_with_manifest(
    config: &Path,
    source: &Path,
    spec: &WhisperManifest,
) -> Result<(), String> {
    if spec.license_hashes.is_empty() {
        return Err("工具许可清单缺失，未安装".into());
    }
    let paths = WhisperPaths::new(config);
    crate::media_access::reject_links(&paths.tools)?;
    if paths.tools.exists() {
        return verify_tool_files(&paths.tools, spec, &|| false).map(|_| ());
    }
    let parent = paths.tools.parent().ok_or("工具目录无效")?;
    safe_directory(parent)?;
    let stage = tempfile::Builder::new()
        .prefix("whisper-install-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    for (folder, files) in [
        ("tools", &spec.binary_hashes),
        ("licenses", &spec.license_hashes),
    ] {
        for (name, sha) in files {
            let (path, _guard) = verified_file(&source.join(folder), name, None, sha, &|| false)?;
            crate::runtime::tools::ensure_space(
                stage.path(),
                std::fs::metadata(&path)
                    .map_err(|e| e.to_string())?
                    .len()
                    .saturating_add(16 * 1024 * 1024),
            )?;
            copy_verified(&path, &stage.path().join(name), None, sha)?;
        }
    }
    drop(verify_tool_files(stage.path(), spec, &|| false)?);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<_> = stage
            .path()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let to: Vec<_> = paths
            .tools
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
        } == 0
        {
            return Err("工具发布失败或目标已存在，未覆盖原组件".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("当前工具仅支持Windows".into())
    }
}

pub(super) fn safe_directory(path: &Path) -> Result<(), String> {
    crate::media_access::reject_links(path)?;
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    crate::media_access::reject_links(path)
}
fn copy_verified(
    source: &Path,
    target: &Path,
    bytes: Option<u64>,
    sha: &str,
) -> Result<(), String> {
    let (_, mut guard) = verified_local_file(source, bytes, sha, &|| false)?;
    let parent = target.parent().ok_or("组件目标目录无效")?;
    crate::media_access::reject_links(target)?;
    let mut stage = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    std::io::copy(&mut guard, &mut stage).map_err(|_| "组件复制失败，请检查磁盘空间")?;
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    if crate::file_digest::digest_file(stage.path())?.1 != sha {
        return Err("组件复制后摘要不符，未启用".into());
    }
    crate::media_access::reject_links(target)?;
    stage
        .persist_noclobber(target)
        .map_err(|_| "组件目标已存在或无法写入，未覆盖".to_owned())?;
    Ok(())
}
