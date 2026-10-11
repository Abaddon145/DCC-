use super::models::*;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
pub fn analyze(
    file: &Path,
    kind: &str,
    resources: Option<&Path>,
    stop: &dyn Fn() -> bool,
) -> Result<AssetMetadataValues, String> {
    if stop() {
        return Err("元数据分析已停止".into());
    }
    if kind == "image" {
        let reader = image::ImageReader::open(file)
            .map_err(|e| format!("图片不可读：{e}"))?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let codec = reader.format().ok_or("图片格式无法识别")?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|e| format!("图片无法解析：{e}"))?;
        if width == 0 || height == 0 {
            return Err("图片尺寸无效".into());
        }
        if stop() {
            return Err("元数据分析已停止".into());
        }
        return Ok(AssetMetadataValues {
            width: Some(width as u64),
            height: Some(height as u64),
            codec: Some(format!("{codec:?}").to_ascii_lowercase()),
            ..Default::default()
        });
    }
    if !matches!(kind, "audio" | "video") {
        return Err("该类型沿用原预览流程，不进行音视频分析".into());
    }
    let tool = crate::runtime::tools::require(resources, "ffprobe")?;
    let info = crate::media_engine::probe::analyze(&tool, file, stop)?;
    Ok(AssetMetadataValues {
        width: if kind == "video" { info.width } else { None },
        height: if kind == "video" { info.height } else { None },
        duration: info.duration,
        codec: Some(info.codec),
        fps: if kind == "video" { info.fps } else { None },
    })
}
pub fn save(db: &Connection, file_id: &str, values: &AssetMetadataValues) -> Result<(), String> {
    if values.duration.is_some_and(|v| !v.is_finite() || v < 0.0)
        || values.fps.is_some_and(|v| !v.is_finite() || v <= 0.0)
        || values.width.is_some_and(|v| v == 0 || v > i64::MAX as u64)
        || values.height.is_some_and(|v| v == 0 || v > i64::MAX as u64)
        || values
            .codec
            .as_ref()
            .is_some_and(|c| c.trim().is_empty() || c.len() > 100)
    {
        return Err("元数据包含无效尺寸、时长、编码或帧率".into());
    }
    db.execute("INSERT INTO asset_metadata(file_id,width,height,duration,codec,fps,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(file_id) DO UPDATE SET width=excluded.width,height=excluded.height,duration=excluded.duration,codec=excluded.codec,fps=excluded.fps,updated_at=excluded.updated_at",rusqlite::params![file_id,values.width.map(|v|v as i64),values.height.map(|v|v as i64),values.duration,values.codec,values.fps,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    Ok(())
}

pub fn process_asset(
    db: &Connection,
    base: &Path,
    resources: Option<&Path>,
    asset_id: &str,
    force: bool,
    stop: &dyn Fn() -> bool,
) -> Result<serde_json::Value, String> {
    let detail = super::repository::get(db, base, asset_id, "zh-CN")?;
    if detail.asset.deleted {
        return Err("资产已在回收站，未处理".into());
    }
    let mut rows = Vec::new();
    for file in detail.files.iter().filter(|f| f.role == "original") {
        if stop() {
            break;
        }
        let result = (|| -> Result<String, String> {
            let kind = file_kind(db, &file.source_ref)?;
            if kind == "model" {
                return Ok("skipped".into());
            }
            let path = super::repository::file_path(db, base, &file.source_ref)?;
            if force || !detail.metadata.iter().any(|m| m.file_id == file.id) {
                let data = analyze(&path, &kind, resources, stop)?;
                save(db, &file.id, &data)?;
            }
            if force || !has_thumbnail(db, base, &file.source_ref)? {
                let thumbnail = crate::media_engine::thumbnail::generate(
                    base, resources, &file.id, &path, &kind, stop,
                )?;
                if let Err(error) = apply_thumbnail(db, &file.source_ref, &thumbnail, base) {
                    let _ = std::fs::remove_file(base.join(&thumbnail.relative_path));
                    return Err(error);
                }
            }
            Ok("processed".into())
        })();
        rows.push(match result {
            Ok(status) => serde_json::json!({"fileId":file.id,"status":status}),
            Err(message) => {
                serde_json::json!({"fileId":file.id,"status":"error","message":message})
            }
        });
    }
    Ok(serde_json::json!({"assetId":asset_id,"rows":rows,"cancelled":stop()}))
}

pub(crate) fn file_kind(db: &Connection, source: &SourceFileRef) -> Result<String, String> {
    let sql=match source.kind.as_str(){
        "image"=>"SELECT 'image' FROM images f JOIN assets a ON a.id=f.asset_id WHERE f.id=?1 AND a.deleted_at IS NULL",
        "assetMedia"=>"SELECT f.kind FROM asset_media f JOIN assets a ON a.id=f.asset_id WHERE f.id=?1 AND a.deleted_at IS NULL",
        "mediaFile"=>"SELECT e.kind FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE f.id=?1 AND e.deleted_at IS NULL",
        _=>return Err("文件引用无效".into()),
    };
    db.query_row(sql, [&source.id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("原件所属条目不存在或已在回收站".into())
}
fn has_thumbnail(db: &Connection, base: &Path, source: &SourceFileRef) -> Result<bool, String> {
    let sql=match source.kind.as_str(){
        "image"=>"SELECT thumbnail_rel_path FROM images WHERE id=?1",
        "assetMedia"=>"SELECT thumbnail_rel_path FROM asset_media WHERE id=?1",
        _=>"SELECT rel_path FROM media_files WHERE entry_id=(SELECT entry_id FROM media_files WHERE id=?1) AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END,sort_order LIMIT 1",
    };
    let path = db
        .query_row(sql, [&source.id], |r| r.get::<_, Option<String>>(0))
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();
    Ok(path
        .and_then(|p| crate::media::safe_relative(base, &p).ok())
        .is_some_and(|p| p.is_file()))
}
fn apply_thumbnail(
    db: &Connection,
    source: &SourceFileRef,
    thumb: &crate::media_engine::thumbnail::ManagedThumbnail,
    base: &Path,
) -> Result<(), String> {
    let old_path: Option<String> = match source.kind.as_str() {
        "image" | "assetMedia" => {
            let table = if source.kind == "image" {"images"} else {"asset_media"};
            db.query_row(&format!("SELECT thumbnail_rel_path FROM {table} WHERE id=?1"),[&source.id],|r|r.get(0)).optional().map_err(|e|e.to_string())?.flatten()
        }
        "mediaFile" => db.query_row("SELECT rel_path FROM media_files WHERE entry_id=(SELECT entry_id FROM media_files WHERE id=?1) AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END,sort_order LIMIT 1",[&source.id],|r|r.get(0)).optional().map_err(|e|e.to_string())?,
        _ => return Err("文件引用无效".into()),
    };
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let origin = match source.kind.as_str() {
        "image" | "assetMedia" => {
            let table = if source.kind == "image" {
                "images"
            } else {
                "asset_media"
            };
            let owner: String = tx
                .query_row(
                    &format!("SELECT asset_id FROM {table} WHERE id=?1"),
                    [&source.id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            tx.execute(
                &format!("UPDATE {table} SET thumbnail_rel_path=?1 WHERE id=?2"),
                params![thumb.relative_path, source.id],
            )
            .map_err(|e| e.to_string())?;
            CoreOrigin {
                kind: "legacy".into(),
                id: owner,
            }
        }
        "mediaFile" => {
            let owner: String = tx
                .query_row(
                    "SELECT entry_id FROM media_files WHERE id=?1",
                    [&source.id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let existing:Option<String>=tx.query_row("SELECT id FROM media_files WHERE entry_id=?1 AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END,sort_order LIMIT 1",[&owner],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            let size = std::fs::metadata(base.join(&thumb.relative_path))
                .map_err(|e| e.to_string())?
                .len() as i64;
            let filename = Path::new(&thumb.relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("缩略图文件名无效")?;
            let mime = if filename.ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            if let Some(id) = existing {
                tx.execute("UPDATE media_files SET rel_path=?1,original_name=?2,mime_type=?3,file_size=?4,width=?5,height=?6,checksum='' WHERE id=?7",params![thumb.relative_path,filename,mime,size,thumb.width,thumb.height,id]).map_err(|e|e.to_string())?;
            } else {
                tx.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,mime_type,file_size,width,height,sort_order,created_at) VALUES(?1,?2,'thumbnail',?3,?3,?4,?5,?6,?7,?8,100,?9)",params![uuid::Uuid::new_v4().to_string(),owner,filename,thumb.relative_path,mime,size,thumb.width,thumb.height,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
            }
            CoreOrigin {
                kind: "media".into(),
                id: owner,
            }
        }
        _ => return Err("文件引用无效".into()),
    };
    let id = super::repository::register_origin(&tx, &origin)?;
    super::repository::register_files(&tx, &id)?;
    tx.commit().map_err(|e| e.to_string())?;
    // Only retire our generated cache. Original legacy thumbnails remain owned by their old pipeline.
    if let Some(old) =
        old_path.filter(|p| p.starts_with(".dcc/cache/thumbnails/") && p != &thumb.relative_path)
    {
        let referenced:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM media_files WHERE rel_path=?1 UNION ALL SELECT 1 FROM images WHERE thumbnail_rel_path=?1 UNION ALL SELECT 1 FROM asset_media WHERE thumbnail_rel_path=?1)",[&old],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !referenced {
            if let (Ok(root), Ok(file)) = (
                base.canonicalize(),
                crate::media::safe_relative(base, &old)
                    .and_then(|p| p.canonicalize().map_err(|e| e.to_string())),
            ) {
                if file.starts_with(root.join(".dcc/cache/thumbnails")) {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
    }
    Ok(())
}
