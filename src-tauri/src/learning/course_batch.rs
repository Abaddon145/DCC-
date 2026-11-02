//! Course batch identity is journal metadata, not source authorization.
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBatchSummary {
    pub id: String,
    pub status: String,
    pub created_at: String,
    pub staged: bool,
    pub committed: i64,
    pub conflicts: i64,
    pub reverted: bool,
    pub revert_damaged: bool,
    pub current_source_epoch: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBatchCursor {
    created_at: String,
    id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBatchEntry {
    pub id: String,
    pub entry_kind: String,
    pub source_id: String,
    pub status: String,
    pub conflict_code: Option<String>,
    pub target_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBatchEntryPage {
    pub items: Vec<CourseBatchEntry>,
    pub has_more: bool,
    pub revert_report: Option<serde_json::Value>,
    pub revert_damaged: bool,
}

pub(super) fn list_course_entries(
    state: &AppState,
    library: &str,
    batch: &str,
    offset: u32,
) -> Result<CourseBatchEntryPage, String> {
    let scope = source_profiles::scope(state, library)?;
    source_profiles::scoped(state, &scope, |db| {
        let report: Option<String> = db.query_row(
            "SELECT r.report_json FROM learning_import_batches b LEFT JOIN learning_import_reverts r ON r.batch_id=b.id WHERE b.id=?1 AND b.library_id=?2 AND b.root_hash=?3 AND b.batch_kind='courses'",
            params![batch, scope.library_id(), scope.root_hash()],
            |row| row.get(0),
        ).optional().map_err(|_| "无法读取课程迁入批次")?.ok_or("课程迁入批次不存在")?;
        let revert_damaged = report
            .as_deref()
            .is_some_and(|value| !valid_revert_report(value, batch));
        let revert_report = report
            .as_deref()
            .filter(|_| !revert_damaged)
            .and_then(|value| serde_json::from_str(value).ok());
        let mut stmt = db.prepare("SELECT id,entry_kind,source_id,status,conflict_code,target_id FROM learning_course_import_entries WHERE batch_id=?1 ORDER BY entry_kind,source_id,id LIMIT 101 OFFSET ?2").map_err(|_| "无法读取课程迁入条目")?;
        let mut items = stmt
            .query_map(params![batch, offset], |row| {
                Ok(CourseBatchEntry {
                    id: row.get(0)?,
                    entry_kind: row.get(1)?,
                    source_id: row.get(2)?,
                    status: row.get(3)?,
                    conflict_code: row.get(4)?,
                    target_id: row.get(5)?,
                })
            })
            .map_err(|_| "无法读取课程迁入条目")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法读取课程迁入条目")?;
        let has_more = items.len() > 100;
        items.truncate(100);
        Ok(CourseBatchEntryPage {
            items,
            has_more,
            revert_report,
            revert_damaged,
        })
    })
}

#[tauri::command]
pub fn list_learning_course_batch_entries(
    state: tauri::State<'_, AppState>,
    expected_library_id: String,
    batch_id: String,
    offset: u32,
) -> Result<CourseBatchEntryPage, String> {
    list_course_entries(&state, &expected_library_id, &batch_id, offset)
}

pub(super) fn valid_revert_report(text: &str, id: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    value.get("batchId").and_then(|v| v.as_str()) == Some(id)
        && value.get("archivesKept").and_then(|v| v.as_bool()) == Some(true)
        && value
            .get("reverted")
            .and_then(|v| v.as_array())
            .is_some_and(|items| items.iter().all(|v| v.is_string()))
        && value
            .get("kept")
            .and_then(|v| v.as_array())
            .is_some_and(|items| {
                items.iter().all(|v| {
                    v.get("entryId").is_some_and(|v| v.is_string())
                        && matches!(
                            v.get("outcome").and_then(|v| v.as_str()),
                            Some(
                                "kept_modified" | "kept_reused" | "kept_conflict" | "kept_missing"
                            )
                        )
                        && matches!(
                            v.get("targetKind").and_then(|v| v.as_str()),
                            Some("course" | "series")
                        )
                        && v.get("targetId")
                            .is_some_and(|v| v.is_null() || v.is_string())
                })
            })
}

#[cfg(test)]
pub(super) fn list_course_batches(
    state: &AppState,
    library: &str,
) -> Result<Vec<CourseBatchSummary>, String> {
    list_course_batches_page(state, library, None)
}

pub(super) fn list_course_batches_page(
    state: &AppState,
    library: &str,
    cursor: Option<&CourseBatchCursor>,
) -> Result<Vec<CourseBatchSummary>, String> {
    if cursor.is_some_and(|value| {
        value.created_at.is_empty()
            || value.created_at.len() > 128
            || !source_profiles::valid_uuid(&value.id)
    }) {
        return Err("课程批次分页位置无效".into());
    }
    let scope = source_profiles::scope(state, library)?;
    source_profiles::scoped(state, &scope, |db| {
        let mut stmt = db.prepare("SELECT b.id,b.status,b.created_at, EXISTS(SELECT 1 FROM learning_course_import_stage s WHERE s.batch_id=b.id), (SELECT count(*) FROM learning_course_import_entries e WHERE e.batch_id=b.id AND e.status='committed'), (SELECT count(*) FROM learning_course_import_entries e WHERE e.batch_id=b.id AND e.status='conflict'), (SELECT report_json FROM learning_import_reverts r WHERE r.batch_id=b.id), p.access_epoch FROM learning_import_batches b LEFT JOIN learning_source_profiles p ON p.id=b.source_profile_id WHERE b.library_id=?1 AND b.root_hash=?2 AND b.batch_kind='courses' AND (?3 IS NULL OR b.created_at < ?3 OR (b.created_at = ?3 AND b.id > ?4)) ORDER BY b.created_at DESC,b.id LIMIT 101").map_err(|_| "无法读取课程迁入批次")?;
        let rows = stmt
            .query_map(
                params![
                    scope.library_id(),
                    scope.root_hash(),
                    cursor.map(|value| &value.created_at),
                    cursor.map(|value| &value.id)
                ],
                |r| {
                    let id: String = r.get(0)?;
                    let report: Option<String> = r.get(6)?;
                    let valid = report
                        .as_deref()
                        .is_some_and(|text| valid_revert_report(text, &id));
                    Ok(CourseBatchSummary {
                        id,
                        status: r.get(1)?,
                        created_at: r.get(2)?,
                        staged: r.get(3)?,
                        committed: r.get(4)?,
                        conflicts: r.get(5)?,
                        reverted: report.is_some() && valid,
                        revert_damaged: report.is_some() && !valid,
                        current_source_epoch: r
                            .get::<_, Option<String>>(7)?
                            .filter(|epoch| source_profiles::valid_uuid(epoch)),
                    })
                },
            )
            .map_err(|_| "无法读取课程迁入批次")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法读取课程迁入批次")?;
        Ok(rows)
    })
}

#[tauri::command]
pub fn list_learning_course_batches(
    state: tauri::State<'_, AppState>,
    expected_library_id: String,
    cursor: Option<CourseBatchCursor>,
) -> Result<Vec<CourseBatchSummary>, String> {
    list_course_batches_page(&state, &expected_library_id, cursor.as_ref())
}

pub(super) fn create_course_batch(
    state: &AppState,
    scope: &LibraryScope,
    profile: &str,
    epoch: &str,
    request_hash: &str,
) -> Result<String, String> {
    if request_hash.len() != 64
        || !request_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("迁入请求摘要无效".into());
    }
    source_profiles::scoped(state, scope, |db| {
        let tx = db.unchecked_transaction().map_err(|_| "无法创建课程批次")?;
        if source_profiles::get_in(&tx, profile)?.access_epoch != epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        let existing: Option<(String,String,String,String,String)> = tx.query_row("SELECT id,batch_kind,source_profile_id,source_epoch,root_hash FROM learning_import_batches WHERE library_id=?1 AND request_sha256=?2 AND status IN ('pending','committed')", params![scope.library_id(),request_hash], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|_| "无法读取迁入批次")?;
        let id = if let Some((id, kind, old_profile, old_epoch, root)) = existing {
            if kind != "courses"
                || old_profile != profile
                || old_epoch != epoch
                || root != scope.root_hash()
            {
                return Err("迁入请求已被其他批次使用".into());
            }
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO learning_import_batches(id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,batch_kind) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7,'courses')", params![id,scope.library_id(),scope.root_hash(),profile,epoch,request_hash,chrono::Utc::now().to_rfc3339()]).map_err(|_| "无法创建课程批次")?;
            id
        };
        tx.commit().map_err(|_| "无法创建课程批次")?;
        Ok(id)
    })
}
