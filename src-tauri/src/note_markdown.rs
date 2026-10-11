//! Markdown portability: downloads are staged outside the database lock, then copied atomically.
use crate::{
    file_digest,
    knowledge::{self, NoteInput},
    knowledge_files,
    note_package::ImportResult,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownSource {
    pub text: String,
    pub title: String,
    pub sha256: String,
}
pub fn read(source: &Path) -> Result<MarkdownSource, String> {
    if source
        .extension()
        .and_then(|v| v.to_str())
        .is_none_or(|v| !v.eq_ignore_ascii_case("md"))
    {
        return Err("请选择 .md 文件".into());
    }
    let mut bytes = vec![];
    fs::File::open(source)
        .map_err(err)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Markdown 文件超过 4 MB".into());
    }
    let sha256 = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&bytes));
    let text = String::from_utf8(bytes).map_err(|_| "Markdown 文件必须使用 UTF-8 编码")?;
    let title = source
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("导入笔记")
        .chars()
        .take(300)
        .collect();
    Ok(MarkdownSource {
        text,
        title,
        sha256,
    })
}
fn local_dependency(parent: &Path, raw: &str) -> Result<PathBuf, String> {
    if url::Url::parse(raw).is_ok() {
        return Err("不会下载远程图片或附件，请先保存到 Markdown 所在目录".into());
    }
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|_| "附件路径编码无效")?;
    let path = Path::new(decoded.as_ref());
    if decoded.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|v| !matches!(v, Component::Normal(_)))
    {
        return Err("附件仅允许 Markdown 目录内的安全相对路径".into());
    }
    let real = parent
        .join(path)
        .canonicalize()
        .map_err(|_| format!("Markdown 附件缺失：{raw}"))?;
    if !real.starts_with(parent) || !real.is_file() {
        return Err("Markdown 附件越出文件所在目录".into());
    }
    Ok(real)
}
fn collect(
    document: &mut Value,
    parent: &Path,
    dependencies: &mut HashMap<String, PathBuf>,
    extra: &mut Vec<Value>,
    remote: &HashMap<String, PathBuf>,
    failures: &HashMap<String, String>,
    warnings: &mut Vec<String>,
    depth: u32,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Markdown 层级过深".into());
    }
    if document["type"] == "image" {
        let source = document["attrs"]["src"]
            .as_str()
            .ok_or("图片缺少路径")?
            .to_owned();
        if source.starts_with("http://") || source.starts_with("https://") {
            if let Some(file) = remote.get(&source) {
                dependencies.insert(source.clone(), file.clone());
            } else {
                let reason = failures
                    .get(&source)
                    .cloned()
                    .unwrap_or_else(|| "网络图片未下载，请点击重试".into());
                let alt = document["attrs"]["alt"]
                    .as_str()
                    .unwrap_or("网络图片")
                    .chars()
                    .take(1000)
                    .collect::<String>();
                if url::Url::parse(&source).is_ok() && source.len() <= 8192 {
                    *document = json!({"type":"remoteImage","attrs":{"src":source,"alt":alt,"error":reason}});
                } else {
                    *document = json!({"type":"paragraph","content":[{"type":"text","text":"图片地址无效，未下载"}]});
                }
                warnings.push(reason);
            }
        } else {
            dependencies.insert(source.clone(), local_dependency(parent, &source)?);
        }
    }
    let label = document["text"].as_str().unwrap_or("关联资料").to_owned();
    if let Some(marks) = document.get_mut("marks").and_then(Value::as_array_mut) {
        for mark in marks.iter() {
            if mark["type"] != "link" {
                continue;
            }
            let href = mark["attrs"]["href"].as_str().ok_or("链接无效")?;
            if href.starts_with("http://") || href.starts_with("https://") {
                continue;
            }
            if href.starts_with("dccref://") {
                let url = url::Url::parse(href).map_err(|_| "引用链接无效")?;
                let kind = url.host_str().unwrap_or("");
                let id = url.path().trim_matches('/');
                if !["note", "asset", "media", "project"].contains(&kind)
                    || Uuid::parse_str(id).is_err()
                {
                    return Err("引用链接无效".into());
                }
                extra.push(
                    json!({"type":"reference","attrs":{"kind":kind,"targetId":id,"label":label}}),
                );
            } else {
                dependencies.insert(href.into(), local_dependency(parent, href)?);
                extra
                    .push(json!({"type":"attachment","attrs":{"attachmentId":href,"label":label}}));
            }
        }
        marks.retain(|mark| {
            mark["type"] != "link"
                || mark["attrs"]["href"]
                    .as_str()
                    .is_some_and(|v| v.starts_with("http://") || v.starts_with("https://"))
        });
    }
    if let Some(children) = document.get_mut("content").and_then(Value::as_array_mut) {
        for node in children {
            collect(
                node,
                parent,
                dependencies,
                extra,
                remote,
                failures,
                warnings,
                depth + 1,
            )?;
        }
    }
    Ok(())
}
fn remap(document: &mut Value, map: &HashMap<String, String>) {
    if document["type"] == "image" {
        if let Some(id) = document["attrs"]["src"].as_str().and_then(|v| map.get(v)) {
            document["attrs"]["src"] = json!(format!("http://dcc-media.localhost/note/{id}"));
        }
    }
    if document["type"] == "attachment" {
        if let Some(id) = document["attrs"]["attachmentId"]
            .as_str()
            .and_then(|v| map.get(v))
        {
            document["attrs"]["attachmentId"] = json!(id);
        }
    }
    if let Some(children) = document.get_mut("content").and_then(Value::as_array_mut) {
        for node in children {
            remap(node, map);
        }
    }
}
#[cfg(test)]
pub fn import(
    db: &mut Connection,
    base: &Path,
    source: &Path,
    sha256: &str,
    document: Value,
    notebook_id: Option<String>,
) -> Result<ImportResult, String> {
    import_staged(
        db,
        base,
        source,
        sha256,
        document,
        notebook_id,
        &HashMap::new(),
        &HashMap::new(),
    )
}
pub fn import_staged(
    db: &mut Connection,
    base: &Path,
    source: &Path,
    sha256: &str,
    mut document: Value,
    notebook_id: Option<String>,
    remote: &HashMap<String, PathBuf>,
    failures: &HashMap<String, String>,
) -> Result<ImportResult, String> {
    let original = read(source)?;
    if original.sha256 != sha256 {
        return Err("Markdown 文件已发生变化，请重新选择后导入".into());
    }
    if document["type"] != "doc" || document.to_string().len() > 4 * 1024 * 1024 {
        return Err("Markdown 解析结果无效或过大".into());
    }
    let parent = source
        .canonicalize()
        .map_err(err)?
        .parent()
        .ok_or("文件目录无效")?
        .to_path_buf();
    let mut dependencies = HashMap::new();
    let mut extra = vec![];
    let mut warnings = vec![];
    collect(
        &mut document,
        &parent,
        &mut dependencies,
        &mut extra,
        remote,
        failures,
        &mut warnings,
        0,
    )?;
    if dependencies.len() > 1000 {
        return Err("附件过多".into());
    }
    let total = dependencies
        .values()
        .try_fold(0u64, |sum, path| -> Result<u64, String> {
            let length = fs::metadata(path).map_err(err)?.len();
            if length > 100 * 1024 * 1024 {
                return Err("单个附件超过 100 MB".into());
            }
            sum.checked_add(length).ok_or("附件过大".into())
        })?;
    if total > 1024 * 1024 * 1024 {
        return Err("附件总量超过 1 GB".into());
    }
    let tx = db.transaction().map_err(err)?;
    let blank = knowledge::save_in(
        &tx,
        NoteInput {
            id: None,
            expected_revision: None,
            notebook_id,
            title: original.title,
            document: json!({"type":"doc","content":[]}),
            tags: vec![],
            favorite: false,
            journal_date: None,
        },
        false,
    )?;
    let directory = base.join("note-attachments").join(&blank.id);
    let result = (|| {
        let mut map = HashMap::new();
        for (logical, path) in dependencies {
            let imported = knowledge_files::import(&tx, base, &blank.id, &path)?;
            map.insert(logical, imported.id);
        }
        let moved_links = !extra.is_empty();
        document
            .as_object_mut()
            .ok_or("文档无效")?
            .entry("content")
            .or_insert(json!([]))
            .as_array_mut()
            .ok_or("文档无效")?
            .extend(extra);
        remap(&mut document, &map);
        let note = knowledge::save_in(
            &tx,
            NoteInput {
                id: Some(blank.id.clone()),
                expected_revision: Some(blank.revision),
                notebook_id: blank.notebook_id,
                title: blank.title,
                document,
                tags: vec![],
                favorite: false,
                journal_date: None,
            },
            false,
        )?;
        tx.execute(
            "DELETE FROM knowledge_revisions WHERE note_id=?1",
            [&note.id],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        if moved_links {
            warnings.push("本地文件链接和应用引用已转换为正文末尾的附件/引用卡片".into());
        }
        if !failures.is_empty() {
            warnings.insert(
                0,
                format!(
                    "{} 张网络图片未下载，正文已保留；可在图片占位处重试",
                    failures.len()
                ),
            );
        }
        warnings.dedup();
        Ok(ImportResult { note, warnings })
    })();
    if result.is_err() && directory.exists() {
        let root = base.join("note-attachments").canonicalize().map_err(err)?;
        let resolved = directory.canonicalize().map_err(err)?;
        if root.starts_with(base.canonicalize().map_err(err)?) && resolved.starts_with(&root) {
            fs::remove_dir_all(resolved).map_err(err)?;
        }
    }
    result
}
pub fn export(
    db: &Connection,
    base: &Path,
    id: &str,
    revision: i64,
    markdown: &str,
    destination: &Path,
) -> Result<(), String> {
    if destination.extension().and_then(|v| v.to_str()) != Some("md") || destination.exists() {
        return Err("请选择尚不存在的 .md 文件名".into());
    }
    if markdown.len() > 8 * 1024 * 1024 {
        return Err("Markdown 导出内容过大".into());
    }
    let note = knowledge::get(db, id)?;
    if note.revision != revision {
        return Err("笔记已变化，请保存后重新导出".into());
    }
    let parent = destination.parent().ok_or("导出目录无效")?;
    let stage = tempfile::tempdir_in(parent).map_err(err)?;
    let folder = format!("note-files-{}", Uuid::new_v4());
    let stage_files = stage.path().join(&folder);
    let mut text = markdown.to_string();
    let mut nodes = vec![&note.document];
    let mut copied = HashSet::new();
    let mut total = 0u64;
    while let Some(node) = nodes.pop() {
        if node["type"] == "mindMap" {
            let map_id = node["attrs"]["id"].as_str().ok_or("导图 ID 无效")?;
            Uuid::parse_str(map_id).map_err(|_| "导图 ID 无效")?;
            fs::create_dir_all(&stage_files).map_err(err)?;
            let filename = format!("mind-map-{map_id}.svg");
            fs::write(
                stage_files.join(&filename),
                crate::note_export::map_svg(&node["attrs"]["data"])?,
            )
            .map_err(err)?;
            text = text.replace(
                &format!("dccmap://{map_id}"),
                &format!("{folder}/{filename}"),
            );
        }
        let id = match node["type"].as_str() {
            Some("image") => node["attrs"]["src"]
                .as_str()
                .and_then(|v| v.strip_prefix("http://dcc-media.localhost/note/")),
            Some("attachment") => node["attrs"]["attachmentId"].as_str(),
            _ => None,
        };
        if let Some(id) = id {
            if copied.insert(id.to_string()) {
                let (path, _) = knowledge_files::path(db, base, id)?;
                let (size, _) = file_digest::digest_file(&path)?;
                total = total.checked_add(size).ok_or("附件过大")?;
                if size > 100 * 1024 * 1024 || total > 1024 * 1024 * 1024 {
                    return Err("导出附件超过大小限制".into());
                }
                let extension = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .ok_or("附件扩展名无效")?;
                let filename = format!("{id}.{extension}");
                fs::create_dir_all(&stage_files).map_err(err)?;
                fs::copy(path, stage_files.join(&filename)).map_err(err)?;
                text = text.replace(
                    &format!("http://dcc-media.localhost/note/{id}"),
                    &format!("{folder}/{filename}"),
                );
            }
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    output.write_all(text.as_bytes()).map_err(err)?;
    output.as_file().sync_all().map_err(err)?;
    let final_files = parent.join(&folder);
    if stage_files.exists() {
        fs::rename(&stage_files, &final_files).map_err(err)?;
    }
    if let Err(error) = output.persist_noclobber(destination) {
        if final_files.exists() {
            let _ = fs::rename(&final_files, &stage_files);
        }
        return Err(error.to_string());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staged_remote_image_is_copied_and_survives_download_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let source = temp.path().join("learn.md");
        fs::write(&source, "![image](https://example.com/image.png)").unwrap();
        let info = read(&source).unwrap();
        let downloads = tempfile::tempdir().unwrap();
        let image = downloads.path().join("image.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        let remote = HashMap::from([("https://example.com/image.png".to_string(), image)]);
        let result = import_staged(&mut db,&base,&source,&info.sha256,json!({"type":"doc","content":[{"type":"image","attrs":{"src":"https://example.com/image.png"}}]}),None,&remote,&HashMap::new()).unwrap();
        drop(downloads);
        let id = result.note.document["content"][0]["attrs"]["src"]
            .as_str()
            .unwrap()
            .strip_prefix("http://dcc-media.localhost/note/")
            .unwrap();
        let (path, _) = knowledge_files::path(&db, &base, id).unwrap();
        assert!(image::open(path).is_ok());
        assert!(result.warnings.is_empty());
    }
    #[test]
    fn remote_failure_preserves_body_and_a_retryable_image() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let source = temp.path().join("learn.md");
        fs::write(&source, "body ![image](https://api2.mubu.com/image.png)").unwrap();
        let info = read(&source).unwrap();
        let result=import(&mut db,&base,&source,&info.sha256,json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"body"}]},{"type":"image","attrs":{"src":"https://api2.mubu.com/image.png","alt":"image"}}]}),None).unwrap();
        assert_eq!(result.note.document["content"][1]["type"], "remoteImage");
        assert!(result.note.plain_text.contains("body"));
        assert!(!result.warnings.is_empty());
    }
    #[test]
    fn import_rejects_missing_and_traversing_dependencies_without_records() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let source = temp.path().join("learn.md");
        fs::write(&source, "![test](missing.png)").unwrap();
        let info = read(&source).unwrap();
        for path in ["../secret.png", "missing.png", "%2e%2e/secret.png"] {
            let doc = json!({"type":"doc","content":[{"type":"image","attrs":{"src":path}}]});
            assert!(import(&mut db, &base, &source, &info.sha256, doc, None).is_err());
        }
        assert_eq!(
            knowledge::search(&db, "", None, false, false, 0, 10)
                .unwrap()
                .total,
            0
        );
    }
    #[test]
    fn markdown_local_image_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("library");
        fs::create_dir(&base).unwrap();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let source = temp.path().join("learn.md");
        fs::write(&source, "![image](image.png)").unwrap();
        image::RgbaImage::new(2, 2)
            .save(temp.path().join("image.png"))
            .unwrap();
        let info = read(&source).unwrap();
        let note = import(
            &mut db,
            &base,
            &source,
            &info.sha256,
            json!({"type":"doc","content":[{"type":"image","attrs":{"src":"image.png"}}]}),
            None,
        )
        .unwrap()
        .note;
        let url = note.document["content"][0]["attrs"]["src"]
            .as_str()
            .unwrap();
        let target = temp.path().join("export.md");
        export(
            &db,
            &base,
            &note.id,
            note.revision,
            &format!("![image]({url})"),
            &target,
        )
        .unwrap();
        let text = fs::read_to_string(&target).unwrap();
        assert!(!text.contains("dcc-media"));
        assert!(text.contains("note-files-"));
        assert!(export(&db, &base, &note.id, note.revision, &text, &target).is_err());
    }
}
