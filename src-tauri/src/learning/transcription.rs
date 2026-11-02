//! Optional local transcription. Source reference: Yingliu c866508 (MIT;
//! resources/YINGLIU-LICENSE.txt). No download or process execution here.
use super::models::TranscriptCue;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WhisperManifest {
    pub version: u32,
    pub whisper_version: String,
    pub model_revision: String,
    pub models: Vec<WhisperModel>,
    pub binary_hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub license_hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub media_hashes: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WhisperModel {
    pub id: String,
    pub label: String,
    pub filename: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
    pub source: String,
    pub license: String,
}

pub struct VerifiedTranscription {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub version: String,
    pub model_sha256: String,
    pub component_sha256: String,
    _files: Vec<File>,
}

pub struct ParsedTranscript {
    pub language: Option<String>,
    pub cues: Vec<TranscriptCue>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseTranscriptionRequest {
    pub course_id: String,
    pub model_id: String,
}

pub fn component_specs() -> Result<WhisperManifest, String> {
    serde_json::from_str(include_str!("../../resources/whisper-components.json"))
        .map_err(|_| "固定语音组件清单无效".into())
}

pub fn verify_components(
    tool_root: &Path,
    model_root: &Path,
    model_id: &str,
    stop: &dyn Fn() -> bool,
) -> Result<VerifiedTranscription, String> {
    verify_with_manifest(tool_root, model_root, model_id, &component_specs()?, stop)
}

pub(super) fn verify_with_manifest(
    tool_root: &Path,
    model_root: &Path,
    model_id: &str,
    manifest: &WhisperManifest,
    stop: &dyn Fn() -> bool,
) -> Result<VerifiedTranscription, String> {
    if manifest.version != 1
        || manifest.whisper_version != "1.9.1"
        || !manifest.binary_hashes.contains_key("whisper-cli.exe")
    {
        return Err("固定语音组件版本或入口无效".into());
    }
    let model = manifest
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or("语音模型选项无效，请选择清单中的模型")?;
    let (executable, mut files) = verify_tool_files(tool_root, manifest, stop)?;
    let (path, file) = verify_model_file(model_root, model, stop)?;
    if stop() {
        return Err("语音组件校验已取消".into());
    }
    files.push(file);
    use sha2::{Digest, Sha256};
    let component_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(manifest).map_err(|_| "组件清单无法编码")?)
    );
    Ok(VerifiedTranscription {
        executable,
        model: path,
        version: manifest.whisper_version.clone(),
        model_sha256: model.sha256.clone(),
        component_sha256,
        _files: files,
    })
}

pub(super) fn verify_tool_files(
    tool_root: &Path,
    manifest: &WhisperManifest,
    stop: &dyn Fn() -> bool,
) -> Result<(PathBuf, Vec<File>), String> {
    let mut files = Vec::new();
    let mut executable = None;
    for (name, sha) in &manifest.binary_hashes {
        let (path, file) = verified_file(tool_root, name, None, sha, stop)?;
        if name == "whisper-cli.exe" {
            executable = Some(path);
        }
        files.push(file);
    }
    // whisper.cpp discovers CPU backends by filename. Verifying only known
    // files would still let an extra ggml-*.dll enter the later process.
    for entry in std::fs::read_dir(tool_root).map_err(|_| "语音组件目录不可读")? {
        if stop() {
            return Err("语音组件校验已取消".into());
        }
        let entry = entry.map_err(|_| "语音组件目录项不可读")?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or("语音组件名称无效")?
            .to_ascii_lowercase();
        if (name.ends_with(".dll") || name.ends_with(".exe"))
            && !manifest.binary_hashes.contains_key(&name)
        {
            return Err("语音工具目录含未登记的可执行组件，未执行此工具".into());
        }
    }
    for (name, sha) in &manifest.license_hashes {
        files.push(verified_file(tool_root, name, None, sha, stop)?.1);
    }
    Ok((executable.ok_or("语音组件缺少入口")?, files))
}

pub(super) fn verify_model_file(
    model_root: &Path,
    model: &WhisperModel,
    stop: &dyn Fn() -> bool,
) -> Result<(PathBuf, File), String> {
    let (path, mut file) = verified_file(
        model_root,
        &model.filename,
        Some(model.bytes),
        &model.sha256,
        stop,
    )?;
    validate_model_header(&mut file)?;
    Ok((path, file))
}

pub(super) fn validate_model_header(file: &mut File) -> Result<(), String> {
    let mut header = [0u8; 44];
    file.read_exact(&mut header)
        .map_err(|_| "语音模型文件不完整")?;
    let value = |n: usize| u32::from_le_bytes(header[n * 4..n * 4 + 4].try_into().unwrap());
    if value(0) != 0x67676d6c
        || !(50000..=60000).contains(&value(1))
        || value(2) != 1500
        || ![384, 512, 768, 1024, 1280].contains(&value(3))
        || !(1..=32).contains(&value(9))
    {
        return Err("不是受支持的Whisper GGML语音模型".into());
    }
    Ok(())
}

pub(super) fn verified_file(
    root: &Path,
    name: &str,
    bytes: Option<u64>,
    sha: &str,
    stop: &dyn Fn() -> bool,
) -> Result<(PathBuf, File), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || sha.len() != 64
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("语音组件路径或摘要不安全".into());
    }
    verified_local_file(&root.join(name), bytes, sha, stop)
}
pub(super) fn verified_local_file(
    path: &Path,
    bytes: Option<u64>,
    sha: &str,
    stop: &dyn Fn() -> bool,
) -> Result<(PathBuf, File), String> {
    let raw = path.to_str().ok_or("语音组件路径无效")?;
    let normal = raw.strip_prefix(r"\\?\").unwrap_or(raw);
    if !path.is_absolute()
        || normal.starts_with(['\\', '/'])
        || normal.get(2..).is_some_and(|p| p.contains(':'))
        || raw.chars().any(char::is_control)
        || sha.len() != 64
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("语音组件路径或摘要不安全".into());
    }
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("语音组件名称无效")?;
    if stop() {
        return Err("语音组件校验已取消".into());
    }
    crate::media_access::reject_links(path)?;
    let path = path
        .canonicalize()
        .map_err(|_| format!("语音组件缺失：{name}；请先选择安装组件或导入模型"))?;
    crate::media_access::reject_links(&path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let file = options
        .open(&path)
        .map_err(|_| format!("语音组件正在写入或不可读：{name}"))?;
    let meta = file.metadata().map_err(|_| "语音组件信息不可读")?;
    if !meta.is_file()
        || meta.len() == 0
        || bytes.is_some_and(|n| n != meta.len())
        || (bytes.is_none() && meta.len() > 256 * 1024 * 1024)
    {
        return Err(format!("语音组件长度无效：{name}"));
    }
    // The read-only handle stays alive through the later child process. Hashing
    // uses the existing bounded/cancellable reader, not metadata or sampled bytes.
    let (_, actual) = crate::file_digest::digest_file_controlled(&path, stop)?;
    if !actual.eq_ignore_ascii_case(sha) {
        return Err(format!("语音组件SHA256校验失败：{name}；未执行此组件"));
    }
    Ok((path, file))
}

pub fn parse_whisper_json(
    bytes: &[u8],
    subtitle_id: &str,
    duration_ms: u64,
) -> Result<ParsedTranscript, String> {
    if bytes.len() > 32 * 1024 * 1024 || duration_ms == 0 || duration_ms > 604800000 {
        return Err("转写结果超过32MiB或媒体真实时长无效，未截断导入".into());
    }
    uuid::Uuid::parse_str(subtitle_id).map_err(|_| "字幕ID无效")?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Whisper转写JSON无效或不完整")?;
    let rows = value
        .get("transcription")
        .and_then(|v| v.as_array())
        .ok_or("Whisper未返回带时间码的转写")?;
    if rows.is_empty() || rows.len() > 200000 {
        return Err("转写没有有效内容或超过200000段，未截断导入".into());
    }
    let language = match value.pointer("/result/language") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let text = v
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 16
                        && s.bytes().all(|c| c.is_ascii_alphabetic() || c == b'-')
                })
                .ok_or("转写语言标识无效")?;
            Some(text.to_owned())
        }
    };
    let mut cues: Vec<TranscriptCue> = Vec::with_capacity(rows.len());
    for row in rows {
        let start = row
            .pointer("/offsets/from")
            .and_then(|v| v.as_u64())
            .ok_or("转写起始时间必须为非负整数毫秒")?;
        let end = row
            .pointer("/offsets/to")
            .and_then(|v| v.as_u64())
            .ok_or("转写结束时间必须为非负整数毫秒")?;
        let text = row
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or("转写文本无效")?;
        if end <= start
            || end > duration_ms
            || cues.last().is_some_and(|c| c.start_ms > start)
            || text.trim().is_empty()
            || text.len() > 16000
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("转写时间、顺序或正文无效，未夹断时间或跳过坏行".into());
        }
        cues.push(TranscriptCue {
            id: format!("{subtitle_id}:{}", cues.len()),
            subtitle_id: subtitle_id.into(),
            start_ms: start,
            end_ms: end,
            text: text.into(),
        });
    }
    Ok(ParsedTranscript { language, cues })
}

pub fn transcript_srt(transcript: &ParsedTranscript) -> Result<Vec<u8>, String> {
    fn time(ms: u64) -> String {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3600000,
            ms / 60000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    }
    let mut bytes = String::new();
    for (n, cue) in transcript.cues.iter().enumerate() {
        use std::fmt::Write;
        writeln!(
            &mut bytes,
            "{}\n{} --> {}\n{}\n",
            n + 1,
            time(cue.start_ms),
            time(cue.end_ms),
            cue.text
        )
        .map_err(|_| "转写字幕编码失败")?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("转写字幕超过8MiB，未截断导入".into());
        }
    }
    let first = transcript.cues.first().ok_or("转写字幕为空")?;
    let parsed =
        super::subtitles::parse(bytes.as_bytes(), "srt", &first.subtitle_id, Some(604800000))?;
    if parsed.len() != transcript.cues.len()
        || parsed
            .iter()
            .zip(&transcript.cues)
            .any(|(a, b)| a.start_ms != b.start_ms || a.end_ms != b.end_ms || a.text != b.text)
    {
        return Err("正文不能无损表示为字幕，未修改或丢弃内容".into());
    }
    Ok(bytes.into_bytes())
}
