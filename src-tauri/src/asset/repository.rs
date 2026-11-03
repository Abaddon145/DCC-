use super::models::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub fn register_origin(db: &Connection, origin: &CoreOrigin) -> Result<String, String> {
    let column = match origin.kind.as_str() {
        "legacy" => "legacy_asset_id",
        "media" => "media_entry_id",
        "entity" => "entity_id",
        _ => return Err("未知资产来源类别".into()),
    };
    if let Some(id) = db
        .query_row(
            &format!("SELECT id FROM asset_core WHERE {column}=?1"),
            [&origin.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    db.execute(
        &format!("INSERT INTO asset_core(id,{column},created_at) VALUES(?1,?2,?3)"),
        params![id, origin.id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn origin(db: &Connection, id: &str) -> Result<CoreOrigin, String> {
    let origin = origin_unscoped(db, id)?;
    if origin.kind == "media" {
        let allowed: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials')", [&origin.id], |r| r.get(0)).map_err(|e| e.to_string())?;
        if !allowed {
            return Err("资产不存在或不属于当前素材库".into());
        }
    }
    Ok(origin)
}

fn origin_unscoped(db: &Connection, id: &str) -> Result<CoreOrigin, String> {
    db.query_row(
        "SELECT legacy_asset_id,media_entry_id,entity_id FROM asset_core WHERE id=?1",
        [id],
        |r| {
            let legacy: Option<String> = r.get(0)?;
            Ok(if let Some(id) = legacy {
                CoreOrigin {
                    kind: "legacy".into(),
                    id,
                }
            } else if let Some(id) = r.get::<_, Option<String>>(1)? {
                CoreOrigin {
                    kind: "media".into(),
                    id,
                }
            } else {
                CoreOrigin {
                    kind: "entity".into(),
                    id: r.get(2)?,
                }
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "资产不存在或不属于当前素材库".into())
}

pub fn register_files(db: &Connection, core_id: &str) -> Result<(), String> {
    let owner = origin_unscoped(db, core_id)?;
    if owner.kind == "entity" {
        return Ok(());
    }
    let sql = if owner.kind == "legacy" {
        "SELECT 'image',id,'original',original_name,0,'',created_at FROM images WHERE asset_id=?1
         UNION ALL SELECT 'image',id,'thumbnail',thumbnail_rel_path,0,'',created_at FROM images WHERE asset_id=?1
         UNION ALL SELECT 'assetMedia',id,'original',original_name,file_size,'',created_at FROM asset_media WHERE asset_id=?1
         UNION ALL SELECT 'assetMedia',id,'proxy',proxy_rel_path,0,'',created_at FROM asset_media WHERE asset_id=?1 AND proxy_rel_path IS NOT NULL
         UNION ALL SELECT 'assetMedia',id,'thumbnail',thumbnail_rel_path,0,'',created_at FROM asset_media WHERE asset_id=?1 AND thumbnail_rel_path IS NOT NULL"
    } else {
        "SELECT 'mediaFile',id,CASE role WHEN 'main' THEN 'original' ELSE role END,original_name,file_size,checksum,created_at FROM media_files WHERE entry_id=?1"
    };
    let mut stmt = db.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([&owner.id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (kind, id, role, name, size, hash, created) = row.map_err(|e| e.to_string())?;
        let filename = Path::new(&name)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(&name);
        let ext = Path::new(filename)
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        db.execute("INSERT INTO asset_files(id,asset_id,source_kind,source_id,role,filename,extension,size,hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(asset_id,source_kind,source_id,role) DO UPDATE SET filename=excluded.filename,extension=excluded.extension,size=excluded.size,hash=excluded.hash", params![Uuid::new_v4().to_string(),core_id,kind,id,role,filename,ext,size.max(0),hash,created]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Resolve only database-owned files, never a path supplied by the frontend.
pub fn file_path(
    db: &Connection,
    base: &Path,
    source: &SourceFileRef,
) -> Result<crate::media_access::CheckedMediaFile, String> {
    if source.kind == "mediaFile" {
        let expected = match source.role.as_str() {
            "original" => "main",
            "dependency" => "dependency",
            "proxy" => "proxy",
            "thumbnail" => "thumbnail",
            "waveform" => "waveform",
            _ => return Err("文件用途无效".into()),
        };
        let valid: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM media_files WHERE id=?1 AND role=?2)",
                params![source.id, expected],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !valid {
            return Err("文件用途与登记不符".into());
        }
        return crate::media_access::resolve_media_file(
            db,
            base,
            &source.id,
            &crate::media_access::LocalMediaGrants::local()?,
        );
    }
    let sql = match (source.kind.as_str(), source.role.as_str()) {
        ("image", "original") => "SELECT original_rel_path FROM images WHERE id=?1",
        ("image", "thumbnail") => "SELECT thumbnail_rel_path FROM images WHERE id=?1",
        ("assetMedia", "original") => "SELECT original_rel_path FROM asset_media WHERE id=?1",
        ("assetMedia", "thumbnail") => "SELECT thumbnail_rel_path FROM asset_media WHERE id=?1",
        ("assetMedia", "proxy") => "SELECT proxy_rel_path FROM asset_media WHERE id=?1",
        ("mediaFile", "original") => "SELECT rel_path FROM media_files WHERE id=?1 AND role='main'",
        ("mediaFile", "dependency") => {
            "SELECT rel_path FROM media_files WHERE id=?1 AND role='dependency'"
        }
        ("mediaFile", "proxy") => "SELECT rel_path FROM media_files WHERE id=?1 AND role='proxy'",
        ("mediaFile", "thumbnail") => {
            "SELECT rel_path FROM media_files WHERE id=?1 AND role='thumbnail'"
        }
        ("mediaFile", "waveform") => {
            "SELECT rel_path FROM media_files WHERE id=?1 AND role='waveform'"
        }
        _ => return Err("文件引用类别或用途无效".into()),
    };
    let relative = db
        .query_row(sql, [&source.id], |r| r.get::<_, Option<String>>(0))
        .optional()
        .map_err(|e| e.to_string())?
        .flatten()
        .ok_or("托管文件不存在")?;
    let library = base.canonicalize().map_err(|e| e.to_string())?;
    let path = crate::media::safe_relative(&library, &relative)?
        .canonicalize()
        .map_err(|_| "托管文件缺失".to_string())?;
    if !path.starts_with(&library) || !path.is_file() {
        return Err("文件不在当前素材库内".into());
    }
    Ok(crate::media_access::CheckedMediaFile::managed(path))
}

pub fn protect_file_removal(
    db: &Connection,
    refs: &[SourceFileRef],
    deleting_core_ids: &[String],
) -> Result<(), String> {
    let mut stmt = db.prepare("SELECT v.asset_id,v.version FROM asset_versions v JOIN asset_files f ON f.id=v.file_id WHERE f.source_kind=?1 AND f.source_id=?2 AND f.role=?3").map_err(|e| e.to_string())?;
    for source in refs {
        let rows = stmt
            .query_map(params![source.kind, source.id, source.role], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (asset_id, version) = row.map_err(|e| e.to_string())?;
            if !deleting_core_ids.contains(&asset_id) {
                return Err(format!(
                    "原件仍被资产 {asset_id} 的版本“{version}”引用，请先解除版本引用"
                ));
            }
        }
    }
    Ok(())
}

pub fn prepare_trash_purge(db: &Connection, batch_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = db.prepare("SELECT c.id FROM asset_core c LEFT JOIN assets a ON a.id=c.legacy_asset_id LEFT JOIN media_entries m ON m.id=c.media_entry_id WHERE a.delete_batch_id=?1 OR m.delete_batch_id=?1").map_err(|e|e.to_string())?;
    let core_ids: Vec<String> = stmt
        .query_map([batch_id], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut refs_stmt = db.prepare("SELECT 'image',i.id FROM images i JOIN assets a ON a.id=i.asset_id WHERE a.delete_batch_id=?1 UNION ALL SELECT 'assetMedia',f.id FROM asset_media f JOIN assets a ON a.id=f.asset_id WHERE a.delete_batch_id=?1 UNION ALL SELECT 'mediaFile',f.id FROM media_files f JOIN media_entries m ON m.id=f.entry_id WHERE m.delete_batch_id=?1 AND f.role='main'").map_err(|e|e.to_string())?;
    let refs = refs_stmt
        .query_map([batch_id], |r| {
            Ok(SourceFileRef {
                kind: r.get(0)?,
                id: r.get(1)?,
                role: "original".into(),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    protect_file_removal(db, &refs, &core_ids)?;
    Ok(core_ids)
}

const VIEW: &str = "WITH unified AS (
 SELECT c.id,c.legacy_asset_id AS owner_id,'legacy' AS kind,'asset' AS asset_type,
 COALESCE(NULLIF(l.name,''),a.name) AS name,COALESCE(NULLIF(l.description,''),a.description) AS description,a.created_at,a.updated_at,a.deleted_at,a.source_url,a.author
 FROM asset_core c JOIN assets a ON a.id=c.legacy_asset_id LEFT JOIN asset_localizations l ON l.asset_id=a.id AND l.locale=?1
 UNION ALL SELECT c.id,c.media_entry_id,'media',m.kind,m.name,m.description,m.created_at,m.updated_at,m.deleted_at,m.source_url,m.author FROM asset_core c JOIN media_entries m ON m.id=c.media_entry_id WHERE EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=m.id AND w.workspace='materials')
 UNION ALL SELECT c.id,c.entity_id,'entity',e.kind,e.name,e.description,e.created_at,e.updated_at,e.deleted_at,'','' FROM asset_core c JOIN asset_entities e ON e.id=c.entity_id)
";

pub fn source_type(raw: &str) -> String {
    if raw.trim().is_empty() {
        return "local".into();
    }
    let host = url::Url::parse(raw)
        .ok()
        .filter(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.username().is_empty()
                && u.password().is_none()
        })
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    let domain = host.strip_prefix("www.").unwrap_or(&host);
    match domain {
        "fab.com" => "fab",
        "youtube.com" | "m.youtube.com" | "youtu.be" => "youtube",
        "pinterest.com" | "pinterest.cn" | "pin.it" => "pinterest",
        _ => "web",
    }
    .into()
}
pub fn sources(
    db: &Connection,
    core_id: &str,
    origin: &CoreOrigin,
) -> Result<Vec<AssetSource>, String> {
    let table = match origin.kind.as_str() {
        "legacy" => "assets",
        "media" => "media_entries",
        "entity" => "asset_entities",
        _ => return Err("未知资产来源".into()),
    };
    let (url, author, created): (String, String, String) = db
        .query_row(
            &if origin.kind == "entity" {
                format!("SELECT '','',created_at FROM {table} WHERE id=?1")
            } else {
                format!("SELECT source_url,author,created_at FROM {table} WHERE id=?1")
            },
            [&origin.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut items = vec![AssetSource {
        id: format!("origin:{}:{}", origin.kind, origin.id),
        asset_id: core_id.into(),
        source_type: source_type(&url),
        url,
        author,
        created_at: created,
        editable: false,
    }];
    let downloads:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='media_sources')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    if origin.kind == "media" && downloads {
        let mut stmt=db.prepare("SELECT id,url,author,created_at FROM media_sources WHERE media_id=?1 ORDER BY created_at,id").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map([&origin.id], |r| {
                Ok(AssetSource {
                    id: format!("download:{}", r.get::<_, String>(0)?),
                    asset_id: core_id.into(),
                    source_type: source_type(&r.get::<_, String>(1)?),
                    url: r.get(1)?,
                    author: r.get(2)?,
                    created_at: r.get(3)?,
                    editable: false,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            items.push(row.map_err(|e| e.to_string())?);
        }
    }
    let mut stmt=db.prepare("SELECT id,source_type,url,author,created_at FROM asset_sources WHERE asset_id=?1 ORDER BY created_at,id").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([core_id], |r| {
            Ok(AssetSource {
                id: r.get(0)?,
                asset_id: core_id.into(),
                source_type: r.get(1)?,
                url: r.get(2)?,
                author: r.get(3)?,
                created_at: r.get(4)?,
                editable: true,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        items.push(row.map_err(|e| e.to_string())?);
    }
    if items.iter().any(|s| !s.url.is_empty()) {
        items.retain(|s| !s.url.is_empty());
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|s| seen.insert((s.source_type.clone(), s.url.clone(), s.author.clone())));
    Ok(items)
}
fn tags(db: &Connection, owner: &CoreOrigin, locale: &str) -> Result<Vec<String>, String> {
    let sql = if owner.kind == "legacy" {
        "SELECT t.name FROM localized_tags t JOIN asset_localized_tags a ON a.tag_id=t.id WHERE a.asset_id=?1 AND t.locale=?2 ORDER BY t.name"
    } else if owner.kind == "entity" {
        "SELECT t.name FROM localized_tags t JOIN asset_entity_tags a ON a.tag_id=t.id WHERE a.entity_id=?1 AND t.locale=?2 ORDER BY t.name"
    } else {
        "SELECT t.name FROM localized_tags t JOIN media_entry_tags m ON m.tag_id=t.id WHERE m.entry_id=?1 AND ?2 IN ('zh-CN','en') ORDER BY t.name"
    };
    let mut stmt = db.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![owner.id, locale], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn thumbnail(db: &Connection, owner: &CoreOrigin) -> Result<Option<SourceFileRef>, String> {
    if owner.kind == "entity" {
        return Ok(None);
    }
    let sql = if owner.kind == "legacy" {
        "SELECT kind,id,role FROM (SELECT 'image' AS kind,id,'thumbnail' AS role,is_cover,sort_order,0 AS priority FROM images WHERE asset_id=?1 UNION ALL SELECT 'assetMedia',id,'thumbnail',is_cover,sort_order,1 FROM asset_media WHERE asset_id=?1 AND thumbnail_rel_path IS NOT NULL) ORDER BY is_cover DESC,priority,sort_order LIMIT 1"
    } else {
        "SELECT 'mediaFile',id,role FROM media_files WHERE entry_id=?1 AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END,sort_order LIMIT 1"
    };
    db.query_row(sql, [&owner.id], |r| {
        Ok(SourceFileRef {
            kind: r.get(0)?,
            id: r.get(1)?,
            role: r.get(2)?,
        })
    })
    .optional()
    .map_err(|e| e.to_string())
}

pub(crate) fn read_asset(db: &Connection, id: &str, locale: &str) -> Result<Asset, String> {
    let mut asset = db.query_row(&format!("{VIEW} SELECT id,owner_id,kind,asset_type,name,description,created_at,updated_at,deleted_at IS NOT NULL FROM unified WHERE id=?2"), params![locale,id], |r| Ok(Asset{id:r.get(0)?,origin:CoreOrigin{id:r.get(1)?,kind:r.get(2)?},asset_type:r.get(3)?,name:r.get(4)?,description:r.get(5)?,created_at:r.get(6)?,updated_at:r.get(7)?,deleted:r.get(8)?,tags:vec![],thumbnail:None})).optional().map_err(|e| e.to_string())?.ok_or("资产不存在或不属于当前素材库")?;
    asset.tags = tags(db, &asset.origin, locale)?;
    asset.thumbnail = thumbnail(db, &asset.origin)?;
    Ok(asset)
}

pub fn search(db: &Connection, request: &CoreSearchRequest) -> Result<Page<Asset>, String> {
    if !matches!(request.locale.as_str(), "zh-CN" | "en") {
        return Err("不支持的资产语言".into());
    }
    if let Some(kind) = &request.asset_type {
        if !matches!(
            kind.as_str(),
            "asset" | "image" | "audio" | "video" | "model" | "prompt" | "shot"
        ) {
            return Err("未知资产类型".into());
        }
    }
    if let Some(source) = &request.source {
        if !matches!(
            source.as_str(),
            "local" | "fab" | "youtube" | "pinterest" | "web" | "other"
        ) {
            return Err("未知资产来源筛选".into());
        }
    }
    if request.query.chars().count() > 1000 || request.offset > i64::MAX as usize {
        return Err("资产查询超出限制".into());
    }
    let limit = request.limit.clamp(1, 200);
    if let Some(id) = &request.project_id {
        uuid::Uuid::parse_str(id).map_err(|_| "项目标识无效")?;
    }
    // Reuse the detail classifier instead of maintaining a second URL/domain parser in SQL.
    db.create_scalar_function(
        "dcc_asset_source_type",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(source_type(&ctx.get::<String>(0)?)),
    )
    .map_err(|e| e.to_string())?;
    let downloads:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='media_sources')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let download_filter = if downloads {
        " OR EXISTS(SELECT 1 FROM media_sources d WHERE u.kind='media' AND d.media_id=u.owner_id AND dcc_asset_source_type(d.url)=?4)"
    } else {
        ""
    };
    let project_links = "((u.kind='legacy' AND EXISTS(SELECT 1 FROM project_assets l WHERE l.project_id=p.id AND l.asset_id=u.owner_id)) OR (u.kind='media' AND EXISTS(SELECT 1 FROM project_media_entries l WHERE l.project_id=p.id AND l.entry_id=u.owner_id)) OR (u.kind='entity' AND EXISTS(SELECT 1 FROM asset_entity_projects l WHERE l.project_id=p.id AND l.entity_id=u.owner_id)))";
    let projects = format!("EXISTS(SELECT 1 FROM projects p WHERE p.id=?5 AND {project_links})");
    let download_text = if downloads {
        " OR EXISTS(SELECT 1 FROM media_sources d WHERE u.kind='media' AND d.media_id=u.owner_id AND (instr(lower(d.url),?3)>0 OR instr(lower(d.author),?3)>0))"
    } else {
        ""
    };
    let filter = format!("FROM unified u WHERE u.deleted_at IS NULL AND (?2 IS NULL OR u.asset_type=?2)
      AND (?3='' OR instr(lower(u.name),?3)>0 OR instr(lower(u.description),?3)>0 OR instr(lower(u.source_url),?3)>0 OR instr(lower(u.author),?3)>0
       OR EXISTS(SELECT 1 FROM localized_tags t WHERE instr(lower(t.name),?3)>0 AND ((u.kind='legacy' AND EXISTS(SELECT 1 FROM asset_localized_tags a WHERE a.asset_id=u.owner_id AND a.tag_id=t.id)) OR (u.kind='media' AND EXISTS(SELECT 1 FROM media_entry_tags m WHERE m.entry_id=u.owner_id AND m.tag_id=t.id)) OR (u.kind='entity' AND EXISTS(SELECT 1 FROM asset_entity_tags e WHERE e.entity_id=u.owner_id AND e.tag_id=t.id))))
       OR EXISTS(SELECT 1 FROM asset_ai_analysis a WHERE a.asset_id=u.id AND (instr(lower(a.description),?3)>0 OR EXISTS(SELECT 1 FROM json_each(a.keywords) k WHERE instr(lower(k.value),?3)>0)))
       OR EXISTS(SELECT 1 FROM asset_sources s WHERE s.asset_id=u.id AND (instr(lower(s.url),?3)>0 OR instr(lower(s.author),?3)>0))
       OR EXISTS(SELECT 1 FROM projects p WHERE instr(lower(p.name),?3)>0 AND {project_links}){download_text})
      AND (?4 IS NULL OR (dcc_asset_source_type(u.source_url)=?4 AND (trim(u.source_url)<>'' OR (NOT EXISTS(SELECT 1 FROM asset_sources s WHERE s.asset_id=u.id AND s.url<>''){local_download_filter}))) OR EXISTS(SELECT 1 FROM asset_sources s WHERE s.asset_id=u.id AND s.source_type=?4){download_filter})
      AND (?5 IS NULL OR {projects})",local_download_filter=if downloads {" AND NOT EXISTS(SELECT 1 FROM media_sources d WHERE u.kind='media' AND d.media_id=u.owner_id AND d.url<>'')"}else{""});
    let query = request.query.trim().to_lowercase();
    let total: usize = db
        .query_row(
            &format!("{VIEW} SELECT count(*) {filter}"),
            params![
                request.locale,
                request.asset_type,
                query,
                request.source,
                request.project_id
            ],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare(&format!(
            "{VIEW} SELECT id {filter} ORDER BY u.updated_at DESC,u.id LIMIT ?6 OFFSET ?7"
        ))
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map(
            params![
                request.locale,
                request.asset_type,
                query,
                request.source,
                request.project_id,
                limit as i64,
                request.offset as i64
            ],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?;
    let mut items = vec![];
    for id in ids {
        items.push(read_asset(
            db,
            &id.map_err(|e| e.to_string())?,
            &request.locale,
        )?);
    }
    Ok(Page {
        items,
        total,
        offset: request.offset,
        limit,
    })
}

pub fn get(
    db: &Connection,
    base: &Path,
    core_id: &str,
    locale: &str,
) -> Result<CoreAssetDetail, String> {
    if !matches!(locale, "zh-CN" | "en") {
        return Err("不支持的资产语言".into());
    }
    let asset = read_asset(db, core_id, locale)?;
    let mut stmt = db.prepare("SELECT id,source_kind,source_id,role,filename,extension,size,hash,created_at FROM asset_files WHERE asset_id=?1 ORDER BY CASE role WHEN 'original' THEN 0 ELSE 1 END,created_at,id").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([core_id], |r| {
            Ok(AssetFile {
                id: r.get(0)?,
                asset_id: core_id.into(),
                source_ref: SourceFileRef {
                    kind: r.get(1)?,
                    id: r.get(2)?,
                    role: r.get(3)?,
                },
                role: r.get(3)?,
                filename: r.get(4)?,
                extension: r.get(5)?,
                size: r.get(6)?,
                hash: r.get(7)?,
                created_at: r.get(8)?,
                available: false,
                preview: None,
                media_entry_id: None,
                logical_path: None,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut files = vec![];
    for row in rows {
        let mut file = row.map_err(|e| e.to_string())?;
        file.available = if file.source_ref.kind == "mediaFile" {
            crate::media_access::LocalMediaGrants::local()
                .and_then(|grants| {
                    crate::media_access::prepare_media_file(db, base, &file.source_ref.id, &grants)
                })
                .is_ok_and(|prepared| prepared.available())
        } else {
            file_path(db, base, &file.source_ref)
                .map(|p| p.is_file())
                .unwrap_or(false)
        };
        if file.source_ref.kind == "mediaFile" {
            let (entry, logical): (String, String) = db
                .query_row(
                    "SELECT entry_id,logical_path FROM media_files WHERE id=?1",
                    [&file.source_ref.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            let proxy:Option<String>=db.query_row("SELECT id FROM media_files WHERE entry_id=?1 AND role='proxy' ORDER BY sort_order LIMIT 1",[&entry],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            let proxy = proxy.filter(|id| {
                file.role == "original"
                    && file_path(
                        db,
                        base,
                        &SourceFileRef {
                            kind: "mediaFile".into(),
                            id: id.clone(),
                            role: "proxy".into(),
                        },
                    )
                    .is_ok()
            });
            file.preview = Some(
                proxy
                    .map(|id| SourceFileRef {
                        kind: "mediaFile".into(),
                        id,
                        role: "proxy".into(),
                    })
                    .unwrap_or_else(|| file.source_ref.clone()),
            );
            file.media_entry_id = Some(entry);
            file.logical_path = Some(logical);
        } else if file.source_ref.kind == "assetMedia" && file.role == "original" {
            let proxy = SourceFileRef {
                kind: "assetMedia".into(),
                id: file.source_ref.id.clone(),
                role: "proxy".into(),
            };
            file.preview = Some(if file_path(db, base, &proxy).is_ok() {
                proxy
            } else {
                file.source_ref.clone()
            });
        } else {
            file.preview = Some(file.source_ref.clone());
        }
        files.push(file);
    }
    let mut versions_stmt = db.prepare("SELECT id,file_id,version,note,created_at FROM asset_versions WHERE asset_id=?1 ORDER BY created_at,id").map_err(|e|e.to_string())?;
    let versions = versions_stmt
        .query_map([core_id], |r| {
            Ok(AssetVersion {
                id: r.get(0)?,
                asset_id: core_id.into(),
                file_id: r.get(1)?,
                version: r.get(2)?,
                note: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut metadata_stmt = db.prepare("SELECT m.file_id,m.width,m.height,m.duration,m.codec,m.fps FROM asset_metadata m JOIN asset_files f ON f.id=m.file_id WHERE f.asset_id=?1").map_err(|e|e.to_string())?;
    let metadata = metadata_stmt
        .query_map([core_id], |r| {
            Ok(AssetMetadata {
                file_id: r.get(0)?,
                width: r.get(1)?,
                height: r.get(2)?,
                duration: r.get(3)?,
                codec: r.get(4)?,
                fps: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut project_stmt=db.prepare(if asset.origin.kind=="legacy" {"SELECT p.id,p.name FROM projects p JOIN project_assets l ON l.project_id=p.id WHERE l.asset_id=?1 ORDER BY p.name"}else if asset.origin.kind=="entity" {"SELECT p.id,p.name FROM projects p JOIN asset_entity_projects l ON l.project_id=p.id WHERE l.entity_id=?1 ORDER BY p.name"}else {"SELECT p.id,p.name FROM projects p JOIN project_media_entries l ON l.project_id=p.id WHERE l.entry_id=?1 ORDER BY p.name"}).map_err(|e|e.to_string())?;
    let projects = project_stmt
        .query_map([&asset.origin.id], |r| {
            Ok(AssetLink {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let sources = sources(db, core_id, &asset.origin)?;
    let mut notes=db.prepare("SELECT DISTINCT n.id,n.title FROM knowledge_notes n JOIN knowledge_references r ON r.note_id=n.id WHERE r.target_kind=?1 AND r.target_id=?2 AND n.deleted_at IS NULL ORDER BY n.updated_at DESC LIMIT 100").map_err(|e|e.to_string())?;
    let knowledge_links = notes
        .query_map(
            params![
                if asset.origin.kind == "legacy" {
                    "asset"
                } else if asset.origin.kind == "entity" {
                    "coreAsset"
                } else {
                    "media"
                },
                asset.origin.id
            ],
            |r| {
                Ok(AssetLink {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let related_sql = if asset.origin.kind == "legacy" {
        "SELECT c.id,e.name FROM media_entry_assets l JOIN media_entries e ON e.id=l.entry_id JOIN asset_core c ON c.media_entry_id=e.id WHERE l.asset_id=?1 AND e.deleted_at IS NULL"
    } else if asset.origin.kind == "entity" {
        "SELECT id,name FROM asset_entities WHERE id=?1 AND 0"
    } else {
        "SELECT c.id,a.name FROM media_entry_assets l JOIN assets a ON a.id=l.asset_id JOIN asset_core c ON c.legacy_asset_id=a.id WHERE l.entry_id=?1 AND a.deleted_at IS NULL"
    };
    let mut related = db.prepare(related_sql).map_err(|e| e.to_string())?;
    let related_assets = related
        .query_map([&asset.origin.id], |r| {
            Ok(AssetLink {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut tag_stmt=db.prepare(if asset.origin.kind=="legacy" {"SELECT t.id,t.name,t.color,t.created_at FROM localized_tags t JOIN asset_localized_tags l ON l.tag_id=t.id WHERE l.asset_id=?1 AND t.locale=?2 ORDER BY t.name"}else if asset.origin.kind=="entity" {"SELECT t.id,t.name,t.color,t.created_at FROM localized_tags t JOIN asset_entity_tags l ON l.tag_id=t.id WHERE l.entity_id=?1 AND t.locale=?2 ORDER BY t.name"}else {"SELECT t.id,t.name,t.color,t.created_at FROM localized_tags t JOIN media_entry_tags l ON l.tag_id=t.id WHERE l.entry_id=?1 AND ?2 IN ('zh-CN','en') ORDER BY t.name"}).map_err(|e|e.to_string())?;
    let tag_details = tag_stmt
        .query_map(params![asset.origin.id, locale], |r| {
            Ok(CoreTag {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                created_at: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    Ok(CoreAssetDetail {
        asset,
        files,
        metadata,
        versions,
        sources,
        projects,
        knowledge_links,
        related_assets,
        processing_history: vec![],
        tag_details,
    })
}
