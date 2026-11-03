use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MembershipChange {
    pub media_id: String,
    pub expected_materials: bool,
    pub expected_learning: bool,
    pub expected_snapshot: String,
    pub materials: bool,
    pub learning: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipPreview {
    pub media_id: String,
    pub name: String,
    pub materials: bool,
    pub learning: bool,
    pub has_course: bool,
    pub project_count: usize,
    pub asset_count: usize,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnassignedLearningVideo {
    pub media_id: String,
    pub name: String,
}

pub fn list_unassigned_for_library(
    state: &crate::state::AppState,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<crate::models::Page<UnassignedLearningVideo>, String> {
    let scope = super::source_profiles::scope(state, library_id)?;
    super::source_profiles::scoped(state, &scope, |db| {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        let offset = offset.max(0);
        let limit = limit.clamp(1, 100);
        let filter="FROM media_entries e JOIN media_workspace_memberships w ON w.media_entry_id=e.id AND w.workspace='learning' WHERE e.kind='video' AND e.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM learning_courses c WHERE c.media_entry_id=e.id)";
        let total = tx
            .query_row(&format!("SELECT count(*) {filter}"), [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let items = {
            let mut stmt=tx.prepare(&format!("SELECT e.id,e.name {filter} ORDER BY e.updated_at DESC,e.id LIMIT ?1 OFFSET ?2")).map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map(params![limit, offset], |r| {
                    Ok(UnassignedLearningVideo {
                        media_id: r.get(0)?,
                        name: r.get(1)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        tx.commit().map_err(|e| e.to_string())?;
        Ok(crate::models::Page {
            items,
            total,
            offset,
            limit,
        })
    })
}

pub fn preview_one(db: &Connection, media_id: &str) -> Result<MembershipPreview, String> {
    uuid::Uuid::parse_str(media_id).map_err(|_| "视频标识无效")?;
    let row: Option<(String,String,String,Option<String>,bool,bool,Option<String>,Option<i64>)> = db.query_row(
        "SELECT e.name,e.primary_file_id,e.updated_at,f.checksum, EXISTS(SELECT 1 FROM media_workspace_memberships w WHERE w.media_entry_id=e.id AND w.workspace='materials'), EXISTS(SELECT 1 FROM media_workspace_effective_memberships w WHERE w.media_entry_id=e.id AND w.workspace='learning'), c.id,c.progress_revision FROM media_entries e LEFT JOIN media_files f ON f.id=e.primary_file_id AND f.entry_id=e.id LEFT JOIN learning_courses c ON c.media_entry_id=e.id WHERE e.id=?1 AND e.kind='video' AND e.deleted_at IS NULL",
        [media_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)),
    ).optional().map_err(|e| e.to_string())?;
    let (name, file, updated, checksum, materials, learning, course, revision) =
        row.ok_or("视频不存在或已删除")?;
    let links = |sql: &str| -> Result<Vec<String>, String> {
        let mut stmt = db.prepare(sql).map_err(|e| e.to_string())?;
        let values = stmt
            .query_map([media_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if values.len() > 10_000 {
            return Err("视频关联过多，无法预览".into());
        }
        Ok(values)
    };
    let projects=links("SELECT project_id FROM project_media_entries WHERE entry_id=?1 ORDER BY project_id LIMIT 10001")?;
    let assets = links(
        "SELECT asset_id FROM media_entry_assets WHERE entry_id=?1 ORDER BY asset_id LIMIT 10001",
    )?;
    let bytes = serde_json::to_vec(&(
        media_id, &name, &file, &updated, &checksum, materials, learning, &course, revision,
        &projects, &assets,
    ))
    .map_err(|e| e.to_string())?;
    Ok(MembershipPreview {
        media_id: media_id.into(),
        name,
        materials,
        learning,
        has_course: course.is_some(),
        project_count: projects.len(),
        asset_count: assets.len(),
        snapshot: format!("{:x}", Sha256::digest(bytes)),
    })
}

pub fn apply_changes(db: &Connection, changes: &[MembershipChange]) -> Result<(), String> {
    if changes.is_empty() || changes.len() > 500 {
        return Err("每次请选择 1—500 个视频".into());
    }
    let mut ids = HashSet::new();
    for change in changes {
        uuid::Uuid::parse_str(&change.media_id).map_err(|_| "视频标识无效")?;
        if !ids.insert(&change.media_id)
            || (!change.materials && !change.learning)
            || change.expected_snapshot.len() != 64
            || !change
                .expected_snapshot
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err("视频重复或未保留任何归属".into());
        }
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    for change in changes {
        if preview_one(&tx, &change.media_id)?.snapshot != change.expected_snapshot {
            return Err("视频或关联已变化，请重新预览".into());
        }
        let (valid, materials, learning, course): (bool, bool, bool, bool) = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_entries WHERE id=?1 AND kind='video' AND deleted_at IS NULL), EXISTS(SELECT 1 FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace='materials'), EXISTS(SELECT 1 FROM media_workspace_effective_memberships WHERE media_entry_id=?1 AND workspace='learning'), EXISTS(SELECT 1 FROM learning_courses WHERE media_entry_id=?1)",
            [&change.media_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).map_err(|e| e.to_string())?;
        if !valid || materials != change.expected_materials || learning != change.expected_learning
        {
            return Err("视频或归属已变化，请重新预览".into());
        }
        if course && !change.learning {
            return Err("视频仍被课程使用，不能移除学习归属".into());
        }
    }
    for change in changes {
        for (workspace, enabled) in [
            ("materials", change.materials),
            ("learning", change.learning),
        ] {
            if enabled {
                tx.execute("INSERT OR IGNORE INTO media_workspace_memberships(media_entry_id,workspace) VALUES(?1,?2)", params![change.media_id,workspace]).map_err(|e|e.to_string())?;
            } else {
                tx.execute("DELETE FROM media_workspace_memberships WHERE media_entry_id=?1 AND workspace=?2", params![change.media_id,workspace]).map_err(|e|e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

fn seal(scope: &super::source_profiles::LibraryScope, snapshot: &str) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        scope.library_id(),
        scope.root_hash(),
        scope.session_nonce().to_string(),
        snapshot,
    ))
    .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn preview_for_library(
    state: &crate::state::AppState,
    library_id: &str,
    media_id: &str,
) -> Result<MembershipPreview, String> {
    let scope = super::source_profiles::scope(state, library_id)?;
    super::source_profiles::scoped(state, &scope, |db| {
        let mut preview = preview_one(db, media_id)?;
        preview.snapshot = seal(&scope, &preview.snapshot)?;
        Ok(preview)
    })
}

pub fn list_for_library(
    state: &crate::state::AppState,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<crate::models::Page<MembershipPreview>, String> {
    let scope = super::source_profiles::scope(state, library_id)?;
    super::source_profiles::scoped(state, &scope, |db| {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        let offset = offset.max(0);
        let limit = limit.clamp(1, 100);
        let total = tx
            .query_row(
                "SELECT count(*) FROM media_entries WHERE kind='video' AND deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let ids = {
            let mut stmt=tx.prepare("SELECT id FROM media_entries WHERE kind='video' AND deleted_at IS NULL ORDER BY id LIMIT ?1 OFFSET ?2").map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map(params![limit, offset], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            let mut preview = preview_one(&tx, &id)?;
            preview.snapshot = seal(&scope, &preview.snapshot)?;
            items.push(preview);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(crate::models::Page {
            items,
            total,
            offset,
            limit,
        })
    })
}

pub fn apply_for_library(
    state: &crate::state::AppState,
    library_id: &str,
    changes: &[MembershipChange],
) -> Result<(), String> {
    if changes.is_empty() || changes.len() > 500 {
        return Err("每次请选择 1—500 个视频".into());
    }
    if changes.iter().any(|change| {
        change.expected_snapshot.len() != 64
            || !change
                .expected_snapshot
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
    }) {
        return Err("迁移预览无效，请重新读取".into());
    }
    let scope = super::source_profiles::scope(state, library_id)?;
    super::source_profiles::scoped(state, &scope, |db| {
        let mut nonce=state.source_scope_nonce.lock().map_err(|_| "来源会话状态不可用")?;
        if *nonce!=scope.session_nonce(){return Err("素材库会话已变化，请重新预览".into());}
        let mut checked = Vec::with_capacity(changes.len());
        for change in changes {
            let raw = preview_one(db, &change.media_id)?.snapshot;
            if seal(&scope, &raw)? != change.expected_snapshot {
                return Err("素材库会话或视频已变化，请重新预览".into());
            }
            let mut verified = change.clone();
            verified.expected_snapshot = raw;
            checked.push(verified);
        }
        apply_changes(db, &checked)?;
        *nonce=uuid::Uuid::new_v4();
        Ok(())
    })
}
