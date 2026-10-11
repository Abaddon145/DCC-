//! Library-scoped notes. All content changes use optimistic revisions and one transaction.
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

type Result<T> = std::result::Result<T, String>;
fn error(e: rusqlite::Error) -> String {
    e.to_string()
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn uid() -> String {
    Uuid::new_v4().to_string()
}

pub fn migrate(db: &Connection) -> Result<()> {
    let tx = db.unchecked_transaction().map_err(error)?;
    tx.execute_batch(include_str!("notes_schema.sql"))
        .map_err(error)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS knowledge_reviews(note_id TEXT PRIMARY KEY REFERENCES knowledge_notes(id) ON DELETE CASCADE,due_date TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('waiting','mastered'))); CREATE INDEX IF NOT EXISTS idx_knowledge_reviews_due ON knowledge_reviews(status,due_date);").map_err(error)?;
    let has_color = tx
        .prepare("PRAGMA table_info(knowledge_notes)")
        .map_err(error)?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?
        .iter()
        .any(|name| name == "navigation_color");
    if !has_color {
        tx.execute_batch("ALTER TABLE knowledge_notes ADD COLUMN navigation_color TEXT;")
            .map_err(error)?;
    }
    tx.execute("UPDATE schema_meta SET value='17' WHERE key='schema_version' AND CAST(value AS INTEGER)<17", []).map_err(error)?;
    tx.commit().map_err(error)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(default)]
    pub color: Option<String>,
    pub id: String,
    pub notebook_id: Option<String>,
    pub title: String,
    pub document: Value,
    pub plain_text: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub revision: i64,
    pub journal_date: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteInput {
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub notebook_id: Option<String>,
    pub title: String,
    pub document: Value,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub journal_date: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: String,
    pub note_id: String,
    pub snapshot: Note,
    pub pinned: bool,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionSummary {
    pub id: String,
    pub note_id: String,
    pub title: String,
    pub pinned: bool,
    pub created_at: String,
}
pub fn revision_summaries(
    db: &Connection,
    note_id: &str,
    offset: u32,
) -> Result<Vec<RevisionSummary>> {
    get(db, note_id)?;
    let mut statement=db.prepare("SELECT id,note_id,json_extract(snapshot_json,'$.title'),pinned,created_at FROM knowledge_revisions WHERE note_id=?1 ORDER BY created_at DESC,id DESC LIMIT 100 OFFSET ?2").map_err(error)?;
    let rows = statement
        .query_map(params![note_id, offset], |r| {
            Ok(RevisionSummary {
                id: r.get(0)?,
                note_id: r.get(1)?,
                title: r.get(2)?,
                pinned: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    Ok(rows)
}
pub fn revision(db: &Connection, note_id: &str, id: &str) -> Result<Revision> {
    db.query_row("SELECT id,note_id,snapshot_json,pinned,created_at FROM knowledge_revisions WHERE id=?1 AND note_id=?2",params![id,note_id],|r|{let json:String=r.get(2)?;Ok(Revision{id:r.get(0)?,note_id:r.get(1)?,snapshot:serde_json::from_str(&json).map_err(|e|rusqlite::Error::FromSqlConversionFailure(2,rusqlite::types::Type::Text,Box::new(e)))?,pinned:r.get(3)?,created_at:r.get(4)?})}).map_err(error)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotePage {
    pub items: Vec<NoteSummary>,
    pub total: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    #[serde(default)]
    pub color: Option<String>,
    pub id: String,
    pub title: String,
    pub plain_text: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteReference {
    pub kind: String,
    pub target_id: String,
    pub label: String,
    pub available: bool,
}

pub fn references(db: &Connection, note_id: &str) -> Result<Vec<NoteReference>> {
    get(db, note_id)?;
    let mut statement = db.prepare("SELECT target_kind,target_id,label FROM knowledge_references WHERE note_id=?1 ORDER BY rowid").map_err(error)?;
    let rows = statement
        .query_map([note_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    rows.into_iter().map(|(kind,target_id,label)| {
        let sql = match kind.as_str() {
            "note" => "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL)",
            "asset" => "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
            "media" => "SELECT EXISTS(SELECT 1 FROM media_entries WHERE id=?1 AND deleted_at IS NULL)",
            "project" => "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            _ => return Err("未知引用类型".into()),
        };
        let available = db.query_row(sql,[&target_id],|r|r.get(0)).map_err(error)?;
        Ok(NoteReference{kind,target_id,label,available})
    }).collect()
}

pub fn backlinks(db: &Connection, note_id: &str) -> Result<Vec<NoteSummary>> {
    let mut statement=db.prepare("SELECT n.id,n.title,substr(n.plain_text,1,240),n.tags_json,n.favorite,n.updated_at,n.navigation_color FROM knowledge_notes n JOIN knowledge_references r ON r.note_id=n.id WHERE r.target_kind='note' AND r.target_id=?1 AND n.deleted_at IS NULL ORDER BY n.updated_at DESC LIMIT 100").map_err(error)?;
    let notes = statement
        .query_map([note_id], |row| {
            let tags: String = row.get(3)?;
            Ok(NoteSummary {
                color: row.get(6)?,
                id: row.get(0)?,
                title: row.get(1)?,
                plain_text: row.get(2)?,
                tags: serde_json::from_str(&tags).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                favorite: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    Ok(notes)
}

pub fn delete_empty_notebook(db: &Connection, id: &str) -> Result<()> {
    // SQLite RESTRICT rejects nonempty notebooks; never silently moves or deletes descendants.
    if db
        .execute("DELETE FROM knowledge_notebooks WHERE id=?1", [id])
        .map_err(|_| "笔记本包含子笔记本或笔记（包括回收站），请先移出内容".to_string())?
        != 1
    {
        return Err("笔记本不存在".into());
    }
    Ok(())
}

fn read_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let document: String = row.get(3)?;
    let tags: String = row.get(5)?;
    let parse_error = |e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    };
    Ok(Note {
        color: row.get(12)?,
        id: row.get(0)?,
        notebook_id: row.get(1)?,
        title: row.get(2)?,
        document: serde_json::from_str(&document).map_err(parse_error)?,
        plain_text: row.get(4)?,
        tags: serde_json::from_str(&tags).map_err(parse_error)?,
        favorite: row.get(6)?,
        revision: row.get(7)?,
        journal_date: row.get(8)?,
        deleted_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}
const COLUMNS: &str = "n.id,n.notebook_id,n.title,n.document_json,n.plain_text,n.tags_json,n.favorite,n.revision,n.journal_date,n.deleted_at,n.created_at,n.updated_at,n.navigation_color";
pub fn set_color(db: &Connection, id: &str, color: Option<&str>) -> Result<Note> {
    if let Some(value) = color {
        if value.len() != 7
            || !value.starts_with('#')
            || !value[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("请选择 #RRGGBB 格式的笔记颜色".into());
        }
    }
    if db
        .execute(
            "UPDATE knowledge_notes SET navigation_color=?2 WHERE id=?1",
            params![id, color.map(str::to_ascii_uppercase)],
        )
        .map_err(error)?
        != 1
    {
        return Err("笔记不存在".into());
    }
    get(db, id)
}
pub fn get(db: &Connection, id: &str) -> Result<Note> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM knowledge_notes n WHERE n.id=?1"),
        [id],
        read_note,
    )
    .optional()
    .map_err(error)?
    .ok_or("笔记不存在".into())
}

// Extract text and references from the same document that is persisted; never trust a separate index payload.
pub(crate) fn inspect_document(
    document: &Value,
) -> Result<(String, Vec<(String, String, String)>)> {
    if document.get("type").and_then(Value::as_str) != Some("doc") {
        return Err("笔记文档格式无效".into());
    }
    if serde_json::to_vec(document)
        .map_err(|e| e.to_string())?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("笔记内容超过 4 MB，请拆分笔记或使用附件".into());
    }
    let mut text = String::new();
    if let Some(blocks) = document["content"].as_array() {
        let mut previous = None;
        let mut topic_ids = std::collections::HashSet::new();
        for block in blocks {
            if let Some(id) = block["attrs"]["topicId"].as_str() {
                if Uuid::parse_str(id).is_err() || !topic_ids.insert(id) {
                    return Err("主题标识无效或重复".into());
                }
            }
            if let Some(color) = block["attrs"]["topicColor"].as_str() {
                if color.len() != 7
                    || !color.starts_with('#')
                    || !color[1..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("主题颜色格式无效".into());
                }
            }
            if let Some(value) = block["attrs"].get("topicDepth").filter(|v| !v.is_null()) {
                let depth = value
                    .as_u64()
                    .filter(|n| *n <= 32)
                    .ok_or("主题层级无效或超过 32 层")?;
                if depth > previous.map_or(0, |n| n + 1) {
                    return Err("主题层级不能跳过父级".into());
                }
                previous = Some(depth);
            } else {
                // Legacy documents remain readable; conversion happens in the editor.
                previous = None;
            }
        }
    }
    let mut references = Vec::new();
    let mut stack = vec![(document, 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth > 64 {
            return Err("笔记嵌套层级过深".into());
        }
        let kind = node
            .get("type")
            .and_then(Value::as_str)
            .ok_or("笔记节点缺少类型")?;
        if ![
            "doc",
            "paragraph",
            "text",
            "heading",
            "bulletList",
            "orderedList",
            "listItem",
            "taskList",
            "taskItem",
            "blockquote",
            "codeBlock",
            "hardBreak",
            "horizontalRule",
            "table",
            "tableRow",
            "tableCell",
            "tableHeader",
            "image",
            "remoteImage",
            "reference",
            "attachment",
            "mindMap",
        ]
        .contains(&kind)
        {
            return Err("笔记含不支持的节点类型".into());
        }
        if let Some(marks) = node.get("marks").and_then(Value::as_array) {
            for mark in marks {
                let mark_kind = mark["type"].as_str().unwrap_or("");
                if !["bold", "italic", "strike", "code", "underline", "link"].contains(&mark_kind) {
                    return Err("笔记含不支持的文本标记".into());
                }
                if mark_kind == "link" {
                    let url = url::Url::parse(mark["attrs"]["href"].as_str().unwrap_or(""))
                        .map_err(|_| "笔记链接无效")?;
                    if !["http", "https"].contains(&url.scheme()) {
                        return Err("笔记链接只允许 HTTP/HTTPS".into());
                    }
                }
            }
        }
        if kind == "image" {
            let source = node["attrs"]["src"].as_str().unwrap_or("");
            let attachment = source
                .strip_prefix("http://dcc-media.localhost/note/")
                .ok_or("图片必须先复制为笔记附件")?;
            Uuid::parse_str(attachment).map_err(|_| "笔记图片 ID 无效")?;
        }
        if kind == "mindMap" {
            Uuid::parse_str(node["attrs"]["id"].as_str().unwrap_or(""))
                .map_err(|_| "思维导图 ID 无效")?;
            let data = &node["attrs"]["data"];
            let object = data.as_object().ok_or("思维导图内容无效")?;
            if object
                .keys()
                .any(|key| !["nodeData", "direction"].contains(&key.as_str()))
            {
                return Err("思维导图含不支持的内容".into());
            }
            let mut maps = vec![(&data["nodeData"], 0usize)];
            let mut ids = std::collections::HashSet::new();
            while let Some((entry, level)) = maps.pop() {
                let object = entry.as_object().ok_or("思维导图节点无效")?;
                if level > 32
                    || ids.len() >= 1000
                    || object.keys().any(|key| {
                        ![
                            "id",
                            "topic",
                            "children",
                            "root",
                            "expanded",
                            "direction",
                            "metadata",
                        ]
                        .contains(&key.as_str())
                    })
                {
                    return Err("思维导图过大或含不支持的节点属性".into());
                }
                let id = entry["id"].as_str().ok_or("思维导图节点缺少 ID")?;
                if id.is_empty()
                    || id.len() > 80
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                    || !ids.insert(id)
                {
                    return Err("思维导图节点 ID 无效或重复".into());
                }
                let topic = entry["topic"].as_str().ok_or("思维导图缺少文字")?;
                if let Some(metadata) = entry.get("metadata") {
                    let metadata = metadata.as_object().ok_or("导图格式无效")?;
                    if metadata.len() != 1 {
                        return Err("导图格式包含未知属性".into());
                    }
                    let format = metadata
                        .get("format")
                        .and_then(Value::as_object)
                        .ok_or("导图格式无效")?;
                    for (key, value) in format {
                        match key.as_str() {
                            "bold" | "italic" | "underline" | "strike" | "code"
                                if value.is_boolean() => {}
                            "heading" if value.as_u64().is_some_and(|n| n <= 3) => {}
                            "link" => {
                                let link = value.as_str().ok_or("导图链接无效")?;
                                let url = url::Url::parse(link).map_err(|_| "导图链接无效")?;
                                if link.len() > 8192 || !matches!(url.scheme(), "http" | "https") {
                                    return Err("导图链接仅支持 http/https".into());
                                }
                            }
                            _ => return Err("导图格式包含不支持的属性".into()),
                        }
                    }
                }
                if topic.chars().count() > 1000 {
                    return Err("思维导图文字过长".into());
                }
                text.push_str(topic);
                text.push('\n');
                if let Some(children) = entry.get("children") {
                    let children = children.as_array().ok_or("思维导图子节点无效")?;
                    maps.extend(children.iter().rev().map(|child| (child, level + 1)));
                }
            }
        }
        if kind == "remoteImage" {
            let source = node["attrs"]["src"]
                .as_str()
                .ok_or("网络图片占位缺少地址")?;
            let url = url::Url::parse(source).map_err(|_| "网络图片地址无效")?;
            if !["http", "https"].contains(&url.scheme())
                || source.len() > 8192
                || node["attrs"]["alt"].as_str().unwrap_or("").len() > 4000
                || node["attrs"]["error"].as_str().unwrap_or("").len() > 2000
            {
                return Err("网络图片占位内容无效".into());
            }
            text.push_str(node["attrs"]["alt"].as_str().unwrap_or("网络图片"));
        }
        if let Some(value) = node.get("text").and_then(Value::as_str) {
            text.push_str(value);
        }
        if node.get("type").and_then(Value::as_str) == Some("reference") {
            let attrs = &node["attrs"];
            let kind = attrs["kind"].as_str().unwrap_or("");
            let id = attrs["targetId"].as_str().unwrap_or("");
            let label = attrs["label"].as_str().unwrap_or("");
            if !["note", "asset", "media", "project"].contains(&kind)
                || Uuid::parse_str(id).is_err()
                || label.len() > 2000
            {
                return Err("笔记引用格式无效".into());
            }
            references.push((kind.into(), id.into(), label.into()));
            text.push_str(label);
        }
        if let Some(children) = node.get("content").and_then(Value::as_array) {
            if node["type"] != "doc" {
                text.push('\n');
            }
            stack.extend(children.iter().rev().map(|child| (child, depth + 1)));
        }
    }
    Ok((text, references))
}
fn index(db: &Connection, note: &Note) -> Result<()> {
    db.execute("DELETE FROM knowledge_search WHERE note_id=?1", [&note.id])
        .map_err(error)?;
    if note.deleted_at.is_none() {
        db.execute(
            "INSERT INTO knowledge_search(note_id,title,plain_text,tags) VALUES(?1,?2,?3,?4)",
            params![note.id, note.title, note.plain_text, note.tags.join(" ")],
        )
        .map_err(error)?;
    }
    Ok(())
}
fn checkpoint(db: &Connection, previous: &Note, force: bool) -> Result<()> {
    let last: Option<String> = db.query_row("SELECT created_at FROM knowledge_revisions WHERE note_id=?1 ORDER BY created_at DESC LIMIT 1",[&previous.id],|r|r.get(0)).optional().map_err(error)?;
    let elapsed = last
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|v| Utc::now().signed_duration_since(v).num_seconds());
    if force || elapsed.is_none_or(|seconds| seconds >= 60) {
        db.execute("INSERT INTO knowledge_revisions(id,note_id,snapshot_json,created_at) VALUES(?1,?2,?3,?4)",params![uid(),previous.id,serde_json::to_string(previous).map_err(|e|e.to_string())?,now()]).map_err(error)?;
    }
    db.execute("DELETE FROM knowledge_revisions WHERE note_id=?1 AND pinned=0 AND id NOT IN (SELECT id FROM knowledge_revisions WHERE note_id=?1 AND pinned=0 ORDER BY created_at DESC,id DESC LIMIT 50)",[&previous.id]).map_err(error)?;
    Ok(())
}
pub fn save(db: &mut Connection, input: NoteInput) -> Result<Note> {
    let tx = db.transaction().map_err(error)?;
    let note = save_in(&tx, input, false)?;
    tx.commit().map_err(error)?;
    Ok(note)
}
pub(crate) fn save_in(tx: &Connection, input: NoteInput, force_checkpoint: bool) -> Result<Note> {
    let (plain, references) = inspect_document(&input.document)?;
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 300 {
        return Err("笔记标题需为 1–300 个字符".into());
    }
    if let Some(date) = &input.journal_date {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "学习日志日期无效")?;
    }
    let mut seen = HashSet::new();
    let tags: Vec<String> = input
        .tags
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .filter(|v| seen.insert(v.to_lowercase()))
        .map(String::from)
        .collect();
    if tags.len() > 100 || tags.iter().any(|v| v.chars().count() > 100) {
        return Err("笔记标签过多或过长".into());
    }
    let previous = input.id.as_deref().map(|id| get(tx, id)).transpose()?;
    if let Some(old) = &previous {
        if old.deleted_at.is_some() {
            return Err("请先从回收站恢复笔记".into());
        }
        if input.expected_revision != Some(old.revision) {
            return Err("笔记已在其他窗口更新，请保留当前草稿并重新加载".into());
        }
        if input.journal_date != old.journal_date {
            return Err("已有笔记的日志日期不可修改".into());
        }
        checkpoint(tx, old, force_checkpoint)?;
    } else if input.expected_revision.is_some() {
        return Err("新笔记不应携带旧版本号".into());
    }
    let timestamp = now();
    let note = Note {
        color: previous.as_ref().and_then(|v| v.color.clone()),
        id: previous.as_ref().map(|v| v.id.clone()).unwrap_or_else(uid),
        notebook_id: input.notebook_id,
        title: title.into(),
        document: input.document,
        plain_text: plain,
        tags,
        favorite: input.favorite,
        revision: previous.as_ref().map_or(1, |v| v.revision + 1),
        journal_date: input.journal_date,
        deleted_at: None,
        created_at: previous
            .as_ref()
            .map(|v| v.created_at.clone())
            .unwrap_or_else(|| timestamp.clone()),
        updated_at: timestamp,
    };
    let mut nodes = vec![&note.document];
    while let Some(node) = nodes.pop() {
        let attachment = match node["type"].as_str() {
            Some("image") => node["attrs"]["src"]
                .as_str()
                .and_then(|v| v.strip_prefix("http://dcc-media.localhost/note/")),
            Some("attachment") => Some(
                node["attrs"]["attachmentId"]
                    .as_str()
                    .ok_or("附件 ID 缺失")?,
            ),
            _ => None,
        };
        if let Some(id) = attachment {
            let owned: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM knowledge_attachments WHERE id=?1 AND note_id=?2)",
                    params![id, note.id],
                    |r| r.get(0),
                )
                .map_err(error)?;
            if !owned {
                return Err("附件不属于当前笔记，请重新导入，避免删除其他笔记后丢失附件".into());
            }
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    tx.execute("INSERT INTO knowledge_notes(id,notebook_id,title,document_json,plain_text,tags_json,favorite,revision,journal_date,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET notebook_id=excluded.notebook_id,title=excluded.title,document_json=excluded.document_json,plain_text=excluded.plain_text,tags_json=excluded.tags_json,favorite=excluded.favorite,revision=excluded.revision,updated_at=excluded.updated_at",params![note.id,note.notebook_id,note.title,note.document.to_string(),note.plain_text,serde_json::to_string(&note.tags).map_err(|e|e.to_string())?,note.favorite,note.revision,note.journal_date,note.created_at,note.updated_at]).map_err(error)?;
    tx.execute(
        "DELETE FROM knowledge_references WHERE note_id=?1",
        [&note.id],
    )
    .map_err(error)?;
    for (kind, id, label) in references {
        tx.execute("INSERT OR IGNORE INTO knowledge_references(note_id,target_kind,target_id,label) VALUES(?1,?2,?3,?4)",params![note.id,kind,id,label]).map_err(error)?;
    }
    index(tx, &note)?;
    Ok(note)
}

pub fn search(
    db: &Connection,
    query: &str,
    notebook: Option<&str>,
    trash: bool,
    favorite: bool,
    offset: u32,
    limit: u32,
) -> Result<NotePage> {
    search_ordered(db, query, notebook, trash, favorite, offset, limit, false)
}

pub fn search_ordered(
    db: &Connection,
    query: &str,
    notebook: Option<&str>,
    trash: bool,
    favorite: bool,
    offset: u32,
    limit: u32,
    manual: bool,
) -> Result<NotePage> {
    let query = query.trim();
    if query.len() > 2000 {
        return Err("搜索词过长".into());
    }
    let use_fts = query.chars().count() >= 3;
    let fts = format!("\"{}\"", query.replace('"', "\"\""));
    let like = format!(
        "%{}%",
        query
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_")
    );
    // Trash has no FTS entries and therefore always uses escaped LIKE.
    let filter = if use_fts && !trash {
        "n.id IN (SELECT note_id FROM knowledge_search WHERE knowledge_search MATCH ?1)"
    } else {
        "(n.title LIKE ?2 ESCAPE '!' OR n.plain_text LIKE ?2 ESCAPE '!' OR n.tags_json LIKE ?2 ESCAPE '!')"
    };
    let where_sql=format!("(?3 IS NULL OR n.notebook_id=?3) AND (n.deleted_at IS NOT NULL)=?4 AND (?5=0 OR n.favorite=1) AND (?6='' OR {filter})");
    let total = db
        .query_row(
            &format!("SELECT count(*) FROM knowledge_notes n WHERE {where_sql}"),
            params![fts, like, notebook, trash, favorite, query],
            |r| r.get(0),
        )
        .map_err(error)?;
    let order = if manual {
        "COALESCE((SELECT position FROM knowledge_note_order o WHERE o.note_id=n.id AND o.scope=COALESCE(?3,'__all__')),9223372036854775807),n.updated_at DESC,n.id"
    } else {
        "n.updated_at DESC,n.id"
    };
    let mut statement=db.prepare(&format!("SELECT n.id,n.title,substr(n.plain_text,1,240),n.tags_json,n.favorite,n.updated_at,n.navigation_color FROM knowledge_notes n WHERE {where_sql} ORDER BY {order} LIMIT ?7 OFFSET ?8")).map_err(error)?;
    let items = statement
        .query_map(
            params![
                fts,
                like,
                notebook,
                trash,
                favorite,
                query,
                limit.clamp(1, 100),
                offset
            ],
            |row| {
                let tags: String = row.get(3)?;
                Ok(NoteSummary {
                    color: row.get(6)?,
                    id: row.get(0)?,
                    title: row.get(1)?,
                    plain_text: row.get(2)?,
                    tags: serde_json::from_str(&tags).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    favorite: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    Ok(NotePage { items, total })
}

pub fn reorder_notes(
    db: &mut Connection,
    source: &str,
    target: &str,
    after: bool,
    notebook: Option<&str>,
    query: &str,
    favorite: bool,
) -> Result<()> {
    let tx = db.transaction().map_err(error)?;
    let mut matching = Vec::new();
    let mut offset = 0;
    loop {
        let page = search_ordered(&tx, query, notebook, false, favorite, offset, 100, true)?;
        matching.extend(page.items.into_iter().map(|n| n.id));
        if matching.len() as u64 >= page.total as u64 {
            break;
        }
        offset += 100;
    }
    if source == target {
        return Ok(());
    }
    let source_index = matching
        .iter()
        .position(|id| id == source)
        .ok_or("源笔记不属于当前筛选结果")?;
    if !matching.iter().any(|id| id == target) {
        return Err("目标笔记不属于当前筛选结果".into());
    }
    let moved = matching.remove(source_index);
    let at = matching
        .iter()
        .position(|id| id == target)
        .ok_or("目标已变化")?
        + usize::from(after);
    matching.insert(at, moved);
    let scope = notebook.unwrap_or("__all__");
    let ids = {
        let mut stmt=tx.prepare("SELECT n.id FROM knowledge_notes n LEFT JOIN knowledge_note_order o ON o.note_id=n.id AND o.scope=?2 WHERE n.deleted_at IS NULL AND (?1 IS NULL OR n.notebook_id=?1) ORDER BY COALESCE(o.position,9223372036854775807),n.updated_at DESC,n.id").map_err(error)?;
        let result = stmt
            .query_map(params![notebook, scope], |r| r.get::<_, String>(0))
            .map_err(error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(error)?;
        result
    };
    let matched: HashSet<_> = matching.iter().cloned().collect();
    let mut replacements = matching.into_iter();
    for (position, id) in ids.into_iter().enumerate() {
        let id = if matched.contains(&id) {
            replacements.next().ok_or("排序结果不完整")?
        } else {
            id
        };
        tx.execute("INSERT INTO knowledge_note_order(scope,note_id,position) VALUES(?1,?2,?3) ON CONFLICT(scope,note_id) DO UPDATE SET position=excluded.position",params![scope,id,position as i64]).map_err(error)?;
    }
    tx.commit().map_err(error)
}

pub fn set_deleted(
    db: &mut Connection,
    id: &str,
    expected_revision: i64,
    deleted: bool,
) -> Result<Note> {
    let tx = db.transaction().map_err(error)?;
    let mut note = get(&tx, id)?;
    if note.revision != expected_revision {
        return Err("笔记版本已变化，请刷新后重试".into());
    }
    note.deleted_at = deleted.then(now);
    note.revision += 1;
    note.updated_at = now();
    tx.execute(
        "UPDATE knowledge_notes SET deleted_at=?2,revision=?3,updated_at=?4 WHERE id=?1",
        params![id, note.deleted_at, note.revision, note.updated_at],
    )
    .map_err(error)?;
    index(&tx, &note)?;
    tx.commit().map_err(error)?;
    Ok(note)
}
#[cfg(test)]
pub fn revisions(db: &Connection, id: &str) -> Result<Vec<Revision>> {
    get(db, id)?;
    let mut statement=db.prepare("SELECT id,note_id,snapshot_json,pinned,created_at FROM knowledge_revisions WHERE note_id=?1 ORDER BY created_at DESC,id DESC").map_err(error)?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(error)?;
    rows.into_iter()
        .map(|(id, note_id, json, pinned, created_at)| {
            Ok(Revision {
                id,
                note_id,
                snapshot: serde_json::from_str(&json).map_err(|e| e.to_string())?,
                pinned,
                created_at,
            })
        })
        .collect()
}
pub fn pin_revision(db: &Connection, id: &str, pinned: bool) -> Result<()> {
    if db
        .execute(
            "UPDATE knowledge_revisions SET pinned=?2 WHERE id=?1",
            params![id, pinned],
        )
        .map_err(error)?
        != 1
    {
        return Err("历史版本不存在".into());
    }
    Ok(())
}
pub fn restore_revision(
    db: &mut Connection,
    note_id: &str,
    revision_id: &str,
    expected_revision: i64,
) -> Result<Note> {
    let tx = db.transaction().map_err(error)?;
    let current = get(&tx, note_id)?;
    if current.revision != expected_revision || current.deleted_at.is_some() {
        return Err("笔记状态已变化，请刷新后重试".into());
    }
    let snapshot: String = tx
        .query_row(
            "SELECT snapshot_json FROM knowledge_revisions WHERE id=?1 AND note_id=?2",
            params![revision_id, note_id],
            |r| r.get(0),
        )
        .map_err(error)?;
    let old: Note = serde_json::from_str(&snapshot).map_err(|e| e.to_string())?;
    let restored = save_in(
        &tx,
        NoteInput {
            id: Some(current.id),
            expected_revision: Some(expected_revision),
            notebook_id: current.notebook_id,
            title: old.title,
            document: old.document,
            tags: old.tags,
            favorite: old.favorite,
            journal_date: current.journal_date,
        },
        true,
    )?;
    tx.commit().map_err(error)?;
    Ok(restored)
}
pub fn upsert_notebook(
    db: &mut Connection,
    id: Option<&str>,
    parent: Option<&str>,
    name: &str,
    order: i64,
) -> Result<Notebook> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("笔记本名称需为 1–100 个字符".into());
    }
    let tx = db.transaction().map_err(error)?;
    let key = id.map(String::from).unwrap_or_else(uid);
    if id.is_some()
        && !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_notebooks WHERE id=?1)",
                [&key],
                |r| r.get::<_, bool>(0),
            )
            .map_err(error)?
    {
        return Err("笔记本不存在".into());
    }
    let cycle:bool=tx.query_row("WITH RECURSIVE children(id) AS (SELECT ?1 UNION SELECT n.id FROM knowledge_notebooks n JOIN children c ON n.parent_id=c.id) SELECT EXISTS(SELECT 1 FROM children WHERE id=?2)",params![key,parent],|r|r.get(0)).map_err(error)?;
    if cycle {
        return Err("不能将笔记本放入自身或后代".into());
    }
    tx.execute("INSERT INTO knowledge_notebooks(id,parent_id,name,sort_order) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,name=excluded.name,sort_order=excluded.sort_order",params![key,parent,name,order]).map_err(error)?;
    tx.commit().map_err(error)?;
    Ok(Notebook {
        id: key,
        parent_id: parent.map(String::from),
        name: name.into(),
        sort_order: order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE schema_meta(key TEXT PRIMARY KEY,value TEXT);INSERT INTO schema_meta VALUES('schema_version','13');").unwrap();
        migrate(&db).unwrap();
        db
    }
    fn input(title: &str) -> NoteInput {
        NoteInput {
            id: None,
            expected_revision: None,
            notebook_id: None,
            title: title.into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Unreal Engine 材质学习"}]}]}),
            tags: vec!["UE".into(), "ue".into()],
            favorite: false,
            journal_date: None,
        }
    }
    fn edit(note: &Note) -> NoteInput {
        NoteInput {
            id: Some(note.id.clone()),
            expected_revision: Some(note.revision),
            notebook_id: note.notebook_id.clone(),
            title: note.title.clone(),
            document: note.document.clone(),
            tags: note.tags.clone(),
            favorite: note.favorite,
            journal_date: note.journal_date.clone(),
        }
    }
    #[test]
    fn migration_idempotent() {
        let db = db();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM schema_meta", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "17"
        );
    }
    #[test]
    fn manual_note_order_preserves_hidden_slots_and_content_revisions() {
        let mut db = db();
        let a = save(&mut db, input("visible A")).unwrap();
        let hidden = save(&mut db, input("hidden")).unwrap();
        let b = save(&mut db, input("visible B")).unwrap();
        let before = search_ordered(&db, "", None, false, false, 0, 100, true)
            .unwrap()
            .items;
        let hidden_slot = before.iter().position(|n| n.id == hidden.id).unwrap();
        reorder_notes(&mut db, &a.id, &b.id, false, None, "visible", false).unwrap();
        let all = search_ordered(&db, "", None, false, false, 0, 100, true)
            .unwrap()
            .items;
        assert_eq!(all[hidden_slot].id, hidden.id);
        let matched = search_ordered(&db, "visible", None, false, false, 0, 100, true)
            .unwrap()
            .items;
        assert_eq!(
            matched.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec![a.id.as_str(), b.id.as_str()]
        );
        assert_eq!(get(&db, &a.id).unwrap().revision, a.revision);
        assert!(reorder_notes(&mut db, &hidden.id, &b.id, false, None, "visible", false).is_err());
        assert_eq!(
            search_ordered(&db, "", None, false, false, 0, 100, true)
                .unwrap()
                .items
                .iter()
                .map(|n| &n.id)
                .collect::<Vec<_>>(),
            all.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }
    #[test]
    fn navigation_color_migration_and_save_are_independent_of_content_revision() {
        let mut db = db();
        let note = save(&mut db, input("颜色")).unwrap();
        db.execute_batch("ALTER TABLE knowledge_notes DROP COLUMN navigation_color; UPDATE schema_meta SET value='14';").unwrap();
        migrate(&db).unwrap();
        assert!(get(&db, &note.id).unwrap().color.is_none());
        assert!(set_color(&db, &note.id, Some("url(bad)")).is_err());
        let colored = set_color(&db, &note.id, Some("#aabbcc")).unwrap();
        assert_eq!(colored.revision, note.revision);
        assert_eq!(colored.color.as_deref(), Some("#AABBCC"));
        let saved = save(&mut db, edit(&colored)).unwrap();
        assert_eq!(saved.color, colored.color);
        assert_eq!(
            search(&db, "", None, false, false, 0, 10).unwrap().items[0].color,
            colored.color
        );
        let deleted = set_deleted(&mut db, &note.id, saved.revision, true).unwrap();
        let restored = set_deleted(&mut db, &note.id, deleted.revision, false).unwrap();
        assert_eq!(restored.color, colored.color);
    }
    #[test]
    fn rejects_remote_images_and_unsafe_links_without_writing() {
        let mut db = db();
        let mut value = input("不安全");
        value.document = serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"src":"https://example.com/tracker.png"}}]});
        assert!(save(&mut db, value.clone()).is_err());
        value.document = serde_json::json!({"type":"doc","content":[{"type":"text","text":"点击","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]}]});
        assert!(save(&mut db, value).is_err());
        assert_eq!(search(&db, "", None, false, false, 0, 10).unwrap().total, 0);
    }

    #[test]
    fn topic_hierarchy_is_independent_of_headings_and_validated_before_save() {
        let mut db = db();
        let mut value = input("深层主题");
        value.document = serde_json::json!({"type":"doc","content":(0..=12).map(|depth| serde_json::json!({"type":"paragraph","attrs":{"topicDepth":depth},"content":[{"type":"text","text":"主题"}]})).collect::<Vec<_>>()});
        let saved = save(&mut db, value).unwrap();
        assert_eq!(saved.document["content"][12]["attrs"]["topicDepth"], 12);
        let mut bad = edit(&saved);
        bad.document["content"][1]["attrs"]["topicDepth"] = serde_json::json!(33);
        assert!(save(&mut db, bad).is_err());
        assert_eq!(get(&db, &saved.id).unwrap().revision, saved.revision);
        let mut bad = edit(&saved);
        bad.document["content"][1]["attrs"]["topicDepth"] = serde_json::json!(3);
        assert!(save(&mut db, bad).is_err());
    }
    #[test]
    fn restore_failure_rolls_back_checkpoint() {
        let mut db = db();
        let first = save(&mut db, input("原始")).unwrap();
        let mut changed = edit(&first);
        changed.title = "新内容".into();
        let latest = save(&mut db, changed).unwrap();
        let history = revisions(&db, &first.id).unwrap();
        db.execute("UPDATE knowledge_revisions SET snapshot_json=json_set(snapshot_json,'$.document',json('{\"type\":\"bad\"}')) WHERE id=?1",[&history[0].id]).unwrap();
        assert!(restore_revision(&mut db, &first.id, &history[0].id, latest.revision).is_err());
        assert_eq!(get(&db, &first.id).unwrap().revision, latest.revision);
        assert_eq!(revisions(&db, &first.id).unwrap().len(), 1);
    }
    #[test]
    fn save_search_conflict_and_trash() {
        let mut db = db();
        let first = save(&mut db, input("材质笔记")).unwrap();
        assert_eq!(first.tags, vec!["UE"]);
        for q in ["材质", "Unreal", "学习"] {
            assert_eq!(search(&db, q, None, false, false, 0, 10).unwrap().total, 1);
        }
        let mut change = edit(&first);
        change.title = "更新".into();
        let second = save(&mut db, change.clone()).unwrap();
        assert!(save(&mut db, change).is_err());
        assert_eq!(get(&db, &first.id).unwrap().title, "更新");
        let deleted = set_deleted(&mut db, &first.id, second.revision, true).unwrap();
        assert_eq!(search(&db, "", None, false, false, 0, 10).unwrap().total, 0);
        assert_eq!(
            search(&db, "Unreal", None, true, false, 0, 10)
                .unwrap()
                .total,
            1
        );
        set_deleted(&mut db, &first.id, deleted.revision, false).unwrap();
        assert_eq!(
            search(&db, "Unreal", None, false, false, 0, 10)
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn notebook_cycles_and_transaction_rollback() {
        let mut db = db();
        let root = upsert_notebook(&mut db, None, None, "UE", 0).unwrap();
        let child = upsert_notebook(&mut db, None, Some(&root.id), "材质", 0).unwrap();
        assert!(upsert_notebook(&mut db, Some(&root.id), Some(&child.id), "UE", 0).is_err());
        let mut bad = input("无效");
        bad.notebook_id = Some(uid());
        assert!(save(&mut db, bad).is_err());
        assert_eq!(search(&db, "", None, false, false, 0, 10).unwrap().total, 0);
    }
    #[test]
    fn coalesced_history_and_restore() {
        let mut db = db();
        let note = save(&mut db, input("原始")).unwrap();
        let mut update = edit(&note);
        update.title = "第二版".into();
        let note = save(&mut db, update).unwrap();
        let mut update = edit(&note);
        update.title = "第三版".into();
        let note = save(&mut db, update).unwrap();
        let history = revisions(&db, &note.id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].snapshot.title, "原始");
        let restored = restore_revision(&mut db, &note.id, &history[0].id, note.revision).unwrap();
        assert_eq!(restored.title, "原始");
        assert!(revisions(&db, &note.id)
            .unwrap()
            .iter()
            .any(|v| v.snapshot.title == "第三版"));
    }
    #[test]
    fn history_limit_keeps_pins() {
        let mut db = db();
        let note = save(&mut db, input("保留历史")).unwrap();
        checkpoint(&db, &note, true).unwrap();
        let pinned = revisions(&db, &note.id).unwrap()[0].id.clone();
        pin_revision(&db, &pinned, true).unwrap();
        for _ in 0..60 {
            checkpoint(&db, &note, true).unwrap();
        }
        let history = revisions(&db, &note.id).unwrap();
        assert_eq!(history.len(), 51);
        assert!(history.iter().any(|v| v.id == pinned && v.pinned));
    }
    #[test]
    fn journal_unique_and_references_survive_missing_targets() {
        let mut db = db();
        let mut note = input("每日学习");
        note.journal_date = Some("2026-09-29".into());
        let target = uid();
        note.document = json!({"type":"doc","content":[{"type":"reference","attrs":{"kind":"asset","targetId":target,"label":"旧模型"}}]});
        save(&mut db, note.clone()).unwrap();
        assert!(save(&mut db, note).is_err());
        assert_eq!(
            db.query_row("SELECT label FROM knowledge_references", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "旧模型"
        );
    }

    #[test]
    fn full_library_backup_restores_notes_history_and_managed_attachments() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        std::fs::create_dir(&base).unwrap();
        let path = base.join("library.db");
        let mut db = crate::db::open_database(&path).unwrap();
        let notebook = upsert_notebook(&mut db, None, None, "学习资料", 0).unwrap();
        let mut value = input("初始笔记");
        value.notebook_id = Some(notebook.id.clone());
        let first = save(&mut db, value).unwrap();
        let source = temp.path().join("notes.txt");
        std::fs::write(&source, "本地学习内容").unwrap();
        let attachment = crate::knowledge_files::import(&db, &base, &first.id, &source).unwrap();
        let mut value = edit(&first);
        value.title = "更新后的笔记".into();
        value.document = json!({"type":"doc","content":[{"type":"attachment","attrs":{"attachmentId":attachment.id,"label":"学习附件"}}]});
        let current = save(&mut db, value).unwrap();
        set_color(&db, &current.id, Some("#85B596")).unwrap();
        let history = revisions(&db, &current.id).unwrap();
        pin_revision(&db, &history[0].id, true).unwrap();
        let package = temp.path().join("backup.dccassetlib");
        crate::backup::export_library(&db, &base, &package).unwrap();
        let deleted = set_deleted(&mut db, &current.id, current.revision, true).unwrap();
        crate::knowledge_files::purge(&mut db, &base, &current.id, deleted.revision).unwrap();
        let marker = crate::state::LibraryMarker {
            format_version: 1,
            library_id: uid(),
            name: "测试".into(),
            created_at: now(),
        };
        let mut active = crate::state::ActiveLibrary {
            base_dir: base.clone(),
            db_path: path,
            marker,
            connection: Some(db),
        };
        crate::backup::restore_library(&mut active, &package).unwrap();
        let db = active.connection.as_ref().unwrap();
        let restored = get(db, &current.id).unwrap();
        assert_eq!(restored.color.as_deref(), Some("#85B596"));
        assert_eq!(restored.document, current.document);
        assert_eq!(restored.notebook_id, Some(notebook.id));
        assert!(revisions(db, &current.id).unwrap()[0].pinned);
        assert_eq!(
            std::fs::read_to_string(
                crate::knowledge_files::path(db, &base, &attachment.id)
                    .unwrap()
                    .0
            )
            .unwrap(),
            "本地学习内容"
        );
        assert!(search(db, "更新", None, false, false, 0, 10)
            .unwrap()
            .items
            .iter()
            .any(|v| v.id == current.id));
    }

    #[test]
    #[ignore = "generated large-data benchmark; run explicitly"]
    fn benchmark_twenty_thousand_notes_and_project_pages() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&temp.path().join("benchmark.db")).unwrap();
        let tx = db.transaction().unwrap();
        for index in 0..20000 {
            save_in(&tx, input(&format!("材质学习 {index}")), false).unwrap();
        }
        tx.commit().unwrap();
        for query in ["材质", "Unreal", "材质学习 19999"] {
            let start = std::time::Instant::now();
            let result = search(&db, query, None, false, false, 0, 40).unwrap();
            let elapsed = start.elapsed();
            println!(
                "knowledge query={query:?} rows={} first_page={} elapsed_ms={:.2}",
                result.total,
                result.items.len(),
                elapsed.as_secs_f64() * 1000.0
            );
            assert!(elapsed < std::time::Duration::from_secs(2));
            assert!(result.items.len() <= 40);
        }
        let project = crate::projects::upsert_project(
            &mut db,
            crate::models::ProjectInput {
                id: None,
                name: "大项目".into(),
                description: String::new(),
                project_type: "scene".into(),
                status: "active".into(),
                target_tools: vec![],
                versions: vec![],
                resolution_width: None,
                resolution_height: None,
                frame_rate: None,
                cover_asset_id: None,
                cover_media_entry_id: None,
            },
        )
        .unwrap()
        .summary
        .id;
        let tx = db.transaction().unwrap();
        for index in 0..5000 {
            let asset = format!("asset-{index}");
            tx.execute("INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES(?1,?1,'','now','now')",[&asset]).unwrap();
            tx.execute("INSERT INTO project_assets(project_id,asset_id,created_at,updated_at) VALUES(?1,?2,'now','now')",params![project,asset]).unwrap();
        }
        for index in 0..1000 {
            tx.execute("INSERT INTO project_tasks(id,project_id,title,status,priority,sort_order,created_at,updated_at) VALUES(?1,?2,?1,'todo','normal',?3,'now','now')",params![format!("task-{index}"),project,index]).unwrap();
        }
        tx.commit().unwrap();
        let start = std::time::Instant::now();
        let workspace = crate::projects::get_project_workspace(&db, &project, false).unwrap();
        let assets = crate::projects::asset_page(&db, &project, "zh-CN", "", None, 0, 60).unwrap();
        let tasks = crate::projects::task_page(&db, &project, Some("todo"), 0, 40).unwrap();
        println!(
            "project assets={} tasks={} bounded_items={} elapsed_ms={:.2}",
            assets.total,
            tasks.total,
            workspace.tasks.len() + assets.items.len() + tasks.items.len(),
            start.elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(assets.items.len(), 60);
        assert_eq!(tasks.items.len(), 40);
        assert!(workspace.assets.is_empty());
    }
}
