use super::analysis_models::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub const FORMAT_VERSION: i64 = 1;
pub const TEMPLATE_VERSION: i64 = 1;
pub const TEMPLATE_ID: &str = "general";
pub const PROMPT_VERSION: &str = "dcc-course-v1";

pub(crate) fn list_history(
    db: &Connection,
    course: &str,
    offset: i64,
    limit: i64,
) -> Result<crate::models::Page<CourseAnalysisHistoryItem>, String> {
    uuid(course)?;
    super::store::course(db, course)?;
    let offset = offset.max(0);
    let limit = limit.clamp(1, 50);
    let total = db
        .query_row(
            "SELECT count(*) FROM learning_analyses WHERE course_id=?1",
            [course],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let items=db.prepare("SELECT a.id,a.prompt_version,a.status,substr(a.error,1,2000),a.created_at,(SELECT count(*) FROM learning_analysis_stages s WHERE s.analysis_id=a.id),(SELECT count(*) FROM learning_analysis_stages s WHERE s.analysis_id=a.id AND s.status='completed') FROM learning_analyses a WHERE a.course_id=?1 ORDER BY a.created_at DESC,a.id DESC LIMIT ?2 OFFSET ?3").map_err(|e|e.to_string())?.query_map(params![course,limit,offset],|r| {
        let prompt:String=r.get(1)?;
        Ok(CourseAnalysisHistoryItem { id:r.get(0)?,stage:if prompt==super::analysis_summary::PROMPT_VERSION {CourseAnalysisPhase::Summary}else{CourseAnalysisPhase::Segments},status:r.get(2)?,error:r.get(3)?,created_at:r.get(4)?,total_stages:r.get(5)?,completed_stages:r.get(6)? })
    }).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(crate::models::Page {
        items,
        total,
        offset,
        limit,
    })
}
// ponytail: one bounded immutable JSON snapshot; use separately streamed frame
// files if real course previews need more than16MiB, never truncate sources.
pub(crate) const MAX_SNAPSHOT: u64 = 16 * 1024 * 1024;
/// A restored backup contains history/material, never a transferable paid approval.
pub(crate) fn invalidate_previews(db: &Connection) -> Result<(), String> {
    db.execute(
        "UPDATE learning_analysis_previews SET consumed_at=COALESCE(consumed_at,?1)",
        [chrono::Utc::now().timestamp()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn cached_stage(
    db: &Connection,
    input: &PreparedCourseAnalysis,
    stage: &CourseStagePreview,
) -> Result<Option<String>, String> {
    let p = &input.preview;
    let raw:Option<String>=db.query_row("SELECT s.result_json FROM learning_analysis_stages s JOIN learning_analyses a ON a.id=s.analysis_id WHERE a.course_id=?1 AND a.source_fingerprint=?2 AND a.model=?3 AND a.template_id=?4 AND a.template_version=?5 AND a.prompt_version=?6 AND a.format_version=?7 AND s.stage_key=?8 AND s.input_hash=?8 AND s.start_ms=?9 AND s.end_ms=?10 AND s.status='completed' AND s.result_json IS NOT NULL AND length(CAST(s.result_json AS BLOB))<=1048576 ORDER BY s.updated_at DESC,a.id LIMIT 1",params![p.course_id,p.fingerprint,p.model,p.template_id,p.template_version,p.prompt_version,input.format_version,stage.key,stage.start_ms,stage.end_ms],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let segment = stage_material(input, &stage.key)?;
    let scope = CourseScope {
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        cues: &segment.cues,
        frames: &segment.frames,
    };
    if super::analysis_result::read_saved_result(&raw, &scope).is_err() {
        return Ok(None);
    }
    Ok(Some(raw))
}

fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn uuid(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "课程预览身份无效".into())
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn library_identity(base: &Path) -> Result<(String, String), String> {
    crate::media_access::reject_links(base)?;
    let root = base.canonicalize().map_err(|_| "素材库目录不可用")?;
    let marker_path = root.join("library.json");
    crate::media_access::reject_links(&marker_path)?;
    let mut bytes = Vec::new();
    fs::File::open(marker_path)
        .map_err(|_| "素材库身份缺失")?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("素材库身份文件超限".into());
    }
    let marker: crate::state::LibraryMarker =
        serde_json::from_slice(&bytes).map_err(|_| "素材库身份文件无效")?;
    uuid(&marker.library_id)?;
    Ok((marker.library_id, hash(root.to_string_lossy().as_bytes())))
}
pub fn fingerprint(input: &PreparedCourseAnalysis) -> Result<String, String> {
    let mut material = serde_json::json!({"library":input.library_id,"root":input.root_hash,"course":input.preview.course_id,"file":input.source_file_id,"source":input.source_sha256,"subtitle":input.preview.subtitle_id,"subtitleHash":input.subtitle_sha256,"title":input.title,"duration":input.duration_ms,"segments":input.segments,"frames":input.frames,"model":input.preview.model,"template":input.preview.template_id,"templateVersion":input.preview.template_version,"prompt":input.preview.prompt_version,"format":input.format_version});
    if let Some(summary) = &input.summary {
        material["summary"] = serde_json::to_value(summary).map_err(|_| "汇总材料无法编码")?;
    }
    if let Some(focus) = &input.template_focus {
        material["templateFocus"] = serde_json::json!(focus);
    }
    let bytes = serde_json::to_vec(&material).map_err(|_| "课程材料无法编码")?;
    Ok(hash(bytes))
}

/// Original cue/frame identities remain the authority for both stage types.
pub(crate) fn stage_material(
    input: &PreparedCourseAnalysis,
    key: &str,
) -> Result<CourseSegment, String> {
    if let Some(summary) = &input.summary {
        let group = summary
            .groups
            .iter()
            .find(|g| g.key == key)
            .ok_or("汇总阶段不属于来源快照")?;
        Ok(CourseSegment {
            start_ms: group.start_ms,
            end_ms: group.end_ms,
            input_hash: key.into(),
            cues: input
                .segments
                .iter()
                .flat_map(|s| s.cues.iter())
                .filter(|c| c.start_ms >= group.start_ms && c.end_ms <= group.end_ms)
                .cloned()
                .collect(),
            frames: input
                .frames
                .iter()
                .map(|f| &f.reference)
                .filter(|f| f.time_ms >= group.start_ms && f.time_ms < group.end_ms)
                .cloned()
                .collect(),
        })
    } else {
        input
            .segments
            .iter()
            .find(|s| s.input_hash == key)
            .cloned()
            .ok_or_else(|| "阶段不属于历史材料".into())
    }
}
fn validate(input: &PreparedCourseAnalysis) -> Result<(), String> {
    let p = &input.preview;
    for id in [
        &input.library_id,
        &input.source_file_id,
        &p.course_id,
        &p.preview_id,
    ] {
        uuid(id)?;
    }
    if let Some(id) = &p.subtitle_id {
        uuid(id)?;
    }
    if !digest(&input.root_hash)
        || !digest(&input.source_sha256)
        || !digest(&p.fingerprint)
        || input.subtitle_sha256.as_ref().is_some_and(|v| !digest(v))
        || p.subtitle_id.is_some() != input.subtitle_sha256.is_some()
        || p.expires_at.checked_sub(p.created_at) != Some(86400)
        || input.format_version != FORMAT_VERSION
        || p.model != crate::translation_service::MODEL
        || super::analysis::snapshot_instructions(input).is_err()
        || p.prompt_version
            != if input.summary.is_some() {
                super::analysis_summary::PROMPT_VERSION
            } else {
                PROMPT_VERSION
            }
        || p.stages.is_empty()
        || p.stages.len() > 4096
        || input.segments.is_empty()
        || input.segments.len() > 4096
        || input.frames.len() > 4096 * 6
        || p.frame_ids
            != input
                .frames
                .iter()
                .map(|f| f.reference.id.clone())
                .collect::<Vec<_>>()
        || input.title.len() > 4000
        || p.warnings.len() > 8
        || p.warnings.iter().any(|w| w.len() > 2000)
    {
        return Err("课程预览范围、版本或身份无效，请重新预览".into());
    }
    let materials = if let Some(summary) = &input.summary {
        uuid(&summary.parent_analysis_id)?;
        if !digest(&summary.parent_fingerprint)
            || summary.groups.is_empty()
            || summary.groups.len() > 64
        {
            return Err("汇总谱系或分组无效".into());
        }
        let groups = super::analysis_summary::plan_summary(
            &input.title,
            input.duration_ms,
            summary.groups[0].layer,
            summary.previous_requests,
            summary
                .groups
                .iter()
                .flat_map(|g| g.sources.iter().cloned())
                .map(Ok),
        )?;
        if serde_json::to_vec(&groups).map_err(|_| "汇总无法编码")?
            != serde_json::to_vec(&summary.groups).map_err(|_| "汇总无法编码")?
        {
            return Err("汇总分组或预算与完整材料不一致".into());
        }
        groups
            .into_iter()
            .map(|g| (g.key, g.start_ms, g.end_ms, g.tokens_max))
            .collect::<Vec<_>>()
    } else {
        input
            .segments
            .iter()
            .map(|s| (s.input_hash.clone(), s.start_ms, s.end_ms, 0))
            .collect()
    };
    if p.stages.len() != materials.len() || input.segments.iter().any(|s| s.frames.len() > 6) {
        return Err("课程阶段数量或帧范围无效".into());
    }
    let mut total = 0u64;
    let mut keys = std::collections::HashSet::new();
    for (stage, (key, start_ms, end_ms, tokens)) in p.stages.iter().zip(&materials) {
        if !digest(&stage.key)
            || !keys.insert(&stage.key)
            || &stage.key != key
            || stage.start_ms != *start_ms
            || stage.end_ms != *end_ms
            || stage.start_ms >= stage.end_ms
            || stage.end_ms > input.duration_ms
            || stage.tokens_max == 0
            || stage.tokens_max > 1000000
            || (input.summary.is_some() && stage.tokens_max != *tokens)
        {
            return Err("课程阶段预览无效".into());
        }
        if !stage.cached {
            total = total.checked_add(stage.tokens_max).ok_or("课程预算超限")?;
        }
    }
    if total != p.tokens_max || fingerprint(input)? != p.fingerprint {
        return Err("课程预览指纹或预算不一致".into());
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotRef {
    relative_path: String,
    sha256: String,
}

pub fn save_preview(
    db: &Connection,
    base: &Path,
    input: &PreparedCourseAnalysis,
) -> Result<(), String> {
    validate(input)?;
    if library_identity(base)? != (input.library_id.clone(), input.root_hash.clone()) {
        return Err("预览不属于此素材库位置".into());
    }
    super::store::course(db, &input.preview.course_id)?;
    let bytes = serde_json::to_vec(input).map_err(|_| "课程预览无法编码")?;
    if bytes.len() as u64 > MAX_SNAPSHOT {
        return Err("课程快照超过16MiB，未截断保存".into());
    }
    let sha256 = hash(&bytes);
    let p = &input.preview;
    let relative_path = format!(
        "learning-data/analysis/{}/{}-{sha256}.json",
        p.course_id, p.preview_id
    );
    let directory = base.join("learning-data/analysis").join(&p.course_id);
    crate::media_access::reject_links(&directory)?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    crate::media_access::reject_links(&directory)?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    if !directory.starts_with(base.canonicalize().map_err(|e| e.to_string())?) {
        return Err("快照不在当前库内".into());
    }
    let target = directory.join(format!("{}-{sha256}.json", p.preview_id));
    let mut temp = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&target).map_err(|e| e.to_string())?;
    let locator = serde_json::to_string(&SnapshotRef {
        relative_path,
        sha256,
    })
    .map_err(|e| e.to_string())?;
    if let Err(error)=db.execute("INSERT INTO learning_analysis_previews(id,course_id,fingerprint,input_json,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6)",params![p.preview_id,p.course_id,p.fingerprint,locator,p.created_at,p.expires_at]){
        let _=fs::remove_file(target);
        return Err(error.to_string());
    }
    Ok(())
}
pub fn load_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    course: &str,
    id: &str,
    now: i64,
) -> Result<PreparedCourseAnalysis, String> {
    load_snapshot(db, base, library, course, id, Some(now))
}

fn load_snapshot(
    db: &Connection,
    base: &Path,
    library: &str,
    course: &str,
    id: &str,
    approval_time: Option<i64>,
) -> Result<PreparedCourseAnalysis, String> {
    uuid(library)?;
    uuid(course)?;
    uuid(id)?;
    let (current, root) = library_identity(base)?;
    if current != library {
        return Err("素材库已切换".into());
    }
    super::store::course(db, course)?;
    let (fingerprint,raw,created,expires,consumed):(String,String,i64,i64,Option<i64>)=db.query_row("SELECT fingerprint,input_json,created_at,expires_at,consumed_at FROM learning_analysis_previews WHERE id=?1 AND course_id=?2",params![id,course],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|"此课程没有该预览")?;
    if approval_time.is_some_and(|now| consumed.is_some() || created > now || expires <= now) {
        return Err("课程预览已过期或已使用，请重新确认".into());
    }
    if raw.len() > 2048 {
        return Err("课程快照登记无效".into());
    }
    let locator: SnapshotRef = serde_json::from_str(&raw).map_err(|_| "课程快照登记损坏")?;
    let expected = format!(
        "learning-data/analysis/{course}/{id}-{}.json",
        locator.sha256
    );
    if !digest(&locator.sha256) || locator.relative_path != expected {
        return Err("课程快照路径无效".into());
    }
    let path = super::files::learning_file(base, &locator.relative_path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_SNAPSHOT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SNAPSHOT || hash(&bytes) != locator.sha256 {
        return Err("课程快照缺失、超限或已改变".into());
    }
    let input: PreparedCourseAnalysis =
        serde_json::from_slice(&bytes).map_err(|_| "课程快照结构无效")?;
    validate(&input)?;
    if input.library_id != library
        || input.root_hash != root
        || input.preview.preview_id != id
        || input.preview.course_id != course
        || input.preview.fingerprint != fingerprint
        || input.preview.created_at != created
        || input.preview.expires_at != expires
    {
        return Err("课程快照身份不一致".into());
    }
    Ok(input)
}

pub(crate) fn history(
    db: &Connection,
    base: &Path,
    library: &str,
    course: &str,
    id: &str,
    stage: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(CourseAnalysisDetail, Option<PreparedCourseAnalysis>), String> {
    for value in [library, course, id] {
        uuid(value)?;
    }
    if stage.is_some_and(|key| !digest(key)) {
        return Err("课程阶段身份无效".into());
    }
    super::store::course(db, course)?;
    let mut detail = db.query_row("SELECT preview_id,source_fingerprint,substr(model,1,200),status,substr(error,1,2000),created_at,updated_at FROM learning_analyses WHERE id=?1 AND course_id=?2", params![id,course], |r| Ok(CourseAnalysisDetail {
        id: id.into(), course_id: course.into(), preview_id:r.get(0)?, source_fingerprint:r.get(1)?, model:r.get(2)?, parent_analysis_id:None, status:r.get(3)?, error:r.get(4)?, created_at:r.get(5)?, updated_at:r.get(6)?, stale_reasons: vec![],
        stages: crate::models::Page { items:vec![], total:0, offset:offset.max(0), limit:limit.clamp(1,50) }, selected_stage_key:stage.map(str::to_owned), result:None,
    })).map_err(|_| "此课程没有该分析记录")?;
    detail.stages.total = db
        .query_row(
            "SELECT count(*) FROM learning_analysis_stages WHERE analysis_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    detail.stages.items = db.prepare("SELECT stage_key,start_ms,end_ms,status,tokens_used,substr(error,1,2000) FROM learning_analysis_stages WHERE analysis_id=?1 ORDER BY start_ms,stage_key LIMIT ?2 OFFSET ?3")
        .map_err(|e|e.to_string())?.query_map(params![id,detail.stages.limit,detail.stages.offset], |r| Ok(CourseAnalysisStage { key:r.get(0)?,start_ms:r.get(1)?,end_ms:r.get(2)?,status:r.get(3)?,tokens_used:r.get(4)?,error:r.get(5)? }))
        .map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    let (status, raw): (String, Option<String>) = if let Some(key) = stage {
        db.query_row("SELECT status,CASE WHEN length(CAST(result_json AS BLOB))<=1048576 THEN result_json END FROM learning_analysis_stages WHERE analysis_id=?1 AND stage_key=?2",params![id,key],|r| Ok((r.get(0)?,r.get(1)?))).map_err(|_|"分析记录没有该阶段")?
    } else {
        db.query_row("SELECT status,CASE WHEN length(CAST(result_json AS BLOB))<=1048576 THEN result_json END FROM learning_analyses WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?
    };
    let snapshot = match load_snapshot(db, base, library, course, &detail.preview_id, None) {
        Ok(input)
            if input.preview.fingerprint == detail.source_fingerprint
                && input.preview.model == detail.model =>
        {
            Some(input)
        }
        _ => {
            detail
                .stale_reasons
                .push("历史来源快照缺失、损坏或身份已改变，不能核验结果".into());
            None
        }
    };
    if let Some(input) = &snapshot {
        detail.parent_analysis_id = input.summary.as_ref().map(|s| s.parent_analysis_id.clone());
        let (cues, frames, start_ms, end_ms) = if let Some(key) = stage {
            let segment = stage_material(input, key)?;
            (
                segment.cues.clone(),
                segment.frames.clone(),
                segment.start_ms,
                segment.end_ms,
            )
        } else {
            (
                input
                    .segments
                    .iter()
                    .flat_map(|s| s.cues.iter().cloned())
                    .collect(),
                input.frames.iter().map(|f| f.reference.clone()).collect(),
                0,
                input.duration_ms,
            )
        };
        let scope = CourseScope {
            start_ms,
            end_ms,
            cues: &cues,
            frames: &frames,
        };
        if let Some(raw) = raw {
            match super::analysis_result::read_saved_result(&raw, &scope) {
                Ok(result) => detail.result = Some(result),
                Err(_) => detail
                    .stale_reasons
                    .push("已保存结果未通过本机校验，未作为有效结果显示".into()),
            }
        } else if status == "completed" && stage.is_some() {
            detail
                .stale_reasons
                .push("已完成阶段的结果缺失或超过读取上限".into());
        }
    }
    Ok((detail, snapshot))
}
/// Call only after the service rechecks source leases/hashes. No command accepts
/// PreparedCourseAnalysis or a caller-supplied fingerprint as proof of consent.
pub(crate) fn begin_analysis(
    db: &mut Connection,
    base: &Path,
    input: &PreparedCourseAnalysis,
    now: i64,
) -> Result<String, String> {
    validate(input)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    super::analysis_summary::verify_parent_material(&tx, base, input)?;
    let course = super::store::course(&tx, &input.preview.course_id)?;
    let file: String = tx
        .query_row(
            "SELECT primary_file_id FROM media_entries WHERE id=?1",
            [course.media_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if file != input.source_file_id {
        return Err("课程原件登记已改变，请重新预览".into());
    }
    let p = &input.preview;
    let locator: String = tx
        .query_row(
            "SELECT input_json FROM learning_analysis_previews WHERE id=?1 AND course_id=?2",
            params![p.preview_id, p.course_id],
            |r| r.get(0),
        )
        .map_err(|_| "课程预览不存在")?;
    if locator.len() > 2048 {
        return Err("课程预览登记无效".into());
    }
    let locator: SnapshotRef = serde_json::from_str(&locator).map_err(|_| "课程预览登记无效")?;
    if hash(serde_json::to_vec(input).map_err(|_| "课程预览无法编码")?) != locator.sha256 {
        return Err("确认预览已改变，请重新预览".into());
    }
    if tx.execute("UPDATE learning_analysis_previews SET consumed_at=?1 WHERE id=?2 AND course_id=?3 AND fingerprint=?4 AND consumed_at IS NULL AND created_at<=?1 AND expires_at>?1",params![now,p.preview_id,p.course_id,p.fingerprint]).map_err(|e|e.to_string())?!=1 {return Err("课程预览已过期或已使用，请重新确认".into());}
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO learning_analyses(id,course_id,preview_id,source_fingerprint,model,template_id,template_version,prompt_version,format_version,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'queued',?10,?10)",params![id,p.course_id,p.preview_id,p.fingerprint,p.model,p.template_id,p.template_version,p.prompt_version,input.format_version,now]).map_err(|e|e.to_string())?;
    for stage in &p.stages {
        // Keep one bounded result in memory; a long course can have4096 stages.
        let result = if stage.cached {
            Some(
                cached_stage(&tx, input, stage)?
                    .ok_or("确认过的缓存已失效，请重新预览；未增加付费请求")?,
            )
        } else {
            None
        };
        tx.execute("INSERT INTO learning_analysis_stages(analysis_id,stage_key,start_ms,end_ms,input_hash,status,result_json,tokens_used,updated_at) VALUES(?1,?2,?3,?4,?2,?5,?6,?7,?8)",params![id,stage.key,stage.start_ms,stage.end_ms,if result.is_some(){"completed"}else{"pending"},result,if stage.cached{Some(0)}else{None::<i64>},now]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

pub(crate) fn record_stage(
    db: &mut Connection,
    input: &PreparedCourseAnalysis,
    id: &str,
    key: &str,
    result: Result<&CourseAnalysisResult, &str>,
    tokens: Option<u64>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let (status, raw, error) = match result {
        Ok(result) => {
            let raw = serde_json::to_string(result).map_err(|_| "课程结果无法编码")?;
            if raw.len() > 1024 * 1024 {
                return Err("课程编码结果超过1MiB，未登记为成功缓存".into());
            }
            ("completed", Some(raw), String::new())
        }
        Err(error) => ("failed", None, error.chars().take(2000).collect()),
    };
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let changed = tx.execute("UPDATE learning_analysis_stages SET status=?1,result_json=?2,tokens_used=?3,error=?4,updated_at=?5 WHERE analysis_id=?6 AND stage_key=?7 AND status='pending' AND EXISTS(SELECT 1 FROM learning_analyses a WHERE a.id=?6 AND a.course_id=?8 AND a.source_fingerprint=?9 AND a.status IN ('queued','running'))",params![status,raw,tokens,error,now,id,key,input.preview.course_id,input.preview.fingerprint]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("课程阶段已失效，未覆盖旧结果".into());
    }
    tx.execute(
        "UPDATE learning_analyses SET status='running',updated_at=?1 WHERE id=?2",
        params![now, id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

pub(crate) fn finish_analysis(
    db: &Connection,
    input: &PreparedCourseAnalysis,
    id: &str,
    status: &str,
    error: &str,
) -> Result<(), String> {
    if !["completed", "partial", "failed", "cancelled"].contains(&status) {
        return Err("课程分析状态无效".into());
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut final_result = None;
    if status == "completed" {
        let (total,completed):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(sum(status='completed'),0) FROM learning_analysis_stages WHERE analysis_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
        if total != input.preview.stages.len() || completed != total {
            return Err("仍有未完成阶段，未发布完整结果".into());
        }
        if input.summary.as_ref().is_some_and(|s| s.groups.len() == 1) {
            let key = &input.preview.stages[0].key;
            let raw:Option<String>=tx.query_row("SELECT CASE WHEN length(CAST(result_json AS BLOB))<=1048576 THEN result_json END FROM learning_analysis_stages WHERE analysis_id=?1 AND stage_key=?2",params![id,key],|r|r.get(0)).map_err(|_|"最终汇总阶段缺失")?;
            let raw = raw.ok_or("最终汇总结果缺失或超限")?;
            let material = stage_material(input, key)?;
            super::analysis_result::read_saved_result(
                &raw,
                &CourseScope {
                    start_ms: material.start_ms,
                    end_ms: material.end_ms,
                    cues: &material.cues,
                    frames: &material.frames,
                },
            )?;
            final_result = Some(raw);
        }
    }
    if tx.execute("UPDATE learning_analyses SET status=?1,error=?2,updated_at=?3,result_json=?7 WHERE id=?4 AND course_id=?5 AND source_fingerprint=?6 AND status IN ('queued','running')",params![status,error.chars().take(2000).collect::<String>(),chrono::Utc::now().timestamp(),id,input.preview.course_id,input.preview.fingerprint,final_result]).map_err(|e|e.to_string())?!=1 {return Err("课程分析记录已失效".into());}
    tx.commit().map_err(|e| e.to_string())
}
