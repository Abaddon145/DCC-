//! Copy an external video into the library without a second media registration.
//! The source file is never deleted. The external row and grant disappear only
//! after the managed copy's hash matches, in one database transaction.
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const CANCELLED: &str = "归档复制已取消，外部原件引用仍保留";
const COMMIT_FAILED: &str = "归档复制已校验，但数据库提交失败；外部引用仍保留，可重试同一操作";
const SOURCE_CHANGED: &str = "SOURCE_CHANGED：原件已变化，未切换托管";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveReport {
    pub entry_id: String,
    pub file_id: String,
    pub core_id: String,
    pub status: String,
    pub storage_mode: String,
    pub source_preserved: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveOptions {
    /// When set, used instead of probing the volume. Tests use this for low space.
    pub available_bytes: Option<u64>,
    /// Copy and hash succeed, then the database switch is refused.
    pub fail_before_commit: bool,
    /// Append one byte to the source after a verified copy, before the switch.
    pub replace_source_after_copy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveJournal {
    operation_id: String,
    library_id: String,
    entry_id: String,
    file_id: String,
    sha256: String,
    target_rel_path: String,
    status: String,
}

struct VideoRecord {
    base: PathBuf,
    entry_id: String,
    file_id: String,
    core_id: String,
    original_name: String,
    rel_path: String,
    checksum: String,
    size: u64,
    canonical: PathBuf,
    external: bool,
}

pub fn archive_external_video(
    state: &AppState,
    expected_library_id: &str,
    entry_id: &str,
    operation_id: &str,
) -> Result<String, String> {
    let payload = crate::tasks::TaskRequest::ArchiveExternalVideo {
        entry_id: entry_id.into(),
        operation_id: operation_id.into(),
    };
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护".into());
    }
    if state.active_library_id()? != expected_library_id {
        return Err("素材库已切换".into());
    }
    let (task, _) = state
        .tasks
        .submit(expected_library_id.to_string(), payload)?;
    Ok(task.id)
}

pub fn perform(
    state: &AppState,
    library: &str,
    entry: &str,
    operation: &str,
    options: &ArchiveOptions,
) -> Result<ArchiveReport, String> {
    uuid(entry, "媒体条目无效")?;
    uuid(operation, "归档操作无效")?;
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护".into());
    }
    let record = state
        .with_library_if_id(library, |db, base| load_video(db, base, entry))?
        .ok_or_else(|| "素材库已切换".to_string())?;
    if !record.external {
        return finish_managed(state, library, operation, &record);
    }
    ensure_operation(library, &record, operation)?;
    if stopped() {
        write_status(library, &record, operation, "cancelled")?;
        return Err(CANCELLED.into());
    }
    write_status(library, &record, operation, "pending")?;
    let relative = managed_relative(&record.entry_id, &record.original_name)?;
    let ready = file_matches(&record.base, &relative, record.size, &record.checksum)?;
    let required = if ready {
        64 * 1024
    } else {
        record.size.saturating_add(1024 * 1024)
    };
    let available = match options.available_bytes {
        Some(value) => value,
        None => probe_available(&record.base)?,
    };
    if let Err(error) = crate::runtime::tools::check_available_space(available, required) {
        return Err(error);
    }
    if let Err(error) = confirm_source(&record) {
        write_status(library, &record, operation, "failed")?;
        return Err(error);
    }
    if !ready {
        let target = crate::media::safe_relative(&record.base, &relative)?;
        if target.exists() {
            state
                .with_library_if_id(library, |db, base| {
                    discard_unregistered(db, base, &relative)
                })?
                .ok_or_else(|| "素材库已切换".to_string())?;
        }
        if let Err(error) = copy_registered(&record, &relative) {
            if error == CANCELLED || stopped() {
                write_status(library, &record, operation, "cancelled")?;
                return Err(CANCELLED.into());
            }
            return Err(error);
        }
    }
    if options.replace_source_after_copy {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&record.canonical)
            .map_err(|_| "外部原件不可写，未切换托管")?;
        file.write_all(b"x")
            .map_err(|_| "外部原件不可写，未切换托管")?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    if !file_matches(&record.base, &relative, record.size, &record.checksum)? {
        let _ = state.with_library_if_id(library, |db, base| {
            discard_unregistered(db, base, &relative)
        });
        write_status(library, &record, operation, "failed")?;
        return Err("复制结果与登记摘要不一致，外部引用仍保留".into());
    }
    if confirm_source(&record).is_err() {
        write_status_with_target(library, &record, operation, "ready", &relative)?;
        return Err(SOURCE_CHANGED.into());
    }
    write_status_with_target(library, &record, operation, "ready", &relative)?;
    if stopped() {
        return Err(CANCELLED.into());
    }
    if options.fail_before_commit {
        return Err(COMMIT_FAILED.into());
    }
    state
        .with_library_mut_if_id(library, |db, base| {
            if stopped() {
                return Err(CANCELLED.into());
            }
            let target = crate::media::safe_relative(base, &relative)?;
            let (size, sha) = digest(&target)?;
            if size != record.size || sha != record.checksum {
                return Err("复制结果与登记摘要不一致，外部引用仍保留".into());
            }
            confirm_source(&record)?;
            publish(db, &record, &relative)?;
            Ok(())
        })?
        .ok_or_else(|| "素材库已切换".to_string())?;
    crate::media_access::invalidate_file_grant(
        &crate::media_access::LocalMediaGrants::new(&state.config_directory()),
        &record.base,
        &record.file_id,
    )?;
    write_status_with_target(library, &record, operation, "committed", &relative)?;
    Ok(report(&record, "archived"))
}

pub(crate) fn recovery_status(base: &Path, operation: &str) -> Result<String, String> {
    read_journal(base, operation)?
        .map(|journal| journal.status)
        .ok_or_else(|| "归档记录不存在".to_string())
}

fn finish_managed(
    state: &AppState,
    library: &str,
    operation: &str,
    record: &VideoRecord,
) -> Result<ArchiveReport, String> {
    if record.rel_path.starts_with("external-references/") {
        return Err("外部文件未登记".into());
    }
    let managed = state
        .with_library_if_id(library, |_, base| {
            let path = crate::media::safe_relative(base, &record.rel_path)?
                .canonicalize()
                .map_err(|_| "托管文件缺失")?;
            let root = base.canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(&root) || !path.is_file() {
                return Err("托管文件缺失".into());
            }
            let (size, sha) = digest(&path)?;
            if size != record.size || sha != record.checksum {
                return Err("托管文件与登记摘要不一致".into());
            }
            Ok(record.rel_path.clone())
        })?
        .ok_or_else(|| "素材库已切换".to_string())?;
    let _ = crate::media_access::invalidate_file_grant(
        &crate::media_access::LocalMediaGrants::new(&state.config_directory()),
        &record.base,
        &record.file_id,
    );
    write_status_with_target(library, record, operation, "committed", &managed)?;
    Ok(report(record, "alreadyManaged"))
}

fn report(record: &VideoRecord, status: &str) -> ArchiveReport {
    ArchiveReport {
        entry_id: record.entry_id.clone(),
        file_id: record.file_id.clone(),
        core_id: record.core_id.clone(),
        status: status.into(),
        storage_mode: "managed".into(),
        source_preserved: true,
    }
}

fn load_video(db: &rusqlite::Connection, base: &Path, entry: &str) -> Result<VideoRecord, String> {
    let row: Option<(String, String, String, String, String, i64)> = db
        .query_row(
            "SELECT f.id, COALESCE((SELECT c.id FROM asset_core c WHERE c.media_entry_id=e.id),''), f.original_name, f.rel_path, f.checksum, f.file_size FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE e.id=?1 AND e.deleted_at IS NULL AND e.kind='video' AND f.role='main'",
            [entry],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((file_id, core_id, original_name, rel_path, checksum, file_size)) = row else {
        return Err("外部视频不存在".into());
    };
    if file_size <= 0 {
        return Err("外部视频大小无效".into());
    }
    let external: Option<(String, String, i64)> = db
        .query_row(
            "SELECT canonical_path, sha256, size_bytes FROM media_external_files WHERE file_id=?1",
            [&file_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (canonical, external_present) = if let Some((path, sha, size)) = external {
        if rel_path != format!("external-references/{file_id}")
            || sha != checksum
            || size != file_size
            || sha.len() != 64
        {
            return Err("外部登记损坏，未切换托管".into());
        }
        (PathBuf::from(path), true)
    } else {
        (PathBuf::new(), false)
    };
    Ok(VideoRecord {
        base: base.to_path_buf(),
        entry_id: entry.into(),
        file_id,
        core_id,
        original_name,
        rel_path,
        checksum,
        size: file_size as u64,
        canonical,
        external: external_present,
    })
}

fn publish(db: &rusqlite::Connection, record: &VideoRecord, relative: &str) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let current: Option<(String, String, String)> = tx
        .query_row(
            "SELECT f.rel_path, f.checksum, x.sha256 FROM media_files f JOIN media_external_files x ON x.file_id=f.id WHERE f.id=?1 AND f.entry_id=?2",
            params![record.file_id, record.entry_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((rel_path, checksum, sha)) = current else {
        return Err("外部登记已变化，未切换托管".into());
    };
    let external_rel = format!("external-references/{}", record.file_id);
    if rel_path != external_rel || checksum != record.checksum || sha != record.checksum {
        return Err("外部登记已变化，未切换托管".into());
    }
    let owner: Option<String> = tx
        .query_row(
            "SELECT id FROM media_files WHERE rel_path=?1",
            [relative],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if owner.is_some() {
        return Err("托管路径已被登记，未覆盖".into());
    }
    let removed = tx
        .execute(
            "DELETE FROM media_external_files WHERE file_id=?1 AND sha256=?2",
            params![record.file_id, record.checksum],
        )
        .map_err(|e| e.to_string())?;
    if removed != 1 {
        return Err("外部登记已变化，未切换托管".into());
    }
    let updated = tx
        .execute(
            "UPDATE media_files SET rel_path=?1 WHERE id=?2 AND rel_path=?3 AND checksum=?4",
            params![relative, record.file_id, external_rel, record.checksum],
        )
        .map_err(|e| e.to_string())?;
    if updated != 1 {
        return Err("外部登记已变化，未切换托管".into());
    }
    tx.execute(
        "UPDATE media_entries SET updated_at=?1 WHERE id=?2",
        params![chrono::Utc::now().to_rfc3339(), record.entry_id],
    )
    .map_err(|e| e.to_string())?;
    let files: i64 = tx
        .query_row(
            "SELECT count(*) FROM media_files WHERE id=?1 AND entry_id=?2 AND role='main'",
            params![record.file_id, record.entry_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let external: i64 = tx
        .query_row(
            "SELECT count(*) FROM media_external_files WHERE file_id=?1",
            [&record.file_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if files != 1 || external != 0 {
        return Err("归档提交未保持单一登记".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

fn confirm_source(record: &VideoRecord) -> Result<(), String> {
    let (size, sha) = crate::media_access::read_external_sha256(&record.canonical, &|| stopped())?;
    if size != record.size || sha != record.checksum {
        return Err(SOURCE_CHANGED.into());
    }
    Ok(())
}

fn copy_registered(record: &VideoRecord, relative: &str) -> Result<(), String> {
    let target = crate::media::safe_relative(&record.base, relative)?;
    if file_matches(&record.base, relative, record.size, &record.checksum)? {
        return Ok(());
    }
    if target.exists() {
        return Err("托管目标已存在且摘要不符，未覆盖".into());
    }
    match crate::media::copy_atomic(&record.canonical, &target) {
        Ok(()) => Ok(()),
        Err(error) if error.contains("安全停止") || stopped() => Err(CANCELLED.into()),
        Err(error) if error.contains("No space") || error.contains("空间不足") => {
            Err("磁盘空间不足：释放空间后恢复任务".into())
        }
        Err(error) => Err(error),
    }
}

fn discard_unregistered(
    db: &rusqlite::Connection,
    base: &Path,
    relative: &str,
) -> Result<(), String> {
    let referenced: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_files WHERE rel_path=?1)",
            [relative],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if referenced {
        return Err("托管目标已被登记，未覆盖".into());
    }
    let path = crate::media::safe_relative(base, relative)?;
    if path.exists() {
        crate::media_access::reject_links(&path)?;
        if !path.is_file() {
            return Err("托管目标不是文件".into());
        }
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn file_matches(base: &Path, relative: &str, size: u64, sha: &str) -> Result<bool, String> {
    let path = crate::media::safe_relative(base, relative)?;
    if !path.exists() {
        return Ok(false);
    }
    crate::media_access::reject_links(&path)?;
    if !path.is_file() {
        return Err("托管目标不是文件".into());
    }
    let (got_size, got_sha) = digest(&path)?;
    Ok(got_size == size && got_sha == sha)
}

fn managed_relative(entry: &str, original_name: &str) -> Result<String, String> {
    uuid(entry, "媒体条目无效")?;
    let name = Path::new(original_name)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .ok_or("外部视频文件名无效")?;
    if name
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return Err("外部视频文件名无效".into());
    }
    let relative = format!("media-library/video/{entry}/files/{name}");
    if relative
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("托管路径无效".into());
    }
    Ok(relative)
}

fn ensure_operation(library: &str, record: &VideoRecord, operation: &str) -> Result<(), String> {
    if let Some(existing) = read_journal(&record.base, operation)? {
        if existing.library_id != library
            || existing.entry_id != record.entry_id
            || existing.file_id != record.file_id
            || existing.sha256 != record.checksum
        {
            return Err("归档操作与已有记录不一致".into());
        }
    }
    if let Some(other) = entry_operation(&record.base, &record.entry_id)? {
        if other != operation {
            if let Some(journal) = read_journal(&record.base, &other)? {
                if matches!(journal.status.as_str(), "pending" | "ready") {
                    return Err("该视频已有进行中的归档，请重试原操作".into());
                }
            }
        }
    }
    Ok(())
}

fn write_status(
    library: &str,
    record: &VideoRecord,
    operation: &str,
    status: &str,
) -> Result<(), String> {
    let relative = read_journal(&record.base, operation)?
        .map(|journal| journal.target_rel_path)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    write_status_with_target(library, record, operation, status, &relative)
}

fn write_status_with_target(
    library: &str,
    record: &VideoRecord,
    operation: &str,
    status: &str,
    relative: &str,
) -> Result<(), String> {
    if !matches!(
        status,
        "pending" | "ready" | "committed" | "cancelled" | "failed"
    ) {
        return Err("归档状态无效".into());
    }
    uuid(library, "素材库已切换")?;
    write_journal(
        &record.base,
        &ArchiveJournal {
            operation_id: operation.into(),
            library_id: library.into(),
            entry_id: record.entry_id.clone(),
            file_id: record.file_id.clone(),
            sha256: record.checksum.clone(),
            target_rel_path: relative.into(),
            status: status.into(),
        },
    )
}

fn write_journal(base: &Path, journal: &ArchiveJournal) -> Result<(), String> {
    uuid(&journal.operation_id, "归档操作无效")?;
    uuid(&journal.entry_id, "媒体条目无效")?;
    uuid(&journal.library_id, "素材库已切换")?;
    let path = journal_file(base, &journal.operation_id)?;
    crate::media_access::reject_links(base)?;
    let parent = path.parent().ok_or("归档记录目录无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    crate::media_access::reject_links(parent)?;
    let bytes = serde_json::to_vec(journal).map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("归档记录超限".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if path.exists() {
        crate::media_access::reject_links(&path)?;
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    temp.persist(&path).map_err(|e| e.to_string())?;
    let pointer = entry_file(base, &journal.entry_id)?;
    fs::create_dir_all(pointer.parent().ok_or("归档记录目录无效")?).map_err(|e| e.to_string())?;
    fs::write(&pointer, format!("{}\n", journal.operation_id)).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_journal(base: &Path, operation: &str) -> Result<Option<ArchiveJournal>, String> {
    let path = journal_file(base, operation)?;
    if !path.is_file() {
        return Ok(None);
    }
    crate::media_access::reject_links(&path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "归档记录不可读")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "归档记录不可读")?;
    if bytes.len() > 8192 {
        return Err("归档记录超限".into());
    }
    let journal: ArchiveJournal = serde_json::from_slice(&bytes).map_err(|_| "归档记录损坏")?;
    if journal.operation_id != operation
        || uuid(&journal.entry_id, "归档记录损坏").is_err()
        || journal.sha256.len() != 64
        || !matches!(
            journal.status.as_str(),
            "pending" | "ready" | "committed" | "cancelled" | "failed"
        )
    {
        return Err("归档记录损坏".into());
    }
    Ok(Some(journal))
}

fn entry_operation(base: &Path, entry: &str) -> Result<Option<String>, String> {
    let path = entry_file(base, entry)?;
    if !path.is_file() {
        return Ok(None);
    }
    crate::media_access::reject_links(&path)?;
    let text = fs::read_to_string(&path).map_err(|_| "归档记录不可读")?;
    let id = text.trim();
    uuid(id, "归档记录损坏")?;
    Ok(Some(id.to_string()))
}

fn journal_file(base: &Path, operation: &str) -> Result<PathBuf, String> {
    uuid(operation, "归档操作无效")?;
    Ok(base
        .join(".dcc/runtime/imports/archive-external")
        .join(format!("{operation}.json")))
}

fn entry_file(base: &Path, entry: &str) -> Result<PathBuf, String> {
    uuid(entry, "媒体条目无效")?;
    Ok(base
        .join(".dcc/runtime/imports/archive-external/by-entry")
        .join(entry))
}

fn digest(path: &Path) -> Result<(u64, String), String> {
    let stop = || stopped();
    crate::file_digest::digest_file_controlled(path, &stop)
}

fn stopped() -> bool {
    crate::media::process_stopped()
}

fn uuid(value: &str, message: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| message.to_string())
}

#[cfg(windows)]
fn probe_available(folder: &Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    let path = folder
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut available = 0u64;
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("无法读取磁盘空间".into());
    }
    Ok(available)
}

#[cfg(target_os = "linux")]
fn probe_available(folder: &Path) -> Result<u64, String> {
    #[repr(C)]
    struct Statvfs {
        f_bsize: u64,
        f_frsize: u64,
        f_blocks: u64,
        f_bfree: u64,
        f_bavail: u64,
        f_files: u64,
        f_ffree: u64,
        f_favail: u64,
        f_fsid: u64,
        f_flag: u64,
        f_namemax: u64,
        f_type: u32,
        spare: [i32; 5],
    }
    extern "C" {
        fn statvfs(path: *const i8, buf: *mut Statvfs) -> i32;
    }
    let text = folder.to_str().ok_or("无法读取磁盘空间")?;
    let c_path = std::ffi::CString::new(text).map_err(|_| "无法读取磁盘空间")?;
    let mut info = unsafe { std::mem::zeroed::<Statvfs>() };
    if unsafe { statvfs(c_path.as_ptr(), &mut info) } != 0 || info.f_frsize == 0 {
        return Err("无法读取磁盘空间".into());
    }
    Ok(info.f_bavail.saturating_mul(info.f_frsize))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn probe_available(_folder: &Path) -> Result<u64, String> {
    Err("无法读取磁盘空间".into())
}

#[cfg(test)]
mod learning_import_recovery {
    use super::*;
    use crate::media_access::{
        inspect_external_source, resolve_media_file, save_external_grant, LocalMediaGrants,
    };
    use crate::state::AppState;
    use rusqlite::params;
    use std::{fs, path::PathBuf, time::SystemTime};

    struct Fixture {
        state: AppState,
        dir: tempfile::TempDir,
        library: String,
        entry: String,
        file: String,
        core: String,
        asset_file: String,
        source: PathBuf,
        bytes: Vec<u8>,
        operation: String,
        modified: SystemTime,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source").join("course.mp4");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let bytes = b"yingliu-external-video-fixture-v1\n".repeat(4).to_vec();
        fs::write(&source, &bytes).unwrap();
        let modified = fs::metadata(&source).unwrap().modified().unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let library = state.active_library_id().unwrap();
        let facts = inspect_external_source(&source).unwrap();
        assert_eq!(facts.size, bytes.len() as u64);
        let entry = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        let epoch = uuid::Uuid::new_v4().to_string();
        let (core, asset_file) = state
            .with_library_mut(|db, base| {
                let now = chrono::Utc::now().to_rfc3339();
                let tx = db.transaction().map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video',?2,?3,?4,?4)",
                    params![entry, "课程", file, now],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,mime_type,file_size,checksum,created_at) VALUES(?1,?2,'main','course.mp4','course.mp4',?3,'video/mp4',?4,?5,?6)",
                    params![
                        file,
                        entry,
                        format!("external-references/{file}"),
                        facts.size as i64,
                        facts.sha256,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO media_external_files(file_id,canonical_path,size_bytes,mtime_ms,sha256,file_identity,revision,last_checked_at,access_epoch,volume_id) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8,?9)",
                    params![
                        file,
                        facts.path.to_string_lossy().to_string(),
                        facts.size as i64,
                        (facts.modified_nanos / 1_000_000) as i64,
                        facts.sha256,
                        facts.identity,
                        now,
                        epoch,
                        facts.identity.split(':').next().unwrap_or_default()
                    ],
                )
                .map_err(|e| e.to_string())?;
                let core = crate::asset::repository::register_origin(
                    &tx,
                    &crate::asset::models::CoreOrigin {
                        kind: "media".into(),
                        id: entry.clone(),
                    },
                )?;
                crate::asset::repository::register_files(&tx, &core)?;
                let asset_file: String = tx
                    .query_row(
                        "SELECT id FROM asset_files WHERE asset_id=?1 AND source_id=?2 AND role='original'",
                        params![core, file],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                save_external_grant(
                    &LocalMediaGrants::new(&state.config_directory()),
                    &library,
                    base,
                    &file,
                    &epoch,
                    &facts,
                )?;
                Ok((core, asset_file))
            })
            .unwrap();
        state
            .with_library(|db, base| {
                let checked = resolve_media_file(
                    db,
                    base,
                    &file,
                    &LocalMediaGrants::new(&state.config_directory()),
                )?;
                assert_eq!(fs::read(checked.as_ref()).unwrap(), bytes);
                Ok(())
            })
            .unwrap();
        Fixture {
            state,
            dir,
            library,
            entry,
            file,
            core,
            asset_file,
            source,
            bytes,
            operation: uuid::Uuid::new_v4().to_string(),
            modified,
        }
    }

    fn counts(db: &rusqlite::Connection) -> (i64, i64, i64, i64, i64) {
        let count = |table: &str| {
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        (
            count("media_entries"),
            count("media_files"),
            count("media_external_files"),
            count("asset_core"),
            count("asset_files"),
        )
    }

    fn source_kept(fx: &Fixture) {
        assert_eq!(fs::read(&fx.source).unwrap(), fx.bytes);
        assert_eq!(
            fs::metadata(&fx.source).unwrap().modified().unwrap(),
            fx.modified
        );
    }

    fn ids(db: &rusqlite::Connection, fx: &Fixture) -> (String, String, String) {
        let file: String = db
            .query_row(
                "SELECT primary_file_id FROM media_entries WHERE id=?1",
                [&fx.entry],
                |r| r.get(0),
            )
            .unwrap();
        let core: String = db
            .query_row(
                "SELECT id FROM asset_core WHERE media_entry_id=?1",
                [&fx.entry],
                |r| r.get(0),
            )
            .unwrap();
        let asset_file: String = db
            .query_row(
                "SELECT id FROM asset_files WHERE asset_id=?1 AND role='original'",
                [&core],
                |r| r.get(0),
            )
            .unwrap();
        (file, core, asset_file)
    }

    #[test]
    fn archive_copies_managed_bytes_and_keeps_identity_without_deleting_source() {
        let fx = fixture();
        let report = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap();
        assert_eq!(report.status, "archived");
        assert_eq!(report.entry_id, fx.entry);
        assert_eq!(report.file_id, fx.file);
        assert_eq!(report.core_id, fx.core);
        assert!(report.source_preserved);
        let again = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &uuid::Uuid::new_v4().to_string(),
            &ArchiveOptions::default(),
        )
        .unwrap();
        assert_eq!(again.status, "alreadyManaged");
        assert_eq!(again.file_id, fx.file);
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 0, 1, 1));
                assert_eq!(
                    ids(db, &fx),
                    (fx.file.clone(), fx.core.clone(), fx.asset_file.clone())
                );
                let rel: String = db
                    .query_row(
                        "SELECT rel_path FROM media_files WHERE id=?1",
                        [&fx.file],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(
                    rel,
                    format!("media-library/video/{}/files/course.mp4", fx.entry)
                );
                let checked = resolve_media_file(
                    db,
                    base,
                    &fx.file,
                    &LocalMediaGrants::new(&fx.dir.path().join("no-grants")),
                )
                .unwrap();
                assert!(checked.starts_with(base.canonicalize().unwrap()));
                assert_eq!(fs::read(checked.as_ref()).unwrap(), fx.bytes);
                let detail = crate::media_library::get(db, &fx.entry).unwrap();
                assert_eq!(detail.files[0].storage_mode, "managed");
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "committed");
                Ok(())
            })
            .unwrap();
        source_kept(&fx);
    }

    #[test]
    fn archive_low_space_keeps_external_reference_pending() {
        let fx = fixture();
        let error = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions {
                available_bytes: Some(1),
                ..ArchiveOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("磁盘空间不足"), "{error}");
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 1, 1, 1));
                let rel: String = db
                    .query_row(
                        "SELECT rel_path FROM media_files WHERE id=?1",
                        [&fx.file],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(rel.starts_with("external-references/"));
                assert!(!base
                    .join(format!("media-library/video/{}/files/course.mp4", fx.entry))
                    .exists());
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "pending");
                assert!(resolve_media_file(
                    db,
                    base,
                    &fx.file,
                    &LocalMediaGrants::new(&fx.state.config_directory())
                )
                .is_ok());
                Ok(())
            })
            .unwrap();
        source_kept(&fx);
    }

    #[test]
    fn archive_rejects_replaced_source_and_keeps_one_registration() {
        let fx = fixture();
        fs::write(&fx.source, b"replaced-source-bytes-are-different!!").unwrap();
        let error = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap_err();
        assert!(error.contains("SOURCE_CHANGED"), "{error}");
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 1, 1, 1));
                assert!(!base
                    .join(format!("media-library/video/{}/files/course.mp4", fx.entry))
                    .exists());
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "failed");
                Ok(())
            })
            .unwrap();
        assert_eq!(
            fs::read(&fx.source).unwrap(),
            b"replaced-source-bytes-are-different!!"
        );

        let fx = fixture();
        let error = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions {
                replace_source_after_copy: true,
                ..ArchiveOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("SOURCE_CHANGED"), "{error}");
        let copied = fx
            .state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 1, 1, 1));
                let rel: String = db
                    .query_row(
                        "SELECT rel_path FROM media_files WHERE id=?1",
                        [&fx.file],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(rel.starts_with("external-references/"));
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "ready");
                let staged =
                    base.join(format!("media-library/video/{}/files/course.mp4", fx.entry));
                assert_eq!(fs::read(&staged).unwrap(), fx.bytes);
                Ok(staged)
            })
            .unwrap();
        assert_eq!(fs::read(&fx.source).unwrap(), {
            let mut changed = fx.bytes.clone();
            changed.extend_from_slice(b"x");
            changed
        });
        fs::write(&fx.source, &fx.bytes).unwrap();
        let report = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap();
        assert_eq!(report.status, "archived");
        assert_eq!(report.file_id, fx.file);
        assert_eq!(report.core_id, fx.core);
        fx.state
            .with_library(|db, _| {
                assert_eq!(counts(db), (1, 1, 0, 1, 1));
                assert_eq!(ids(db, &fx).0, fx.file);
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read(&copied).unwrap(), fx.bytes);
        assert_eq!(fs::read(&fx.source).unwrap(), fx.bytes);
    }

    #[test]
    fn archive_cancel_keeps_source_and_registration() {
        let fx = fixture();
        let _control = crate::media::ProcessControl::new(|| true);
        let error = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap_err();
        assert!(error.contains("取消"), "{error}");
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 1, 1, 1));
                assert!(!base
                    .join(format!("media-library/video/{}/files/course.mp4", fx.entry))
                    .exists());
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "cancelled");
                Ok(())
            })
            .unwrap();
        source_kept(&fx);
    }

    #[test]
    fn archive_commit_failure_retries_without_a_second_registration() {
        let fx = fixture();
        let error = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions {
                fail_before_commit: true,
                ..ArchiveOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("提交失败"), "{error}");
        let staged_modified = fx
            .state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 1, 1, 1));
                assert_eq!(recovery_status(base, &fx.operation).unwrap(), "ready");
                let staged =
                    base.join(format!("media-library/video/{}/files/course.mp4", fx.entry));
                assert_eq!(fs::read(&staged).unwrap(), fx.bytes);
                let checked = resolve_media_file(
                    db,
                    base,
                    &fx.file,
                    &LocalMediaGrants::new(&fx.state.config_directory()),
                )
                .unwrap();
                assert_eq!(fs::read(checked.as_ref()).unwrap(), fx.bytes);
                Ok(fs::metadata(&staged).unwrap().modified().unwrap())
            })
            .unwrap();
        source_kept(&fx);
        let report = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap();
        assert_eq!(report.status, "archived");
        assert_eq!(
            (report.entry_id, report.file_id, report.core_id),
            (fx.entry.clone(), fx.file.clone(), fx.core.clone())
        );
        let third = perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap();
        assert_eq!(third.status, "alreadyManaged");
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 0, 1, 1));
                assert_eq!(
                    ids(db, &fx),
                    (fx.file.clone(), fx.core.clone(), fx.asset_file.clone())
                );
                let staged =
                    base.join(format!("media-library/video/{}/files/course.mp4", fx.entry));
                assert_eq!(
                    fs::metadata(&staged).unwrap().modified().unwrap(),
                    staged_modified
                );
                let checked = resolve_media_file(
                    db,
                    base,
                    &fx.file,
                    &LocalMediaGrants::new(&fx.dir.path().join("no-grants")),
                )
                .unwrap();
                assert_eq!(fs::read(checked.as_ref()).unwrap(), fx.bytes);
                Ok(())
            })
            .unwrap();
        source_kept(&fx);
    }

    #[test]
    fn archive_task_reuses_operation_and_backup_restores_managed_file() {
        let fx = fixture();
        let first =
            archive_external_video(&fx.state, &fx.library, &fx.entry, &fx.operation).unwrap();
        let second =
            archive_external_video(&fx.state, &fx.library, &fx.entry, &fx.operation).unwrap();
        assert_eq!(first, second);
        uuid::Uuid::parse_str(&first).unwrap();
        let task = fx.state.tasks.get(&first).unwrap();
        match task.payload {
            crate::tasks::TaskRequest::ArchiveExternalVideo {
                entry_id,
                operation_id,
            } => {
                assert_eq!(entry_id, fx.entry);
                assert_eq!(operation_id, fx.operation);
            }
            _ => panic!("归档任务类型不正确"),
        }
        assert!(archive_external_video(
            &fx.state,
            &fx.library,
            &fx.entry,
            &uuid::Uuid::new_v4().to_string()
        )
        .unwrap_err()
        .contains("进行中"));
        assert!(archive_external_video(
            &fx.state,
            &uuid::Uuid::new_v4().to_string(),
            &fx.entry,
            &fx.operation
        )
        .is_err());
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db).2, 1);
                assert!(!base
                    .join(format!("media-library/video/{}/files/course.mp4", fx.entry))
                    .exists());
                Ok(())
            })
            .unwrap();
        source_kept(&fx);

        let fx = fixture();
        perform(
            &fx.state,
            &fx.library,
            &fx.entry,
            &fx.operation,
            &ArchiveOptions::default(),
        )
        .unwrap();
        let archive = fx.dir.path().join("library-backup.dccassetlib");
        fx.state
            .with_library(|db, base| {
                let summary = crate::backup::external_media_backup_summary(db)?;
                assert_eq!(summary.count, 0);
                crate::backup::export_library(db, base, &archive)?;
                Ok(())
            })
            .unwrap();
        fx.state
            .with_active_mut(|active| crate::backup::restore_library(active, &archive))
            .unwrap();
        fx.state
            .with_library(|db, base| {
                assert_eq!(counts(db), (1, 1, 0, 1, 1));
                let checked = resolve_media_file(
                    db,
                    base,
                    &fx.file,
                    &LocalMediaGrants::new(&fx.dir.path().join("no-grants")),
                )
                .unwrap();
                assert_eq!(fs::read(checked.as_ref()).unwrap(), fx.bytes);
                assert!(super::probe_available(base).unwrap() > fx.bytes.len() as u64);
                Ok(())
            })
            .unwrap();
        source_kept(&fx);
    }
}
