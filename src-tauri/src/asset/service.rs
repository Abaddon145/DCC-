use super::{models::*, repository, scanner};
use rusqlite::{Connection, OptionalExtension};
use std::{collections::HashSet, path::Path};

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
    let imported = crate::media_library::import_controlled(
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
        &|_, _, _, _| {},
        stop,
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
