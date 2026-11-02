//! Managed note attachments. Callers can read registered UUIDs, never arbitrary paths.
use crate::{file_digest, knowledge};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub note_id: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
}
pub(crate) fn content_type(extension: &str) -> Option<&'static str> {
    match extension {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "pdf" => Some("application/pdf"),
        "txt" | "md" => Some("text/plain; charset=utf-8"),
        _ => None,
    }
}
pub(crate) fn validate_image(path: &Path) -> Result<(), String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (w, h) = reader.into_dimensions().map_err(|_| "无法解析附件图片")?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 {
        return Err("图片尺寸无效或超过 1 亿像素".into());
    }
    Ok(())
}

pub fn stage_clipboard_rgba(
    width: usize,
    height: usize,
    bytes: &[u8],
) -> Result<tempfile::TempDir, String> {
    let pixels = width.checked_mul(height).ok_or("截图尺寸无效")?;
    if pixels == 0 || pixels > 25_000_000 || pixels.checked_mul(4) != Some(bytes.len()) {
        return Err("截图尺寸无效或超过 2500 万像素，请先缩小截图".into());
    }
    let image = image::RgbaImage::from_raw(width as u32, height as u32, bytes.to_vec())
        .ok_or("无法解析剪贴板图片")?;
    let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = staging.path().join("剪贴板截图.png");
    image
        .save(&file)
        .map_err(|e| format!("保存截图失败：{e}"))?;
    Ok(staging)
}
pub fn import(
    db: &Connection,
    base: &Path,
    note_id: &str,
    source: &Path,
) -> Result<Attachment, String> {
    let note = knowledge::get(db, note_id)?;
    if note.deleted_at.is_some() {
        return Err("请先恢复笔记".into());
    }
    Uuid::parse_str(note_id).map_err(|_| "笔记 ID 无效")?;
    let source = source.canonicalize().map_err(|_| "附件源文件不存在")?;
    let extension = source
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = content_type(&extension).ok_or("仅支持 PNG/JPEG/WebP/GIF、PDF 和纯文本附件")?;
    let metadata = fs::metadata(&source).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 100 * 1024 * 1024 {
        return Err("附件必须是小于 100 MB 的普通文件".into());
    }
    let root = base.join("note-attachments");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let base_real = base.canonicalize().map_err(|e| e.to_string())?;
    if !root
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&base_real)
    {
        return Err("附件目录指向素材库外部".into());
    }
    let directory = root.join(note_id);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if !directory
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("附件目录不安全".into());
    }
    let id = Uuid::new_v4().to_string();
    let relative = format!("note-attachments/{note_id}/{id}.{extension}");
    let mut staging = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    let mut original = fs::File::open(&source)
        .map_err(|e| e.to_string())?
        .take(100 * 1024 * 1024 + 1);
    let size = std::io::copy(&mut original, &mut staging).map_err(|e| e.to_string())?;
    if size > 100 * 1024 * 1024 {
        return Err("附件复制过程中超过大小上限".into());
    }
    staging.flush().map_err(|e| e.to_string())?;
    staging.as_file().sync_all().map_err(|e| e.to_string())?;
    if mime.starts_with("image/") {
        validate_image(staging.path())?;
    }
    let (_, sha) = file_digest::digest_file(staging.path())?;
    let target = base.join(&relative);
    staging
        .persist_noclobber(&target)
        .map_err(|e| e.to_string())?;
    let filename = source.file_name().unwrap().to_string_lossy().to_string();
    let imported = Attachment {
        id,
        note_id: note_id.into(),
        filename,
        mime: mime.into(),
        size,
    };
    if let Err(error) = insert_metadata(db, &imported, &relative, &sha) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(imported)
}
fn insert_metadata(
    db: &Connection,
    item: &Attachment,
    relative: &str,
    sha: &str,
) -> Result<(), String> {
    db.execute("INSERT INTO knowledge_attachments(id,note_id,filename,relative_path,mime,sha256,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![item.id,item.note_id,item.filename,relative,item.mime,sha,item.size,Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    Ok(())
}
pub(crate) fn register_published_jpeg(
    db: &Connection,
    base: &Path,
    note_id: &str,
    id: &str,
    filename: &str,
    sha: &str,
    size: u64,
) -> Result<Attachment, String> {
    if ![note_id, id]
        .iter()
        .all(|v| Uuid::parse_str(v).is_ok_and(|id| id.to_string() == *v))
        || size == 0
        || size > 256 * 1024
        || filename.is_empty()
        || filename.len() > 1000
        || filename.chars().any(char::is_control)
    {
        return Err("已发布截图登记无效".into());
    }
    if knowledge::get(db, note_id)?.deleted_at.is_some() {
        return Err("请先恢复目标笔记".into());
    }
    let relative = format!("note-attachments/{note_id}/{id}.jpg");
    let path = crate::media::safe_relative(base, &relative)?;
    crate::media_access::reject_links(&path)?;
    if !path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(base.canonicalize().map_err(|e| e.to_string())?)
        || fs::metadata(&path).map_err(|e| e.to_string())?.len() != size
    {
        return Err("截图副本缺失或大小不符".into());
    }
    validate_image(&path)?;
    if file_digest::digest_file(&path)?.1 != sha {
        return Err("截图副本摘要不符".into());
    }
    let item = Attachment {
        id: id.into(),
        note_id: note_id.into(),
        filename: filename.into(),
        mime: "image/jpeg".into(),
        size,
    };
    insert_metadata(db, &item, &relative, sha)?;
    Ok(item)
}
pub fn path(db: &Connection, base: &Path, id: &str) -> Result<(PathBuf, String), String> {
    Uuid::parse_str(id).map_err(|_| "附件 ID 无效")?;
    let (relative, mime): (String, String) = db
        .query_row(
            "SELECT relative_path,mime FROM knowledge_attachments WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "附件不存在")?;
    let relative = Path::new(&relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("附件路径不安全".into());
    }
    let root = base
        .join("note-attachments")
        .canonicalize()
        .map_err(|_| "附件目录不存在")?;
    let base = base.canonicalize().map_err(|_| "素材库目录不存在")?;
    let full = base
        .join(relative)
        .canonicalize()
        .map_err(|_| "附件文件缺失")?;
    if !root.starts_with(&base) || !full.starts_with(&root) || !full.is_file() {
        return Err("附件路径越界".into());
    }
    Ok((full, mime))
}
pub fn response(
    db: &Connection,
    base: &Path,
    id: &str,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let (path, mime) = path(db, base, id)?;
    if !mime.starts_with("image/") {
        return Err("非图片附件请使用系统程序打开".into());
    }
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 100 * 1024 * 1024 {
        return Err("图片附件过大".into());
    }
    tauri::http::Response::builder()
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Access-Control-Allow-Origin", "*")
        .body(fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn purge(
    db: &mut Connection,
    base: &Path,
    note_id: &str,
    expected_revision: i64,
) -> Result<(), String> {
    Uuid::parse_str(note_id).map_err(|_| "笔记 ID 无效")?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let note = knowledge::get(&tx, note_id)?;
    if note.deleted_at.is_none() || note.revision != expected_revision {
        return Err("只能永久删除回收站中版本未变化的笔记".into());
    }
    let directory = base.join("note-attachments").join(note_id);
    let staging = tempfile::Builder::new()
        .prefix(".note-purge-")
        .tempdir_in(base)
        .map_err(|e| e.to_string())?;
    let staged = staging.path().join("attachments");
    if directory.exists() {
        let root = base
            .join("note-attachments")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let canonical = directory.canonicalize().map_err(|e| e.to_string())?;
        if !root.starts_with(base.canonicalize().map_err(|e| e.to_string())?)
            || !canonical.starts_with(&root)
        {
            return Err("笔记附件目录越界".into());
        }
        fs::rename(&directory, &staged).map_err(|e| format!("无法清理附件，笔记保持原状：{e}"))?;
    }
    let result = (|| {
        tx.execute("DELETE FROM knowledge_search WHERE note_id=?1", [note_id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM knowledge_notes WHERE id=?1", [note_id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        if staged.exists() {
            if let Err(restore_error) = fs::rename(&staged, &directory) {
                let retained = staging.keep();
                return Err(format!(
                    "删除已回滚；附件暂存于 {}，恢复目录失败：{restore_error}；{error}",
                    retained.display()
                ));
            }
        }
        return Err(error);
    }
    staging
        .close()
        .map_err(|e| format!("笔记已永久删除，但临时附件清理失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshot_staging_validates_pixels_and_cleans_temporary_file() {
        assert!(stage_clipboard_rgba(0, 1, &[]).is_err());
        assert!(stage_clipboard_rgba(1, 1, &[0, 0, 0]).is_err());
        assert!(stage_clipboard_rgba(usize::MAX, 2, &[]).is_err());
        let staged = stage_clipboard_rgba(1, 1, &[20, 40, 60, 255]).unwrap();
        let path = staged.path().join("剪贴板截图.png");
        assert_eq!(
            image::open(&path).unwrap().to_rgba8().get_pixel(0, 0).0,
            [20, 40, 60, 255]
        );
        drop(staged);
        assert!(!path.exists());
    }
    #[test]
    fn trash_retains_files_until_explicit_purge() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let note = knowledge::save(
            &mut db,
            knowledge::NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: None,
                title: "学习资料".into(),
                document: serde_json::json!({"type":"doc","content":[]}),
                tags: vec![],
                favorite: false,
                journal_date: None,
            },
        )
        .unwrap();
        let source = temp.path().join("learning.txt");
        fs::write(&source, b"notes").unwrap();
        let item = import(&db, &base, &note.id, &source).unwrap();
        let managed = path(&db, &base, &item.id).unwrap().0;
        assert!(purge(&mut db, &base, &note.id, note.revision).is_err());
        let deleted = knowledge::set_deleted(&mut db, &note.id, note.revision, true).unwrap();
        assert!(managed.exists());
        let restored = knowledge::set_deleted(&mut db, &note.id, deleted.revision, false).unwrap();
        assert!(path(&db, &base, &item.id).is_ok());
        let deleted = knowledge::set_deleted(&mut db, &note.id, restored.revision, true).unwrap();
        purge(&mut db, &base, &note.id, deleted.revision).unwrap();
        assert!(!managed.exists());
        assert!(knowledge::get(&db, &note.id).is_err());
        assert!(source.exists());
    }
    #[test]
    fn attachment_copied_and_path_escape_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let note = knowledge::save(
            &mut db,
            knowledge::NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: None,
                title: "笔记".into(),
                document: serde_json::json!({"type":"doc","content":[]}),
                tags: vec![],
                favorite: false,
                journal_date: None,
            },
        )
        .unwrap();
        let source = temp.path().join("note.txt");
        fs::write(&source, b"learning").unwrap();
        let attachment = import(&db, &base, &note.id, &source).unwrap();
        fs::remove_file(&source).unwrap();
        assert_eq!(
            fs::read(path(&db, &base, &attachment.id).unwrap().0).unwrap(),
            b"learning"
        );
        assert!(response(&db, &base, &attachment.id).is_err());
        db.execute(
            "UPDATE knowledge_attachments SET relative_path='../outside.txt' WHERE id=?1",
            [&attachment.id],
        )
        .unwrap();
        assert!(path(&db, &base, &attachment.id).is_err());
    }
}
