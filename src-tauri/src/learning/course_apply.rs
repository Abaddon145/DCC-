//! Course-batch apply journal. Staging a preview is not source authorization and not a Tauri command.
use super::course_origins::{self, CourseOriginKey, CourseOriginState, SeriesOriginKey};
use super::import_course_preview::{
    preview_courses_from_profile, ConflictKind, CourseImportPreview, CourseItem,
    CourseMediaBinding, Progress, SourceSeries,
};
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

const MAX_PAYLOAD_BYTES: usize = 1_048_576;
const SOURCE_CHANGED: &str = "课程来源已变化，请重新预检";
const TARGET_CHANGED: &str = "目标课程或媒体绑定已变化，请重新预检";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CourseApplyReceipt {
    pub batch_id: String,
    pub status: String,
    pub courses_committed: i64,
    pub course_conflicts: i64,
    pub series_committed: i64,
    pub series_conflicts: i64,
}

struct BatchHead {
    id: String,
    library_id: String,
    root_hash: String,
    source_profile_id: String,
    source_epoch: String,
    status: String,
    kind: String,
}

struct StageRow {
    source_fingerprint: String,
    target_fingerprint: String,
    payload_sha256: String,
    payload_json: String,
}

struct LiveCourse {
    id: String,
    position_ms: i64,
    duration_ms: Option<i64>,
    rate: f64,
    completed: i64,
    revision: i64,
}

struct ItemOutcome {
    source_id: String,
    status: &'static str,
    conflict_code: Option<&'static str>,
    target_id: Option<String>,
    receipt: String,
    source_hash: String,
    record: bool,
}

struct SeriesOutcome {
    source_id: String,
    status: &'static str,
    conflict_code: Option<&'static str>,
    target_id: Option<String>,
    receipt: String,
    source_hash: String,
    record: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBinding {
    source_item_id: String,
    media_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionOrigin {
    state: &'static str,
    target_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionProgress {
    position_ms: i64,
    duration_ms: Option<i64>,
    rate: f64,
    completed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionCourse<'a> {
    source_id: &'a str,
    title: &'a str,
    media_id: Option<&'a str>,
    source_hash: &'a str,
    origin: DecisionOrigin,
    source_progress: Option<DecisionProgress>,
    target_id: Option<&'a str>,
    target_revision: Option<i64>,
    target_progress: Option<DecisionProgress>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionSeries<'a> {
    source_id: &'a str,
    title: &'a str,
    description: Option<&'a str>,
    source_hash: &'a str,
    origin: DecisionOrigin,
    items: Vec<DecisionCourse<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionConflict {
    kind: &'static str,
    media_id: String,
    source_item_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionPayload<'a> {
    version: u32,
    bindings: Vec<DecisionBinding>,
    source_fingerprint: &'a str,
    target_fingerprint: &'a str,
    series: Vec<DecisionSeries<'a>>,
    conflicts: Vec<DecisionConflict>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn begin(db: &Connection) -> Result<rusqlite::Transaction<'_>, String> {
    db.unchecked_transaction()
        .map_err(|_| "无法提交课程迁入".into())
}

/// Touch the batch row so this transaction holds the write lock before later reads.
fn lock_batch(db: &Connection, batch_id: &str) -> Result<(), String> {
    let locked = db
        .execute(
            "UPDATE learning_import_batches SET created_at=created_at WHERE id=?1",
            [batch_id],
        )
        .map_err(|_| "无法提交课程迁入")?;
    if locked != 1 {
        return Err("无法读取迁入批次".into());
    }
    Ok(())
}

fn load_batch(db: &Connection, id: &str) -> Result<BatchHead, String> {
    db.query_row(
        "SELECT id,library_id,root_hash,source_profile_id,source_epoch,status,batch_kind FROM learning_import_batches WHERE id=?1",
        [id],
        |row| {
            Ok(BatchHead {
                id: row.get(0)?,
                library_id: row.get(1)?,
                root_hash: row.get(2)?,
                source_profile_id: row.get(3)?,
                source_epoch: row.get(4)?,
                status: row.get(5)?,
                kind: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|_| "无法读取迁入批次")?
    .ok_or_else(|| "无法读取迁入批次".into())
}

fn prepare(db: &Connection, scope: &LibraryScope, batch_id: &str) -> Result<BatchHead, String> {
    let batch = load_batch(db, batch_id)?;
    if batch.library_id != scope.library_id() || batch.root_hash != scope.root_hash() {
        return Err("迁入批次不属于当前素材库".into());
    }
    if batch.kind != "courses" {
        return Err("笔记迁入批次不能使用课程操作".into());
    }
    Ok(batch)
}

fn ensure_epoch(db: &Connection, batch: &BatchHead) -> Result<(), String> {
    let profile = source_profiles::get_in(db, &batch.source_profile_id)?;
    if profile.access_epoch != batch.source_epoch {
        return Err("来源状态已变化，请重新检查".into());
    }
    Ok(())
}

fn load_stage(db: &Connection, batch_id: &str) -> Result<Option<StageRow>, String> {
    db.query_row(
        "SELECT source_fingerprint,target_fingerprint,payload_sha256,payload_json FROM learning_course_import_stage WHERE batch_id=?1",
        [batch_id],
        |row| {
            Ok(StageRow {
                source_fingerprint: row.get(0)?,
                target_fingerprint: row.get(1)?,
                payload_sha256: row.get(2)?,
                payload_json: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|_| "无法读取课程迁入暂存".into())
}

fn load_receipt(db: &Connection, batch_id: &str) -> Result<CourseApplyReceipt, String> {
    let status: String = db
        .query_row(
            "SELECT status FROM learning_import_batches WHERE id=?1",
            [batch_id],
            |row| row.get(0),
        )
        .map_err(|_| "无法读取迁入批次")?;
    let mut statement = db
        .prepare("SELECT entry_kind,status FROM learning_course_import_entries WHERE batch_id=?1")
        .map_err(|_| "无法读取迁入批次")?;
    let rows = statement
        .query_map([batch_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| "无法读取迁入批次")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "无法读取迁入批次")?;
    let count = |kind: &str, entry_status: &str| -> i64 {
        rows.iter()
            .filter(|(entry_kind, status)| entry_kind == kind && status == entry_status)
            .count() as i64
    };
    Ok(CourseApplyReceipt {
        batch_id: batch_id.into(),
        status,
        courses_committed: count("course", "committed"),
        course_conflicts: count("course", "conflict"),
        series_committed: count("series", "committed"),
        series_conflicts: count("series", "conflict"),
    })
}

fn decision_origin(state: &CourseOriginState) -> DecisionOrigin {
    match state {
        CourseOriginState::Unmapped => DecisionOrigin {
            state: "unmapped",
            target_id: None,
        },
        CourseOriginState::Unchanged(id) => DecisionOrigin {
            state: "unchanged",
            target_id: Some(id.clone()),
        },
        CourseOriginState::SourceChanged(id) => DecisionOrigin {
            state: "source_changed",
            target_id: Some(id.clone()),
        },
        CourseOriginState::TargetChanged(id) => DecisionOrigin {
            state: "target_changed",
            target_id: Some(id.clone()),
        },
        CourseOriginState::BothChanged(id) => DecisionOrigin {
            state: "both_changed",
            target_id: Some(id.clone()),
        },
        CourseOriginState::TargetUnavailable(id) => DecisionOrigin {
            state: "target_unavailable",
            target_id: Some(id.clone()),
        },
    }
}

fn decision_progress(progress: &Progress) -> DecisionProgress {
    DecisionProgress {
        position_ms: progress.position_ms,
        duration_ms: progress.duration_ms,
        rate: progress.rate,
        completed: progress.completed,
    }
}

fn conflict_label(kind: &ConflictKind) -> &'static str {
    match kind {
        ConflictKind::DifferentSourceProgress => "different_source_progress",
        ConflictKind::SharedMedia => "shared_media",
        ConflictKind::RepeatedSeriesMembership => "repeated_membership",
        ConflictKind::DifferentTargetProgress => "divergent_progress",
    }
}

fn block_rank(code: &str) -> u8 {
    match code {
        "different_source_progress" => 0,
        "shared_media" => 1,
        "repeated_membership" => 2,
        "divergent_progress" => 3,
        _ => 4,
    }
}

fn decision_bytes(
    preview: &CourseImportPreview,
    bindings: &[CourseMediaBinding],
) -> Result<Vec<u8>, String> {
    let mut bindings: Vec<_> = bindings
        .iter()
        .map(|binding| DecisionBinding {
            source_item_id: binding.source_item_id.clone(),
            media_id: binding.media_id.clone(),
        })
        .collect();
    bindings.sort_by(|left, right| {
        (&left.source_item_id, &left.media_id).cmp(&(&right.source_item_id, &right.media_id))
    });
    let series = preview
        .series
        .iter()
        .map(decision_series)
        .collect::<Result<Vec<_>, _>>()?;
    let payload = DecisionPayload {
        version: 1,
        bindings,
        source_fingerprint: &preview.source_fingerprint,
        target_fingerprint: &preview.target_fingerprint,
        series,
        conflicts: preview
            .conflicts
            .iter()
            .map(|conflict| DecisionConflict {
                kind: conflict_label(&conflict.kind),
                media_id: conflict.media_id.clone(),
                source_item_ids: conflict.source_item_ids.clone(),
            })
            .collect(),
    };
    serde_json::to_vec(&payload).map_err(|_| "无法保存课程迁入暂存".into())
}

fn decision_series(series: &SourceSeries) -> Result<DecisionSeries<'_>, String> {
    let origin = series
        .origin
        .as_ref()
        .map(decision_origin)
        .ok_or("课程来源身份无效")?;
    let items = series
        .items
        .iter()
        .map(|item| {
            let origin = item
                .origin
                .as_ref()
                .map(decision_origin)
                .ok_or("课程来源身份无效")?;
            Ok(DecisionCourse {
                source_id: &item.id,
                title: &item.title,
                media_id: item.media_id.as_deref(),
                source_hash: &item.source_item_hash,
                origin,
                source_progress: item.source_progress.as_ref().map(decision_progress),
                target_id: item.target.as_ref().map(|target| target.id.as_str()),
                target_revision: item.target.as_ref().map(|target| target.revision),
                target_progress: item
                    .target
                    .as_ref()
                    .map(|target| decision_progress(&target.progress)),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DecisionSeries {
        source_id: &series.id,
        title: &series.title,
        description: series.description.as_deref(),
        source_hash: &series.source_series_hash,
        origin,
        items,
    })
}

fn block_codes(preview: &CourseImportPreview) -> HashMap<String, &'static str> {
    let mut codes: HashMap<String, &'static str> = HashMap::new();
    for conflict in &preview.conflicts {
        let code = conflict_label(&conflict.kind);
        for id in &conflict.source_item_ids {
            match codes.get(id) {
                Some(existing) if block_rank(code) >= block_rank(existing) => {}
                _ => {
                    codes.insert(id.clone(), code);
                }
            }
        }
    }
    codes
}

fn live_course(db: &Connection, media: &str) -> Result<Option<LiveCourse>, String> {
    db.query_row(
        "SELECT c.id,c.position_ms,c.duration_ms,c.rate,c.completed,c.progress_revision FROM learning_courses c JOIN media_entries e ON e.id=c.media_entry_id WHERE c.media_entry_id=?1 AND e.kind='video' AND e.deleted_at IS NULL",
        [media],
        |row| {
            Ok(LiveCourse {
                id: row.get(0)?,
                position_ms: row.get(1)?,
                duration_ms: row.get(2)?,
                rate: row.get(3)?,
                completed: row.get(4)?,
                revision: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|_| "无法读取目标课程".into())
}

fn live_matches(item: &CourseItem, live: &Option<LiveCourse>) -> bool {
    match (&item.target, live) {
        (None, None) => true,
        (Some(expected), Some(found)) => {
            found.id == expected.id
                && found.revision == expected.revision
                && found.position_ms == expected.progress.position_ms
                && found.duration_ms == expected.progress.duration_ms
                && found.rate == expected.progress.rate
                && (found.completed == 1) == expected.progress.completed
        }
        _ => false,
    }
}

fn verify_live(
    db: &Connection,
    batch: &BatchHead,
    preview: &CourseImportPreview,
) -> Result<(), String> {
    for series in &preview.series {
        let key = SeriesOriginKey::new(&batch.source_profile_id, &series.id)?;
        let expected = series.origin.clone().ok_or("课程来源身份无效")?;
        if course_origins::inspect_series(db, &key, &series.source_series_hash)? != expected {
            return Err(TARGET_CHANGED.into());
        }
        for item in &series.items {
            let key = CourseOriginKey::new(&batch.source_profile_id, &item.id)?;
            let expected = item.origin.clone().ok_or("课程来源身份无效")?;
            if course_origins::inspect(db, &key, &item.source_item_hash)? != expected {
                return Err(TARGET_CHANGED.into());
            }
            let live = match item.media_id.as_deref() {
                Some(media) => live_course(db, media)?,
                None => None,
            };
            if !live_matches(item, &live) {
                return Err(TARGET_CHANGED.into());
            }
        }
    }
    Ok(())
}

fn item_receipt(item: &CourseItem, action: &str, progress: &str, code: Option<&str>) -> String {
    let mut receipt = json!({
        "action": action,
        "progress": progress,
        "sourceId": item.id,
    });
    if let Some(code) = code {
        receipt["code"] = json!(code);
    }
    if let Some(source) = &item.source_progress {
        receipt["sourcePositionMs"] = json!(source.position_ms);
    }
    if let Some(target) = &item.target {
        receipt["targetPositionMs"] = json!(target.progress.position_ms);
        receipt["targetRevision"] = json!(target.revision);
    }
    receipt.to_string()
}

fn conflict_item(item: &CourseItem, code: &'static str, target_id: Option<String>) -> ItemOutcome {
    let progress = if code == "progress_absent" {
        "absent"
    } else {
        "kept"
    };
    ItemOutcome {
        source_id: item.id.clone(),
        status: "conflict",
        conflict_code: Some(code),
        target_id,
        receipt: item_receipt(item, "conflict", progress, Some(code)),
        source_hash: item.source_item_hash.clone(),
        record: false,
    }
}

fn committed_item(
    item: &CourseItem,
    action: &str,
    progress: &str,
    target_id: &str,
    record: bool,
) -> ItemOutcome {
    ItemOutcome {
        source_id: item.id.clone(),
        status: "committed",
        conflict_code: None,
        target_id: Some(target_id.into()),
        receipt: item_receipt(item, action, progress, None),
        source_hash: item.source_item_hash.clone(),
        record,
    }
}

fn acceptable_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed == title
        && !trimmed.is_empty()
        && trimmed.chars().count() <= 200
        && !title.chars().any(char::is_control)
}

fn acceptable_series(title: &str, description: &str) -> bool {
    acceptable_title(title)
        && description.len() <= 20_000
        && !description.chars().any(char::is_control)
}

fn progress_matches(course: &super::models::Course, progress: &Progress) -> bool {
    course.position_ms == progress.position_ms as u64
        && course.duration_ms == progress.duration_ms.map(|value| value as u64)
        && course.rate == progress.rate
        && course.completed == progress.completed
}

fn write_created_course(
    db: &Connection,
    course_id: &str,
    title: &str,
    progress: &Progress,
) -> Result<(), String> {
    let course = super::store::course(db, course_id)?;
    if acceptable_title(title) && course.title != title {
        let updated = db
            .execute(
                "UPDATE learning_courses SET title_override=?1, updated_at=?2 WHERE id=?3 AND title_override=''",
                params![title, now(), course_id],
            )
            .map_err(|_| "无法提交课程迁入")?;
        if updated != 1 && super::store::course(db, course_id)?.title != title {
            return Err("无法提交课程迁入".into());
        }
    }
    let course = super::store::course(db, course_id)?;
    if progress_matches(&course, progress) {
        return Ok(());
    }
    let next = course
        .progress_revision
        .checked_add(1)
        .ok_or("进度修订号已满")?;
    let updated = db
        .execute(
            "UPDATE learning_courses SET position_ms=?1,duration_ms=?2,rate=?3,completed=?4,progress_revision=?5,updated_at=?6 WHERE id=?7 AND progress_revision=?8",
            params![
                progress.position_ms,
                progress.duration_ms,
                progress.rate,
                i64::from(progress.completed),
                next,
                now(),
                course_id,
                course.progress_revision
            ],
        )
        .map_err(|_| "无法提交课程迁入")?;
    if updated != 1 {
        return Err("课程进度已变化，未覆盖".into());
    }
    Ok(())
}

fn same_progress(live: &LiveCourse, progress: &Progress) -> bool {
    live.position_ms == progress.position_ms
        && live.duration_ms == progress.duration_ms
        && live.rate == progress.rate
        && (live.completed == 1) == progress.completed
}

fn apply_unmapped(db: &Connection, item: &CourseItem, media: &str) -> Result<ItemOutcome, String> {
    let live = live_course(db, media)?;
    if !live_matches(item, &live) {
        return Err(TARGET_CHANGED.into());
    }
    match (live, &item.source_progress) {
        (Some(live), Some(progress)) if same_progress(&live, progress) => {
            Ok(committed_item(item, "mapped", "compatible", &live.id, true))
        }
        (Some(live), Some(_)) => Ok(conflict_item(item, "divergent_progress", Some(live.id))),
        (Some(live), None) => Ok(committed_item(item, "mapped", "absent", &live.id, true)),
        (None, None) => Ok(conflict_item(item, "progress_absent", None)),
        (None, Some(progress)) => {
            let course = super::store::ensure_course(db, media)?;
            write_created_course(db, &course.id, &item.title, progress)?;
            Ok(committed_item(item, "created", "applied", &course.id, true))
        }
    }
}

fn apply_item(
    db: &Connection,
    item: &CourseItem,
    blocked: &HashMap<String, &'static str>,
) -> Result<ItemOutcome, String> {
    if let Some(code) = blocked
        .get(&item.id)
        .copied()
        .filter(|code| *code != "divergent_progress")
    {
        let target = item.target.as_ref().map(|target| target.id.clone());
        return Ok(conflict_item(item, code, target));
    }
    let Some(media) = item.media_id.as_deref() else {
        return Ok(conflict_item(item, "unresolved", None));
    };
    let origin = item.origin.as_ref().ok_or("课程来源身份无效")?;
    match origin {
        CourseOriginState::Unmapped => apply_unmapped(db, item, media),
        CourseOriginState::Unchanged(id) => {
            if item.target.as_ref().map(|target| target.id.as_str()) != Some(id) {
                return Err(TARGET_CHANGED.into());
            }
            let progress = match &item.source_progress {
                None => "absent",
                Some(progress) => match &item.target {
                    Some(target) if progress == &target.progress => "compatible",
                    _ => "kept",
                },
            };
            if progress == "kept" {
                return Ok(conflict_item(item, "divergent_progress", Some(id.clone())));
            }
            Ok(committed_item(item, "unchanged", progress, id, false))
        }
        CourseOriginState::SourceChanged(id) => {
            Ok(conflict_item(item, "source_changed", Some(id.clone())))
        }
        CourseOriginState::TargetChanged(id) => {
            Ok(conflict_item(item, "target_changed", Some(id.clone())))
        }
        CourseOriginState::BothChanged(id) => {
            Ok(conflict_item(item, "both_changed", Some(id.clone())))
        }
        CourseOriginState::TargetUnavailable(id) => {
            Ok(conflict_item(item, "target_unavailable", Some(id.clone())))
        }
    }
}

fn series_receipt(
    series: &SourceSeries,
    action: &str,
    membership: &str,
    code: Option<&str>,
) -> String {
    let mut receipt = json!({
        "action": action,
        "membership": membership,
        "sourceId": series.id,
    });
    if let Some(code) = code {
        receipt["code"] = json!(code);
    }
    receipt.to_string()
}

fn apply_series(
    db: &Connection,
    series: &SourceSeries,
    courses: &[ItemOutcome],
) -> Result<SeriesOutcome, String> {
    let origin = series.origin.as_ref().ok_or("课程来源身份无效")?;
    let description = series.description.clone().unwrap_or_default();
    match origin {
        CourseOriginState::Unchanged(id) => Ok(SeriesOutcome {
            source_id: series.id.clone(),
            status: "committed",
            conflict_code: None,
            target_id: Some(id.clone()),
            receipt: series_receipt(series, "unchanged", "kept", None),
            source_hash: series.source_series_hash.clone(),
            record: false,
        }),
        CourseOriginState::SourceChanged(id) => {
            conflict_series(series, "source_changed", Some(id.clone()))
        }
        CourseOriginState::TargetChanged(id) => {
            conflict_series(series, "target_changed", Some(id.clone()))
        }
        CourseOriginState::BothChanged(id) => {
            conflict_series(series, "both_changed", Some(id.clone()))
        }
        CourseOriginState::TargetUnavailable(id) => {
            conflict_series(series, "target_unavailable", Some(id.clone()))
        }
        CourseOriginState::Unmapped => {
            let newly_mapped = series.items.iter().all(|item| {
                courses.iter().any(|outcome| {
                    outcome.source_id == item.id
                        && outcome.status == "committed"
                        && outcome.record
                        && matches!(item.origin, Some(CourseOriginState::Unmapped))
                })
            });
            if !newly_mapped || !acceptable_series(&series.title, &description) {
                return conflict_series(
                    series,
                    if newly_mapped {
                        "invalid_series"
                    } else {
                        "membership_withheld"
                    },
                    None,
                );
            }
            let series_id = Uuid::new_v4().to_string();
            let stamped = now();
            db.execute(
                "INSERT INTO learning_series(id,name,description,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
                params![series_id, series.title, description, stamped],
            )
            .map_err(|_| "无法提交课程迁入")?;
            if !series.items.is_empty() {
                let bumped = db
                    .execute(
                        "UPDATE learning_series SET revision=1, updated_at=?1 WHERE id=?2 AND revision=0",
                        params![now(), series_id],
                    )
                    .map_err(|_| "无法提交课程迁入")?;
                if bumped != 1 {
                    return Err("无法提交课程迁入".into());
                }
                for (position, item) in series.items.iter().enumerate() {
                    let target = courses
                        .iter()
                        .find(|outcome| outcome.source_id == item.id)
                        .and_then(|outcome| outcome.target_id.as_deref())
                        .ok_or("无法提交课程迁入")?;
                    db.execute(
                        "INSERT INTO learning_series_courses(series_id,course_id,position) VALUES(?1,?2,?3)",
                        params![series_id, target, position as i64],
                    )
                    .map_err(|_| "无法提交课程迁入")?;
                }
            }
            Ok(SeriesOutcome {
                source_id: series.id.clone(),
                status: "committed",
                conflict_code: None,
                target_id: Some(series_id),
                receipt: series_receipt(series, "created", "applied", None),
                source_hash: series.source_series_hash.clone(),
                record: true,
            })
        }
    }
}

fn conflict_series(
    series: &SourceSeries,
    code: &'static str,
    target_id: Option<String>,
) -> Result<SeriesOutcome, String> {
    Ok(SeriesOutcome {
        source_id: series.id.clone(),
        status: "conflict",
        conflict_code: Some(code),
        target_id,
        receipt: series_receipt(series, "conflict", "withheld", Some(code)),
        source_hash: series.source_series_hash.clone(),
        record: false,
    })
}

fn insert_entry(
    db: &Connection,
    batch_id: &str,
    kind: &str,
    source_id: &str,
    status: &str,
    conflict_code: Option<&str>,
    target_id: Option<&str>,
    receipt: &str,
) -> Result<(), String> {
    if receipt.len() > 65_536 {
        return Err("无法保存课程迁入结果".into());
    }
    db.execute(
        "INSERT INTO learning_course_import_entries(id,batch_id,entry_kind,source_id,status,conflict_code,target_id,receipt_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            Uuid::new_v4().to_string(),
            batch_id,
            kind,
            source_id,
            status,
            conflict_code,
            target_id,
            receipt,
            now()
        ],
    )
    .map(|_| ())
    .map_err(|_| "无法保存课程迁入结果".into())
}

fn apply(
    db: &Connection,
    batch: &BatchHead,
    preview: &CourseImportPreview,
) -> Result<(Vec<ItemOutcome>, Vec<SeriesOutcome>), String> {
    let blocked = block_codes(preview);
    let mut courses = Vec::new();
    for series in &preview.series {
        for item in &series.items {
            courses.push(apply_item(db, item, &blocked)?);
        }
    }
    let mut series_outcomes = Vec::new();
    for series in &preview.series {
        series_outcomes.push(apply_series(db, series, &courses)?);
    }
    for item in &courses {
        if !item.record {
            continue;
        }
        let key = CourseOriginKey::new(&batch.source_profile_id, &item.source_id)?;
        let target = item.target_id.as_deref().ok_or("无法提交课程迁入")?;
        course_origins::record_in(db, &key, target, &item.source_hash, &batch.source_epoch)?;
    }
    for series in &series_outcomes {
        if !series.record {
            continue;
        }
        let key = SeriesOriginKey::new(&batch.source_profile_id, &series.source_id)?;
        let target = series.target_id.as_deref().ok_or("无法提交课程迁入")?;
        course_origins::record_series_in(
            db,
            &key,
            target,
            &series.source_hash,
            &batch.source_epoch,
        )?;
    }
    Ok((courses, series_outcomes))
}

fn summary(courses: &[ItemOutcome], series: &[SeriesOutcome]) -> String {
    let count = |status: &str, items: &[ItemOutcome]| {
        items.iter().filter(|item| item.status == status).count()
    };
    let series_count = |status: &str| series.iter().filter(|item| item.status == status).count();
    json!({
        "coursesCommitted": count("committed", courses),
        "courseConflicts": count("conflict", courses),
        "seriesCommitted": series_count("committed"),
        "seriesConflicts": series_count("conflict"),
    })
    .to_string()
}

pub(super) fn stage_course_batch(
    state: &AppState,
    scope: &LibraryScope,
    batch_id: &str,
    source: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<(), String> {
    source_profiles::scoped(state, scope, |db| {
        let batch = prepare(db, scope, batch_id)?;
        if batch.status != "pending" {
            return Err("迁入批次已结束".into());
        }
        ensure_epoch(db, &batch)?;
        let preview = preview_courses_from_profile(source, db, bindings, &batch.source_profile_id)?;
        if preview.series.is_empty() {
            return Err("迁入批次没有条目".into());
        }
        let bytes = decision_bytes(&preview, bindings)?;
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err("课程迁入暂存内容超限".into());
        }
        let payload = String::from_utf8(bytes).map_err(|_| "无法保存课程迁入暂存")?;
        let digest = sha256_hex(payload.as_bytes());
        let tx = begin(db)?;
        lock_batch(&tx, batch_id)?;
        let batch = prepare(&tx, scope, batch_id)?;
        if batch.status != "pending" {
            return Err("迁入批次已结束".into());
        }
        ensure_epoch(&tx, &batch)?;
        if let Some(existing) = load_stage(&tx, batch_id)? {
            if existing.payload_json == payload
                && existing.payload_sha256 == digest
                && existing.source_fingerprint == preview.source_fingerprint
                && existing.target_fingerprint == preview.target_fingerprint
            {
                return Ok(());
            }
            return Err("课程迁入暂存已存在".into());
        }
        tx.execute(
            "INSERT INTO learning_course_import_stage(batch_id,source_fingerprint,target_fingerprint,payload_sha256,payload_json,staged_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                batch_id,
                preview.source_fingerprint,
                preview.target_fingerprint,
                digest,
                payload,
                now()
            ],
        )
        .map_err(|_| "无法保存课程迁入暂存")?;
        tx.commit().map_err(|_| "无法保存课程迁入暂存")?;
        Ok(())
    })
}

pub(super) fn commit_course_batch(
    state: &AppState,
    scope: &LibraryScope,
    batch_id: &str,
    source: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<CourseApplyReceipt, String> {
    source_profiles::scoped(state, scope, |db| {
        let batch = prepare(db, scope, batch_id)?;
        if batch.status != "pending" {
            return load_receipt(db, batch_id);
        }
        ensure_epoch(db, &batch)?;
        let staged = load_stage(db, batch_id)?.ok_or("课程迁入尚未暂存")?;
        if !valid_hash(&staged.payload_sha256)
            || !valid_hash(&staged.source_fingerprint)
            || !valid_hash(&staged.target_fingerprint)
            || sha256_hex(staged.payload_json.as_bytes()) != staged.payload_sha256
        {
            return Err("课程迁入暂存损坏".into());
        }
        let current = preview_courses_from_profile(source, db, bindings, &batch.source_profile_id)?;
        if current.source_fingerprint != staged.source_fingerprint {
            return Err(SOURCE_CHANGED.into());
        }
        if current.target_fingerprint != staged.target_fingerprint {
            return Err(TARGET_CHANGED.into());
        }
        if decision_bytes(&current, bindings)? != staged.payload_json.as_bytes() {
            return Err("课程迁入暂存与预检不一致，请重新暂存".into());
        }
        let tx = begin(db)?;
        lock_batch(&tx, batch_id)?;
        let batch = prepare(&tx, scope, batch_id)?;
        if batch.status != "pending" {
            drop(tx);
            return load_receipt(db, batch_id);
        }
        ensure_epoch(&tx, &batch)?;
        verify_live(&tx, &batch, &current)?;
        let (courses, series) = apply(&tx, &batch, &current)?;
        for item in &courses {
            insert_entry(
                &tx,
                batch_id,
                "course",
                &item.source_id,
                item.status,
                item.conflict_code,
                item.target_id.as_deref(),
                &item.receipt,
            )?;
        }
        for item in &series {
            insert_entry(
                &tx,
                batch_id,
                "series",
                &item.source_id,
                item.status,
                item.conflict_code,
                item.target_id.as_deref(),
                &item.receipt,
            )?;
        }
        let finished = now();
        let updated = tx
            .execute(
                "UPDATE learning_import_batches SET status='committed', finished_at=?1, receipt_json=?2 WHERE id=?3 AND status='pending'",
                params![finished, summary(&courses, &series), batch_id],
            )
            .map_err(|_| "无法提交课程迁入")?;
        if updated != 1 {
            return Err("无法提交课程迁入".into());
        }
        let deleted = tx
            .execute(
                "DELETE FROM learning_course_import_stage WHERE batch_id=?1",
                [batch_id],
            )
            .map_err(|_| "无法提交课程迁入")?;
        if deleted != 1 {
            return Err("无法提交课程迁入".into());
        }
        tx.commit().map_err(|_| "无法提交课程迁入")?;
        load_receipt(db, batch_id)
    })
}

pub(super) fn cancel_course_batch(
    state: &AppState,
    scope: &LibraryScope,
    batch_id: &str,
) -> Result<CourseApplyReceipt, String> {
    source_profiles::scoped(state, scope, |db| {
        let batch = prepare(db, scope, batch_id)?;
        if batch.status != "pending" {
            return load_receipt(db, batch_id);
        }
        let tx = begin(db)?;
        lock_batch(&tx, batch_id)?;
        let batch = prepare(&tx, scope, batch_id)?;
        if batch.status != "pending" {
            drop(tx);
            return load_receipt(db, batch_id);
        }
        tx.execute(
            "DELETE FROM learning_course_import_stage WHERE batch_id=?1",
            [batch_id],
        )
        .map_err(|_| "无法取消课程迁入")?;
        let updated = tx
            .execute(
                "UPDATE learning_import_batches SET status='cancelled', finished_at=?1 WHERE id=?2 AND status='pending'",
                params![now(), batch_id],
            )
            .map_err(|_| "无法取消课程迁入")?;
        if updated != 1 {
            return Err("无法取消课程迁入".into());
        }
        tx.commit().map_err(|_| "无法取消课程迁入")?;
        load_receipt(db, batch_id)
    })
}
