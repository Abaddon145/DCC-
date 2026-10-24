//! Portable snapshot of one note. Complete library backups retain all version history.
use crate::{
    file_digest,
    knowledge::{self, Note, NoteInput},
    knowledge_files,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path},
};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    id: String,
    filename: String,
    path: String,
    size: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    format: String,
    version: u32,
    note: Note,
    notebook_path: Vec<String>,
    files: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    archives: Vec<crate::learning::note_archive::ArchiveBundle>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub note: Note,
    pub warnings: Vec<String>,
}
const MAX_FILE: u64 = 100 * 1024 * 1024;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn uuid() -> String {
    Uuid::new_v4().to_string()
}
fn attachment_id(node: &Value) -> Option<&str> {
    match node["type"].as_str() {
        Some("image") => node["attrs"]["src"]
            .as_str()?
            .strip_prefix("http://dcc-media.localhost/note/"),
        Some("attachment") => node["attrs"]["attachmentId"].as_str(),
        _ => None,
    }
}
fn referenced_files(document: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut nodes = vec![document];
    while let Some(node) = nodes.pop() {
        if let Some(id) = attachment_id(node) {
            ids.insert(id.into());
        }
        if let Some(content) = node["content"].as_array() {
            nodes.extend(content);
        }
    }
    ids
}
fn rewrite(
    document: &mut Value,
    files: &HashMap<String, String>,
    old_note: &str,
    new_note: &str,
    depth: u32,
) -> Result<(), String> {
    if depth > 64 {
        return Err("笔记层级过深".into());
    }
    if let Some(id) = attachment_id(document).map(String::from) {
        let target = files.get(&id).ok_or("笔记包缺少引用的附件")?;
        if document["type"] == "image" {
            document["attrs"]["src"] = json!(format!("http://dcc-media.localhost/note/{target}"));
        } else {
            document["attrs"]["attachmentId"] = json!(target);
        }
    }
    if document["type"] == "preservedMarkdown" {
        let id = document["attrs"]["archiveItemId"]
            .as_str()
            .ok_or("原文片段标识缺失")?;
        document["attrs"]["archiveItemId"] =
            json!(files.get(id).ok_or("笔记包缺少引用的原文片段")?);
    }
    if document["type"] == "reference"
        && document["attrs"]["kind"] == "note"
        && document["attrs"]["targetId"] == old_note
    {
        document["attrs"]["targetId"] = json!(new_note);
    }
    if let Some(children) = document.get_mut("content").and_then(Value::as_array_mut) {
        for child in children {
            rewrite(child, files, old_note, new_note, depth + 1)?;
        }
    }
    Ok(())
}
fn notebook_path(db: &Connection, id: Option<&str>) -> Result<Vec<String>, String> {
    let mut next = id.map(String::from);
    let mut names = vec![];
    let mut seen = HashSet::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) || names.len() >= 64 {
            return Err("笔记本层级无效或过深".into());
        }
        let (parent, name): (Option<String>, String) = db
            .query_row(
                "SELECT parent_id,name FROM knowledge_notebooks WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        names.push(name);
        next = parent;
    }
    names.reverse();
    Ok(names)
}
pub fn export(db: &Connection, base: &Path, id: &str, destination: &Path) -> Result<(), String> {
    export_snapshot(db, base, knowledge::get(db, id)?, destination)
}
pub fn export_draft(
    db: &Connection,
    base: &Path,
    input: NoteInput,
    destination: &Path,
) -> Result<(), String> {
    let mut note = knowledge::get(db, input.id.as_deref().ok_or("草稿没有来源笔记")?)?;
    let (plain, _) = knowledge::inspect_document(&input.document)?;
    if input.title.chars().count() > 300
        || input.tags.len() > 100
        || input.tags.iter().any(|v| v.chars().count() > 100)
    {
        return Err("草稿标题或标签过长".into());
    }
    note.title = if input.title.trim().is_empty() {
        "未命名草稿".into()
    } else {
        input.title.trim().into()
    };
    note.document = input.document;
    note.plain_text = plain;
    note.tags = input.tags;
    note.favorite = input.favorite;
    note.deleted_at = None;
    note.journal_date = None;
    // Retain the source notebook path and owned attachments without changing the source note.
    export_snapshot(db, base, note, destination)
}
fn export_snapshot(
    db: &Connection,
    base: &Path,
    note: Note,
    destination: &Path,
) -> Result<(), String> {
    if destination.extension().and_then(|v| v.to_str()) != Some("dccnotes") {
        return Err("请选择 .dccnotes 文件".into());
    }
    if destination.exists() {
        return Err("输出文件已存在，请选择新文件名以免覆盖".into());
    }
    let mut files = vec![];
    let mut paths = vec![];
    let mut size = 0u64;
    for attachment in referenced_files(&note.document) {
        let (path, _) = knowledge_files::path(db, base, &attachment)?;
        let filename: String = db
            .query_row(
                "SELECT filename FROM knowledge_attachments WHERE id=?1 AND note_id=?2",
                params![attachment, note.id],
                |r| r.get(0),
            )
            .map_err(err)?;
        let (bytes, sha256) = file_digest::digest_file(&path)?;
        size = size.checked_add(bytes).ok_or("笔记包过大")?;
        if bytes > MAX_FILE || size > 1024 * 1024 * 1024 {
            return Err("单附件上限 100 MB，笔记包附件总量上限 1 GB".into());
        }
        let extension = path
            .extension()
            .and_then(|v| v.to_str())
            .ok_or("附件扩展名缺失")?;
        files.push(FileEntry {
            id: attachment.clone(),
            filename,
            path: format!("attachments/{attachment}.{extension}"),
            size: bytes,
            sha256,
        });
        paths.push(path);
    }
    let archives = crate::learning::note_archive::bundle_items(
        db,
        &crate::learning::note_nodes::archive_ids(&note.document)?,
    )?;
    if files.len() + archives.len() > 1000 {
        return Err("笔记包文件数量超过1000".into());
    }
    for bundle in &archives {
        let bytes = crate::learning::note_archive::read_archive_item(db, base, &bundle.id)?;
        crate::learning::note_archive::verify_bundle(bundle, &bytes)?;
        size = size.checked_add(bundle.size).ok_or("笔记包过大")?;
        if size > 1024 * 1024 * 1024 {
            return Err("笔记包总量超过1GB".into());
        }
    }
    let package = Package {
        format: "stack-dcc-notes".into(),
        version: if archives.is_empty() { 1 } else { 2 },
        notebook_path: notebook_path(db, note.notebook_id.as_deref())?,
        note,
        files,
        archives,
    };
    let parent = destination.parent().ok_or("输出目录无效")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    {
        let mut zip = ZipWriter::new(temporary.as_file_mut());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("note.json", options).map_err(err)?;
        zip.write_all(&serde_json::to_vec(&package).map_err(err)?)
            .map_err(err)?;
        for (file, path) in package.files.iter().zip(paths) {
            zip.start_file(&file.path, options).map_err(err)?;
            std::io::copy(&mut fs::File::open(path).map_err(err)?, &mut zip).map_err(err)?;
        }
        for bundle in &package.archives {
            zip.start_file(format!("archives/{}.bin", bundle.id), options)
                .map_err(err)?;
            zip.write_all(&crate::learning::note_archive::read_archive_item(
                db, base, &bundle.id,
            )?)
            .map_err(err)?;
        }
        zip.finish().map_err(err)?;
    }
    temporary.as_file().sync_all().map_err(err)?;
    temporary.persist_noclobber(destination).map_err(err)?;
    Ok(())
}
pub fn import(db: &mut Connection, base: &Path, source: &Path) -> Result<ImportResult, String> {
    import_with_notebook(db, base, source, None)
}
fn import_with_notebook(
    db: &mut Connection,
    base: &Path,
    source: &Path,
    notebook: Option<Option<String>>,
) -> Result<ImportResult, String> {
    let mut archive =
        ZipArchive::new(fs::File::open(source).map_err(err)?).map_err(|_| "不是有效的笔记包")?;
    if archive.len() > 1001 {
        return Err("笔记包条目过多".into());
    }
    let mut names = HashSet::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(err)?;
        let name = file.name();
        if !names.insert(name.to_owned())
            || name.contains('\\')
            || Path::new(name)
                .components()
                .any(|v| !matches!(v, Component::Normal(_)))
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("笔记包包含重复、不安全路径或符号链接".into());
        }
    }
    let mut bytes = vec![];
    archive
        .by_name("note.json")
        .map_err(|_| "笔记包缺少清单")?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("笔记清单超过 8 MB".into());
    }
    let package: Package = serde_json::from_slice(&bytes).map_err(|_| "笔记包清单无效")?;
    if package.format != "stack-dcc-notes"
        || ![1, 2].contains(&package.version)
        || (package.version == 1 && !package.archives.is_empty())
        || package.notebook_path.len() > 64
        || package.files.len() + package.archives.len() + 1 != archive.len()
    {
        return Err("不支持的笔记包版本或文件清单不一致".into());
    }
    for name in &package.notebook_path {
        if name.trim().is_empty() || name.chars().count() > 100 {
            return Err("笔记本名称无效".into());
        }
    }
    Uuid::parse_str(&package.note.id).map_err(|_| "笔记 ID 无效")?;
    let staging = tempfile::tempdir().map_err(err)?;
    let mut original_ids = HashSet::new();
    let mut extracted = vec![];
    let mut total = 0u64;
    for file in &package.files {
        Uuid::parse_str(&file.id).map_err(|_| "附件 ID 无效")?;
        let extension = Path::new(&file.path)
            .extension()
            .and_then(|v| v.to_str())
            .ok_or("附件类型无效")?;
        if !["png", "jpg", "jpeg", "webp", "gif", "pdf", "txt", "md"].contains(&extension)
            || file.path != format!("attachments/{}.{extension}", file.id)
            || !original_ids.insert(&file.id)
            || file.filename.len() > 1000
        {
            return Err("附件清单无效".into());
        }
        total = total.checked_add(file.size).ok_or("笔记包过大")?;
        if file.size > MAX_FILE || total > 1024 * 1024 * 1024 {
            return Err("笔记包超过大小限制".into());
        }
        let target = staging.path().join(format!("{}.{}", file.id, extension));
        let mut output = fs::File::create(&target).map_err(err)?;
        let mut input = archive
            .by_name(&file.path)
            .map_err(|_| "笔记包缺少附件")?
            .take(MAX_FILE + 1);
        let size = std::io::copy(&mut input, &mut output).map_err(err)?;
        let (_, digest) = file_digest::digest_file(&target)?;
        if size != file.size || digest != file.sha256 {
            return Err("笔记包附件校验失败，未导入任何笔记".into());
        }
        extracted.push(target);
    }
    let mut archive_paths = Vec::new();
    let mut archived_items = HashSet::new();
    let mut archive_ids = HashSet::new();
    for bundle in &package.archives {
        if Uuid::parse_str(&bundle.id).is_err()
            || !archive_ids.insert(&bundle.id)
            || bundle.size > 5 * 1024 * 1024
        {
            return Err("笔记包原文清单无效".into());
        }
        total = total.checked_add(bundle.size).ok_or("笔记包过大")?;
        if total > 1024 * 1024 * 1024 {
            return Err("笔记包超过1GB".into());
        }
        let path = staging.path().join(format!("{}.bin", Uuid::new_v4()));
        let mut bytes = Vec::new();
        archive
            .by_name(&format!("archives/{}.bin", bundle.id))
            .map_err(|_| "笔记包缺少原文归档")?
            .take(5 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        crate::learning::note_archive::verify_bundle(bundle, &bytes)?;
        for item in &bundle.items {
            if !archived_items.insert(item.id.clone()) || original_ids.contains(&item.id) {
                return Err("笔记包原文标识重复".into());
            }
        }
        fs::write(&path, bytes).map_err(err)?;
        archive_paths.push(path);
    }
    if archived_items != crate::learning::note_nodes::archive_ids(&package.note.document)? {
        return Err("笔记包原文片段引用与清单不一致".into());
    }
    let mut archive_map = HashMap::new();
    for (bundle, path) in package.archives.iter().zip(archive_paths) {
        archive_map.extend(crate::learning::note_archive::import_bundle(
            db,
            base,
            bundle,
            &fs::read(path).map_err(err)?,
        )?);
    }
    let tx = db.transaction().map_err(err)?;
    let mut parent: Option<String> = None;
    let mut warnings = vec![];
    for name in package.notebook_path.iter().filter(|_| notebook.is_none()) {
        let existing:Option<String>=tx.query_row("SELECT id FROM knowledge_notebooks WHERE parent_id IS ?1 AND lower(name)=lower(?2) ORDER BY sort_order,id LIMIT 1",params![parent,name],|r|r.get(0)).optional().map_err(err)?;
        let id = existing.unwrap_or_else(uuid);
        tx.execute("INSERT OR IGNORE INTO knowledge_notebooks(id,parent_id,name,sort_order) VALUES(?1,?2,?3,0)",params![id,parent,name]).map_err(err)?;
        parent = Some(id);
    }
    if let Some(notebook) = notebook {
        parent = notebook;
    }
    let mut journal = package.note.journal_date.clone();
    if let Some(date) = &journal {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE journal_date=?1)",
                [date],
                |r| r.get(0),
            )
            .map_err(err)?;
        if exists {
            journal = None;
            warnings.push("同日学习日志已存在，已作为普通笔记导入，未覆盖原日志".into());
        }
    }
    let blank = knowledge::save_in(
        &tx,
        NoteInput {
            id: None,
            expected_revision: None,
            notebook_id: parent,
            title: package.note.title.clone(),
            document: json!({"type":"doc","content":[]}),
            tags: package.note.tags.clone(),
            favorite: package.note.favorite,
            journal_date: journal,
        },
        false,
    )?;
    let directory = base.join("note-attachments").join(&blank.id);
    let result = (|| {
        let mut map = archive_map;
        for (file, source) in package.files.iter().zip(extracted) {
            let imported = knowledge_files::import(&tx, base, &blank.id, &source)?;
            tx.execute(
                "UPDATE knowledge_attachments SET filename=?2 WHERE id=?1",
                params![imported.id, file.filename],
            )
            .map_err(err)?;
            map.insert(file.id.clone(), imported.id);
        }
        let mut document = package.note.document;
        rewrite(&mut document, &map, &package.note.id, &blank.id, 0)?;
        let note = knowledge::save_in(
            &tx,
            NoteInput {
                id: Some(blank.id.clone()),
                expected_revision: Some(blank.revision),
                notebook_id: blank.notebook_id,
                title: blank.title,
                document,
                tags: blank.tags,
                favorite: blank.favorite,
                journal_date: blank.journal_date,
            },
            false,
        )?;
        tx.execute(
            "DELETE FROM knowledge_revisions WHERE note_id=?1",
            [&note.id],
        )
        .map_err(err)?;
        let note = knowledge::set_color(&tx, &note.id, package.note.color.as_deref())?;
        tx.commit().map_err(err)?;
        Ok(ImportResult { note, warnings })
    })();
    if result.is_err() && directory.exists() {
        let root = base.join("note-attachments").canonicalize().map_err(err)?;
        let resolved = directory.canonicalize().map_err(err)?;
        if root.starts_with(base.canonicalize().map_err(err)?) && resolved.starts_with(&root) {
            fs::remove_dir_all(&resolved)
                .map_err(|e| format!("笔记未导入，但临时附件清理失败：{e}"))?;
        }
    }
    result
}

/// Reuse the validated package pipeline: attachments and self-references get new IDs.
pub fn duplicate(
    db: &mut Connection,
    base: &Path,
    id: &str,
    revision: i64,
) -> Result<Note, String> {
    let mut note = knowledge::get(db, id)?;
    if note.deleted_at.is_some() || note.revision != revision {
        return Err("笔记已删除或版本已变化，请重新读取".into());
    }
    note.title = format!("{}（副本）", note.title);
    note.favorite = false;
    note.journal_date = None;
    let notebook = note.notebook_id.clone();
    let temporary = tempfile::tempdir().map_err(err)?;
    let path = temporary.path().join("duplicate.dccnotes");
    export_snapshot(db, base, note, &path)?;
    Ok(import_with_notebook(db, base, &path, Some(notebook))?.note)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exporting_conflicting_draft_preserves_both_library_version_and_draft() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let input = NoteInput {
            id: None,
            expected_revision: None,
            notebook_id: None,
            title: "已保存".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"库中内容"}]}]}),
            tags: vec![],
            favorite: false,
            journal_date: None,
        };
        let note = knowledge::save(&mut db, input.clone()).unwrap();
        let draft = NoteInput {
            id: Some(note.id.clone()),
            expected_revision: Some(0),
            title: "未保存草稿".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"不可以丢失的修改"}]}]}),
            ..input
        };
        let output = temp.path().join("draft.dccnotes");
        export_draft(&db, &base, draft, &output).unwrap();
        assert_eq!(
            knowledge::get(&db, &note.id).unwrap().document,
            note.document
        );
        assert_eq!(knowledge::get(&db, &note.id).unwrap().revision, 1);
        let imported = import(&mut db, &base, &output).unwrap().note;
        assert_eq!(imported.title, "未保存草稿");
        assert!(imported.plain_text.contains("不可以丢失的修改"));
        assert_ne!(imported.id, note.id);
    }
    #[test]
    fn package_roundtrip_remaps_attachment_and_self_reference() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let blank = knowledge::save(
            &mut db,
            NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: None,
                title: "技术学习".into(),
                document: json!({"type":"doc"}),
                tags: vec!["UE".into()],
                favorite: true,
                journal_date: None,
            },
        )
        .unwrap();
        let source = temp.path().join("notes.txt");
        fs::write(&source, b"learning").unwrap();
        let attachment = knowledge_files::import(&db, &base, &blank.id, &source).unwrap();
        let note=knowledge::save(&mut db,NoteInput{id:Some(blank.id.clone()),expected_revision:Some(1),notebook_id:None,title:blank.title,document:json!({"type":"doc","content":[{"type":"attachment","attrs":{"attachmentId":attachment.id,"label":"资料"}},{"type":"reference","attrs":{"kind":"note","targetId":blank.id,"label":"自己"}}]}),tags:blank.tags,favorite:true,journal_date:None}).unwrap();
        let output = temp.path().join("note.dccnotes");
        knowledge::set_color(&db, &note.id, Some("#7CACCB")).unwrap();
        export(&db, &base, &note.id, &output).unwrap();
        let imported = import(&mut db, &base, &output).unwrap().note;
        assert_ne!(imported.id, note.id);
        assert_eq!(imported.title, note.title);
        assert_eq!(imported.color.as_deref(), Some("#7CACCB"));
        assert_eq!(
            imported.document["content"][1]["attrs"]["targetId"],
            imported.id
        );
        let new_file = imported.document["content"][0]["attrs"]["attachmentId"]
            .as_str()
            .unwrap();
        assert_ne!(new_file, attachment.id);
        assert_eq!(
            fs::read(knowledge_files::path(&db, &base, new_file).unwrap().0).unwrap(),
            b"learning"
        );
        assert!(export(&db, &base, &note.id, &output).is_err());
        assert_eq!(
            knowledge::search(&db, "", None, false, false, 0, 10)
                .unwrap()
                .total,
            2
        );
        assert!(duplicate(&mut db, &base, &note.id, note.revision - 1).is_err());
        let copy = duplicate(&mut db, &base, &note.id, note.revision).unwrap();
        assert_eq!(copy.title, "技术学习（副本）");
        assert_eq!(copy.color.as_deref(), Some("#7CACCB"));
        assert!(!copy.favorite);
        assert!(copy.journal_date.is_none());
        assert!(knowledge::revisions(&db, &copy.id).unwrap().is_empty());
        assert_eq!(copy.document["content"][1]["attrs"]["targetId"], copy.id);
        let copied_file = copy.document["content"][0]["attrs"]["attachmentId"]
            .as_str()
            .unwrap();
        assert_ne!(copied_file, attachment.id);
        let deleted = knowledge::set_deleted(&mut db, &note.id, note.revision, true).unwrap();
        knowledge_files::purge(&mut db, &base, &note.id, deleted.revision).unwrap();
        assert_eq!(
            fs::read(knowledge_files::path(&db, &base, copied_file).unwrap().0).unwrap(),
            b"learning"
        );
    }
    #[test]
    fn unsafe_archive_does_not_write() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let source = temp.path().join("bad.dccnotes");
        let mut zip = ZipWriter::new(fs::File::create(&source).unwrap());
        zip.start_file("../escape.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(import(&mut db, &base, &source).is_err());
        assert_eq!(
            knowledge::search(&db, "", None, false, false, 0, 10)
                .unwrap()
                .total,
            0
        );
        assert!(!temp.path().join("escape.txt").exists());
    }
}
