//! Durable conversion approvals; editable content still belongs to knowledge_notes.
use super::{
    note_archive,
    note_conversion::{
        self, AttachmentKind, AttachmentMap, AttachmentTarget, ConversionPreview, MediaMap,
        MediaTarget,
    },
};
use crate::knowledge::NoteInput;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Destination {
    New {
        title: String,
        notebook_id: Option<String>,
    },
    Replace {
        note_id: String,
        expected_revision: i64,
    },
}
pub struct AttachmentSource {
    pub source_key: String,
    pub path: PathBuf,
    pub sha256: String,
    pub filename: Option<String>,
}
pub type CourseMap = HashMap<String, String>;
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteCommitReceipt {
    pub note_id: String,
    pub revision: i64,
    pub pending: bool,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredNotePreview {
    pub id: String,
    pub status: String,
    pub conversion: ConversionPreview,
    pub receipt: Option<NoteCommitReceipt>,
}
const MAX_SNAPSHOT: usize = 16 * 1024 * 1024;
const MAX_FILE: u64 = 100 * 1024 * 1024;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
pub(crate) fn invalidate_previews(db: &Connection) -> Result<(), String> {
    db.execute(
        "UPDATE learning_note_previews SET status='cancelled' WHERE status='ready'",
        [],
    )
    .map_err(error)?;
    Ok(())
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) {
        Ok(())
    } else {
        Err("转换预览身份无效".into())
    }
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_key(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn check_stop(stop: &dyn Fn() -> bool) -> Result<(), String> {
    if stop() {
        Err("笔记操作已停止、素材库已切换或正在维护，原文保留".into())
    } else {
        Ok(())
    }
}
fn source_name(source: &AttachmentSource) -> Result<String, String> {
    let name = source
        .filename
        .as_deref()
        .or_else(|| source.path.file_name().and_then(|v| v.to_str()))
        .ok_or("附件名称无效")?;
    if name.is_empty() || name.len() > 1000 || name.chars().any(char::is_control) {
        return Err("附件名称过长或含控制字符".into());
    }
    Ok(name.into())
}
fn identity(base: &Path, library: &str) -> Result<String, String> {
    uuid(library)?;
    let (actual, root) = super::analysis_store::library_identity(base)?;
    if actual != library {
        return Err("素材库已切换".into());
    }
    Ok(root)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    id: String,
    library: String,
    root: String,
    archive: String,
    input: NoteInput,
    conversion: ConversionPreview,
    files: Vec<StagedAttachment>,
    courses: CourseMap,
    media: MediaMap,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedAttachment {
    key: String,
    placeholder: String,
    relative: String,
    filename: String,
    sha256: String,
    size: u64,
    kind: AttachmentKind,
}
struct Binding {
    course: String,
    media: String,
    file: String,
    duration: u64,
    prepared: crate::media_access::PreparedMediaFile,
    _lease: crate::media_access::CheckedMediaFile,
}
fn media_map(
    db: &Connection,
    base: &Path,
    courses: &CourseMap,
    grants: &crate::media_access::LocalMediaGrants,
    stop: &dyn Fn() -> bool,
) -> Result<(MediaMap, Vec<Binding>), String> {
    if courses.len() > 256 {
        return Err("课程映射过多".into());
    }
    let mut bindings = Vec::new();
    let mut by_course: HashMap<String, MediaTarget> = HashMap::new();
    let mut map = MediaMap::new();
    for (key, id) in courses {
        check_stop(stop)?;
        if !valid_key(key) {
            return Err("课程来源键无效".into());
        }
        uuid(id)?;
        if !by_course.contains_key(id) {
            let course = super::store::course(db, id)?;
            let duration = course
                .duration_ms
                .filter(|d| *d > 0)
                .ok_or("课程时长尚未确定")?;
            let file: String = db
                .query_row(
                    "SELECT primary_file_id FROM media_entries WHERE id=?1",
                    [&course.media_id],
                    |r| r.get(0),
                )
                .map_err(error)?;
            let prepared = crate::media_access::prepare_media_file(db, base, &file, grants)?;
            let (lease, fingerprint) = prepared.open_for_analysis(stop)?;
            by_course.insert(
                id.clone(),
                MediaTarget {
                    course_id: id.clone(),
                    media_id: course.media_id.clone(),
                    fingerprint,
                    duration_ms: duration,
                },
            );
            bindings.push(Binding {
                course: id.clone(),
                media: course.media_id,
                file,
                duration,
                prepared,
                _lease: lease,
            });
        }
        map.insert(key.clone(), by_course[id].clone());
    }
    Ok((map, bindings))
}
fn check_bindings(
    db: &Connection,
    base: &Path,
    bindings: &[Binding],
    grants: &crate::media_access::LocalMediaGrants,
) -> Result<(), String> {
    for binding in bindings {
        let course = super::store::course(db, &binding.course)?;
        let file: String = db
            .query_row(
                "SELECT primary_file_id FROM media_entries WHERE id=?1",
                [&course.media_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if course.media_id != binding.media
            || course.duration_ms != Some(binding.duration)
            || file != binding.file
            || !binding.prepared.matches_current(db, base, &file, grants)
        {
            return Err("课程、原件或本机授权已变化，请重新预览".into());
        }
    }
    Ok(())
}
fn destination_input(db: &Connection, destination: &Destination) -> Result<NoteInput, String> {
    Ok(match destination {
        Destination::New { title, notebook_id } => {
            if title.trim().is_empty() || title.chars().count() > 300 {
                return Err("笔记标题需为 1–300 个字符".into());
            }
            if let Some(id) = notebook_id {
                uuid(id)?;
                let exists: bool = db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM knowledge_notebooks WHERE id=?1)",
                        [id],
                        |r| r.get(0),
                    )
                    .map_err(error)?;
                if !exists {
                    return Err("目标笔记本不存在".into());
                }
            }
            NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: notebook_id.clone(),
                title: title.trim().into(),
                document: json!({"type":"doc","content":[]}),
                tags: vec![],
                favorite: false,
                journal_date: None,
            }
        }
        Destination::Replace {
            note_id,
            expected_revision,
        } => {
            uuid(note_id)?;
            let note = crate::knowledge::get(db, note_id)?;
            if note.deleted_at.is_some() || note.revision != *expected_revision {
                return Err("目标笔记已变化或进入回收站，请重新预览".into());
            }
            NoteInput {
                id: Some(note.id),
                expected_revision: Some(note.revision),
                notebook_id: note.notebook_id,
                title: note.title,
                document: json!({"type":"doc","content":[]}),
                tags: note.tags,
                favorite: note.favorite,
                journal_date: note.journal_date,
            }
        }
    })
}
fn directory(base: &Path, id: &str) -> Result<PathBuf, String> {
    uuid(id)?;
    let directory = base.join("learning-data/note-previews").join(id);
    crate::media_access::reject_links(&directory)?;
    fs::create_dir_all(&directory).map_err(error)?;
    crate::media_access::reject_links(&directory)?;
    if !directory.canonicalize().map_err(error)?.starts_with(
        base.canonicalize()
            .map_err(error)?
            .join("learning-data/note-previews"),
    ) {
        return Err("预览目录越出当前库".into());
    }
    Ok(directory)
}
fn cleanup(base: &Path, files: &[StagedAttachment]) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut parents = HashSet::new();
    for file in files {
        match super::files::learning_file(base, &file.relative) {
            Ok(path) => {
                parents.insert(path.parent().unwrap().to_owned());
                if fs::remove_file(path).is_err() {
                    warnings.push("部分预览附件尚未清理，可再次重试取消或读取回执".into());
                }
            }
            Err(_) => {
                if !matches!(base.join(&file.relative).try_exists(), Ok(false)) {
                    warnings.push("预览附件路径已变化，未清理未知文件".into());
                }
            }
        }
    }
    for parent in parents {
        let _ = fs::remove_dir(parent);
    }
    warnings.sort();
    warnings.dedup();
    warnings
}
fn stage_source(
    base: &Path,
    id: &str,
    source: &AttachmentSource,
    stop: &dyn Fn() -> bool,
) -> Result<StagedAttachment, String> {
    check_stop(stop)?;
    crate::media_access::reject_links(&source.path)?;
    let filename = source_name(source)?;
    let extension = source
        .path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = crate::knowledge_files::content_type(&extension).ok_or("不支持的笔记附件格式")?;
    let dir = directory(base, id)?;
    let size = fs::metadata(&source.path).map_err(error)?.len();
    if size > MAX_FILE {
        return Err("单个附件超过100MiB".into());
    }
    crate::runtime::tools::ensure_space(&dir, size + 1024 * 1024)?;
    let mut input = fs::File::open(&source.path).map_err(error)?;
    if !input.metadata().map_err(error)?.is_file() {
        return Err("附件必须为普通文件".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(error)?;
    let mut actual = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_stop(stop)?;
        let n = input.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        actual = actual
            .checked_add(n as u64)
            .filter(|v| *v <= MAX_FILE)
            .ok_or("附件复制过程中超过上限")?;
        temp.write_all(&buffer[..n]).map_err(error)?;
    }
    if actual != size || actual > MAX_FILE {
        return Err("附件复制过程中发生变化或超过上限".into());
    }
    temp.flush().map_err(error)?;
    temp.as_file().sync_all().map_err(error)?;
    let (_, sha256) = crate::file_digest::digest_file(temp.path())?;
    if sha256 != source.sha256 {
        return Err("附件原件摘要已变化，请重新预览".into());
    }
    if mime.starts_with("image/") {
        crate::knowledge_files::validate_image(temp.path())?;
    }
    check_stop(stop)?;
    let placeholder = uuid::Uuid::new_v4().to_string();
    let relative = format!("learning-data/note-previews/{id}/{placeholder}.{extension}");
    temp.persist_noclobber(base.join(&relative))
        .map_err(error)?;
    Ok(StagedAttachment {
        key: source.source_key.clone(),
        placeholder,
        relative,
        filename,
        sha256,
        size,
        kind: if mime.starts_with("image/") {
            AttachmentKind::Image
        } else {
            AttachmentKind::File
        },
    })
}
fn load(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
) -> Result<(Snapshot, String, Option<NoteCommitReceipt>), String> {
    uuid(id)?;
    let root = identity(base, library)?;
    let(archive,raw,sha,status,receipt):(String,String,String,String,Option<String>)=db.query_row("SELECT archive_id,payload_json,payload_sha256,status,receipt_json FROM learning_note_previews WHERE id=?1 AND length(CAST(payload_json AS BLOB))<=16777216",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|"转换预览不存在或超限")?;
    if raw.len() > MAX_SNAPSHOT || hash(&raw) != sha {
        return Err("转换预览快照损坏".into());
    }
    let snapshot: Snapshot = serde_json::from_str(&raw).map_err(|_| "转换预览格式损坏")?;
    if snapshot.id != id
        || snapshot.library != library
        || snapshot.root != root
        || snapshot.archive != archive
        || snapshot.files.len() > 1000
        || snapshot.courses.len() > 256
    {
        return Err("预览不属于当前素材库位置或清单无效".into());
    }
    let mut ids = HashSet::new();
    let mut total = 0u64;
    for file in &snapshot.files {
        uuid(&file.placeholder)?;
        let extension = Path::new(&file.relative)
            .extension()
            .and_then(|v| v.to_str())
            .ok_or("预览附件缺少格式")?;
        if crate::knowledge_files::content_type(extension).is_none()
            || file.relative
                != format!(
                    "learning-data/note-previews/{id}/{}.{}",
                    file.placeholder, extension
                )
            || !ids.insert(&file.placeholder)
            || !valid_hash(&file.sha256)
            || file.size > MAX_FILE
            || !valid_key(&file.key)
        {
            return Err("预览附件清单无效".into());
        }
        total = total
            .checked_add(file.size)
            .filter(|n| *n <= MAX_TOTAL)
            .ok_or("预览附件总量超限")?;
    }
    let receipt = receipt
        .map(|v| {
            if v.len() > 65536 {
                return Err("转换回执超限".into());
            }
            serde_json::from_str(&v).map_err(|_| "转换回执损坏".to_owned())
        })
        .transpose()?;
    if (status == "committed") != receipt.is_some() {
        return Err("转换状态和回执不一致".into());
    }
    Ok((snapshot, status, receipt))
}
pub fn prepare_note_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    archive: &str,
    destination: Destination,
    sources: &[AttachmentSource],
    courses: &CourseMap,
    grants: &crate::media_access::LocalMediaGrants,
    stop: &dyn Fn() -> bool,
) -> Result<StoredNotePreview, String> {
    check_stop(stop)?;
    if !db.is_autocommit() {
        return Err("请在独立预览操作中发布快照".into());
    }
    let root = identity(base, library)?;
    let bytes = note_archive::read_archive(db, base, archive)?;
    let input = destination_input(db, &destination)?;
    let (media, bindings) = media_map(db, base, courses, grants, stop)?;
    if sources.len() > 1000 {
        return Err("附件超过1000个".into());
    }
    let mut specs = BTreeMap::new();
    for source in sources {
        if !valid_key(&source.source_key)
            || !valid_hash(&source.sha256)
            || specs
                .insert(&source.source_key, (&source.sha256, source_name(source)?))
                .is_some()
        {
            return Err("附件来源键、摘要或重复映射无效".into());
        }
    }
    let identity_key=hash(serde_json::to_vec(&json!({"library":library,"root":root,"archive":archive,"sha256":hash(&bytes),"input":input,"sources":specs,"media":media.iter().collect::<BTreeMap<_,_>>() })).map_err(error)?);
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM learning_note_previews WHERE identity_key=?1 AND status!='cancelled'",
            [&identity_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    if let Some(id) = existing {
        let (snapshot, status, receipt) = load(db, base, library, &id)?;
        return Ok(StoredNotePreview {
            id,
            status,
            conversion: snapshot.conversion,
            receipt,
        });
    }
    let active: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM learning_note_previews WHERE status='ready' AND CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.root') ELSE NULL END=?1",
            [&root],
            |r| r.get(0),
        )
        .map_err(error)?;
    if active >= 100 {
        return Err("待确认预览过多，请先提交或取消已有预览".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut files = Vec::new();
    let result = (|| {
        let mut total = 0u64;
        for source in sources {
            check_stop(stop)?;
            let size = fs::metadata(&source.path).map_err(error)?.len();
            total = total
                .checked_add(size)
                .filter(|v| *v <= MAX_TOTAL)
                .ok_or("附件总量超过1GiB")?;
            files.push(stage_source(base, &id, source, stop)?);
        }
        let attachments = files
            .iter()
            .map(|file| {
                (
                    file.key.clone(),
                    AttachmentTarget {
                        attachment_id: file.placeholder.clone(),
                        kind: file.kind,
                    },
                )
            })
            .collect::<AttachmentMap>();
        let conversion = note_conversion::convert_source_note(&bytes, &attachments, &media)?;
        check_stop(stop)?;
        check_bindings(db, base, &bindings, grants)?;
        if identity(base, library)? != root {
            return Err("素材库位置已变化".into());
        }
        let snapshot = Snapshot {
            id: id.clone(),
            library: library.into(),
            root,
            archive: archive.into(),
            input,
            conversion: conversion.clone(),
            files: files.clone(),
            courses: courses.clone(),
            media,
        };
        let raw = serde_json::to_string(&snapshot).map_err(error)?;
        if raw.len() > MAX_SNAPSHOT {
            return Err("转换快照超过16MiB，未截断保存".into());
        }
        let stored=db.execute("INSERT INTO learning_note_previews(id,archive_id,identity_key,payload_json,payload_sha256,status,created_at) VALUES(?1,?2,?3,?4,?5,'ready',?6)",params![id,archive,identity_key,raw,hash(&raw),chrono::Utc::now().to_rfc3339()]);
        if let Err(e) = stored {
            return Err(error(e));
        }
        if stop() {
            db.execute("UPDATE learning_note_previews SET status='cancelled' WHERE id=?1 AND status='ready'",[&id]).map_err(error)?;
            return Err("笔记预览已停止，原文保留".into());
        }
        Ok(StoredNotePreview {
            id: id.clone(),
            status: "ready".into(),
            conversion,
            receipt: None,
        })
    })();
    if result.is_err() {
        let _ = cleanup(base, &files);
        if !stop() {
            let existing:Option<String>=db.query_row("SELECT id FROM learning_note_previews WHERE identity_key=?1 AND status!='cancelled'",[&identity_key],|row|row.get(0)).optional().map_err(error)?;
            if let Some(id) = existing {
                return read_note_preview(db, base, library, &id);
            }
        }
    }
    result
}
fn checked_staging(
    base: &Path,
    files: &[StagedAttachment],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<fs::File>, String> {
    let mut leases = Vec::new();
    for file in files {
        check_stop(stop)?;
        let path = super::files::learning_file(base, &file.relative)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        }
        let lease = options.open(&path).map_err(error)?;
        let (size, hash) = crate::file_digest::digest_file(&path)?;
        if size != file.size || hash != file.sha256 {
            return Err("预览附件副本已变化，未提交任何正文".into());
        }
        leases.push(lease);
    }
    Ok(leases)
}
fn remap(
    node: &mut Value,
    archives: &HashMap<String, String>,
    attachments: &HashMap<String, String>,
) {
    if node["type"] == "preservedMarkdown" {
        if let Some(id) = node["attrs"]["archiveItemId"]
            .as_str()
            .and_then(|id| archives.get(id))
        {
            node["attrs"]["archiveItemId"] = json!(id);
        }
    }
    if node["type"] == "attachment" {
        if let Some(id) = node["attrs"]["attachmentId"]
            .as_str()
            .and_then(|id| attachments.get(id))
        {
            node["attrs"]["attachmentId"] = json!(id);
        }
    }
    if node["type"] == "image" {
        if let Some(id) = node["attrs"]["src"]
            .as_str()
            .and_then(|v| v.strip_prefix("http://dcc-media.localhost/note/"))
            .and_then(|id| attachments.get(id))
        {
            node["attrs"]["src"] = json!(format!("http://dcc-media.localhost/note/{id}"));
        }
    }
    if let Some(children) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in children {
            remap(child, archives, attachments);
        }
    }
}
pub fn commit_note_preview(
    db: &mut Connection,
    base: &Path,
    library: &str,
    id: &str,
    grants: &crate::media_access::LocalMediaGrants,
    finalize: impl FnOnce(Transaction<'_>) -> Result<(), String>,
    stop: &dyn Fn() -> bool,
) -> Result<NoteCommitReceipt, String> {
    let (snapshot, status, receipt) = load(db, base, library, id)?;
    if let Some(receipt) = receipt {
        let _ = cleanup(base, &snapshot.files);
        return Ok(receipt);
    }
    if status != "ready" {
        return Err("转换预览已取消".into());
    }
    check_stop(stop)?;
    let mut document = snapshot
        .conversion
        .document
        .clone()
        .ok_or("原文尚未转换为有效编码，已保留原件，不能创建空白笔记")?;
    if hash(note_archive::read_archive(db, base, &snapshot.archive)?)
        != snapshot.conversion.archive_hash
    {
        return Err("原件摘要与预览不一致".into());
    }
    let (media, bindings) = media_map(db, base, &snapshot.courses, grants, stop)?;
    if media != snapshot.media {
        return Err("课程原件、时长或身份已变化，请重新预览".into());
    }
    let leases = checked_staging(base, &snapshot.files, stop)?;
    crate::runtime::tools::ensure_space(
        base,
        snapshot.files.iter().map(|file| file.size).sum::<u64>() + 1024 * 1024,
    )?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let (_, status, receipt) = load(&tx, base, library, id)?;
    if let Some(receipt) = receipt {
        drop(tx);
        drop(leases);
        let _ = cleanup(base, &snapshot.files);
        return Ok(receipt);
    }
    if status != "ready" {
        return Err("预览已取消，未提交".into());
    }
    let mut published = Vec::new();
    let mut result = (|| {
        let mut archive_map = HashMap::new();
        let spans = snapshot
            .conversion
            .unsupported
            .iter()
            .map(|span| {
                (
                    span.start_byte as u64,
                    span.end_byte as u64,
                    span.label.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let items = note_archive::add_archive_items(&tx, base, &snapshot.archive, &spans)?;
        for (span, item) in snapshot.conversion.unsupported.iter().zip(items) {
            archive_map.insert(span.placeholder_id.clone(), item.id);
        }
        let input = snapshot.input.clone();
        let (note_id, revision) = if let Some(id) = &input.id {
            let existing = crate::knowledge::get(&tx, id)?;
            if existing.deleted_at.is_some() || Some(existing.revision) != input.expected_revision {
                return Err("笔记修订已变化，原文和预览保留，请选择重新预览或另存".into());
            }
            (id.clone(), existing.revision)
        } else {
            let blank = crate::knowledge::save_in(&tx, input.clone(), false)?;
            (blank.id, blank.revision)
        };
        let mut attachment_map = HashMap::new();
        // ponytail: one note transaction uses the existing bounded attachment copier;
        // split file publication only if real imports show write contention.
        for file in &snapshot.files {
            check_stop(stop)?;
            let source = super::files::learning_file(base, &file.relative)?;
            let imported = crate::knowledge_files::import(&tx, base, &note_id, &source)?;
            let (path, _) = crate::knowledge_files::path(&tx, base, &imported.id)?;
            published.push(path);
            tx.execute(
                "UPDATE knowledge_attachments SET filename=?1 WHERE id=?2",
                params![file.filename, imported.id],
            )
            .map_err(error)?;
            attachment_map.insert(file.placeholder.clone(), imported.id);
        }
        remap(&mut document, &archive_map, &attachment_map);
        let note = crate::knowledge::save_in(
            &tx,
            NoteInput {
                id: Some(note_id),
                expected_revision: Some(revision),
                document,
                ..input.clone()
            },
            true,
        )?;
        if input.id.is_none() {
            tx.execute(
                "DELETE FROM knowledge_revisions WHERE note_id=?1",
                [&note.id],
            )
            .map_err(error)?;
        }
        check_bindings(&tx, base, &bindings, grants)?;
        if identity(base, library)? != snapshot.root {
            return Err("素材库位置已变化，未提交".into());
        }
        let receipt = NoteCommitReceipt {
            note_id: note.id,
            revision: note.revision,
            pending: snapshot.conversion.pending,
            warnings: snapshot.conversion.warnings.clone(),
        };
        tx.execute("UPDATE learning_note_previews SET status='committed',receipt_json=?1,committed_at=?2 WHERE id=?3 AND status='ready'",params![serde_json::to_string(&receipt).map_err(error)?,chrono::Utc::now().to_rfc3339(),id]).map_err(error)?;
        check_stop(stop)?;
        finalize(tx)?;
        Ok(receipt)
    })();
    drop(leases);
    if result.is_err() {
        // A committed transaction may outlive a lost response. Consult the durable
        // receipt before deleting any published file, including on callback failure.
        let saved:Option<String>=db.query_row("SELECT receipt_json FROM learning_note_previews WHERE id=?1 AND status='committed'",[id],|r|r.get(0)).optional().map_err(|_|"无法确认提交结果，文件已保留，请重新读取回执")?;
        if let Some(raw) = saved {
            if raw.len() > 65536 {
                return Err("提交回执超限，已保留文件".into());
            }
            result = Ok(serde_json::from_str(&raw).map_err(|_| "提交回执损坏，已保留文件")?);
        }
    }
    if result.is_err() {
        let root = base.canonicalize().map_err(error)?.join("note-attachments");
        for path in published {
            if crate::media_access::reject_links(&path).is_ok() && path.starts_with(&root) {
                let _ = fs::remove_file(&path);
                if let Some(parent) = path.parent() {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
    }
    match result {
        Ok(mut receipt) => {
            receipt.warnings.extend(cleanup(base, &snapshot.files));
            Ok(receipt)
        }
        Err(e) => Err(e),
    }
}
pub fn preview_image_response(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
    placeholder: &str,
    stop: &dyn Fn() -> bool,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    check_stop(stop)?;
    uuid(placeholder)?;
    let (snapshot, status, _) = load(db, base, library, id)?;
    if status != "ready" {
        return Err("预览已取消或提交，请重新预览或打开已保存笔记".into());
    }
    let file = snapshot
        .files
        .iter()
        .find(|file| file.placeholder == placeholder)
        .filter(|file| matches!(file.kind, AttachmentKind::Image))
        .ok_or("图片不属于此预览")?;
    let extension = Path::new(&file.relative)
        .extension()
        .and_then(|v| v.to_str())
        .ok_or("图片格式无效")?;
    let mime = crate::knowledge_files::content_type(extension)
        .filter(|mime| mime.starts_with("image/"))
        .ok_or("非图片附件不能内嵌预览")?;
    let mut leases = checked_staging(base, std::slice::from_ref(file), stop)?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_stop(stop)?;
        let count = leases[0].read(&mut buffer).map_err(error)?;
        if count == 0 {
            break;
        }
        if bytes.len() as u64 + count as u64 > file.size {
            return Err("图片副本读取过程中发生变化".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    if bytes.len() as u64 != file.size || hash(&bytes) != file.sha256 {
        return Err("图片副本摘要不匹配".into());
    }
    check_stop(stop)?;
    tauri::http::Response::builder()
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes)
        .map_err(error)
}

pub fn read_preview_block(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
    placeholder: &str,
) -> Result<String, String> {
    uuid(placeholder)?;
    let (snapshot, _, _) = load(db, base, library, id)?;
    let span = snapshot
        .conversion
        .unsupported
        .iter()
        .find(|span| span.placeholder_id == placeholder)
        .ok_or("原文块不属于此预览")?;
    let bytes = note_archive::read_archive(db, base, &snapshot.archive)?;
    if hash(&bytes) != snapshot.conversion.archive_hash {
        return Err("预览原件摘要不匹配".into());
    }
    let bytes = bytes
        .get(span.start_byte..span.end_byte)
        .ok_or("原文块范围无效")?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| "原文块不是有效 UTF-8".into())
}

pub fn read_note_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
) -> Result<StoredNotePreview, String> {
    let (snapshot, status, receipt) = load(db, base, library, id)?;
    Ok(StoredNotePreview {
        id: id.into(),
        status,
        conversion: snapshot.conversion,
        receipt,
    })
}
pub fn cancel_note_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
) -> Result<Vec<String>, String> {
    let (snapshot, status, _) = load(db, base, library, id)?;
    if status == "committed" {
        return Err("已提交笔记不能作为预览取消，请在知识库中管理".into());
    }
    let changed = db
        .execute(
            "UPDATE learning_note_previews SET status='cancelled' WHERE id=?1 AND status='ready'",
            [id],
        )
        .map_err(error)?;
    if changed == 0 {
        let status: String = db
            .query_row(
                "SELECT status FROM learning_note_previews WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if status == "committed" {
            return Err("预览已提交，未作为取消报告".into());
        }
    }
    Ok(cleanup(base, &snapshot.files))
}
