//! Immutable source bytes. This is not the editable note document or a Vault writer.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

const MAX_BYTES: usize = 5 * 1024 * 1024;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_id(id: &str) -> Result<(), String> {
    let uuid = Uuid::parse_str(id).map_err(|_| "归档标识无效")?;
    if uuid.to_string() != id {
        return Err("归档标识必须使用规范 UUID".into());
    }
    Ok(())
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

pub struct ArchiveSource<'a> {
    pub profile_id: &'a str,
    pub source_key: &'a str,
    pub version_key: &'a str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteArchive {
    pub id: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveItem {
    pub id: String,
    pub archive_id: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub sha256: String,
    pub label: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveBundle {
    pub id: String,
    pub profile_id: String,
    pub source_key: String,
    pub version_key: String,
    pub sha256: String,
    pub size: u64,
    pub items: Vec<ArchiveItem>,
}
pub fn bundle_items(
    db: &Connection,
    ids: &std::collections::HashSet<String>,
) -> Result<Vec<ArchiveBundle>, String> {
    let mut bundles = Vec::new();
    for id in ids {
        valid_id(id)?;
        let item=db.query_row("SELECT id,archive_id,start_byte,end_byte,sha256,label FROM learning_note_archive_items WHERE id=?1",[id],item_row).map_err(error)?;
        let (profile_id, origin_hash): (String, String) = db
            .query_row(
                "SELECT source_profile_id,sha256 FROM learning_note_archives WHERE id=?1",
                [&item.archive_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(error)?;
        let size = item
            .end_byte
            .checked_sub(item.start_byte)
            .ok_or("原文范围无效")?;
        // Portable notes carry the selected bytes, never unrelated parts of the source document.
        bundles.push(ArchiveBundle {
            id: item.id.clone(),
            profile_id,
            source_key: format!("dcc-note-item/{}", item.id),
            version_key: origin_hash,
            sha256: item.sha256.clone(),
            size,
            items: vec![ArchiveItem {
                archive_id: item.id.clone(),
                start_byte: 0,
                end_byte: size,
                ..item
            }],
        });
    }
    bundles.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(bundles)
}
pub fn verify_bundle(bundle: &ArchiveBundle, bytes: &[u8]) -> Result<(), String> {
    valid_id(&bundle.id)?;
    valid_id(&bundle.profile_id)?;
    if !valid_text(&bundle.source_key, 4096)
        || !valid_text(&bundle.version_key, 300)
        || bundle.items.is_empty()
        || bytes.len() > MAX_BYTES
        || bytes.len() as u64 != bundle.size
        || digest(bytes) != bundle.sha256
    {
        return Err("笔记包原文归档清单或摘要无效".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut ranges = std::collections::HashSet::new();
    for item in &bundle.items {
        valid_id(&item.id)?;
        if item.archive_id != bundle.id
            || !valid_text(&item.label, 2000)
            || !ids.insert(&item.id)
            || !ranges.insert((item.start_byte, item.end_byte))
            || digest(checked_slice(bytes, item.start_byte, item.end_byte)?) != item.sha256
        {
            return Err("笔记包原文片段范围、标识或摘要无效".into());
        }
    }
    Ok(())
}
pub fn import_bundle(
    db: &Connection,
    base: &Path,
    bundle: &ArchiveBundle,
    bytes: &[u8],
) -> Result<std::collections::HashMap<String, String>, String> {
    verify_bundle(bundle, bytes)?;
    let archive = archive_bytes(
        db,
        base,
        &ArchiveSource {
            profile_id: &bundle.profile_id,
            source_key: &bundle.source_key,
            version_key: &bundle.version_key,
        },
        bytes,
        &bundle.sha256,
    )?;
    let mut map = std::collections::HashMap::new();
    for item in &bundle.items {
        let saved = add_archive_item(
            db,
            base,
            &archive.id,
            item.start_byte,
            item.end_byte,
            &item.label,
        )?;
        map.insert(item.id.clone(), saved.id);
    }
    Ok(map)
}
pub fn archive_bytes(
    db: &Connection,
    base: &Path,
    source: &ArchiveSource<'_>,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<NoteArchive, String> {
    valid_id(source.profile_id)?;
    if !db.is_autocommit() {
        return Err("归档文件发布不能嵌套在外层事务中".into());
    }
    if !valid_text(source.source_key, 4096) || !valid_text(source.version_key, 300) {
        return Err("归档来源或版本标识无效".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("原始笔记超过5MiB，未截断归档".into());
    }
    if expected_sha256.len() != 64 || digest(bytes) != expected_sha256 {
        return Err("原始笔记摘要不匹配，未归档".into());
    }
    let tx = db.unchecked_transaction().map_err(error)?;
    let existing:Option<NoteArchive>=tx.query_row(
        "SELECT id,sha256,size FROM learning_note_archives WHERE source_profile_id=?1 AND source_key=?2 AND version_key=?3 AND sha256=?4",
        params![source.profile_id,source.source_key,source.version_key,expected_sha256],
        |r|Ok(NoteArchive{id:r.get(0)?,sha256:r.get(1)?,size:r.get(2)?})
    ).optional().map_err(error)?;
    if let Some(existing) = existing {
        read_archive(&tx, base, &existing.id)?;
        tx.commit().map_err(error)?;
        return Ok(existing);
    }
    crate::media_access::reject_links(base)?;
    let base = base.canonicalize().map_err(error)?;
    if !base.is_dir() {
        return Err("归档目标不是素材库目录".into());
    }
    let directory = base.join("learning-data").join("archives");
    crate::media_access::reject_links(&directory)?;
    fs::create_dir_all(&directory).map_err(error)?;
    crate::media_access::reject_links(&directory)?;
    #[cfg(windows)]
    let _directories = {
        let (actual, directories) = super::source_markdown::lock_vault_root(&directory)?;
        if actual != directory {
            return Err("原文归档目录已变化，未写入".into());
        }
        directories
    };
    crate::runtime::tools::ensure_space(&directory, (bytes.len() + 1024 * 1024) as u64)?;
    let id = Uuid::new_v4().to_string();
    let relative = format!("learning-data/archives/{id}.bin");
    let target = directory.join(format!("{id}.bin"));
    let mut stage = tempfile::NamedTempFile::new_in(&directory).map_err(error)?;
    stage.write_all(bytes).map_err(error)?;
    stage.flush().map_err(error)?;
    stage.as_file().sync_all().map_err(error)?;
    stage.persist_noclobber(&target).map_err(error)?;
    let publish = (|| {
        tx.execute("INSERT INTO learning_note_archives(id,source_profile_id,source_key,version_key,sha256,relative_path,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id,source.profile_id,source.source_key,source.version_key,expected_sha256,relative,bytes.len() as u64,chrono::Utc::now().to_rfc3339()]).map_err(error)?;
        tx.commit().map_err(error)
    })();
    if let Err(failure) = publish {
        // Only the new, exclusively published UUID file is ours to remove.
        let _ = fs::remove_file(&target);
        return Err(failure);
    }
    Ok(NoteArchive {
        id,
        sha256: expected_sha256.into(),
        size: bytes.len() as u64,
    })
}
pub fn read_archive(db: &Connection, base: &Path, id: &str) -> Result<Vec<u8>, String> {
    valid_id(id)?;
    let (relative, hash, size): (String, String, u64) = db
        .query_row(
            "SELECT relative_path,sha256,size FROM learning_note_archives WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(error)?
        .ok_or("归档不存在或不属于当前素材库")?;
    if relative != format!("learning-data/archives/{id}.bin") || size > MAX_BYTES as u64 {
        return Err("归档登记路径或大小无效".into());
    }
    let path = super::files::learning_file(base, &relative)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "归档正在变化、缺失或不可读")?;
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.len() != size {
        return Err("归档文件大小已变化".into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 != size || digest(&bytes) != hash {
        return Err("归档内容摘要不匹配，未返回损坏内容".into());
    }
    Ok(bytes)
}
pub(crate) fn add_archive_items(
    db: &Connection,
    base: &Path,
    archive_id: &str,
    items: &[(u64, u64, &str)],
) -> Result<Vec<ArchiveItem>, String> {
    if items.len() > 100_000 {
        return Err("归档片段过多".into());
    }
    let bytes = read_archive(db, base, archive_id)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "原字节不是有效UTF-8，仅保留归档待处理")?;
    let hashes = items
        .iter()
        .map(|(start, end, label)| {
            if !valid_text(label, 2000) {
                return Err("归档片段标题无效或超过2000字节".into());
            }
            checked_text_slice(text, *start, *end).map(digest)
        })
        .collect::<Result<Vec<_>, String>>()?;
    db.execute_batch("SAVEPOINT learning_note_archive_batch")
        .map_err(error)?;
    let result = (|| {
        let mut saved = Vec::with_capacity(items.len());
        for ((start, end, label), hash) in items.iter().zip(hashes) {
            let id = Uuid::new_v4().to_string();
            db.execute("INSERT INTO learning_note_archive_items(id,archive_id,start_byte,end_byte,sha256,label) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(archive_id,start_byte,end_byte) DO NOTHING",params![id,archive_id,start,end,hash,label]).map_err(error)?;
            let item=db.query_row("SELECT id,archive_id,start_byte,end_byte,sha256,label FROM learning_note_archive_items WHERE archive_id=?1 AND start_byte=?2 AND end_byte=?3",params![archive_id,start,end],item_row).map_err(error)?;
            if item.sha256 != hash {
                return Err("已登记归档片段摘要损坏，未覆盖".into());
            }
            saved.push(item);
        }
        Ok(saved)
    })();
    if result.is_err() {
        db.execute_batch(
            "ROLLBACK TO learning_note_archive_batch; RELEASE learning_note_archive_batch",
        )
        .map_err(error)?;
    } else {
        db.execute_batch("RELEASE learning_note_archive_batch")
            .map_err(error)?;
    }
    result
}
pub fn add_archive_item(
    db: &Connection,
    base: &Path,
    archive_id: &str,
    start_byte: u64,
    end_byte: u64,
    label: &str,
) -> Result<ArchiveItem, String> {
    add_archive_items(db, base, archive_id, &[(start_byte, end_byte, label)])?
        .pop()
        .ok_or("未登记原文片段".into())
}
fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ArchiveItem> {
    Ok(ArchiveItem {
        id: r.get(0)?,
        archive_id: r.get(1)?,
        start_byte: r.get(2)?,
        end_byte: r.get(3)?,
        sha256: r.get(4)?,
        label: r.get(5)?,
    })
}
fn checked_slice(bytes: &[u8], start: u64, end: u64) -> Result<&[u8], String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "原字节不是有效UTF-8，仅保留归档待处理")?;
    checked_text_slice(text, start, end)
}
fn checked_text_slice(text: &str, start: u64, end: u64) -> Result<&[u8], String> {
    if start >= end
        || end > text.len() as u64
        || !text.is_char_boundary(start as usize)
        || !text.is_char_boundary(end as usize)
    {
        return Err("归档片段范围越界或切开UTF-8字符".into());
    }
    Ok(&text.as_bytes()[start as usize..end as usize])
}
pub fn read_archive_item(db: &Connection, base: &Path, id: &str) -> Result<Vec<u8>, String> {
    valid_id(id)?;
    let item=db.query_row("SELECT id,archive_id,start_byte,end_byte,sha256,label FROM learning_note_archive_items WHERE id=?1",[id],item_row)
        .optional().map_err(error)?.ok_or("归档片段不存在或不属于当前素材库")?;
    let bytes = read_archive(db, base, &item.archive_id)?;
    let slice = checked_slice(&bytes, item.start_byte, item.end_byte)?;
    if digest(slice) != item.sha256 {
        return Err("归档片段摘要不匹配".into());
    }
    Ok(slice.to_vec())
}
