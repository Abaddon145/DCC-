use crate::{images, media, models::*};
use chrono::Utc;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

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
    let mut file = fs::File::open(path).map_err(|e| format!("读取文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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
fn upsert_tags(connection: &Connection, entry_id: &str, tags: &[String]) -> Result<(), String> {
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
fn reindex(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM media_search WHERE entry_id=?1", [id])
        .map_err(|e| e.to_string())?;
    connection.execute("INSERT INTO media_search(entry_id,text) SELECT e.id,e.name||' '||e.description||' '||e.author||' '||e.license||' '||COALESCE(f.name,'')||' '||COALESCE(group_concat(t.name,' '),'') FROM media_entries e LEFT JOIN media_folders f ON f.id=e.folder_id LEFT JOIN media_entry_tags et ON et.entry_id=e.id LEFT JOIN localized_tags t ON t.id=et.tag_id WHERE e.id=?1 GROUP BY e.id",[id]).map_err(|e|e.to_string())?;
    Ok(())
}
fn file_row(row: &Row<'_>) -> rusqlite::Result<MediaFile> {
    Ok(MediaFile {
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

pub fn search(
    connection: &Connection,
    request: &MediaSearchRequest,
) -> Result<Page<MediaEntry>, String> {
    let mut conditions = vec!["e.deleted_at IS NULL".to_string(), "e.kind=?".to_string()];
    let mut values = vec![Value::Text(request.kind.as_str().into())];
    if !request.query.trim().is_empty() {
        conditions
            .push("e.id IN (SELECT entry_id FROM media_search WHERE media_search MATCH ?)".into());
        values.push(Value::Text(format!(
            "\"{}\"",
            request.query.trim().replace('"', "\"")
        )))
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
    let where_sql = conditions.join(" AND ");
    let total=connection.query_row(&format!("SELECT count(*) FROM media_entries e JOIN media_files m ON m.id=e.primary_file_id WHERE {where_sql}"),params_from_iter(values.clone()),|r|r.get(0)).map_err(|e|e.to_string())?;
    let order = match request.sort.as_str() {
        "name" => "e.name COLLATE NOCASE",
        "created" => "e.created_at DESC",
        "favorite" => "e.favorite DESC,e.updated_at DESC",
        _ => "e.updated_at DESC",
    };
    let mut page_values = values;
    page_values.push(Value::Integer(request.limit.clamp(1, 500)));
    page_values.push(Value::Integer(request.offset.max(0)));
    let sql =
        format!("{ENTRY_SELECT} WHERE {where_sql} GROUP BY e.id ORDER BY {order} LIMIT ? OFFSET ?");
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
pub fn get(connection: &Connection, id: &str) -> Result<MediaEntryDetail, String> {
    let sql = format!("{ENTRY_SELECT} WHERE e.id=?1 AND e.deleted_at IS NULL GROUP BY e.id");
    let entry = connection
        .query_row(&sql, [id], entry_row)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("媒体条目不存在")?;
    let mut stmt=connection.prepare("SELECT id,entry_id,role,logical_path,original_name,mime_type,file_size,checksum,width,height,duration_ms FROM media_files WHERE entry_id=?1 ORDER BY sort_order,id").map_err(|e|e.to_string())?;
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
    let mut stmt=connection.prepare("SELECT f.id,f.kind,f.parent_id,f.name,f.sort_order,(SELECT count(*) FROM media_entries e WHERE e.folder_id=f.id AND e.deleted_at IS NULL) FROM media_folders f WHERE f.kind=?1 AND f.deleted_at IS NULL ORDER BY f.parent_id,f.sort_order,f.name COLLATE NOCASE").map_err(|e|e.to_string())?;
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
    let media_count:i64=connection.query_row("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) SELECT count(*) FROM media_entries WHERE deleted_at IS NULL AND folder_id IN (SELECT id FROM tree)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
    let batch = Uuid::new_v4().to_string();
    let timestamp = now();
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO deletion_batches(id,kind,label,media_count,media_folder_count,created_at) VALUES(?1,'mediaFolder',?2,?3,?4,?5)",params![batch,format!("{}库文件夹：{name}",kind),media_count,folder_count,timestamp]).map_err(|e|e.to_string())?;
    tx.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) UPDATE media_entries SET deleted_at=?2,delete_batch_id=?3 WHERE deleted_at IS NULL AND folder_id IN (SELECT id FROM tree)",params![id,timestamp,batch]).map_err(|e|e.to_string())?;
    tx.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM media_folders f JOIN tree t ON f.parent_id=t.id WHERE f.deleted_at IS NULL) UPDATE media_folders SET deleted_at=?2,delete_batch_id=?3 WHERE id IN (SELECT id FROM tree)",params![id,timestamp,batch]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

pub fn import(
    connection: &mut Connection,
    base_dir: &Path,
    resource_dir: Option<&Path>,
    request: MediaImportRequest,
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
    for raw in request.paths.clone() {
        let source = PathBuf::from(&raw);
        let mut staged_root: Option<PathBuf> = None;
        let result = (|| -> Result<(String, Option<String>), String> {
            if !source.is_file() {
                return Err("文件不存在".into());
            }
            let (ext, _mime) = validate_kind_path(&request.kind, &source)?;
            let checksum = digest(&source)?;
            if let Some(existing)=connection.query_row("SELECT e.id FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE f.checksum=?1",[&checksum],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?{return Ok((existing,Some("duplicate".into())))}
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
                insert_entry(
                    connection, &id, &request, &name, &main_id, &timestamp, files,
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
                insert_entry(
                    connection, &id, &request, &name, &main_id, &timestamp, files,
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
    reindex(&tx, id)?;
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
    let changed=connection.execute("UPDATE media_entries SET name=?1,description=?2,author=?3,source_url=?4,license=?5,updated_at=?6 WHERE id=?7 AND deleted_at IS NULL",params![name.trim(),description.trim(),author.trim(),source_url.trim(),license.trim(),now(),id]).map_err(|e|e.to_string())?;
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
    for id in ids {
        count+=tx.execute("UPDATE media_entries SET deleted_at=?1,delete_batch_id=?2 WHERE id=?3 AND deleted_at IS NULL",params![timestamp,batch,id]).map_err(|e|e.to_string())?;
    }
    if count > 0 {
        tx.execute("INSERT INTO deletion_batches(id,kind,label,media_count,created_at) VALUES(?1,'media',?2,?3,?4)",params![batch,format!("{count} 个媒体条目"),count as i64,timestamp]).map_err(|e|e.to_string())?;
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
    let (rel,mime):(String,String)=connection.query_row("SELECT f.rel_path,f.mime_type FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE f.id=?1 AND e.deleted_at IS NULL",[file_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("媒体文件不存在")?;
    let path = media::safe_relative(base_dir, &rel)?;
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
            "SELECT rel_path,mime_type FROM media_files WHERE entry_id=?1 AND logical_path=?2",
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
        fs::write(&model, "v 0 0 0\\nv 1 0 0\\nv 0 1 0\\nf 1 2 3\\n").unwrap();
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
}
