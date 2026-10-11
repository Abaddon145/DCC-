//! A process-local rich clipboard authenticated by the Windows clipboard sequence.
//! OS text remains readable by other applications; structured data never trusts external HTML.
use crate::{knowledge, knowledge_files};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Clone)]
struct ClipboardFragment {
    library: String,
    note: String,
    slice: Value,
    sequence: u32,
}
static CLIPBOARD: OnceLock<Mutex<Option<ClipboardFragment>>> = OnceLock::new();
#[cfg(windows)]
fn sequence() -> u32 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
    }
    // No pointer arguments. The OS increments this for any clipboard owner change.
    unsafe { GetClipboardSequenceNumber() }
}
#[cfg(not(windows))]
fn sequence() -> u32 {
    0
}

fn files(slice: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    let mut nodes = vec![slice];
    while let Some(node) = nodes.pop() {
        let id = if node["type"] == "image" {
            node["attrs"]["src"]
                .as_str()
                .and_then(|s| s.strip_prefix("http://dcc-media.localhost/note/"))
        } else if node["type"] == "attachment" {
            node["attrs"]["attachmentId"].as_str()
        } else {
            None
        };
        if let Some(id) = id {
            if !ids.iter().any(|v| v == id) {
                ids.push(id.to_string());
            }
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    ids
}
pub fn validate(db: &Connection, note: &str, slice: &Value) -> Result<String, String> {
    knowledge::get(db, note)?;
    if slice["openStart"].as_u64().unwrap_or(0) > 64 || slice["openEnd"].as_u64().unwrap_or(0) > 64
    {
        return Err("剪贴板选区层级无效".into());
    }
    let content = slice["content"].as_array().ok_or("剪贴板缺少选区")?;
    let document = json!({"type":"doc","content":content});
    let (text, _) = knowledge::inspect_document(&document)?;
    let ids = files(slice);
    if ids.len() > 100 {
        return Err("一次最多复制 100 个附件".into());
    }
    for id in ids {
        let owned: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_attachments WHERE id=?1 AND note_id=?2)",
                params![id, note],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !owned {
            return Err("不能复制其他笔记的附件地址".into());
        }
    }
    Ok(if text.trim().is_empty() {
        "[笔记图片或附件]".into()
    } else {
        text
    })
}
pub fn write(db: &Connection, library: &str, note: &str, slice: Value) -> Result<(), String> {
    let text = validate(db, note, &slice)?;
    let token = uuid::Uuid::new_v4();
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let html = format!("<span data-stack-note-clip=\"{token}\">{escaped}</span>");
    let mut cached = CLIPBOARD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "剪贴板锁损坏")?;
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_html(html, Some(text)))
        .map_err(|_| "无法写入剪贴板，原内容未剪切")?;
    *cached = Some(ClipboardFragment {
        library: library.into(),
        note: note.into(),
        slice,
        sequence: sequence(),
    });
    Ok(())
}
fn rewrite(node: &mut Value, map: &HashMap<String, String>) {
    if node["type"] == "mindMap" {
        node["attrs"]["id"] = json!(uuid::Uuid::new_v4().to_string());
    }
    if node["type"] == "image" {
        if let Some(id) = node["attrs"]["src"]
            .as_str()
            .and_then(|s| s.strip_prefix("http://dcc-media.localhost/note/"))
            .and_then(|id| map.get(id))
        {
            node["attrs"]["src"] = json!(format!("http://dcc-media.localhost/note/{id}"));
        }
    }
    if node["type"] == "attachment" {
        if let Some(id) = node["attrs"]["attachmentId"]
            .as_str()
            .and_then(|id| map.get(id))
        {
            node["attrs"]["attachmentId"] = json!(id);
        }
    }
    if let Some(children) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in children {
            rewrite(child, map);
        }
    }
}
pub fn transfer(
    db: &mut Connection,
    base: &Path,
    source: &str,
    target: &str,
    mut slice: Value,
) -> Result<Value, String> {
    validate(db, source, &slice)?;
    if knowledge::get(db, target)?.deleted_at.is_some() {
        return Err("请先恢复目标笔记".into());
    }
    if source == target {
        rewrite(&mut slice, &HashMap::new());
        return Ok(slice);
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let mut created = Vec::new();
    let result = (|| {
        let mut map = HashMap::new();
        let mut total = 0u64;
        for id in files(&slice) {
            let (path, _) = knowledge_files::path(&tx, base, &id)?;
            total += std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if total > 1024 * 1024 * 1024 {
                return Err("复制附件总量超过 1 GB".into());
            }
            let item = knowledge_files::import(&tx, base, target, &path)?;
            let (new_path, _) = knowledge_files::path(&tx, base, &item.id)?;
            created.push(new_path);
            map.insert(id, item.id);
        }
        rewrite(&mut slice, &map);
        validate(&tx, target, &slice)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(slice)
    })();
    if result.is_err() {
        for path in created {
            let _ = std::fs::remove_file(path);
        }
    }
    result
}
pub fn read(
    db: &mut Connection,
    base: &Path,
    library: &str,
    target: &str,
) -> Result<Option<Value>, String> {
    let cached = CLIPBOARD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "剪贴板锁损坏")?
        .clone();
    let Some(cached) = cached else {
        return Ok(None);
    };
    if sequence() == 0 || cached.sequence != sequence() {
        return Ok(None);
    }
    if cached.library != library {
        return Err("图片和富文本选区不能跨素材库粘贴，请先导出笔记包".into());
    }
    transfer(db, base, &cached.note, target, cached.slice).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(db: &mut Connection) -> knowledge::Note {
        knowledge::save(
            db,
            knowledge::NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: None,
                title: "test".into(),
                document: json!({"type":"doc","content":[{"type":"paragraph"}]}),
                tags: vec![],
                favorite: false,
                journal_date: None,
            },
        )
        .unwrap()
    }
    #[test]
    fn transfer_images_clones_ownership_and_same_note_retains_id() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&temp.path().join("library.db")).unwrap();
        let a = note(&mut db);
        let b = note(&mut db);
        let source = temp.path().join("image.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let attachment = knowledge_files::import(&db, temp.path(), &a.id, &source).unwrap();
        let slice = json!({"content":[{"type":"image","attrs":{"src":format!("http://dcc-media.localhost/note/{}",attachment.id)}}]});
        assert_eq!(
            transfer(&mut db, temp.path(), &a.id, &a.id, slice.clone()).unwrap(),
            slice
        );
        assert!(validate(&db, &b.id, &slice).is_err());
        let copied = transfer(&mut db, temp.path(), &a.id, &b.id, slice).unwrap();
        validate(&db, &b.id, &copied).unwrap();
        assert!(!copied.to_string().contains(&attachment.id));
    }
}
