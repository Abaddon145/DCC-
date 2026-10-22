//! One checked read boundary for managed files and explicitly approved external videos.
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalVideoPreview {
    pub preview_id: String,
    pub library_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub duration_ms: Option<u64>,
    pub expires_at: i64,
}
#[derive(Default)]
pub struct ExternalVideoRegistry {
    previews: HashMap<String, PendingVideo>,
}
struct PendingVideo {
    preview: ExternalVideoPreview,
    source: Fingerprint,
    info: crate::media_engine::probe::MediaInfo,
    committed: Option<String>,
    root: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Fingerprint {
    path: PathBuf,
    size: u64,
    modified: u128,
    identity: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
struct Grant {
    library: String,
    root: PathBuf,
    file_id: String,
    epoch: String,
    source: Fingerprint,
}
pub struct LocalMediaGrants {
    root: PathBuf,
}
impl LocalMediaGrants {
    pub fn new(config: &Path) -> Self {
        Self {
            root: config.join("learning-access"),
        }
    }
    pub fn local() -> Result<Self, String> {
        Ok(Self::new(
            &PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法定位本机访问批准")?)
                .join("DCCAssetLibrary"),
        ))
    }
    fn path(&self, base: &Path, file: &str) -> Result<PathBuf, String> {
        uuid::Uuid::parse_str(file).map_err(|_| "外部文件ID无效")?;
        let key = format!("{:x}", Sha256::digest(base.to_string_lossy().as_bytes()));
        Ok(self.root.join(key).join(format!("{file}.json")))
    }
    fn save(&self, grant: &Grant) -> Result<(), String> {
        let path = self.path(&grant.root, &grant.file_id)?;
        fs::create_dir_all(path.parent().ok_or("批准目录无效")?).map_err(|e| e.to_string())?;
        reject_links(&path)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
        temp.write_all(&serde_json::to_vec(grant).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[derive(Debug)]
pub struct CheckedMediaFile {
    path: PathBuf,
    _lease: Option<File>,
}
#[derive(PartialEq)]
pub struct PreparedMediaFile {
    path: PathBuf,
    expected: Option<Fingerprint>,
    epoch: Option<String>,
    checksum: String,
}
impl PreparedMediaFile {
    pub fn open_for_playback(&self, stop: &dyn Fn() -> bool) -> Result<CheckedMediaFile, String> {
        let (checked, current) = checked_source(&self.path, stop)?;
        if self
            .expected
            .as_ref()
            .is_some_and(|expected| expected != &current)
            || (!self.checksum.is_empty() && self.checksum != current.sha256)
        {
            return Err("SOURCE_CHANGED：原件已变化".into());
        }
        Ok(checked)
    }
    fn open_for_read(&self) -> Result<CheckedMediaFile, String> {
        if self.expected.is_some() {
            self.open_for_playback(&|| false)
        } else {
            Ok(CheckedMediaFile::managed(self.path.clone()))
        }
    }
    pub fn matches_current(
        &self,
        db: &Connection,
        base: &Path,
        id: &str,
        grants: &LocalMediaGrants,
    ) -> bool {
        prepare_media_file(db, base, id, grants).is_ok_and(|current| &current == self)
    }
    /// Display hint only, never a read capability. Actual consumers open a checked lease.
    pub fn available(&self) -> bool {
        if reject_links(&self.path).is_err() {
            return false;
        }
        let Ok(meta) = fs::metadata(&self.path) else {
            return false;
        };
        meta.is_file()
            && self.expected.as_ref().is_none_or(|f| {
                meta.len() == f.size
                    && meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .is_some_and(|d| d.as_nanos() == f.modified)
            })
    }
}
impl AsRef<Path> for CheckedMediaFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
impl std::ops::Deref for CheckedMediaFile {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}
impl PartialEq<PathBuf> for CheckedMediaFile {
    fn eq(&self, other: &PathBuf) -> bool {
        self.path == *other
    }
}
impl CheckedMediaFile {
    pub(crate) fn managed(path: PathBuf) -> Self {
        Self { path, _lease: None }
    }
}
// Rotate a library-owned nonce; never delete external originals or trust restored grants.
pub fn invalidate_external_grants(db: &Connection) -> Result<(), String> {
    db.execute(
        "UPDATE media_external_files SET access_epoch=?1",
        [uuid::Uuid::new_v4().to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn reject_links(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        if crate::asset::scanner::is_link(ancestor) {
            return Err("不接受符号链接、junction或重解析点".into());
        }
    }
    Ok(())
}
fn source_path(path: &Path) -> Result<PathBuf, String> {
    let text = path.to_str().ok_or("文件路径无效")?;
    let normal = text.strip_prefix(r"\\?\").unwrap_or(text);
    if !path.is_absolute()
        || normal.starts_with(['\\', '/'])
        || normal.get(2..).is_some_and(|s| s.contains(':'))
        || normal.chars().any(char::is_control)
    {
        return Err("不接受网络、设备或备用数据流路径".into());
    }
    if !matches!(
        path.extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "webm" | "mov" | "mkv" | "wmv" | "avi"
    ) {
        return Err("请选择MP4/WebM/MOV/MKV/WMV/AVI视频文件".into());
    }
    reject_links(path)?;
    let path = path.canonicalize().map_err(|_| "视频原件缺失或不可读")?;
    reject_links(&path)?;
    Ok(path)
}
fn checked_source(
    path: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<(CheckedMediaFile, Fingerprint), String> {
    let path = source_path(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "视频原件正在写入、已变化或不可读")?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() == 0 {
        return Err("请选择非空普通视频文件".into());
    }
    #[cfg(windows)]
    let identity = {
        use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, GetFinalPathNameByHandleW, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err("无法核验文件身份".into());
        }
        let mut buffer = vec![0u16; 32768];
        let count = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        } as usize;
        if count == 0
            || count >= buffer.len()
            || PathBuf::from(std::ffi::OsString::from_wide(&buffer[..count])) != path
        {
            return Err("SOURCE_CHANGED：文件路径在读取中变化".into());
        }
        format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        )
    };
    #[cfg(not(windows))]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", meta.dev(), meta.ino())
    };
    let modified = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if stop() {
            return Err("文件校验已停止".into());
        }
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    reject_links(&path)?;
    let source = Fingerprint {
        path: path.clone(),
        size: meta.len(),
        modified,
        identity,
        sha256: format!("{:x}", hash.finalize()),
    };
    Ok((
        CheckedMediaFile {
            path,
            _lease: Some(file),
        },
        source,
    ))
}
fn library_root(state: &AppState, library: &str) -> Result<PathBuf, String> {
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护".into());
    }
    state
        .with_library_if_id(library, |_, base| {
            base.canonicalize().map_err(|e| e.to_string())
        })?
        .ok_or_else(|| "素材库已切换".into())
}
#[cfg(test)]
pub fn preview_external_video(
    state: &AppState,
    library: &str,
    path: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<ExternalVideoPreview, String> {
    preview_external_video_with_resources(state, library, path, stop, None)
}
pub fn preview_external_video_with_resources(
    state: &AppState,
    library: &str,
    path: &Path,
    stop: &dyn Fn() -> bool,
    resources: Option<&Path>,
) -> Result<ExternalVideoPreview, String> {
    let root = library_root(state, library)?;
    // Paths submitted by the frontend must not use extended/device spellings.
    if path.to_string_lossy().starts_with(r"\\") {
        return Err("不接受网络或设备路径".into());
    }
    let (checked, source) = checked_source(path, stop)?;
    if checked.starts_with(&root) {
        return Err("外部视频必须位于素材库之外".into());
    }
    let tool = crate::runtime::tools::require(resources, "ffprobe")?;
    let info = crate::media_engine::probe::analyze(&tool, &checked, stop)?;
    if info.width.unwrap_or(0) == 0 || info.height.unwrap_or(0) == 0 {
        return Err("文件不包含可用视频流".into());
    }
    if library_root(state, library)? != root {
        return Err("素材库已切换".into());
    }
    let preview = ExternalVideoPreview {
        preview_id: uuid::Uuid::new_v4().to_string(),
        library_id: library.into(),
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("视频")
            .into(),
        size_bytes: source.size,
        sha256: source.sha256.clone(),
        duration_ms: info.duration.map(|v| (v * 1000.0) as u64),
        expires_at: chrono::Utc::now().timestamp() + 900,
    };
    let mut registry = state.external_videos.lock().map_err(|_| "视频预览锁损坏")?;
    registry
        .previews
        .retain(|_, p| p.preview.expires_at > chrono::Utc::now().timestamp());
    if registry.previews.len() >= 100 {
        return Err("待确认视频过多，请稍后重试".into());
    }
    registry.previews.insert(
        preview.preview_id.clone(),
        PendingVideo {
            preview: preview.clone(),
            source,
            info,
            committed: None,
            root,
        },
    );
    Ok(preview)
}
pub fn commit_external_video(
    state: &AppState,
    library: &str,
    preview: &str,
) -> Result<String, String> {
    let root = library_root(state, library)?;
    let mut registry = state.external_videos.lock().map_err(|_| "视频预览锁损坏")?;
    let pending = registry
        .previews
        .get_mut(preview)
        .ok_or("预览已失效，请重新选择视频")?;
    if pending.preview.library_id != library
        || pending.root != root
        || pending.preview.expires_at <= chrono::Utc::now().timestamp()
    {
        return Err("预览已失效或属于其他素材库".into());
    }
    if let Some(id) = &pending.committed {
        return Ok(id.clone());
    }
    let (_checked, current) = checked_source(&pending.source.path, &|| false)?;
    if current != pending.source {
        return Err("SOURCE_CHANGED：原件已变化，请重新读取".into());
    }
    let grants = LocalMediaGrants::new(&state.config_directory());
    let result=state.with_library_mut_if_id(library,|db,base| {
        if state.tasks.maintenance().is_some() {return Err("素材库正在维护".into());}
        let duplicate:Option<(String,bool)>=db.query_row("SELECT e.id,e.deleted_at IS NOT NULL FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE f.checksum=?1",[&current.sha256],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((id,deleted))=duplicate {if deleted{return Err("相同视频已在回收站，请先恢复".into());} return Ok(id);}
        let entry=uuid::Uuid::new_v4().to_string(); let file=uuid::Uuid::new_v4().to_string(); let epoch=uuid::Uuid::new_v4().to_string(); let timestamp=chrono::Utc::now().to_rfc3339();
        let filename=current.path.file_name().and_then(|s|s.to_str()).ok_or("文件名无效")?;
        let tx=db.transaction().map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video',?2,?3,?4,?4)",params![entry,pending.preview.name,file,timestamp]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,file_size,checksum,width,height,duration_ms,created_at) VALUES(?1,?2,'main',?3,?3,?4,?5,?6,?7,?8,?9,?10)",params![file,entry,filename,format!("external-references/{file}"),current.size as i64,current.sha256,pending.info.width.map(|v|v as i64),pending.info.height.map(|v|v as i64),pending.preview.duration_ms.map(|v|v as i64),timestamp]).map_err(|e|e.to_string())?;
        let mime=match current.path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {"mp4"=>"video/mp4","webm"=>"video/webm","mov"=>"video/quicktime","mkv"=>"video/x-matroska","wmv"=>"video/x-ms-wmv",_=>"video/x-msvideo"};
        tx.execute("UPDATE media_files SET mime_type=?1 WHERE id=?2",params![mime,file]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO media_external_files(file_id,canonical_path,size_bytes,mtime_ms,sha256,file_identity,revision,last_checked_at,access_epoch) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8)",params![file,current.path.to_string_lossy(),current.size as i64,(current.modified/1_000_000) as i64,current.sha256,current.identity,timestamp,epoch]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE media_external_files SET volume_id=?1 WHERE file_id=?2",params![current.identity.split(':').next().unwrap_or_default(),file]).map_err(|e|e.to_string())?;
        crate::media_library::reindex(&tx,&entry)?;
        let core=crate::asset::repository::register_origin(&tx,&crate::asset::models::CoreOrigin{kind:"media".into(),id:entry.clone()})?;
        crate::asset::repository::register_files(&tx,&core)?;
        grants.save(&Grant{library:library.into(),root:base.canonicalize().map_err(|e|e.to_string())?,file_id:file,epoch,source:current.clone()})?;
        tx.commit().map_err(|e|e.to_string())?;
        Ok(entry)
    })?.ok_or("素材库已切换")?;
    pending.committed = Some(result.clone());
    Ok(result)
}
pub fn resolve_media_file(
    db: &Connection,
    base: &Path,
    id: &str,
    grants: &LocalMediaGrants,
) -> Result<CheckedMediaFile, String> {
    prepare_media_file(db, base, id, grants)?.open_for_read()
}
pub fn prepare_media_file(
    db: &Connection,
    base: &Path,
    id: &str,
    grants: &LocalMediaGrants,
) -> Result<PreparedMediaFile, String> {
    let (relative,checksum):(String,String)=db.query_row("SELECT f.rel_path,f.checksum FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE f.id=?1 AND e.deleted_at IS NULL",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("媒体文件不存在或已回收")?;
    let external:Option<(String,String,i64,String,String)>=db.query_row("SELECT canonical_path,sha256,revision,file_identity,access_epoch FROM media_external_files WHERE file_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|e|e.to_string())?;
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    if let Some((path, hash, _revision, identity, epoch)) = external {
        if relative != format!("external-references/{id}") {
            return Err("外部文件登记损坏".into());
        }
        let grant_path = grants.path(&base, id)?;
        reject_links(&grant_path)?;
        let grant: Grant =
            serde_json::from_slice(&fs::read(grant_path).map_err(|_| "需重新确认外部视频访问")?)
                .map_err(|_| "本机批准损坏，需重新确认")?;
        let marker: crate::state::LibraryMarker = serde_json::from_slice(
            &fs::read(base.join("library.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if grant.root != base
            || grant.library != marker.library_id
            || grant.file_id != id
            || grant.epoch != epoch
            || grant.source.path != Path::new(&path)
            || grant.source.sha256 != hash
            || grant.source.identity != identity
        {
            return Err("需重新确认外部视频访问".into());
        }
        return Ok(PreparedMediaFile {
            path: grant.source.path.clone(),
            expected: Some(grant.source),
            epoch: Some(epoch),
            checksum,
        });
    }
    if relative.starts_with("external-references/") {
        return Err("外部文件未登记".into());
    }
    let path = crate::media::safe_relative(&base, &relative)?
        .canonicalize()
        .map_err(|_| "托管文件缺失")?;
    if !path.starts_with(&base) || !path.is_file() {
        return Err("文件不在当前素材库内".into());
    }
    Ok(PreparedMediaFile {
        path,
        expected: None,
        epoch: None,
        checksum,
    })
}
pub fn relocate_external_video(
    state: &AppState,
    library: &str,
    file: &str,
    preview: &str,
    revision: i64,
) -> Result<String, String> {
    let root = library_root(state, library)?;
    let mut registry = state.external_videos.lock().map_err(|_| "视频预览锁损坏")?;
    let pending = registry.previews.get_mut(preview).ok_or("预览已失效")?;
    if pending.preview.library_id != library
        || pending.root != root
        || pending.preview.expires_at <= chrono::Utc::now().timestamp()
        || pending.committed.is_some()
    {
        return Err("预览已失效或已使用".into());
    }
    let (_lease, current) = checked_source(&pending.source.path, &|| false)?;
    if current != pending.source {
        return Err("SOURCE_CHANGED：文件已变化".into());
    }
    let result=state.with_library_mut_if_id(library,|db,base| {
        if state.tasks.maintenance().is_some() {return Err("素材库正在维护".into());}
        let (entry,hash,old_revision):(String,String,i64)=db.query_row("SELECT f.entry_id,x.sha256,x.revision FROM media_external_files x JOIN media_files f ON f.id=x.file_id JOIN media_entries e ON e.id=f.entry_id WHERE x.file_id=?1 AND e.deleted_at IS NULL",[file],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?.ok_or("外部媒体不存在")?;
        if revision!=old_revision {return Err("SOURCE_CONFLICT：源记录已更新".into());}
        if hash!=current.sha256 {return Err("SOURCE_CHANGED：内容不同，请作为新视频登记".into());}
        let epoch=uuid::Uuid::new_v4().to_string();
        let tx=db.transaction().map_err(|e|e.to_string())?;
        tx.execute("UPDATE media_external_files SET canonical_path=?1,size_bytes=?2,mtime_ms=?3,file_identity=?4,revision=revision+1,access_epoch=?5,last_checked_at=?6 WHERE file_id=?7 AND revision=?8",params![current.path.to_string_lossy(),current.size as i64,(current.modified/1_000_000) as i64,current.identity,epoch,chrono::Utc::now().to_rfc3339(),file,revision]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE media_external_files SET volume_id=?1 WHERE file_id=?2",params![current.identity.split(':').next().unwrap_or_default(),file]).map_err(|e|e.to_string())?;
        LocalMediaGrants::new(&state.config_directory()).save(&Grant{library:library.into(),root:base.canonicalize().map_err(|e|e.to_string())?,file_id:file.into(),epoch,source:current.clone()})?;
        tx.commit().map_err(|e|e.to_string())?;
        Ok(entry)
    })?.ok_or("素材库已切换")?;
    pending.committed = Some(result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn video(root: &Path) -> PathBuf {
        let path = root.join("测试课程.mp4");
        let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/media-tools/ffmpeg.exe");
        crate::runtime::process::capture(
            &tool,
            &[
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=0.1",
                "-c:v",
                "mpeg4",
                "-n",
                path.to_str().unwrap(),
            ]
            .map(str::to_owned),
            std::time::Duration::from_secs(30),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        path
    }
    #[test]
    fn external_registration_keeps_source_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let original = std::fs::read(&source).unwrap();
        let modified = std::fs::metadata(&source).unwrap().modified().unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        assert!(commit_external_video(
            &state,
            &uuid::Uuid::new_v4().to_string(),
            &preview.preview_id
        )
        .is_err());
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        assert_eq!(
            commit_external_video(&state, &library, &preview.preview_id).unwrap(),
            entry
        );
        state
            .with_library(|db, base| {
                let file: String = db
                    .query_row(
                        "SELECT primary_file_id FROM media_entries WHERE id=?1",
                        [&entry],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&dir.path().join("empty"))
                )
                .is_err());
                let checked = resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&state.config_directory()),
                )
                .unwrap();
                assert_eq!(std::fs::read(&checked).unwrap(), original);
                #[cfg(windows)]
                assert!(std::fs::OpenOptions::new()
                    .write(true)
                    .open(&source)
                    .is_err());
                Ok(())
            })
            .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(
            std::fs::metadata(source).unwrap().modified().unwrap(),
            modified
        );
    }
    #[test]
    fn managed_resolution_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("test.db")).unwrap();
        std::fs::write(dir.path().join("test.png"), b"managed").unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('e','image','test','f','now','now')",[]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES('f','e','main','test.png','test.png','test.png','now')",[]).unwrap();
        let checked =
            resolve_media_file(&db, dir.path(), "f", &LocalMediaGrants::new(dir.path())).unwrap();
        assert_eq!(std::fs::read(checked).unwrap(), b"managed");
    }
    #[test]
    fn external_relocation_preserves_identity_and_rejects_changed_content() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        let file: String = state
            .with_library(|db, _| {
                db.query_row(
                    "SELECT primary_file_id FROM media_entries WHERE id=?1",
                    [&entry],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())
            })
            .unwrap();
        let moved = dir.path().join("moved.mp4");
        fs::rename(&source, &moved).unwrap();
        let relocated = preview_external_video(&state, &library, &moved, &|| false).unwrap();
        assert_eq!(
            relocate_external_video(&state, &library, &file, &relocated.preview_id, 1).unwrap(),
            entry
        );
        assert!(
            relocate_external_video(&state, &library, &file, &relocated.preview_id, 1).is_err()
        );
        state
            .with_library(|db, base| {
                assert!(resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&state.config_directory())
                )
                .is_ok());
                assert_eq!(
                    db.query_row("SELECT count(*) FROM media_entries", [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
                Ok(())
            })
            .unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&moved)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        let changed = preview_external_video(&state, &library, &moved, &|| false).unwrap();
        assert!(
            relocate_external_video(&state, &library, &file, &changed.preview_id, 2)
                .unwrap_err()
                .contains("SOURCE_CHANGED")
        );
    }
    #[test]
    fn external_resolution_rejects_reparse_and_stale_grants() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        for invalid in [
            r"\\server\share\a.mp4",
            r"\\.\C:\a.mp4",
            r"C:\a.mp4:payload",
            r"C:\run.exe",
        ] {
            assert!(
                preview_external_video(&state, &library, Path::new(invalid), &|| false).is_err()
            );
        }
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        let file: String = state
            .with_library(|db, _| {
                db.query_row(
                    "SELECT primary_file_id FROM media_entries WHERE id=?1",
                    [&entry],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())
            })
            .unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        state
            .with_library(|db, base| {
                assert!(resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&state.config_directory())
                )
                .is_err());
                Ok(())
            })
            .unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        state
            .external_videos
            .lock()
            .unwrap()
            .previews
            .get_mut(&preview.preview_id)
            .unwrap()
            .preview
            .expires_at = 0;
        assert!(commit_external_video(&state, &library, &preview.preview_id).is_err());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let link = dir.path().join("junction");
            let command = format!(
                "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
                link.to_str().unwrap().replace('\'', "''"),
                dir.path().to_str().unwrap().replace('\'', "''")
            );
            let output = std::process::Command::new("powershell.exe")
                .creation_flags(0x08000000)
                .args(["-NoProfile", "-NonInteractive", "-Command", &command])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(preview_external_video(
                &state,
                &library,
                &link.join("测试课程.mp4"),
                &|| false
            )
            .is_err());
            fs::remove_dir(link).unwrap();
        }
    }
    #[test]
    fn external_media_trash_purge_never_deletes_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let original = fs::read(&source).unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        state
            .with_library_mut(|db, base| {
                crate::media_library::delete_entries(db, vec![entry.clone()])?;
                let batch: String = db
                    .query_row(
                        "SELECT delete_batch_id FROM media_entries WHERE id=?1",
                        [&entry],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(crate::media_library::get(db, &entry).is_err());
                crate::db::restore_trash_batch(db, &batch)?;
                assert!(crate::media_library::get(db, &entry).is_ok());
                crate::media_library::delete_entries(db, vec![entry.clone()])?;
                let batch: String = db
                    .query_row(
                        "SELECT delete_batch_id FROM media_entries WHERE id=?1",
                        [&entry],
                        |r| r.get(0),
                    )
                    .unwrap();
                crate::db::purge_trash_batch(db, base, &batch)?;
                assert_eq!(
                    db.query_row("SELECT count(*) FROM media_external_files", [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    fn learning_data_backup_invalidates_external_approval() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let bytes = fs::read(&source).unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        let archive = dir.path().join("backup.dccassetlib");
        let file = state
            .with_library(|db, base| {
                let file: String = db
                    .query_row(
                        "SELECT primary_file_id FROM media_entries WHERE id=?1",
                        [&entry],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&state.config_directory())
                )
                .is_ok());
                let summary = crate::backup::external_media_backup_summary(db)?;
                assert_eq!(summary.count, 1);
                assert_eq!(summary.bytes, bytes.len() as u64);
                crate::backup::export_library(db, base, &archive)?;
                Ok(file)
            })
            .unwrap();
        state
            .with_active_mut(|active| crate::backup::restore_library(active, &archive))
            .unwrap();
        state
            .with_library(|db, base| {
                assert!(
                    resolve_media_file(
                        db,
                        base,
                        &file,
                        &LocalMediaGrants::new(&state.config_directory())
                    )
                    .is_err(),
                    "restore must not reuse machine approval from before the backup"
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read(source).unwrap(), bytes);
    }
    #[test]
    fn learning_native_prepared_read_is_revocable_and_opens_outside_database_lock() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let preview = preview_external_video(&state, &library, &source, &|| false).unwrap();
        let entry = commit_external_video(&state, &library, &preview.preview_id).unwrap();
        let grants = LocalMediaGrants::new(&state.config_directory());
        let (file, prepared) = state
            .with_library(|db, base| {
                let file: String = db
                    .query_row(
                        "SELECT primary_file_id FROM media_entries WHERE id=?1",
                        [&entry],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok((file.clone(), prepare_media_file(db, base, &file, &grants)?))
            })
            .unwrap();
        assert!(state.active.try_lock().is_ok());
        assert!(prepared.open_for_playback(&|| true).is_err());
        let checked = prepared.open_for_playback(&|| false).unwrap();
        assert!(checked.is_file());
        state
            .with_library(|db, base| {
                assert!(prepared.matches_current(db, base, &file, &grants));
                invalidate_external_grants(db)?;
                assert!(!prepared.matches_current(db, base, &file, &grants));
                Ok(())
            })
            .unwrap();
    }
    #[test]
    fn external_preview_rejects_disguised_playlist() {
        let dir = tempfile::tempdir().unwrap();
        let source = video(dir.path());
        let ascii = dir.path().join("source.mp4");
        fs::rename(source, &ascii).unwrap();
        let playlist = dir.path().join("playlist.mp4");
        fs::write(&playlist, "ffconcat version 1.0\nfile 'source.mp4'\n").unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        assert!(
            preview_external_video(&state, &library, &playlist, &|| false).is_err(),
            "a playlist must not grant access to its referenced files"
        );
    }
}
