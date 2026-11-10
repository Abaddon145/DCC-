use crate::{images, media, models::*};
use chrono::Utc;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::Cursor,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelThumbnailJob {
    pub id: String,
    pub logical_path: String,
    pub original_name: String,
}

pub fn pending_model_thumbnails(connection: &Connection) -> Result<Vec<ModelThumbnailJob>, String> {
    let mut statement = connection
        .prepare(
            "SELECT e.id, m.logical_path, m.original_name FROM media_entries e \
         JOIN media_files m ON m.id=e.primary_file_id \
         WHERE e.kind='model' AND e.deleted_at IS NULL AND e.processing_message='' AND NOT EXISTS \
         (SELECT 1 FROM media_files t WHERE t.entry_id=e.id AND t.role='thumbnail') \
         ORDER BY e.created_at, e.id",
        )
        .map_err(|e| e.to_string())?;
    let jobs = statement
        .query_map([], |row| {
            Ok(ModelThumbnailJob {
                id: row.get(0)?,
                logical_path: row.get(1)?,
                original_name: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(jobs)
}

pub fn save_model_thumbnail(
    connection: &mut Connection,
    base_dir: &Path,
    entry_id: &str,
    bytes: &[u8],
) -> Result<String, String> {
    Uuid::parse_str(entry_id).map_err(|_| "模型 ID 无效")?;
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Err("模型缩略图大小无效".into());
    }
    let kind: Option<String> = connection
        .query_row(
            "SELECT kind FROM media_entries WHERE id=?1 AND deleted_at IS NULL",
            [entry_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if kind.as_deref() != Some("model") {
        return Err("模型条目不存在".into());
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (width, height) = reader.into_dimensions().map_err(|_| "缩略图无法解析")?;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 16_000_000
    {
        return Err("模型缩略图尺寸无效".into());
    }
    let picture = image::load_from_memory(bytes)
        .map_err(|_| "缩略图无法解析")?
        .resize(480, 480, image::imageops::FilterType::Lanczos3);
    let file_id = Uuid::new_v4().to_string();
    let rel_path = format!("media-library/model/{entry_id}/thumbnails/{file_id}.webp");
    let final_path = media::safe_relative(base_dir, &rel_path)?;
    let parent = final_path.parent().ok_or("缩略图路径无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp_path = parent.join(format!("{file_id}.tmp"));
    if let Err(error) = picture.save_with_format(&temp_path, image::ImageFormat::WebP) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("保存模型缩略图失败：{error}"));
    }
    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("提交模型缩略图失败：{error}"));
    }
    let result = (|| -> Result<Vec<String>, String> {
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        let previous = {
            let mut stmt = tx
                .prepare("SELECT rel_path FROM media_files WHERE entry_id=?1 AND role='thumbnail'")
                .map_err(|e| e.to_string())?;
            let paths = stmt
                .query_map([entry_id], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            paths
        };
        tx.execute(
            "DELETE FROM media_files WHERE entry_id=?1 AND role='thumbnail'",
            [entry_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,mime_type,file_size,checksum,width,height,sort_order,created_at) VALUES(?1,?2,'thumbnail','thumbnail.webp','thumbnail.webp',?3,'image/webp',?4,?5,?6,?7,100,?8)", params![file_id,entry_id,rel_path,fs::metadata(&final_path).map_err(|e|e.to_string())?.len() as i64,digest(&final_path)?,picture.width() as i64,picture.height() as i64,now()]).map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE media_entries SET processing_message='' WHERE id=?1",
            [entry_id],
        )
        .map_err(|e| e.to_string())?;
        let core_id = crate::asset::repository::register_origin(
            &tx,
            &crate::asset::models::CoreOrigin {
                kind: "media".into(),
                id: entry_id.into(),
            },
        )?;
        crate::asset::repository::register_files(&tx, &core_id)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(previous)
    })();
    match result {
        Ok(previous) => {
            for old in previous {
                if let Ok(path) = media::safe_relative(base_dir, &old) {
                    let _ = fs::remove_file(path);
                }
            }
            Ok(file_id)
        }
        Err(error) => {
            let _ = fs::remove_file(final_path);
            Err(error)
        }
    }
}

pub fn report_model_thumbnail_failure(
    connection: &Connection,
    entry_id: &str,
    message: &str,
) -> Result<(), String> {
    let detail = format!(
        "缩略图生成失败：{}",
        message.chars().take(250).collect::<String>()
    );
    let changed = connection.execute(
        "UPDATE media_entries SET processing_message=?2 WHERE id=?1 AND kind='model' AND deleted_at IS NULL",
        params![entry_id, detail],
    ).map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("模型条目不存在".into());
    }
    Ok(())
}

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn kind_from(value: &str) -> rusqlite::Result<MediaKind> {
    match value {
        "image" => Ok(MediaKind::Image),
        "model" => Ok(MediaKind::Model),
        "audio" => Ok(MediaKind::Audio),
        "video" => Ok(MediaKind::Video),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn validate_kind_path(kind: &MediaKind, path: &Path) -> Result<(String, String), String> {
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if *kind == MediaKind::Image {
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            return Err("图片库仅支持 JPG、PNG、WebP、GIF、BMP 和 TIFF".into());
        }
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => "image/tiff",
        };
        return Ok((ext, mime.into()));
    }
    let (actual, mime) = media::media_kind(path)?;
    if actual != kind.as_str() {
        return Err(format!("该文件不属于{}库", kind.as_str()));
    }
    Ok((ext, mime.into()))
}
fn digest(path: &Path) -> Result<String, String> {
    crate::file_digest::digest_file_controlled(path, &media::process_stopped).map(|(_, hash)| hash)
}
fn safe_dependency(raw: &str) -> Result<PathBuf, String> {
    let clean = raw.trim().trim_matches('"').replace('\\', "/");
    if clean.is_empty()
        || clean.contains("://")
        || clean.starts_with("data:")
        || Path::new(&clean).is_absolute()
        || Path::new(&clean).components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("模型包含不安全依赖：{raw}"));
    }
    Ok(PathBuf::from(clean))
}
fn collect_model_dependencies(source: &Path, ext: &str) -> Result<Vec<PathBuf>, String> {
    let root = source.parent().ok_or("模型路径无效")?;
    let mut result = BTreeSet::new();
    if ext == "gltf" {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(source).map_err(|e| e.to_string())?)
                .map_err(|e| format!("GLTF 无法解析：{e}"))?;
        for group in ["buffers", "images"] {
            if let Some(items) = value.get(group).and_then(|v| v.as_array()) {
                for item in items {
                    if let Some(uri) = item.get("uri").and_then(|v| v.as_str()) {
                        if uri.starts_with("data:") {
                            continue;
                        }
                        let rel = safe_dependency(uri)?;
                        if !root.join(&rel).is_file() {
                            return Err(format!("模型依赖缺失：{}", rel.display()));
                        }
                        result.insert(rel);
                    }
                }
            }
        }
    } else if ext == "obj" {
        let text = fs::read_to_string(source).map_err(|e| format!("OBJ 无法解析：{e}"))?;
        if !text.lines().any(|line| line.trim_start().starts_with("v "))
            || !text.lines().any(|line| line.trim_start().starts_with("f "))
        {
            return Err("OBJ 不包含可预览的顶点和面".into());
        }
        for line in text.lines() {
            let line = line.trim();
            if let Some(raw) = line.strip_prefix("mtllib ") {
                let rel = safe_dependency(raw)?;
                let mtl = root.join(&rel);
                if !mtl.is_file() {
                    return Err(format!("模型依赖缺失：{}", rel.display()));
                }
                result.insert(rel.clone());
                let body = fs::read_to_string(mtl).map_err(|e| e.to_string())?;
                for row in body.lines() {
                    let row = row.trim();
                    if [
                        "map_Kd ",
                        "map_Ka ",
                        "map_Ks ",
                        "map_d ",
                        "bump ",
                        "map_bump ",
                    ]
                    .iter()
                    .find_map(|p| row.strip_prefix(p))
                    .is_some()
                    {
                        let raw = row.split_whitespace().last().unwrap_or_default();
                        let texture = rel
                            .parent()
                            .unwrap_or(Path::new(""))
                            .join(safe_dependency(raw)?);
                        if !root.join(&texture).is_file() {
                            return Err(format!("模型纹理缺失：{}", texture.display()));
                        }
                        result.insert(texture);
                    }
                }
            }
        }
    }
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    if ext == "glb" && (bytes.len() < 12 || &bytes[..4] != b"glTF") {
        return Err("GLB 文件头无效".into());
    }
    if ext == "fbx"
        && !(bytes.starts_with(b"Kaydara FBX Binary")
            || String::from_utf8_lossy(&bytes[..bytes.len().min(512)])
                .contains("FBXHeaderExtension"))
    {
        return Err("FBX 文件头无效".into());
    }
    if ext == "stl" && bytes.len() < 84 && !bytes.starts_with(b"solid") {
        return Err("STL 文件无效".into());
    }
    let size = bytes.len() as u64;
    if size == 0 {
        return Err("模型文件为空".into());
    }
    Ok(result.into_iter().collect())
}
fn ensure_folder(
    connection: &Connection,
    id: Option<&str>,
    kind: &MediaKind,
) -> Result<(), String> {
    if let Some(id) = id {
        let found=connection.query_row("SELECT EXISTS(SELECT 1 FROM media_folders WHERE id=?1 AND kind=?2 AND deleted_at IS NULL)",params![id,kind.as_str()],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?!=0;
        if !found {
            return Err("媒体文件夹不存在或类型不匹配".into());
        }
    }
    Ok(())
}
pub(crate) fn upsert_tags(
    connection: &Connection,
    entry_id: &str,
    tags: &[String],
) -> Result<(), String> {
    connection
        .execute("DELETE FROM media_entry_tags WHERE entry_id=?1", [entry_id])
        .map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    for name in tags {
        let normalized = name.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        let id = connection
            .query_row(
                "SELECT id FROM localized_tags WHERE locale='zh-CN' AND normalized_name=?1",
                [&normalized],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        connection.execute("INSERT OR IGNORE INTO localized_tags(id,locale,name,normalized_name) VALUES(?1,'zh-CN',?2,?3)",params![id,name.trim(),normalized]).map_err(|e|e.to_string())?;
        connection
            .execute(
                "INSERT OR IGNORE INTO media_entry_tags(entry_id,tag_id) VALUES(?1,?2)",
                params![entry_id, id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(crate) fn reindex(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM media_search WHERE entry_id=?1", [id])
        .map_err(|e| e.to_string())?;
    connection.execute("INSERT INTO media_search(entry_id,text) SELECT e.id,e.name||' '||e.description||' '||e.author||' '||e.license||' '||COALESCE(f.name,'')||' '||COALESCE(group_concat(t.name,' '),'') FROM media_entries e LEFT JOIN media_folders f ON f.id=e.folder_id LEFT JOIN media_entry_tags et ON et.entry_id=e.id LEFT JOIN localized_tags t ON t.id=et.tag_id WHERE e.id=?1 GROUP BY e.id",[id]).map_err(|e|e.to_string())?;
    Ok(())
}
fn file_row(row: &Row<'_>) -> rusqlite::Result<MediaFile> {
    Ok(MediaFile {
        storage_mode: if row.get::<_, bool>(11)? {
            "external"
        } else {
            "managed"
        }
        .into(),
        source_revision: row.get(12)?,
        id: row.get(0)?,
        entry_id: row.get(1)?,
        role: row.get(2)?,
        logical_path: row.get(3)?,
        original_name: row.get(4)?,
        mime_type: row.get(5)?,
        file_size: row.get(6)?,
        checksum: row.get(7)?,
        width: row.get(8)?,
        height: row.get(9)?,
        duration_ms: row.get(10)?,
    })
}
fn entry_row(row: &Row<'_>) -> rusqlite::Result<MediaEntry> {
    Ok(MediaEntry {
        id: row.get(0)?,
        kind: kind_from(&row.get::<_, String>(1)?)?,
        folder_id: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        author: row.get(5)?,
        source_url: row.get(6)?,
        license: row.get(7)?,
        favorite: row.get::<_, i64>(8)? != 0,
        processing_status: row.get(9)?,
        processing_message: row.get(10)?,
        format: row.get(11)?,
        file_size: row.get(12)?,
        tags: row
            .get::<_, String>(13)?
            .split(',')
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect(),
        thumbnail_file_id: row.get(14)?,
        primary_file_id: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
const ENTRY_SELECT:&str="SELECT e.id,e.kind,e.folder_id,e.name,e.description,e.author,e.source_url,e.license,e.favorite,e.processing_status,e.processing_message,lower(substr(m.original_name,instr(m.original_name,'.')+1)),m.file_size,COALESCE(group_concat(DISTINCT t.name),'') ,(SELECT id FROM media_files WHERE entry_id=e.id AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END LIMIT 1),e.primary_file_id,e.created_at,e.updated_at FROM media_entries e JOIN media_files m ON m.id=e.primary_file_id LEFT JOIN media_entry_tags et ON et.entry_id=e.id LEFT JOIN localized_tags t ON t.id=et.tag_id";

fn search_filter(request: &MediaSearchRequest) -> (String, Vec<Value>) {
    let mut conditions = vec![
        "e.deleted_at IS NULL".to_string(),
        "e.kind=?".to_string(),
        "EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials')".to_string(),
    ];
    let mut values = vec![Value::Text(request.kind.as_str().into())];
    if !request.query.trim().is_empty() {
        let query = request.query.trim();
        if query.chars().count() < 3 {
            conditions.push(
                "e.id IN (SELECT entry_id FROM media_search WHERE instr(lower(text),lower(?))>0)"
                    .into(),
            );
            values.push(Value::Text(query.into()));
        } else {
            conditions.push(
                "e.id IN (SELECT entry_id FROM media_search WHERE media_search MATCH ?)".into(),
            );
            values.push(Value::Text(format!("\"{}\"", query.replace('"', "\"\""))));
        }
    }
    if let Some(folder) = &request.folder_id {
        if request.include_child_folders {
            conditions.push("e.folder_id IN (WITH RECURSIVE tree(id) AS (SELECT ? UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) SELECT id FROM tree)".into())
        } else {
            conditions.push("e.folder_id=?".into())
        }
        values.push(Value::Text(folder.clone()));
    }
    if request.favorite_only {
        conditions.push("e.favorite=1".into())
    }
    for tag in &request.tags {
        conditions.push("EXISTS(SELECT 1 FROM media_entry_tags x JOIN localized_tags t2 ON t2.id=x.tag_id WHERE x.entry_id=e.id AND lower(t2.name)=lower(?))".into());
        values.push(Value::Text(tag.clone()))
    }
    if !request.formats.is_empty() {
        let marks = vec!["?"; request.formats.len()].join(",");
        conditions.push(format!(
            "lower(substr(m.original_name,instr(m.original_name,'.')+1)) IN ({marks})"
        ));
        values.extend(
            request
                .formats
                .iter()
                .map(|v| Value::Text(v.to_lowercase())),
        )
    }
    (conditions.join(" AND "), values)
}

fn search_order(sort: &str) -> &str {
    match sort {
        "name" => "e.name COLLATE NOCASE",
        "created" => "e.created_at DESC",
        "favorite" => "e.favorite DESC,e.updated_at DESC",
        _ => "e.updated_at DESC",
    }
}

pub fn search(
    connection: &Connection,
    request: &MediaSearchRequest,
) -> Result<Page<MediaEntry>, String> {
    let (where_sql, values) = search_filter(request);
    let total=connection.query_row(&format!("SELECT count(*) FROM media_entries e JOIN media_files m ON m.id=e.primary_file_id WHERE {where_sql}"),params_from_iter(values.clone()),|r|r.get(0)).map_err(|e|e.to_string())?;
    let order = search_order(&request.sort);
    let mut page_values = values;
    page_values.push(Value::Integer(request.limit.clamp(1, 500)));
    page_values.push(Value::Integer(request.offset.max(0)));
    let sql = format!(
        "{ENTRY_SELECT} WHERE {where_sql} GROUP BY e.id ORDER BY {order},e.id LIMIT ? OFFSET ?"
    );
    let mut stmt = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params_from_iter(page_values), entry_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page {
        items,
        total,
        offset: request.offset.max(0),
        limit: request.limit.clamp(1, 500),
    })
}
pub fn select_ids(
    connection: &Connection,
    request: &MediaSearchRequest,
) -> Result<Vec<String>, String> {
    let (where_sql, values) = search_filter(request);
    let order = search_order(&request.sort);
    let sql = format!("SELECT e.id FROM media_entries e JOIN media_files m ON m.id=e.primary_file_id WHERE {where_sql} ORDER BY {order},e.id");
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let ids = statement
        .query_map(params_from_iter(values), |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}
pub fn get(connection: &Connection, id: &str) -> Result<MediaEntryDetail, String> {
    let sql = format!("{ENTRY_SELECT} WHERE e.id=?1 AND e.deleted_at IS NULL AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials') GROUP BY e.id");
    let entry = connection
        .query_row(&sql, [id], entry_row)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("媒体条目不存在")?;
    let mut stmt=connection.prepare("SELECT id,entry_id,role,logical_path,original_name,mime_type,file_size,checksum,width,height,duration_ms,EXISTS(SELECT 1 FROM media_external_files x WHERE x.file_id=media_files.id),(SELECT revision FROM media_external_files x WHERE x.file_id=media_files.id) FROM media_files WHERE entry_id=?1 ORDER BY sort_order,id").map_err(|e|e.to_string())?;
    let files = stmt
        .query_map([id], file_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let ids = |sql: &str| -> Result<Vec<String>, String> {
        let mut s = connection.prepare(sql).map_err(|e| e.to_string())?;
        let values = s
            .query_map([id], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(values)
    };
    Ok(MediaEntryDetail {
        entry,
        files,
        asset_ids: ids("SELECT asset_id FROM media_entry_assets WHERE entry_id=?1")?,
        project_ids: ids("SELECT project_id FROM project_media_entries WHERE entry_id=?1")?,
    })
}
pub fn list_folders(connection: &Connection, kind: &MediaKind) -> Result<Vec<MediaFolder>, String> {
    let mut stmt=connection.prepare("SELECT f.id,f.kind,f.parent_id,f.name,f.sort_order,(SELECT count(*) FROM media_entries e WHERE e.folder_id=f.id AND e.deleted_at IS NULL AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials')) FROM media_folders f WHERE f.kind=?1 AND f.deleted_at IS NULL ORDER BY f.parent_id,f.sort_order,f.name COLLATE NOCASE").map_err(|e|e.to_string())?;
    let values = stmt
        .query_map([kind.as_str()], |r| {
            Ok(MediaFolder {
                id: r.get(0)?,
                kind: kind_from(&r.get::<_, String>(1)?)?,
                parent_id: r.get(2)?,
                name: r.get(3)?,
                sort_order: r.get(4)?,
                entry_count: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}
pub fn upsert_folder(
    connection: &Connection,
    kind: MediaKind,
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
) -> Result<MediaFolder, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("文件夹名称不能为空".into());
    }
    ensure_folder(connection, parent_id.as_deref(), &kind)?;
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if parent_id.as_deref() == Some(&id) {
        return Err("文件夹不能放入自身".into());
    }
    if let Some(parent) = &parent_id {
        let cycle=connection.query_row("WITH RECURSIVE tree(id) AS (SELECT id FROM media_folders WHERE parent_id=?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id) SELECT EXISTS(SELECT 1 FROM tree WHERE id=?2)",params![id,parent],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?!=0;
        if cycle {
            return Err("文件夹不能放入自己的后代".into());
        }
    }
    let timestamp = now();
    let order=connection.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM media_folders WHERE kind=?1 AND parent_id IS ?2",params![kind.as_str(),parent_id],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?;
    connection.execute("INSERT INTO media_folders(id,kind,parent_id,name,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,name=excluded.name,updated_at=excluded.updated_at",params![id,kind.as_str(),parent_id,name,order,timestamp]).map_err(|e|e.to_string())?;
    list_folders(connection, &kind)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or("文件夹保存失败".into())
}
pub fn move_folder(
    connection: &mut Connection,
    id: &str,
    target_parent_id: Option<&str>,
    target_index: usize,
) -> Result<(), String> {
    let kind = connection
        .query_row(
            "SELECT kind FROM media_folders WHERE id=?1 AND deleted_at IS NULL",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("媒体文件夹不存在")?;
    if target_parent_id == Some(id) {
        return Err("文件夹不能放入自身".into());
    }
    if let Some(parent) = target_parent_id {
        let parent_kind = connection
            .query_row(
                "SELECT kind FROM media_folders WHERE id=?1 AND deleted_at IS NULL",
                [parent],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("目标文件夹不存在")?;
        if parent_kind != kind {
            return Err("不能跨媒体库移动文件夹".into());
        }
        let cycle=connection.query_row("WITH RECURSIVE tree(id) AS (SELECT id FROM media_folders WHERE parent_id=?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id) SELECT EXISTS(SELECT 1 FROM tree WHERE id=?2)",params![id,parent],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?!=0;
        if cycle {
            return Err("文件夹不能放入自己的后代".into());
        }
    }
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let mut siblings = {
        let mut statement=tx.prepare("SELECT id FROM media_folders WHERE kind=?1 AND parent_id IS ?2 AND id<>?3 AND deleted_at IS NULL ORDER BY sort_order,name COLLATE NOCASE").map_err(|e|e.to_string())?;
        let values = statement
            .query_map(params![kind, target_parent_id, id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    let index = target_index.min(siblings.len());
    siblings.insert(index, id.into());
    for (order, sibling) in siblings.iter().enumerate() {
        tx.execute(
            "UPDATE media_folders SET parent_id=?1,sort_order=?2,updated_at=?3 WHERE id=?4",
            params![target_parent_id, order as i64, now(), sibling],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
pub fn delete_folder(connection: &mut Connection, id: &str) -> Result<(), String> {
    let (name, kind): (String, String) = connection
        .query_row(
            "SELECT name,kind FROM media_folders WHERE id=?1 AND deleted_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("媒体文件夹不存在")?;
    let folder_count:i64=connection.query_row("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) SELECT count(*) FROM tree",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
    let media_count:i64=connection.query_row("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) SELECT count(*) FROM media_entries e WHERE e.deleted_at IS NULL AND e.folder_id IN (SELECT id FROM tree) AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials')",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
    let batch = Uuid::new_v4().to_string();
    let timestamp = now();
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO deletion_batches(id,kind,label,media_count,media_folder_count,created_at) VALUES(?1,'mediaFolder',?2,?3,?4,?5)",params![batch,format!("{}库文件夹：{name}",kind),media_count,folder_count,timestamp]).map_err(|e|e.to_string())?;
    tx.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) INSERT INTO media_workspace_trash(batch_id,media_entry_id,workspace) SELECT ?2,e.id,'materials' FROM media_entries e JOIN media_workspace_memberships w ON w.media_entry_id=e.id AND w.workspace='materials' WHERE e.deleted_at IS NULL AND e.folder_id IN (SELECT id FROM tree) AND EXISTS(SELECT 1 FROM media_workspace_effective_memberships x WHERE x.media_entry_id=e.id AND x.workspace='learning')",params![id,batch]).map_err(|e|e.to_string())?;
    tx.execute("DELETE FROM media_workspace_memberships WHERE workspace='materials' AND media_entry_id IN (SELECT media_entry_id FROM media_workspace_trash WHERE batch_id=?1)",[&batch]).map_err(|e|e.to_string())?;
    tx.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) UPDATE media_entries SET deleted_at=?2,delete_batch_id=?3 WHERE deleted_at IS NULL AND folder_id IN (SELECT id FROM tree) AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=media_entries.id AND w.workspace='materials')",params![id,timestamp,batch]).map_err(|e|e.to_string())?;
    tx.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) UPDATE media_folders SET deleted_at=?2,delete_batch_id=?3 WHERE id IN (SELECT id FROM tree)",params![id,timestamp,batch]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

pub fn import(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
) -> Result<MediaImportReport, String> {
    import_controlled(
        connection,
        base_dir,
        resource_dir,
        request,
        &|_, _, _, _| {},
        &|| false,
    )
}

pub fn import_controlled(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
    progress: &dyn Fn(usize, usize, String, &MediaImportReport),
    stop: &dyn Fn() -> bool,
) -> Result<MediaImportReport, String> {
    import_controlled_with_capture(
        connection,
        base_dir,
        resource_dir,
        request,
        progress,
        stop,
        None,
    )
}

pub fn import_learning_controlled(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    mut request: MediaImportRequest,
    progress: &dyn Fn(usize, usize, String, &MediaImportReport),
    stop: &dyn Fn() -> bool,
) -> Result<MediaImportReport, String> {
    if request.kind != MediaKind::Video
        || request.folder_id.is_some()
        || !request.tags.is_empty()
        || request.favorite
    {
        return Err("学习库导入仅接受未指定素材文件夹和标签的视频".into());
    }
    request.paths = expand_learning_sources(&request.paths, base_dir, stop)?;
    import_checked(
        connection,
        base_dir,
        resource_dir,
        request,
        progress,
        stop,
        None,
        None,
        true,
    )
}

pub(crate) fn group_learning_import(
    connection: &mut Connection,
    series_id: &str,
    report: &MediaImportReport,
) -> Result<(), String> {
    let series = crate::learning::store::series(connection, series_id)?;
    let mut courses = series.ordered_course_ids;
    let original_count = courses.len();
    for row in &report.rows {
        let Some(id) = row.entry_id.as_ref().or(row.duplicate_entry_id.as_ref()) else {
            continue;
        };
        let course_id: Option<String> = connection.query_row(
            "SELECT c.id FROM learning_courses c JOIN media_workspace_effective_memberships w ON w.media_entry_id=c.media_entry_id AND w.workspace='learning' WHERE c.media_entry_id=?1",
            [id], |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        if let Some(id) = course_id {
            if !courses.contains(&id) {
                courses.push(id);
            }
        }
    }
    if courses.len() != original_count {
        crate::learning::store::set_series_members(
            connection,
            series_id,
            series.revision,
            &courses,
        )?;
    }
    Ok(())
}

fn expand_learning_sources(
    paths: &[String],
    base_dir: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<String>, String> {
    if paths.len() > 500 {
        return Err("一次最多选择 500 项".into());
    }
    let mut files = Vec::new();
    for raw in paths {
        if stop() {
            return Err("扫描已取消，未开始导入".into());
        }
        let source = Path::new(raw);
        if !source.is_dir() {
            files.push(raw.clone());
        } else {
            if source.ancestors().any(crate::asset::scanner::is_link) {
                return Err("不接受符号链接、junction 或重解析目录".into());
            }
            let root = source
                .canonicalize()
                .map_err(|e| format!("文件夹不可访问：{e}"))?;
            let library = base_dir.canonicalize().map_err(|e| e.to_string())?;
            if root.starts_with(&library)
                || root
                    .ancestors()
                    .any(|p| p.join("library.json").is_file() && p.join("library.db").is_file())
            {
                return Err("请选择素材库之外的文件夹".into());
            }
            let walker = walkdir::WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    !crate::asset::scanner::is_link(e.path())
                        && !e.path().starts_with(&library)
                        && !matches!(
                            e.file_name().to_str(),
                            Some(".git" | ".dcc" | "node_modules" | "target")
                        )
                        && !(e.file_type().is_dir()
                            && e.path().join("library.json").is_file()
                            && e.path().join("library.db").is_file())
                });
            for (index, entry) in walker.enumerate() {
                if stop() {
                    return Err("扫描已取消，未开始导入".into());
                }
                if index >= 10000 {
                    return Err("文件夹条目超过 10000 项，请选择较小的文件夹".into());
                }
                let entry = entry.map_err(|e| format!("读取文件夹失败：{e}"))?;
                if entry.file_type().is_file()
                    && crate::asset::scanner::kind(entry.path()) == Some("video")
                {
                    let checked = crate::asset::scanner::validate_source(entry.path(), base_dir)?;
                    files.push(checked.to_str().ok_or("文件路径不是有效文本")?.to_owned());
                    if files.len() > 500 {
                        return Err("一次最多导入 500 个视频，请选择较小的文件夹".into());
                    }
                }
            }
        }
        if files.len() > 500 {
            return Err("一次最多导入 500 个视频".into());
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err("所选文件夹中没有支持的视频".into());
    }
    Ok(files)
}

#[cfg(test)]
mod learning_folder_tests {
    use super::*;

    #[test]
    fn learning_import_groups_multiple_courses_without_losing_existing_members() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        std::fs::create_dir_all(&library).unwrap();
        let first = temp.path().join("one.mp4");
        let second = temp.path().join("two.mp4");
        std::fs::write(&first, b"synthetic first").unwrap();
        std::fs::write(&second, b"synthetic second").unwrap();
        let mut db = crate::db::open_database(&library.join("db.sqlite")).unwrap();
        let target =
            crate::learning::store::upsert_series(&mut db, None, "课程系列", "", None).unwrap();
        let report = import_learning_controlled(
            &mut db,
            &library,
            None,
            MediaImportRequest {
                kind: MediaKind::Video,
                paths: vec![
                    first.to_string_lossy().into(),
                    second.to_string_lossy().into(),
                ],
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(report.imported, 2, "{:?}", report);
        group_learning_import(&mut db, &target.id, &report).unwrap();
        group_learning_import(&mut db, &target.id, &report).unwrap();
        assert_eq!(
            crate::learning::store::series(&db, &target.id)
                .unwrap()
                .ordered_course_ids
                .len(),
            2
        );
    }

    #[test]
    fn materials_link_commands_reject_learning_only_ids() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&temp.path().join("db.sqlite")).unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('lesson','video','lesson','file','now','now')", []).unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('lesson','learning')", []).unwrap();
        db.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id='lesson' AND workspace='materials'", []).unwrap();
        assert!(require_materials_entries(&db, &["lesson".into()]).is_err());
    }

    #[test]
    fn folder_expansion_finds_nested_videos_and_rejects_over_limit() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        let library = temp.path().join("library");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(root.join("nested/lesson.MP4"), b"fixture").unwrap();
        std::fs::write(root.join("ignore.txt"), b"fixture").unwrap();
        let files =
            expand_learning_sources(&[root.to_string_lossy().into()], &library, &|| false).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("lesson.MP4"));
        for index in 0..500 {
            std::fs::write(root.join(format!("{index}.mp4")), b"fixture").unwrap();
        }
        assert!(
            expand_learning_sources(&[root.to_string_lossy().into()], &library, &|| false).is_err()
        );
        assert!(
            expand_learning_sources(&[root.to_string_lossy().into()], &library, &|| true).is_err()
        );
    }

    #[test]
    fn folder_import_creates_learning_courses_without_materials_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source/nested");
        let library = temp.path().join("library");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        let file = root.join("lesson.mp4");
        std::fs::write(&file, b"synthetic video").unwrap();
        let mut db = crate::db::open_database(&library.join("db.sqlite")).unwrap();
        let report = import_learning_controlled(
            &mut db,
            &library,
            None,
            MediaImportRequest {
                kind: MediaKind::Video,
                paths: vec![temp.path().join("source").to_string_lossy().into()],
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!((report.imported, report.failed), (1, 0));
        assert_eq!(
            db.query_row("SELECT count(*) FROM learning_courses", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_workspace_memberships WHERE workspace='materials'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(std::fs::read(file).unwrap(), b"synthetic video");
    }
}

pub fn import_controlled_with_capture(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
    progress: &dyn Fn(usize, usize, String, &MediaImportReport),
    stop: &dyn Fn() -> bool,
    capture: Option<&crate::browser_capture::Capture>,
) -> Result<MediaImportReport, String> {
    import_checked(
        connection,
        base_dir,
        resource_dir,
        request,
        progress,
        stop,
        capture,
        None,
        false,
    )
}

/// Core imports bind the analyzed file's digest through final managed-file registration.
pub(crate) fn import_controlled_verified(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
    stop: &dyn Fn() -> bool,
    expected_hash: &str,
) -> Result<MediaImportReport, String> {
    if request.paths.len() != 1 {
        return Err("校验导入仅接受一个文件".into());
    }
    import_checked(
        connection,
        base_dir,
        resource_dir,
        request,
        &|_, _, _, _| {},
        stop,
        None,
        Some(expected_hash),
        false,
    )
}

fn import_checked(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
    progress: &dyn Fn(usize, usize, String, &MediaImportReport),
    stop: &dyn Fn() -> bool,
    capture: Option<&crate::browser_capture::Capture>,
    expected_hash: Option<&str>,
    learning: bool,
) -> Result<MediaImportReport, String> {
    if request.paths.is_empty() {
        return Ok(MediaImportReport {
            imported: 0,
            skipped: 0,
            failed: 0,
            rows: vec![],
        });
    }
    if request.paths.len() > 500 {
        return Err("一次最多导入 500 个文件".into());
    }
    ensure_folder(connection, request.folder_id.as_deref(), &request.kind)?;
    let (ffmpeg, ffprobe) = media::ffmpeg_paths(resource_dir);
    let mut report = MediaImportReport {
        imported: 0,
        skipped: 0,
        failed: 0,
        rows: vec![],
    };
    let total = request.paths.len();
    for (index, raw) in request.paths.clone().into_iter().enumerate() {
        if stop() {
            break;
        }
        progress(
            index,
            total,
            Path::new(&raw)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            &report,
        );
        let source = PathBuf::from(&raw);
        let mut staged_root: Option<PathBuf> = None;
        let result = (|| -> Result<(String, Option<String>), String> {
            if !source.is_file() {
                return Err("文件不存在".into());
            }
            let (ext, _mime) = validate_kind_path(&request.kind, &source)?;
            if expected_hash.is_some() {
                crate::asset::scanner::validate_source(&source, base_dir)?;
            }
            let checksum = digest(&source)?;
            if expected_hash.is_some_and(|hash| hash != checksum) {
                return Err("文件在扫描或分析后发生变化，请重新扫描".into());
            }
            if let Some((existing,materials,in_learning))=connection.query_row("SELECT e.id,EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials'),EXISTS(SELECT 1 FROM media_workspace_effective_memberships w WHERE w.media_entry_id=e.id AND w.workspace='learning') FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE f.checksum=?1",[&checksum],|r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?,r.get::<_,bool>(2)?))).optional().map_err(|e|e.to_string())? {
                if learning && !in_learning {
                    return Err("相同文件已存在于素材库，尚未加入学习库；未更改归属".into());
                }
                if !learning && !materials {
                    return Err("相同文件已存在于学习库，尚未加入素材库；未更改归属".into());
                }
                return Ok((existing,Some("duplicate".into())));
            }
            let id = Uuid::new_v4().to_string();
            let root = format!("media-library/{}/{}", request.kind.as_str(), id);
            staged_root = Some(media::safe_relative(base_dir, &root)?);
            let timestamp = now();
            let name = source
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("未命名媒体")
                .chars()
                .take(255)
                .collect::<String>();
            let mut files: Vec<(
                String,
                String,
                String,
                String,
                String,
                i64,
                String,
                Option<i64>,
                Option<i64>,
                Option<i64>,
            )> = vec![];
            if request.kind == MediaKind::Image {
                let stored = images::import_image_to(base_dir, &source, &root)?;
                if digest(&media::safe_relative(base_dir, &stored.original_rel_path)?)? != checksum
                {
                    return Err("源文件在复制时发生变化，未写入素材库".into());
                }
                let main_id = Uuid::new_v4().to_string();
                let thumb_id = Uuid::new_v4().to_string();
                files.push((
                    main_id.clone(),
                    "main".into(),
                    stored.original_name.clone(),
                    stored.original_name.clone(),
                    stored.original_rel_path,
                    fs::metadata(&source).map_err(|e| e.to_string())?.len() as i64,
                    checksum,
                    Some(stored.pixel_width as i64),
                    Some(stored.pixel_height as i64),
                    None,
                ));
                files.push((
                    thumb_id,
                    "thumbnail".into(),
                    "thumbnail.webp".into(),
                    "thumbnail.webp".into(),
                    stored.thumbnail_rel_path,
                    0,
                    String::new(),
                    Some(480),
                    None,
                    None,
                ));
                if expected_hash.is_some() {
                    crate::asset::scanner::validate_source(&source, base_dir)?;
                }
                insert_entry(
                    connection, &id, &request, &name, &main_id, &timestamp, files, capture,
                    learning,
                )?;
            } else {
                let deps = if request.kind == MediaKind::Model {
                    collect_model_dependencies(&source, &ext)?
                } else {
                    vec![]
                };
                let main_id = Uuid::new_v4().to_string();
                let file_name = source
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("media")
                    .to_string();
                let original_rel = format!("{root}/files/{file_name}");
                media::copy_atomic(&source, &media::safe_relative(base_dir, &original_rel)?)?;
                if digest(&media::safe_relative(base_dir, &original_rel)?)? != checksum {
                    return Err("源文件在复制时发生变化，未写入素材库".into());
                }
                let (duration, width, height) = media::probe(ffprobe.as_deref(), &source);
                files.push((
                    main_id.clone(),
                    "main".into(),
                    file_name.clone(),
                    file_name.clone(),
                    original_rel,
                    fs::metadata(&source).map_err(|e| e.to_string())?.len() as i64,
                    checksum,
                    width,
                    height,
                    duration,
                ));
                for rel in deps {
                    let dep_source = source.parent().unwrap().join(&rel);
                    let dep_rel =
                        format!("{root}/files/{}", rel.to_string_lossy().replace('\\', "/"));
                    media::copy_atomic(&dep_source, &media::safe_relative(base_dir, &dep_rel)?)?;
                    files.push((
                        Uuid::new_v4().to_string(),
                        "dependency".into(),
                        rel.to_string_lossy().into_owned(),
                        rel.file_name()
                            .and_then(|v| v.to_str())
                            .unwrap_or("dependency")
                            .into(),
                        dep_rel,
                        fs::metadata(dep_source).map_err(|e| e.to_string())?.len() as i64,
                        String::new(),
                        None,
                        None,
                        None,
                    ));
                }
                if matches!(request.kind, MediaKind::Audio | MediaKind::Video) {
                    let proxy_rel = format!(
                        "{root}/proxy.{}",
                        if request.kind == MediaKind::Video {
                            "webm"
                        } else {
                            "ogg"
                        }
                    );
                    let thumb_rel = format!("{root}/thumbnail.png");
                    let proxy = media::safe_relative(base_dir, &proxy_rel)?;
                    let thumb = media::safe_relative(base_dir, &thumb_rel)?;
                    match media::process(
                        ffmpeg.as_deref(),
                        request.kind.as_str(),
                        &ext,
                        &source,
                        &proxy,
                        &thumb,
                    ) {
                        Ok((p, t, _)) => {
                            if p.is_some() {
                                let proxy_name = if request.kind == MediaKind::Video {
                                    "proxy.webm"
                                } else {
                                    "proxy.ogg"
                                };
                                files.push((
                                    Uuid::new_v4().to_string(),
                                    "proxy".into(),
                                    proxy_name.into(),
                                    proxy_name.into(),
                                    proxy_rel,
                                    fs::metadata(&proxy).map(|m| m.len() as i64).unwrap_or(0),
                                    String::new(),
                                    None,
                                    None,
                                    duration,
                                ))
                            }
                            if t.is_some() {
                                files.push((
                                    Uuid::new_v4().to_string(),
                                    if request.kind == MediaKind::Audio {
                                        "waveform"
                                    } else {
                                        "thumbnail"
                                    }
                                    .into(),
                                    "thumbnail.png".into(),
                                    "thumbnail.png".into(),
                                    thumb_rel,
                                    fs::metadata(&thumb).map(|m| m.len() as i64).unwrap_or(0),
                                    String::new(),
                                    width,
                                    height,
                                    None,
                                ))
                            }
                        }
                        Err(error) => return Err(error),
                    }
                }
                if expected_hash.is_some() {
                    crate::asset::scanner::validate_source(&source, base_dir)?;
                }
                insert_entry(
                    connection, &id, &request, &name, &main_id, &timestamp, files, capture,
                    learning,
                )?;
            }
            Ok((id, None))
        })();
        match result {
            Ok((id, Some(_))) => {
                report.skipped += 1;
                report.rows.push(MediaImportRow {
                    path: raw,
                    status: "duplicate".into(),
                    message: "文件内容与现有媒体完全相同".into(),
                    entry_id: None,
                    duplicate_entry_id: Some(id),
                })
            }
            Ok((id, None)) => {
                report.imported += 1;
                report.rows.push(MediaImportRow {
                    path: raw,
                    status: "imported".into(),
                    message: "导入成功".into(),
                    entry_id: Some(id),
                    duplicate_entry_id: None,
                })
            }
            Err(error) => {
                if let Some(root) = staged_root.as_ref() {
                    let _ = fs::remove_dir_all(root);
                }
                report.failed += 1;
                report.rows.push(MediaImportRow {
                    path: raw,
                    status: "error".into(),
                    message: error,
                    entry_id: None,
                    duplicate_entry_id: None,
                })
            }
        }
    }
    progress(report.rows.len(), total, String::new(), &report);
    Ok(report)
}

pub fn collect_asset_previews(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    asset_id: &str,
) -> Result<MediaImportReport, String> {
    let mut groups: Vec<(MediaKind, Vec<String>)> = Vec::new();
    let image_paths = {
        let mut statement = connection
            .prepare("SELECT original_rel_path FROM images WHERE asset_id=?1 ORDER BY sort_order")
            .map_err(|e| e.to_string())?;
        let values = statement
            .query_map([asset_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    groups.push((MediaKind::Image, image_paths));
    for (kind, value) in [
        (MediaKind::Model, "model"),
        (MediaKind::Audio, "audio"),
        (MediaKind::Video, "video"),
    ] {
        let paths = {
            let mut statement=connection.prepare("SELECT original_rel_path FROM asset_media WHERE asset_id=?1 AND kind=?2 ORDER BY sort_order").map_err(|e|e.to_string())?;
            let values = statement
                .query_map(params![asset_id, value], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            values
        };
        groups.push((kind, paths));
    }
    let mut combined = MediaImportReport {
        imported: 0,
        skipped: 0,
        failed: 0,
        rows: vec![],
    };
    for (kind, relative) in groups {
        if relative.is_empty() {
            continue;
        }
        let paths = relative
            .into_iter()
            .map(|path| {
                media::safe_relative(base_dir, &path).map(|p| p.to_string_lossy().into_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let report = import(
            connection,
            base_dir,
            resource_dir,
            MediaImportRequest {
                kind,
                paths,
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
        )?;
        for row in &report.rows {
            if let Some(entry_id) = row.entry_id.as_ref().or(row.duplicate_entry_id.as_ref()) {
                connection
                    .execute(
                        "INSERT OR IGNORE INTO media_entry_assets(entry_id,asset_id) VALUES(?1,?2)",
                        params![entry_id, asset_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        combined.imported += report.imported;
        combined.skipped += report.skipped;
        combined.failed += report.failed;
        combined.rows.extend(report.rows);
    }
    Ok(combined)
}
fn insert_entry(
    connection: &mut Connection,
    id: &str,
    request: &MediaImportRequest,
    name: &str,
    main_id: &str,
    timestamp: &str,
    files: Vec<(
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    )>,
    capture: Option<&crate::browser_capture::Capture>,
    learning: bool,
) -> Result<(), String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO media_entries(id,kind,folder_id,name,favorite,processing_status,primary_file_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'ready',?6,?7,?7)",params![id,request.kind.as_str(),request.folder_id,name,request.favorite as i64,main_id,timestamp]).map_err(|e|e.to_string())?;
    for (index, (fid, role, logical, original, rel, size, sum, w, h, d)) in
        files.into_iter().enumerate()
    {
        let mime = mime_for(&original);
        tx.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,mime_type,file_size,checksum,width,height,duration_ms,sort_order,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![fid,id,role,logical,original,rel,mime,size,sum,w,h,d,index as i64,timestamp]).map_err(|e|e.to_string())?;
    }
    upsert_tags(&tx, id, &request.tags)?;
    if let Some(capture) = capture {
        crate::browser_capture::apply_source(&tx, capture, id)?;
    }
    reindex(&tx, id)?;
    let core_id = crate::asset::repository::register_origin(
        &tx,
        &crate::asset::models::CoreOrigin {
            kind: "media".into(),
            id: id.into(),
        },
    )?;
    crate::asset::repository::register_files(&tx, &core_id)?;
    if learning {
        crate::learning::store::ensure_course(&tx, id)?;
        tx.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES(?1,'learning')",[id]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'",[id]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
fn mime_for(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "obj" => "model/obj",
        "stl" => "model/stl",
        _ => "application/octet-stream",
    }
}

pub fn update(
    connection: &Connection,
    id: &str,
    name: String,
    description: String,
    author: String,
    source_url: String,
    license: String,
    tags: Vec<String>,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("名称不能为空".into());
    }
    let changed=connection.execute("UPDATE media_entries SET name=?1,description=?2,author=?3,source_url=?4,license=?5,updated_at=?6 WHERE id=?7 AND deleted_at IS NULL AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=media_entries.id AND w.workspace='materials')",params![name.trim(),description.trim(),author.trim(),source_url.trim(),license.trim(),now(),id]).map_err(|e|e.to_string())?;
    if changed == 0 {
        return Err("媒体条目不存在".into());
    }
    upsert_tags(connection, id, &tags)?;
    reindex(connection, id)
}
pub fn batch_update(
    connection: &mut Connection,
    update: MediaBatchUpdate,
) -> Result<BatchUpdateReport, String> {
    if update.ids.len() > 5000 {
        return Err("一次最多整理 5000 条媒体".into());
    }
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let mut changed = 0;
    for id in &update.ids {
        let belongs:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM media_entries e JOIN media_workspace_memberships w ON w.media_entry_id=e.id AND w.workspace='materials' WHERE e.id=?1 AND e.deleted_at IS NULL)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !belongs {
            return Err("媒体条目不存在或不属于素材库".into());
        }
        if update.clear_folder {
            changed+=tx.execute("UPDATE media_entries SET folder_id=NULL,updated_at=?1 WHERE id=?2 AND deleted_at IS NULL",params![now(),id]).map_err(|e|e.to_string())?
        } else if let Some(folder) = &update.folder_id {
            changed+=tx.execute("UPDATE media_entries SET folder_id=?1,updated_at=?2 WHERE id=?3 AND deleted_at IS NULL",params![folder,now(),id]).map_err(|e|e.to_string())?
        }
        if let Some(favorite) = update.favorite {
            tx.execute("UPDATE media_entries SET favorite=?1,updated_at=?2 WHERE id=?3 AND deleted_at IS NULL",params![favorite as i64,now(),id]).map_err(|e|e.to_string())?;
        }
        for tag in &update.add_tags {
            let current = get(&tx, id)?.entry.tags;
            let mut next = current;
            if !next.iter().any(|v| v.eq_ignore_ascii_case(tag)) {
                next.push(tag.clone())
            }
            upsert_tags(&tx, id, &next)?
        }
        if !update.remove_tags.is_empty() {
            let current = get(&tx, id)?.entry.tags;
            let next = current
                .into_iter()
                .filter(|v| !update.remove_tags.iter().any(|x| x.eq_ignore_ascii_case(v)))
                .collect::<Vec<_>>();
            upsert_tags(&tx, id, &next)?
        }
        reindex(&tx, id)?
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(BatchUpdateReport {
        requested: update.ids.len(),
        updated: changed,
    })
}
pub fn delete_entries(connection: &mut Connection, ids: Vec<String>) -> Result<usize, String> {
    let batch = Uuid::new_v4().to_string();
    let timestamp = now();
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let mut count = 0;
    tx.execute("INSERT INTO deletion_batches(id,kind,label,media_count,created_at) VALUES(?1,'media','',0,?2)",params![batch,timestamp]).map_err(|e|e.to_string())?;
    for id in ids {
        let shared:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM media_workspace_effective_memberships WHERE media_entry_id=?1 AND workspace='learning')",[&id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if shared {
            let inserted=tx.execute("INSERT INTO media_workspace_trash(batch_id,media_entry_id,workspace) SELECT ?1,w.media_entry_id,'materials' FROM media_workspace_memberships w JOIN media_entries e ON e.id=w.media_entry_id WHERE w.media_entry_id=?2 AND w.workspace='materials' AND e.deleted_at IS NULL",params![batch,id]).map_err(|e|e.to_string())?;
            if inserted > 0 {
                tx.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'",[&id]).map_err(|e|e.to_string())?;
                count += 1;
            }
        } else {
            count+=tx.execute("UPDATE media_entries SET deleted_at=?1,delete_batch_id=?2 WHERE id=?3 AND deleted_at IS NULL AND EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=media_entries.id AND w.workspace='materials')",params![timestamp,batch,id]).map_err(|e|e.to_string())?;
        }
    }
    if count > 0 {
        tx.execute(
            "UPDATE deletion_batches SET label=?1,media_count=?2 WHERE id=?3",
            params![format!("{count} 个媒体条目"), count as i64, batch],
        )
        .map_err(|e| e.to_string())?;
    } else {
        tx.execute("DELETE FROM deletion_batches WHERE id=?1", [&batch])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(count)
}
pub fn link_assets(
    connection: &Connection,
    entry_ids: Vec<String>,
    asset_ids: Vec<String>,
    link: bool,
) -> Result<usize, String> {
    let mut n = 0;
    for e in entry_ids {
        for a in &asset_ids {
            n += if link {
                connection.execute(
                    "INSERT OR IGNORE INTO media_entry_assets(entry_id,asset_id) VALUES(?1,?2)",
                    params![e, a],
                )
            } else {
                connection.execute(
                    "DELETE FROM media_entry_assets WHERE entry_id=?1 AND asset_id=?2",
                    params![e, a],
                )
            }
            .map_err(|x| x.to_string())?
        }
    }
    Ok(n)
}

pub(crate) fn require_materials_entries(
    connection: &Connection,
    ids: &[String],
) -> Result<(), String> {
    for id in ids {
        let allowed: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM media_entries e JOIN media_workspace_memberships w ON w.media_entry_id=e.id AND w.workspace='materials' WHERE e.id=?1 AND e.deleted_at IS NULL)", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
        if !allowed {
            return Err("媒体条目不存在或不属于素材库".into());
        }
    }
    Ok(())
}
pub fn link_projects(
    connection: &Connection,
    entry_ids: Vec<String>,
    project_ids: Vec<String>,
    link: bool,
) -> Result<usize, String> {
    let mut n = 0;
    for e in entry_ids {
        for p in &project_ids {
            n+=if link{connection.execute("INSERT OR IGNORE INTO project_media_entries(project_id,entry_id,created_at) VALUES(?1,?2,?3)",params![p,e,now()])}else{connection.execute("DELETE FROM project_media_entries WHERE project_id=?1 AND entry_id=?2",params![p,e])}.map_err(|x|x.to_string())?
        }
    }
    Ok(n)
}
pub fn protocol_response(
    connection: &Connection,
    base_dir: &Path,
    file_id: &str,
    range: Option<&str>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let mime: String = connection
        .query_row(
            "SELECT mime_type FROM media_files WHERE id=?1",
            [file_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("媒体文件不存在")?;
    let path = crate::media_access::resolve_media_file(
        connection,
        base_dir,
        file_id,
        &crate::media_access::LocalMediaGrants::local()?,
    )?;
    media::file_response(&path, &mime, range)
}
pub fn bundle_response(
    connection: &Connection,
    base_dir: &Path,
    entry_id: &str,
    logical: &str,
    range: Option<&str>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let logical = safe_dependency(logical)?
        .to_string_lossy()
        .replace('\\', "/");
    let (rel, mime): (String, String) = connection
        .query_row(
            "SELECT f.rel_path,f.mime_type FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE f.entry_id=?1 AND f.logical_path=?2 AND e.kind='model' AND e.deleted_at IS NULL",
            params![entry_id, logical],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("模型依赖不存在")?;
    media::file_response(&media::safe_relative(base_dir, &rel)?, &mime, range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_database;
    use tempfile::tempdir;
    #[test]
    fn learning_import_creates_course_without_materials_membership() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let source = dir.path().join("lesson.mp4");
        fs::write(&source, b"synthetic video bytes").unwrap();
        let request = MediaImportRequest {
            kind: MediaKind::Video,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: vec![],
            favorite: false,
        };
        let report = import_learning_controlled(
            &mut db,
            dir.path(),
            None,
            request.clone(),
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!((report.imported, report.failed), (1, 0));
        let id = report.rows[0].entry_id.as_ref().unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'",[id],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_effective_memberships WHERE media_entry_id=?1 AND workspace='learning'",[id],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM learning_courses WHERE media_entry_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let repeated = import_learning_controlled(
            &mut db,
            dir.path(),
            None,
            request.clone(),
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(
            (repeated.imported, repeated.skipped, repeated.failed),
            (0, 1, 0)
        );
        let materials = import(&mut db, dir.path(), None, request).unwrap();
        assert_eq!(
            (materials.imported, materials.skipped, materials.failed),
            (0, 0, 1)
        );
        assert_eq!(fs::read(source).unwrap(), b"synthetic video bytes");
    }
    #[test]
    fn learning_import_failure_rolls_back_and_preserves_source() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let source = dir.path().join("lesson.mp4");
        fs::write(&source, b"synthetic video bytes").unwrap();
        db.execute_batch("CREATE TRIGGER reject_learning_course BEFORE INSERT ON learning_courses BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        let request = MediaImportRequest {
            kind: MediaKind::Video,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: vec![],
            favorite: false,
        };
        let report = import_learning_controlled(
            &mut db,
            dir.path(),
            None,
            request,
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!((report.imported, report.failed), (0, 1));
        assert_eq!(
            db.query_row("SELECT count(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_workspace_memberships",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(fs::read(source).unwrap(), b"synthetic video bytes");
    }
    #[test]
    fn materials_import_reports_learning_only_duplicate_without_exposing_it() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let source = dir.path().join("test.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let request = MediaImportRequest {
            kind: MediaKind::Image,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: vec![],
            favorite: false,
        };
        let first = import(&mut db, dir.path(), None, request.clone()).unwrap();
        let entry = first.rows[0].entry_id.as_ref().unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES(?1,'learning')", [entry]).unwrap();
        db.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'", [entry]).unwrap();
        let second = import(&mut db, dir.path(), None, request).unwrap();
        assert_eq!((second.imported, second.skipped, second.failed), (0, 0, 1));
        assert!(second.rows[0].message.contains("学习库"));
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'", [entry], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }
    #[test]
    fn trash_and_restore_materials_membership_preserves_shared_learning_media() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('shared','video','Shared','file','now','now')", []).unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('shared','learning')", []).unwrap();
        assert_eq!(delete_entries(&mut db, vec!["shared".into()]).unwrap(), 1);
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='shared' AND workspace='materials'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_entries WHERE id='shared' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let trash = crate::db::list_trash(&db, 0, 10).unwrap();
        crate::db::restore_trash_batch(&mut db, &trash.items[0].id).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='shared'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn trash_and_restore_folder_keeps_shared_course_playable() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let folder = upsert_folder(&db, MediaKind::Video, None, None, "Courses".into()).unwrap();
        db.execute("INSERT INTO media_entries(id,kind,folder_id,name,primary_file_id,created_at,updated_at) VALUES('shared','video',?1,'Shared','file','now','now')", [&folder.id]).unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('shared','learning')", []).unwrap();
        delete_folder(&mut db, &folder.id).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_entries WHERE id='shared' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='shared' AND workspace='materials'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        let trash = crate::db::list_trash(&db, 0, 10).unwrap();
        crate::db::restore_trash_batch(&mut db, &trash.items[0].id).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='shared'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn purging_materials_membership_keeps_shared_video_bytes() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let file = dir.path().join("video.mp4");
        fs::write(&file, b"test-video").unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('shared','video','Shared','file','now','now')", []).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES('file','shared','main','video.mp4','video.mp4','video.mp4','now')", []).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES('course','shared','now','now')", []).unwrap();
        delete_entries(&mut db, vec!["shared".into()]).unwrap();
        let trash = crate::db::list_trash(&db, 0, 10).unwrap();
        crate::db::purge_trash_batch(&mut db, dir.path(), &trash.items[0].id).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"test-video");
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM learning_courses WHERE id='course'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_entries WHERE id='shared' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM media_workspace_trash", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn media_with_learning_membership_cannot_be_trashed_or_physically_deleted() {
        let dir = tempdir().unwrap();
        let db = open_database(&dir.path().join("db.sqlite")).unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('shared','video','Shared','file','now','now')", []).unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('shared','learning')", []).unwrap();
        assert!(db
            .execute(
                "UPDATE media_entries SET deleted_at='now' WHERE id='shared'",
                []
            )
            .is_err());
        assert!(db
            .execute("DELETE FROM media_entries WHERE id='shared'", [])
            .is_err());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_entries WHERE id='shared' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='shared'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn materials_search_and_select_exclude_learning_only_media() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let folder = upsert_folder(&db, MediaKind::Video, None, None, "Videos".into()).unwrap();
        for id in ["materials", "learning"] {
            db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video',?1,?2,'now','now')", params![id, format!("{id}-file")]).unwrap();
            db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','v.mp4','v.mp4',?3,'now')", params![format!("{id}-file"), id, format!("{id}/v.mp4")]).unwrap();
        }
        db.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id='learning' AND workspace='materials'", []).unwrap();
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('learning','learning')", []).unwrap();
        db.execute(
            "UPDATE media_entries SET folder_id=?1 WHERE id IN ('materials','learning')",
            [&folder.id],
        )
        .unwrap();
        let request = MediaSearchRequest {
            kind: MediaKind::Video,
            query: String::new(),
            folder_id: None,
            include_child_folders: false,
            tags: vec![],
            formats: vec![],
            favorite_only: false,
            sort: "updated".into(),
            offset: 0,
            limit: 20,
        };
        assert_eq!(search(&db, &request).unwrap().total, 1);
        assert_eq!(select_ids(&db, &request).unwrap(), vec!["materials"]);
        assert!(get(&db, "learning").is_err());
        assert_eq!(
            list_folders(&db, &MediaKind::Video).unwrap()[0].entry_count,
            1
        );
        assert!(update(
            &db,
            "learning",
            "changed".into(),
            "".into(),
            "".into(),
            "".into(),
            "".into(),
            vec![]
        )
        .is_err());
        let batch = MediaBatchUpdate {
            ids: vec!["materials".into(), "learning".into()],
            folder_id: None,
            clear_folder: false,
            add_tags: vec![],
            remove_tags: vec![],
            favorite: Some(true),
        };
        assert!(batch_update(&mut db, batch).is_err());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_entries WHERE favorite=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn selects_every_matching_media_id_beyond_the_first_virtual_page() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let folder = upsert_folder(&db, MediaKind::Image, None, None, "参考图".into()).unwrap();
        let transaction = db.transaction().unwrap();
        for index in 0..507 {
            let id = format!("image-{index}");
            let file_id = format!("file-{index}");
            transaction.execute("INSERT INTO media_entries(id,kind,folder_id,name,primary_file_id,created_at,updated_at) VALUES(?1,'image',?2,?1,?3,'now','now')", params![id,folder.id,file_id]).unwrap();
            transaction.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','image.png','image.png',?3,'now')", params![file_id,id,format!("media-library/image/{index}/originals/image.png")]).unwrap();
        }
        transaction.commit().unwrap();
        let ids = select_ids(
            &db,
            &MediaSearchRequest {
                kind: MediaKind::Image,
                query: String::new(),
                folder_id: Some(folder.id),
                include_child_folders: true,
                tags: vec![],
                formats: vec![],
                favorite_only: false,
                sort: "updated".into(),
                offset: 150,
                limit: 5,
            },
        )
        .unwrap();
        assert_eq!(ids.len(), 507);
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 507);
    }
    #[test]
    fn rejects_unsafe_model_dependency_and_folder_cycle() {
        assert!(safe_dependency("../secret.png").is_err());
        assert!(safe_dependency("https://example.com/a.bin").is_err());
        let dir = tempdir().unwrap();
        let db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let a = upsert_folder(&db, MediaKind::Model, None, None, "A".into()).unwrap();
        let b = upsert_folder(&db, MediaKind::Model, None, Some(a.id.clone()), "B".into()).unwrap();
        assert!(upsert_folder(&db, MediaKind::Model, Some(a.id), Some(b.id), "A".into()).is_err());
    }
    #[test]
    fn imports_image_and_skips_same_hash() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let source = dir.path().join("a.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let req = MediaImportRequest {
            kind: MediaKind::Image,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: vec!["概念图".into()],
            favorite: false,
        };
        let one = import(&mut db, dir.path(), None, req.clone()).unwrap();
        let two = import(&mut db, dir.path(), None, req).unwrap();
        assert_eq!(one.imported, 1);
        assert_eq!(two.skipped, 1);
        assert_eq!(
            search(
                &db,
                &MediaSearchRequest {
                    kind: MediaKind::Image,
                    query: "a".into(),
                    folder_id: None,
                    include_child_folders: true,
                    tags: vec![],
                    formats: vec![],
                    favorite_only: false,
                    sort: "updated".into(),
                    offset: 0,
                    limit: 80
                }
            )
            .unwrap()
            .total,
            1
        );
        assert_eq!(
            search(
                &db,
                &MediaSearchRequest {
                    kind: MediaKind::Image,
                    query: "".into(),
                    folder_id: None,
                    include_child_folders: true,
                    tags: vec![],
                    formats: vec![],
                    favorite_only: false,
                    sort: "updated".into(),
                    offset: 0,
                    limit: 80
                }
            )
            .unwrap()
            .total,
            1
        );
        let entry = one.rows[0].entry_id.clone().unwrap();
        delete_entries(&mut db, vec![entry.clone()]).unwrap();
        let trash = crate::db::list_trash(&db, 0, 20).unwrap();
        assert_eq!(trash.items[0].media_count, 1);
        crate::db::restore_trash_batch(&mut db, &trash.items[0].id).unwrap();
        assert_eq!(get(&db, &entry).unwrap().entry.id, entry);
    }

    #[test]
    fn imports_model_audio_and_video_without_terminating_the_library() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let model = dir.path().join("mesh.obj");
        let audio = dir.path().join("sound.mp3");
        let video = dir.path().join("clip.mp4");
        fs::write(&model, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        fs::write(&audio, b"test mp3 payload").unwrap();
        fs::write(&video, b"test mp4 payload").unwrap();

        for (kind, source) in [
            (MediaKind::Model, model),
            (MediaKind::Audio, audio),
            (MediaKind::Video, video),
        ] {
            let report = import(
                &mut db,
                dir.path(),
                None,
                MediaImportRequest {
                    kind: kind.clone(),
                    paths: vec![source.to_string_lossy().into_owned()],
                    folder_id: None,
                    tags: vec![],
                    favorite: false,
                },
            )
            .unwrap();
            assert_eq!(report.imported, 1, "{kind:?} import failed");
            assert_eq!(
                search(
                    &db,
                    &MediaSearchRequest {
                        kind,
                        query: String::new(),
                        folder_id: None,
                        include_child_folders: true,
                        tags: vec![],
                        formats: vec![],
                        favorite_only: false,
                        sort: "updated".into(),
                        offset: 0,
                        limit: 80,
                    },
                )
                .unwrap()
                .total,
                1
            );
        }
    }

    #[test]
    fn stores_and_replaces_a_model_cover_without_touching_its_original() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("db.sqlite")).unwrap();
        let source = dir.path().join("mesh.obj");
        fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let report = import(
            &mut db,
            dir.path(),
            None,
            MediaImportRequest {
                kind: MediaKind::Model,
                paths: vec![source.to_string_lossy().into_owned()],
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
        )
        .unwrap();
        let id = report.rows[0].entry_id.as_ref().unwrap();
        assert_eq!(pending_model_thumbnails(&db).unwrap().len(), 1);
        let picture = dir.path().join("cover.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([20, 70, 120, 255]))
            .save(&picture)
            .unwrap();
        let bytes = fs::read(picture).unwrap();
        let first = save_model_thumbnail(&mut db, dir.path(), id, &bytes).unwrap();
        assert_eq!(
            get(&db, id).unwrap().entry.thumbnail_file_id.as_deref(),
            Some(first.as_str())
        );
        assert!(pending_model_thumbnails(&db).unwrap().is_empty());
        let second = save_model_thumbnail(&mut db, dir.path(), id, &bytes).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            get(&db, id).unwrap().entry.thumbnail_file_id.as_deref(),
            Some(second.as_str())
        );
        assert_eq!(
            get(&db, id)
                .unwrap()
                .files
                .iter()
                .filter(|file| file.role == "thumbnail")
                .count(),
            1
        );
        assert!(save_model_thumbnail(&mut db, dir.path(), id, b"not an image").is_err());
    }

    #[test]
    fn checksum_runs_on_a_small_callback_stack() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("payload.bin");
        fs::write(&source, vec![0x5a; 2 * 1024 * 1024]).unwrap();
        let worker = std::thread::Builder::new()
            .name("webview-callback-sized-stack".into())
            .stack_size(256 * 1024)
            .spawn(move || digest(&source))
            .unwrap();
        let checksum = worker.join().unwrap().unwrap();
        assert_eq!(checksum.len(), 64);
    }
}
