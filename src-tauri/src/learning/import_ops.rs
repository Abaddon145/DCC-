//! Durable Markdown import batches. Callers must already verify source bytes; this module never opens a vault path.
use super::{note_origins, source_profiles};
use crate::{
    knowledge::{self, NoteInput},
    state::AppState,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_STAGE_BYTES: u64 = 5 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    static COMMIT_FAIL_AFTER_APPLIES: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Test-only: after this many newly applied items in one `commit_batch` call, fail closed.
/// `None` disables. Used to prove mid-batch resume / cancel recovery.
#[cfg(test)]
pub(super) fn set_commit_fail_after_applies(after: Option<u32>) {
    COMMIT_FAIL_AFTER_APPLIES.with(|c| c.set(after));
}

#[cfg(test)]
fn note_commit_apply_and_maybe_fail(applied_in_call: u32) -> Result<(), String> {
    COMMIT_FAIL_AFTER_APPLIES.with(|c| {
        if let Some(limit) = c.get() {
            if applied_in_call >= limit {
                c.set(None);
                return Err("迁入提交中断（测试注入）".into());
            }
        }
        Ok(())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportBatch {
    pub id: String,
    pub library_id: String,
    pub root_hash: String,
    pub source_profile_id: String,
    pub source_epoch: String,
    pub request_sha256: String,
    pub status: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub receipt_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportItem {
    pub id: String,
    pub batch_id: String,
    pub source_key_json: String,
    pub source_sha256: String,
    pub status: String,
    pub stage_relative: Option<String>,
    pub conflict_code: Option<String>,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct NewImportItem {
    pub vault: String,
    pub relative_path: String,
    pub source_sha256: String,
    pub title: String,
    pub document: Value,
    pub expected_target_id: Option<String>,
    pub expected_target_revision: Option<i64>,
    pub expected_target_sha256: Option<String>,
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn scoped_base<T>(
    state: &AppState,
    expected: &source_profiles::LibraryScope,
    f: impl FnOnce(&Connection, &Path) -> Result<T, String>,
) -> Result<T, String> {
    state
        .with_library_if_id(expected.library_id(), |db, base| {
            if state.tasks.maintenance().is_some() {
                return Err("素材库正在维护，请稍后重试".into());
            }
            let current = super::analysis_store::library_identity(base)?;
            if current
                != (
                    expected.library_id().to_string(),
                    expected.root_hash().to_string(),
                )
            {
                return Err("素材库身份已变化".into());
            }
            if *state
                .source_scope_nonce
                .lock()
                .map_err(|_| "来源会话状态不可用")?
                != expected.session_nonce()
            {
                return Err("素材库会话已变化，请重新检查来源".into());
            }
            f(db, base)
        })?
        .ok_or_else(|| "素材库已切换".into())
}

fn load_batch(db: &Connection, id: &str) -> Result<ImportBatch, String> {
    if !source_profiles::valid_uuid(id) {
        return Err("迁入批次无效".into());
    }
    db.query_row(
        "SELECT id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,finished_at,receipt_json FROM learning_import_batches WHERE id=?1",
        [id],
        |row| {
            Ok(ImportBatch {
                id: row.get(0)?,
                library_id: row.get(1)?,
                root_hash: row.get(2)?,
                source_profile_id: row.get(3)?,
                source_epoch: row.get(4)?,
                request_sha256: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                finished_at: row.get(8)?,
                receipt_json: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|_| "无法读取迁入批次")?
    .ok_or_else(|| "迁入批次不存在".into())
}

fn load_item(db: &Connection, id: &str) -> Result<ImportItem, String> {
    if !source_profiles::valid_uuid(id) {
        return Err("迁入条目无效".into());
    }
    db.query_row(
        "SELECT id,batch_id,source_key_json,source_sha256,status,stage_relative,conflict_code,target_id FROM learning_import_items WHERE id=?1",
        [id],
        |row| {
            Ok(ImportItem {
                id: row.get(0)?,
                batch_id: row.get(1)?,
                source_key_json: row.get(2)?,
                source_sha256: row.get(3)?,
                status: row.get(4)?,
                stage_relative: row.get(5)?,
                conflict_code: row.get(6)?,
                target_id: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|_| "无法读取迁入条目")?
    .ok_or_else(|| "迁入条目不存在".into())
}

fn stage_relative(batch_id: &str, item_id: &str) -> String {
    format!(".dcc/runtime/imports/{batch_id}/{item_id}.md")
}

fn stage_path(base: &Path, relative: &str) -> Result<PathBuf, String> {
    if !relative.starts_with(".dcc/runtime/imports/")
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("迁入暂存路径无效".into());
    }
    let path = base.join(relative);
    if !path.starts_with(base.join(".dcc/runtime/imports")) {
        return Err("迁入暂存路径无效".into());
    }
    Ok(path)
}

fn remove_stage(base: &Path, relative: Option<&str>) {
    if let Some(rel) = relative {
        if let Ok(path) = stage_path(base, rel) {
            let _ = fs::remove_file(&path);
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

/// Drop runtime staging bytes. Clears DB pointer only when schema allows
/// (`staged` rows must keep a non-NULL `stage_relative` until status changes).
fn clear_item_stage(db: &Connection, base: &Path, item: &ImportItem) -> Result<(), String> {
    if let Some(rel) = item.stage_relative.as_deref() {
        remove_stage(base, Some(rel));
    } else {
        // Crashed write may leave a file without a DB pointer.
        let relative = stage_relative(&item.batch_id, &item.id);
        remove_stage(base, Some(relative.as_str()));
    }
    if item.status != "staged" && item.stage_relative.is_some() {
        db.execute(
            "UPDATE learning_import_items SET stage_relative=NULL WHERE id=?1 AND status!='staged'",
            [&item.id],
        )
        .map_err(|_| "无法清理迁入暂存登记")?;
    }
    Ok(())
}

fn remove_batch_stage_dir(base: &Path, batch_id: &str) {
    let dir = base.join(".dcc/runtime/imports").join(batch_id);
    let _ = fs::remove_dir(&dir);
}

fn write_stage(base: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
    let path = stage_path(base, relative)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建迁入暂存目录")?;
    }
    let tmp = path.with_extension("md.partial");
    {
        let mut file = fs::File::create(&tmp).map_err(|_| "无法写入迁入暂存")?;
        file.write_all(bytes).map_err(|_| "无法写入迁入暂存")?;
        file.sync_all().map_err(|_| "无法写入迁入暂存")?;
    }
    if path.exists() {
        fs::remove_file(&path).map_err(|_| "无法替换迁入暂存")?;
    }
    fs::rename(&tmp, &path).map_err(|_| "无法发布迁入暂存")?;
    if let Ok(file) = fs::File::open(&path) {
        let _ = file.sync_all();
    }
    Ok(())
}

fn key_from_item(
    profile_id: &str,
    source_key_json: &str,
) -> Result<note_origins::NoteOriginKey, String> {
    let parts: Vec<String> =
        serde_json::from_str(source_key_json).map_err(|_| "笔记来源身份无效")?;
    if parts.len() != 4 || parts[0] != profile_id || parts[1] != "markdown" {
        return Err("笔记来源身份无效".into());
    }
    note_origins::NoteOriginKey::new(&parts[0], &parts[2], &parts[3])
}

pub(super) fn create_batch(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    profile_id: &str,
    epoch: &str,
    request_sha256: &str,
) -> Result<ImportBatch, String> {
    if !valid_hash(request_sha256) {
        return Err("迁入请求摘要无效".into());
    }
    scoped_base(state, scope, |db, _| {
        let profile = source_profiles::get_in(db, profile_id)?;
        if profile.access_epoch != epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        if let Some(existing) = db
            .query_row(
                "SELECT id FROM learning_import_batches WHERE library_id=?1 AND request_sha256=?2 AND status IN ('pending','committed')",
                params![scope.library_id(), request_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| "无法读取迁入批次")?
        {
            return load_batch(db, &existing);
        }
        let batch = ImportBatch {
            id: Uuid::new_v4().to_string(),
            library_id: scope.library_id().into(),
            root_hash: scope.root_hash().into(),
            source_profile_id: profile.id,
            source_epoch: epoch.into(),
            request_sha256: request_sha256.into(),
            status: "pending".into(),
            created_at: now(),
            finished_at: None,
            receipt_json: None,
        };
        db.execute(
            "INSERT INTO learning_import_batches(id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,finished_at,receipt_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,NULL)",
            params![
                batch.id,
                batch.library_id,
                batch.root_hash,
                batch.source_profile_id,
                batch.source_epoch,
                batch.request_sha256,
                batch.status,
                batch.created_at
            ],
        )
        .map_err(|_| "无法创建迁入批次")?;
        Ok(batch)
    })
}

pub(super) fn add_item(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    batch_id: &str,
    input: NewImportItem,
) -> Result<ImportItem, String> {
    if !valid_hash(&input.source_sha256) {
        return Err("来源摘要无效".into());
    }
    if input.title.trim().is_empty() || input.title.len() > 512 {
        return Err("迁入笔记标题无效".into());
    }
    let expected = match (
        input.expected_target_id.as_deref(),
        input.expected_target_revision,
        input.expected_target_sha256.as_deref(),
    ) {
        (None, None, None) => None,
        (Some(id), Some(rev), Some(hash))
            if source_profiles::valid_uuid(id) && rev >= 0 && valid_hash(hash) =>
        {
            Some((id.to_string(), rev, hash.to_string()))
        }
        _ => return Err("迁入目标预期无效".into()),
    };
    let document_json = serde_json::to_string(&input.document).map_err(|_| "迁入笔记正文无效")?;
    if document_json.len() > 16 * 1024 * 1024 {
        return Err("迁入笔记正文过大".into());
    }
    scoped_base(state, scope, |db, _| {
        let batch = load_batch(db, batch_id)?;
        if batch.library_id != scope.library_id() || batch.root_hash != scope.root_hash() {
            return Err("迁入批次不属于当前素材库".into());
        }
        if batch.status != "pending" {
            return Err("迁入批次已结束".into());
        }
        let profile = source_profiles::get_in(db, &batch.source_profile_id)?;
        if profile.access_epoch != batch.source_epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        let key = note_origins::NoteOriginKey::new(
            &batch.source_profile_id,
            &input.vault,
            &input.relative_path,
        )?;
        let item = ImportItem {
            id: Uuid::new_v4().to_string(),
            batch_id: batch.id.clone(),
            source_key_json: key.encoded().into(),
            source_sha256: input.source_sha256.clone(),
            status: "pending".into(),
            stage_relative: None,
            conflict_code: None,
            target_id: None,
        };
        let (expected_id, expected_rev, expected_hash) = match &expected {
            Some((id, rev, hash)) => (Some(id.as_str()), Some(*rev), Some(hash.as_str())),
            None => (None, None, None),
        };
        if let Some(existing_id) = db
            .query_row(
                "SELECT id FROM learning_import_items WHERE batch_id=?1 AND source_key_json=?2",
                params![item.batch_id, item.source_key_json],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| "无法读取迁入条目")?
        {
            let existing = load_item(db, &existing_id)?;
            let (title, doc, sha): (String, String, String) = db
                .query_row(
                    "SELECT title,document_json,source_sha256 FROM learning_import_items WHERE id=?1",
                    [&existing.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|_| "无法读取迁入条目")?;
            if title == input.title && doc == document_json && sha == item.source_sha256 {
                return Ok(existing);
            }
            return Err("迁入条目已存在或无法登记".into());
        }
        db.execute(
            "INSERT INTO learning_import_items(id,batch_id,source_key_json,source_sha256,target_kind,expected_target_id,expected_target_revision,expected_target_sha256,title,document_json,stage_relative,status,conflict_code,target_id,receipt_json,created_at) VALUES(?1,?2,?3,?4,'note',?5,?6,?7,?8,?9,NULL,'pending',NULL,NULL,NULL,?10)",
            params![
                item.id,
                item.batch_id,
                item.source_key_json,
                item.source_sha256,
                expected_id,
                expected_rev,
                expected_hash,
                input.title,
                document_json,
                now()
            ],
        )
        .map_err(|_| "迁入条目已存在或无法登记")?;
        Ok(item)
    })
}

pub(super) fn stage_item_bytes(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    batch_id: &str,
    item_id: &str,
    bytes: &[u8],
) -> Result<ImportItem, String> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_STAGE_BYTES {
        return Err("迁入暂存内容超限".into());
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    scoped_base(state, scope, |db, base| {
        let batch = load_batch(db, batch_id)?;
        if batch.status != "pending" {
            return Err("迁入批次已结束".into());
        }
        let item = load_item(db, item_id)?;
        if item.batch_id != batch.id {
            return Err("迁入条目不属于该批次".into());
        }
        if item.status != "pending" && item.status != "staged" {
            return Err("迁入条目不可再暂存".into());
        }
        if digest != item.source_sha256 {
            return Err("迁入暂存摘要不匹配".into());
        }
        let relative = stage_relative(&batch.id, &item.id);
        if item.status == "staged" && item.stage_relative.as_deref() == Some(relative.as_str()) {
            if let Ok(path) = stage_path(base, &relative) {
                if path.is_file() {
                    let on_disk = fs::read(&path).map_err(|_| "无法读取迁入暂存")?;
                    if on_disk == bytes {
                        return Ok(item);
                    }
                }
            }
        }
        write_stage(base, &relative, bytes)?;
        db.execute(
            "UPDATE learning_import_items SET status='staged', stage_relative=?1, conflict_code=NULL WHERE id=?2",
            params![relative, item.id],
        )
        .map_err(|_| "无法更新迁入暂存状态")?;
        load_item(db, &item.id)
    })
}

pub(super) fn commit_batch(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    batch_id: &str,
) -> Result<ImportBatch, String> {
    scoped_base(state, scope, |db, base| {
        let batch = load_batch(db, batch_id)?;
        if batch.library_id != scope.library_id() {
            return Err("迁入批次不属于当前素材库".into());
        }
        if batch.status != "pending" {
            return load_batch(db, batch_id);
        }
        let profile = source_profiles::get_in(db, &batch.source_profile_id)?;
        if profile.access_epoch != batch.source_epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        let mut items = db
            .prepare(
                "SELECT id FROM learning_import_items WHERE batch_id=?1 ORDER BY created_at,id",
            )
            .map_err(|_| "无法读取迁入条目")?
            .query_map([&batch.id], |row| row.get::<_, String>(0))
            .map_err(|_| "无法读取迁入条目")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法读取迁入条目")?;
        if items.is_empty() {
            return Err("迁入批次没有条目".into());
        }
        let mut committed = 0u32;
        let mut conflicts = 0u32;
        let mut applied_in_call = 0u32;
        for item_id in items.drain(..) {
            let item = load_item(db, &item_id)?;
            match item.status.as_str() {
                "committed" => {
                    // Resume/repair: keep durable note, drop leftover runtime staging.
                    clear_item_stage(db, base, &item)?;
                    committed += 1;
                    continue;
                }
                "conflict" => {
                    clear_item_stage(db, base, &item)?;
                    conflicts += 1;
                    continue;
                }
                "cancelled" => continue,
                "staged" | "pending" => {}
                _ => return Err("迁入条目状态无效".into()),
            }
            let key = key_from_item(&batch.source_profile_id, &item.source_key_json)?;
            let state_now = note_origins::inspect(db, &key, &item.source_sha256)?;
            let conflict = match &state_now {
                note_origins::NoteOriginState::Unmapped => None,
                note_origins::NoteOriginState::Unchanged(id) => {
                    db.execute(
                        "UPDATE learning_import_items SET status='committed', target_id=?1, conflict_code=NULL, receipt_json=?2 WHERE id=?3",
                        params![id, json!({"action":"unchanged"}).to_string(), item.id],
                    )
                    .map_err(|_| "无法提交迁入条目")?;
                    let refreshed = load_item(db, &item.id)?;
                    clear_item_stage(db, base, &refreshed)?;
                    committed += 1;
                    applied_in_call += 1;
                    #[cfg(test)]
                    note_commit_apply_and_maybe_fail(applied_in_call)?;
                    continue;
                }
                note_origins::NoteOriginState::SourceChanged(_) => Some("source_changed"),
                note_origins::NoteOriginState::TargetChanged(_) => Some("target_changed"),
                note_origins::NoteOriginState::BothChanged(_) => Some("both_changed"),
                note_origins::NoteOriginState::TargetUnavailable(_) => Some("target_unavailable"),
            };
            if let Some(code) = conflict {
                db.execute(
                    "UPDATE learning_import_items SET status='conflict', conflict_code=?1 WHERE id=?2",
                    params![code, item.id],
                )
                .map_err(|_| "无法记录迁入冲突")?;
                let refreshed = load_item(db, &item.id)?;
                clear_item_stage(db, base, &refreshed)?;
                conflicts += 1;
                applied_in_call += 1;
                #[cfg(test)]
                note_commit_apply_and_maybe_fail(applied_in_call)?;
                continue;
            }
            // Unmapped: create note + origin in one transaction.
            let (title, document_json): (String, String) = db
                .query_row(
                    "SELECT title,document_json FROM learning_import_items WHERE id=?1",
                    [&item.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|_| "无法读取迁入正文")?;
            let document: Value =
                serde_json::from_str(&document_json).map_err(|_| "迁入笔记正文无效")?;
            let tx = db
                .unchecked_transaction()
                .map_err(|_| "无法开始迁入提交事务")?;
            let note = knowledge::save_in(
                &tx,
                NoteInput {
                    id: None,
                    expected_revision: None,
                    notebook_id: None,
                    title,
                    document,
                    tags: vec![],
                    favorite: false,
                    journal_date: None,
                },
                false,
            )
            .map_err(|_| "无法创建迁入笔记")?;
            note_origins::record_note_in(
                &tx,
                &key,
                &note.id,
                &item.source_sha256,
                &batch.source_epoch,
            )?;
            tx.execute(
                "UPDATE learning_import_items SET status='committed', target_id=?1, conflict_code=NULL, receipt_json=?2 WHERE id=?3",
                params![
                    note.id,
                    json!({"action":"created","noteId":note.id}).to_string(),
                    item.id
                ],
            )
            .map_err(|_| "无法提交迁入条目")?;
            tx.commit().map_err(|_| "无法提交迁入事务")?;
            let refreshed = load_item(db, &item.id)?;
            clear_item_stage(db, base, &refreshed)?;
            committed += 1;
            applied_in_call += 1;
            #[cfg(test)]
            note_commit_apply_and_maybe_fail(applied_in_call)?;
        }
        remove_batch_stage_dir(base, &batch.id);
        let receipt = json!({
            "committed": committed,
            "conflicts": conflicts,
        })
        .to_string();
        let finished = now();
        db.execute(
            "UPDATE learning_import_batches SET status='committed', finished_at=?1, receipt_json=?2 WHERE id=?3",
            params![finished, receipt, batch.id],
        )
        .map_err(|_| "无法完成迁入批次")?;
        load_batch(db, &batch.id)
    })
}

pub(super) fn cancel_batch(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    batch_id: &str,
) -> Result<ImportBatch, String> {
    scoped_base(state, scope, |db, base| {
        let batch = load_batch(db, batch_id)?;
        if batch.library_id != scope.library_id() {
            return Err("迁入批次不属于当前素材库".into());
        }
        if batch.status != "pending" {
            return load_batch(db, batch_id);
        }
        let items = db
            .prepare(
                "SELECT id FROM learning_import_items WHERE batch_id=?1 ORDER BY created_at,id",
            )
            .map_err(|_| "无法读取迁入条目")?
            .query_map([&batch.id], |row| row.get::<_, String>(0))
            .map_err(|_| "无法读取迁入条目")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法读取迁入条目")?;
        for id in items {
            let item = load_item(db, &id)?;
            // Always drop runtime staging, including orphans left by a mid-commit crash.
            clear_item_stage(db, base, &item)?;
            if item.status == "committed" || item.status == "conflict" {
                // Durable outcomes stay; cancel only abandons unfinished work.
                continue;
            }
            db.execute(
                "UPDATE learning_import_items SET status='cancelled', stage_relative=NULL, conflict_code=NULL WHERE id=?1",
                [&id],
            )
            .map_err(|_| "无法取消迁入条目")?;
        }
        remove_batch_stage_dir(base, &batch.id);
        let finished = now();
        db.execute(
            "UPDATE learning_import_batches SET status='cancelled', finished_at=?1 WHERE id=?2",
            params![finished, batch.id],
        )
        .map_err(|_| "无法取消迁入批次")?;
        load_batch(db, &batch.id)
    })
}

pub(super) fn list_batches(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
) -> Result<Vec<ImportBatch>, String> {
    scoped_base(state, scope, |db, _| {
        let mut stmt = db
            .prepare(
                "SELECT id FROM learning_import_batches WHERE library_id=?1 ORDER BY created_at DESC,id",
            )
            .map_err(|_| "无法列出迁入批次")?;
        let ids = stmt
            .query_map([scope.library_id()], |row| row.get::<_, String>(0))
            .map_err(|_| "无法列出迁入批次")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法列出迁入批次")?;
        ids.into_iter().map(|id| load_batch(db, &id)).collect()
    })
}

pub(super) fn get_item(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    item_id: &str,
) -> Result<ImportItem, String> {
    scoped_base(state, scope, |db, _| load_item(db, item_id))
}

/// Recovery read: list durable item journal rows for a batch (includes mid-apply leftovers).
pub(super) fn list_items(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    batch_id: &str,
) -> Result<Vec<ImportItem>, String> {
    scoped_base(state, scope, |db, _| {
        let batch = load_batch(db, batch_id)?;
        if batch.library_id != scope.library_id() {
            return Err("迁入批次不属于当前素材库".into());
        }
        let ids = db
            .prepare(
                "SELECT id FROM learning_import_items WHERE batch_id=?1 ORDER BY created_at,id",
            )
            .map_err(|_| "无法列出迁入条目")?
            .query_map([&batch.id], |row| row.get::<_, String>(0))
            .map_err(|_| "无法列出迁入条目")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法列出迁入条目")?;
        ids.into_iter().map(|id| load_item(db, &id)).collect()
    })
}
