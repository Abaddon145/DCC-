use super::{models::*, repository, scanner};
use rusqlite::{Connection, OptionalExtension};
use std::{collections::HashSet, path::Path};

pub fn annotate_scan(
    db: &Connection,
    manifest: &Path,
    mut summary: ScanSummary,
    stop: &dyn Fn() -> bool,
) -> Result<ScanSummary, String> {
    use std::io::{BufRead, BufReader, BufWriter, Read, Write};
    let parent = manifest.parent().ok_or("扫描清单目录无效")?;
    let stage = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(stage.as_file());
    let mut header = String::new();
    Read::by_ref(&mut BufReader::new(
        std::fs::File::open(manifest).map_err(|e| e.to_string())?,
    ))
    .take(128 * 1024 + 1)
    .read_line(&mut header)
    .map_err(|e| e.to_string())?;
    if header.len() > 128 * 1024 {
        return Err("扫描清单头超出大小限制".into());
    }
    writer
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut counts = ScanSummary {
        cancelled: summary.cancelled,
        ..Default::default()
    };
    // ponytail: one streaming manifest pass, indexed checksum lookups; batch queries only if measured scan latency warrants them.
    let result = scanner::visit_manifest(manifest, &mut |_, mut row| {
        if stop() {
            return Err("扫描核对已停止".into());
        }
        mark_scan_duplicate(db, &mut row)?;
        match row.status.as_str() {
            "ready" => counts.importable += 1,
            "duplicate" => counts.duplicate += 1,
            "unsupported" => counts.unsupported += 1,
            _ => counts.failed += 1,
        }
        counts.total += 1;
        serde_json::to_writer(&mut writer, &row).map_err(|e| e.to_string())?;
        writeln!(writer).map_err(|e| e.to_string())
    });
    if stop() {
        summary.cancelled = true;
        return Ok(summary);
    }
    result?;
    writer.flush().map_err(|e| e.to_string())?;
    drop(writer);
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage.persist(manifest).map_err(|e| e.to_string())?;
    Ok(counts)
}

pub fn mark_scan_duplicate(db: &Connection, row: &mut ScanRow) -> Result<(), String> {
    if row.hash.is_empty() || !matches!(row.status.as_str(), "ready" | "duplicate") {
        return Ok(());
    }
    let duplicate:Option<(String,bool)>=db.query_row("SELECT c.id,e.deleted_at IS NOT NULL FROM media_files f JOIN media_entries e ON e.primary_file_id=f.id JOIN asset_core c ON c.media_entry_id=e.id WHERE f.checksum=?1",[&row.hash],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    if let Some((id, deleted)) = duplicate {
        row.status = "duplicate".into();
        row.duplicate_asset_id = Some(id);
        row.message = if deleted {
            "相同文件已在回收站，请先恢复"
        } else {
            "当前库已有相同文件，可打开或建立项目引用"
        }
        .into();
    }
    Ok(())
}

pub fn create_version(
    db: &Connection,
    base: &Path,
    input: &CoreVersionInput,
) -> Result<AssetVersion, String> {
    active_origin(db, &input.asset_id)?;
    let label = input.version.trim();
    if label.is_empty() || label.chars().count() > 80 || input.note.chars().count() > 10_000 {
        return Err("版本标签须为 1–80 字符，说明最多 10000 字符".into());
    }
    let file = repository::get(db, base, &input.asset_id, "zh-CN")?
        .files
        .into_iter()
        .find(|f| f.id == input.file_id && f.role == "original")
        .ok_or("请先附加属于当前资产的托管原件")?;
    super::metadata::file_kind(db, &file.source_ref)?;
    repository::file_path(db, base, &file.source_ref)?;
    let version = AssetVersion {
        id: uuid::Uuid::new_v4().to_string(),
        asset_id: input.asset_id.clone(),
        file_id: input.file_id.clone(),
        version: label.into(),
        note: input.note.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,note,created_at) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![version.id,version.asset_id,version.file_id,version.version,version.note,version.created_at]).map_err(|e|format!("版本保存失败（标签可能已存在）：{e}"))?;
    Ok(version)
}
pub fn delete_version(db: &Connection, id: &str) -> Result<(), String> {
    let core: String = db
        .query_row(
            "SELECT asset_id FROM asset_versions WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("版本不存在或不属于当前素材库")?;
    active_origin(db, &core)?;
    db.execute("DELETE FROM asset_versions WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn attach_file(
    db: &Connection,
    base: &Path,
    core_id: &str,
    source: &SourceFileRef,
) -> Result<AssetFile, String> {
    active_origin(db, core_id)?;
    if source.role != "original" {
        return Err("版本只能附加原件".into());
    }
    super::metadata::file_kind(db, source)?;
    repository::file_path(db, base, source)?;
    let (name,ext,size,hash):(String,String,i64,String)=db.query_row("SELECT filename,extension,size,hash FROM asset_files WHERE source_kind=?1 AND source_id=?2 AND role='original' LIMIT 1",rusqlite::params![source.kind,source.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?.ok_or("文件未登记在当前素材库")?;
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT OR IGNORE INTO asset_files(id,asset_id,source_kind,source_id,role,filename,extension,size,hash,created_at) VALUES(?1,?2,?3,?4,'original',?5,?6,?7,?8,?9)",rusqlite::params![uuid::Uuid::new_v4().to_string(),core_id,source.kind,source.id,name,ext,size,hash,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    tx.execute("INSERT OR IGNORE INTO asset_metadata(file_id,width,height,duration,codec,fps,updated_at) SELECT f.id,m.width,m.height,m.duration,m.codec,m.fps,m.updated_at FROM asset_files f JOIN asset_files original ON original.source_kind=f.source_kind AND original.source_id=f.source_id AND original.role=f.role JOIN asset_metadata m ON m.file_id=original.id WHERE f.asset_id=?1 AND f.source_kind=?2 AND f.source_id=?3 AND f.role='original'",rusqlite::params![core_id,source.kind,source.id]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    repository::get(db, base, core_id, "zh-CN")?
        .files
        .into_iter()
        .find(|f| f.source_ref == *source)
        .ok_or("文件附加失败".into())
}
pub fn add_source(db: &Connection, input: &CoreSourceInput) -> Result<AssetSource, String> {
    let origin = active_origin(db, &input.asset_id)?;
    if !matches!(
        input.source_type.as_str(),
        "local" | "fab" | "youtube" | "pinterest" | "web" | "other"
    ) || input.author.chars().count() > 300
        || input.url.len() > 8192
    {
        return Err("来源类型、作者或链接长度无效".into());
    }
    let raw = input.url.trim();
    let url = if input.source_type == "local" && raw.is_empty() {
        String::new()
    } else {
        let url = url::Url::parse(raw).map_err(|_| "来源链接无效")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("仅允许不包含用户名密码的 HTTP(S) 来源".into());
        }
        url.to_string()
    };
    let sources = repository::sources(db, &input.asset_id, &origin)?;
    if let Some(old) = sources.into_iter().find(|s| {
        s.source_type == input.source_type && s.url == url && s.author == input.author.trim()
    }) {
        return Ok(old);
    }
    let result = AssetSource {
        id: uuid::Uuid::new_v4().to_string(),
        asset_id: input.asset_id.clone(),
        source_type: input.source_type.clone(),
        url,
        author: input.author.trim().into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        editable: true,
    };
    db.execute("INSERT INTO asset_sources(id,asset_id,source_type,url,author,created_at) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![result.id,result.asset_id,result.source_type,result.url,result.author,result.created_at]).map_err(|e|e.to_string())?;
    Ok(result)
}
pub fn remove_source(db: &Connection, core_id: &str, id: &str) -> Result<(), String> {
    active_origin(db, core_id)?;
    if db
        .execute(
            "DELETE FROM asset_sources WHERE id=?1 AND asset_id=?2",
            rusqlite::params![id, core_id],
        )
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("此来源属于旧详情或其他资产，未删除".into());
    }
    Ok(())
}

pub fn active_origin(db: &Connection, core_id: &str) -> Result<CoreOrigin, String> {
    let origin = repository::origin(db, core_id)?;
    let table = match origin.kind.as_str() {
        "legacy" => "assets",
        "media" => "media_entries",
        "entity" => "asset_entities",
        _ => return Err("未知资产来源".into()),
    };
    let active: bool = db
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1 AND deleted_at IS NULL)"),
            [&origin.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !active {
        return Err("资产已在回收站，不能修改".into());
    }
    Ok(origin)
}
pub fn set_tags(
    db: &mut Connection,
    core_id: &str,
    locale: &str,
    tags: &[String],
) -> Result<(), String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    set_tags_in_tx(&tx, core_id, locale, tags)?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn set_tags_in_tx(
    tx: &Connection,
    core_id: &str,
    locale: &str,
    tags: &[String],
) -> Result<(), String> {
    if !matches!(locale, "zh-CN" | "en")
        || tags.len() > 200
        || tags.iter().any(|t| t.chars().count() > 100)
    {
        return Err("标签语言、数量或长度无效".into());
    }
    let origin = active_origin(tx, core_id)?;
    if origin.kind == "entity" {
        tx.execute("DELETE FROM asset_entity_tags WHERE entity_id=?1 AND tag_id IN (SELECT id FROM localized_tags WHERE locale=?2)",rusqlite::params![origin.id,locale]).map_err(|e|e.to_string())?;
        for tag in tags.iter().map(|v| v.trim()).filter(|v| !v.is_empty()) {
            tx.execute("INSERT OR IGNORE INTO localized_tags(id,locale,name,normalized_name) VALUES(?1,?2,?3,?4)",rusqlite::params![uuid::Uuid::new_v4().to_string(),locale,tag,tag.to_lowercase()]).map_err(|e|e.to_string())?;
            tx.execute("INSERT OR IGNORE INTO asset_entity_tags(entity_id,tag_id) SELECT ?1,id FROM localized_tags WHERE locale=?2 AND normalized_name=?3",rusqlite::params![origin.id,locale,tag.to_lowercase()]).map_err(|e|e.to_string())?;
        }
        tx.execute(
            "UPDATE asset_entities SET updated_at=?1 WHERE id=?2",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), origin.id],
        )
        .map_err(|e| e.to_string())?;
    } else if origin.kind == "media" {
        crate::media_library::upsert_tags(&tx, &origin.id, tags)?;
        crate::media_library::reindex(&tx, &origin.id)?;
        tx.execute(
            "UPDATE media_entries SET updated_at=?1 WHERE id=?2",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), origin.id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        tx.execute("DELETE FROM asset_localized_tags WHERE asset_id=?1 AND tag_id IN (SELECT id FROM localized_tags WHERE locale=?2)",rusqlite::params![origin.id,locale]).map_err(|e|e.to_string())?;
        for tag in tags {
            crate::db::insert_localized_tag(&tx, &origin.id, locale, tag)?;
        }
        if locale == "zh-CN" {
            tx.execute("DELETE FROM asset_tags WHERE asset_id=?1", [&origin.id])
                .map_err(|e| e.to_string())?;
            for tag in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                tx.execute(
                    "INSERT OR IGNORE INTO tags(id,name,normalized_name) VALUES(?1,?2,?3)",
                    rusqlite::params![uuid::Uuid::new_v4().to_string(), tag, tag.to_lowercase()],
                )
                .map_err(|e| e.to_string())?;
                tx.execute("INSERT OR IGNORE INTO asset_tags(asset_id,tag_id) SELECT ?1,id FROM tags WHERE normalized_name=?2",rusqlite::params![origin.id,tag.to_lowercase()]).map_err(|e|e.to_string())?;
            }
        }
        crate::db::reindex_asset(&tx, &origin.id)?;
        tx.execute(
            "UPDATE assets SET updated_at=?1 WHERE id=?2",
            rusqlite::params![chrono::Utc::now().to_rfc3339(), origin.id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn set_tag_color(db: &Connection, tag_id: &str, color: &str) -> Result<(), String> {
    if color.len() != 7
        || !color.starts_with('#')
        || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err("标签颜色必须是 #RRGGBB".into());
    }
    let count=db.execute("UPDATE localized_tags SET color=?1,created_at=CASE WHEN created_at='' THEN ?2 ELSE created_at END WHERE id=?3",rusqlite::params![color,chrono::Utc::now().to_rfc3339(),tag_id]).map_err(|e|e.to_string())?;
    if count == 0 {
        return Err("标签不存在或不属于当前素材库".into());
    }
    Ok(())
}
pub fn link_project(
    db: &mut Connection,
    core_id: &str,
    project_id: &str,
    link: bool,
) -> Result<(), String> {
    let origin = active_origin(db, core_id)?;
    let active: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status<>'archived')",
            [project_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !active {
        return Err("目标项目不存在或已归档".into());
    }
    if origin.kind == "entity" {
        if link {
            db.execute(
                "INSERT OR IGNORE INTO asset_entity_projects(entity_id,project_id) VALUES(?1,?2)",
                rusqlite::params![origin.id, project_id],
            )
            .map_err(|e| e.to_string())?;
        } else {
            db.execute(
                "DELETE FROM asset_entity_projects WHERE entity_id=?1 AND project_id=?2",
                rusqlite::params![origin.id, project_id],
            )
            .map_err(|e| e.to_string())?;
        }
    } else if origin.kind == "legacy" {
        if link {
            crate::projects::add_project_assets(db, project_id, vec![origin.id])?;
        } else {
            crate::projects::remove_project_assets(db, project_id, vec![origin.id])?;
        }
    } else {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        crate::media_library::link_projects(&tx, vec![origin.id], vec![project_id.into()], link)?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn validate_import(request: &CoreImportRequest) -> Result<(), String> {
    if request.paths.len() > 500 {
        return Err("单次文件选择最多 500 项".into());
    }
    if request.scan_task_id.is_some() && !request.paths.is_empty() {
        return Err("扫描导入与直接文件选择不能混用".into());
    }
    if request.scan_task_id.is_none() && !request.scan_row_ids.is_empty() {
        return Err("扫描条目缺少任务 ID".into());
    }
    if let Some(id) = &request.scan_task_id {
        uuid::Uuid::parse_str(id).map_err(|_| "扫描任务 ID 无效")?;
        if request.scan_row_ids.is_empty() || request.scan_row_ids.len() > 100_000 {
            return Err("请选择扫描条目（最多 100000 项）".into());
        }
        for id in &request.scan_row_ids {
            uuid::Uuid::parse_str(id).map_err(|_| "扫描条目 ID 无效")?;
        }
    }
    if request.tags.len() > 200 || request.tags.iter().any(|t| t.chars().count() > 100) {
        return Err("标签数量或长度超出限制".into());
    }
    Ok(())
}
fn import_one(
    db: &mut Connection,
    base: &Path,
    resources: Option<&Path>,
    request: &CoreImportRequest,
    path: &str,
    expected_hash: Option<&str>,
    stop: &dyn Fn() -> bool,
) -> Result<CoreImportRow, String> {
    let source = scanner::validate_source(Path::new(path), base)?;
    let kind = scanner::kind(&source).ok_or("不支持此扫描文件类型，请使用原媒体库专属入口")?;
    let (size, hash) = crate::file_digest::digest_file_controlled(&source, stop)?;
    if expected_hash.is_some_and(|old| old != hash) {
        return Err("文件在扫描后发生变化，请重新扫描".into());
    }
    let duplicate:Option<(String,bool)>=db.query_row("SELECT e.id,e.deleted_at IS NOT NULL FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE f.checksum=?1",[&hash],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    if let Some((entry_id, deleted)) = duplicate {
        let core_id = repository::register_origin(
            db,
            &CoreOrigin {
                kind: "media".into(),
                id: entry_id.clone(),
            },
        )?;
        repository::register_files(db, &core_id)?;
        if !deleted {
            let file_id: String = db.query_row("SELECT id FROM asset_files WHERE asset_id=?1 AND role='original' AND source_kind='mediaFile' LIMIT 1", [&core_id], |r| r.get(0)).map_err(|e| e.to_string())?;
            let missing: bool = db
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM asset_metadata WHERE file_id=?1)",
                    [&file_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if missing {
                let owned = repository::file_path(
                    db,
                    base,
                    &SourceFileRef {
                        kind: "mediaFile".into(),
                        id: db
                            .query_row(
                                "SELECT source_id FROM asset_files WHERE id=?1",
                                [&file_id],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?,
                        role: "original".into(),
                    },
                )?;
                let data = super::metadata::analyze(&owned, kind, resources, stop)?;
                super::metadata::save(db, &file_id, &data)?;
            }
            if let Some(project) = &request.project_id {
                crate::media_library::link_projects(
                    db,
                    vec![entry_id],
                    vec![project.clone()],
                    true,
                )?;
            }
        }
        return Ok(CoreImportRow {
            scan_row_id: None,
            path: path.into(),
            status: "duplicate".into(),
            message: if deleted {
                "相同文件已在回收站，请先恢复"
            } else if request.project_id.is_some() {
                "相同文件已存在，已建立项目引用"
            } else {
                "相同文件已存在，可打开已有条目"
            }
            .into(),
            asset_id: None,
            duplicate_asset_id: Some(core_id),
        });
    }
    if stop() {
        return Err("导入已停止".into());
    }
    #[cfg(windows)]
    crate::runtime::tools::ensure_space(
        base,
        size.saturating_mul(3).saturating_add(16 * 1024 * 1024),
    )?;
    let analyzed = super::metadata::analyze(&source, kind, resources, stop)?;
    let media_kind = match kind {
        "image" => crate::models::MediaKind::Image,
        "video" => crate::models::MediaKind::Video,
        _ => crate::models::MediaKind::Audio,
    };
    let imported = crate::media_library::import_controlled_verified(
        db,
        base,
        resources,
        crate::models::MediaImportRequest {
            kind: media_kind,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: request.tags.clone(),
            favorite: false,
        },
        stop,
        &hash,
    )?;
    let row = imported.rows.first().ok_or("导入停止，未生成条目")?;
    let entry_id = row
        .entry_id
        .as_ref()
        .or(row.duplicate_entry_id.as_ref())
        .ok_or_else(|| row.message.clone())?;
    let core_id = repository::register_origin(
        db,
        &CoreOrigin {
            kind: "media".into(),
            id: entry_id.clone(),
        },
    )?;
    repository::register_files(db, &core_id)?;
    let file_id:String=db.query_row("SELECT id FROM asset_files WHERE asset_id=?1 AND role='original' AND source_kind='mediaFile' LIMIT 1",[&core_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    super::metadata::save(db, &file_id, &analyzed)?;
    if let Some(project) = &request.project_id {
        crate::media_library::link_projects(
            db,
            vec![entry_id.clone()],
            vec![project.clone()],
            true,
        )?;
    }
    Ok(CoreImportRow {
        scan_row_id: None,
        path: path.into(),
        status: row.status.clone(),
        message: row.message.clone(),
        asset_id: row.entry_id.as_ref().map(|_| core_id.clone()),
        duplicate_asset_id: row.duplicate_entry_id.as_ref().map(|_| core_id),
    })
}
pub fn import_files(
    db: &mut Connection,
    base: &Path,
    resources: Option<&Path>,
    request: &CoreImportRequest,
    progress: &dyn Fn(usize, usize),
    stop: &dyn Fn() -> bool,
) -> Result<CoreImportReport, String> {
    validate_import(request)?;
    if let Some(project) = &request.project_id {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status<>'archived')",
                [project],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("目标项目不存在或已归档，未导入".into());
        }
    }
    let mut report = CoreImportReport::default();
    let total = if request.scan_task_id.is_some() {
        request.scan_row_ids.len()
    } else {
        request.paths.len()
    };
    let mut append = |path: &str, hash: Option<&str>, scan_row_id: Option<&str>| {
        if stop() {
            report.cancelled = true;
            return;
        }
        let mut row = match import_one(db, base, resources, request, path, hash, stop) {
            Ok(row) => row,
            Err(message) => CoreImportRow {
                scan_row_id: None,
                path: path.into(),
                status: "error".into(),
                message,
                asset_id: None,
                duplicate_asset_id: None,
            },
        };
        row.scan_row_id = scan_row_id.map(str::to_string);
        match row.status.as_str() {
            "imported" => report.imported += 1,
            "duplicate" => report.skipped += 1,
            _ => report.failed += 1,
        }
        report.rows.push(row);
        progress(report.rows.len(), total);
    };
    if let Some(task_id) = &request.scan_task_id {
        let manifest = scanner::manifest_path(base, task_id)?;
        let selected: HashSet<&str> = request.scan_row_ids.iter().map(String::as_str).collect();
        let mut found = HashSet::new();
        scanner::visit_manifest(&manifest, &mut |_, row| {
            if selected.contains(row.id.as_str()) {
                found.insert(row.id);
            }
            Ok(())
        })?;
        if found.len() != selected.len() {
            return Err("扫描条目不属于此任务或清单已失效".into());
        }
        let visited = scanner::visit_manifest(&manifest, &mut |_, row| {
            if stop() {
                return Err("扫描导入已停止".into());
            }
            if selected.contains(row.id.as_str()) {
                append(&row.path, Some(&row.hash), Some(&row.id));
            }
            Ok(())
        });
        if let Err(error) = visited {
            if !stop() {
                return Err(error);
            }
        }
    } else {
        for path in &request.paths {
            if stop() {
                break;
            }
            append(path, None, None);
        }
    }
    report.cancelled |= stop();
    Ok(report)
}
