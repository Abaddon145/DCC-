//! The two learning atoms share the existing note document, index and clipboard.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeRange {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningReference {
    pub course_id: String,
    pub media_id: String,
    pub fingerprint: String,
    pub ranges: Vec<TimeRange>,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreservedMarkdown {
    pub archive_item_id: String,
    pub label: String,
}
pub enum LearningNode {
    Reference(LearningReference),
    Preserved(PreservedMarkdown),
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteJump {
    pub course_id: String,
    pub media_id: String,
    pub position_ms: u64,
    pub end_ms: u64,
}
pub fn prepare_jump(
    state: &crate::state::AppState,
    library: &str,
    reference: &LearningReference,
    index: usize,
) -> Result<NoteJump, String> {
    reference.validate()?;
    let range = reference.ranges.get(index).ok_or("课程片段索引无效")?;
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护，请稍后打开引用".into());
    }
    let base = state
        .with_library_if_id(library, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换")?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let course = super::store::course(&db, &reference.course_id)?;
    if course.media_id != reference.media_id {
        return Err("课程与媒体身份不一致，未打开其他视频".into());
    }
    let duration = course
        .duration_ms
        .filter(|d| *d > 0)
        .ok_or("课程时长未知，请先检查视频")?;
    if reference.ranges.iter().any(|range| range.end_ms > duration) {
        return Err("引用超出当前视频时长，未截断跳转".into());
    }
    let file = super::analysis::source_file_id(&db, &course.media_id)?;
    let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
    let prepared = crate::media_access::prepare_media_file(&db, &base, &file, &grants)?;
    let stop = || {
        state.active_library_id().ok().as_deref() != Some(library)
            || state.tasks.maintenance().is_some()
    };
    let (lease, hash) = prepared.open_for_analysis(&stop)?;
    if hash != reference.fingerprint {
        return Err("视频原件已变化，引用保留；请重新确认来源".into());
    }
    drop(lease);
    state
        .with_library_if_id(library, |db, current| {
            let current_course = super::store::course(db, &reference.course_id)?;
            let current_file = super::analysis::source_file_id(db, &current_course.media_id)?;
            if current != base
                || current_course.media_id != course.media_id
                || current_course.duration_ms != course.duration_ms
                || current_file != file
                || !prepared.matches_current(db, current, &file, &grants)
                || state.tasks.maintenance().is_some()
            {
                return Err("素材库、课程、原件或本机访问批准已变化，请重新打开引用".into());
            }
            Ok(())
        })?
        .ok_or("素材库已切换")?;
    Ok(NoteJump {
        course_id: reference.course_id.clone(),
        media_id: reference.media_id.clone(),
        position_ms: range.start_ms,
        end_ms: range.end_ms,
    })
}
fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
pub(super) fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 2000 && !value.chars().any(char::is_control)
}
impl LearningReference {
    pub fn validate(&self) -> Result<(), String> {
        if !uuid(&self.course_id)
            || !uuid(&self.media_id)
            || !label(&self.label)
            || self.fingerprint.len() != 64
            || !self
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.ranges.is_empty()
            || self.ranges.len() > 256
        {
            return Err("课程引用标识、摘要、标题或片段数量无效".into());
        }
        let mut end = 0;
        for range in &self.ranges {
            if range.start_ms < end || range.start_ms >= range.end_ms || range.end_ms > 604800000 {
                return Err("课程引用片段顺序、重叠或时间范围无效".into());
            }
            end = range.end_ms;
        }
        Ok(())
    }
}
pub fn reference_text(reference: &LearningReference) -> String {
    format!(
        "{}\n课程: {}\n媒体: {}\nSHA-256: {}\n片段: {}",
        reference.label,
        reference.course_id,
        reference.media_id,
        reference.fingerprint,
        reference
            .ranges
            .iter()
            .map(|range| format!("{}–{} ms", range.start_ms, range.end_ms))
            .collect::<Vec<_>>()
            .join("; ")
    )
}
pub fn inspect_node(node: &Value) -> Result<Option<LearningNode>, String> {
    let kind = node["type"].as_str().unwrap_or("");
    if !matches!(kind, "learningReference" | "preservedMarkdown") {
        return Ok(None);
    }
    if node
        .get("content")
        .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
        || node.get("text").is_some()
        || node
            .get("marks")
            .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
    {
        return Err("学习引用与保留原文必须为独立内容块".into());
    }
    let mut attrs = node["attrs"]
        .as_object()
        .ok_or("学习内容块属性缺失")?
        .clone();
    for key in ["topicId", "topicDepth", "topicColor"] {
        attrs.remove(key);
    }
    if kind == "learningReference" {
        let reference: LearningReference =
            serde_json::from_value(Value::Object(attrs)).map_err(|_| "课程引用属性格式无效")?;
        reference.validate()?;
        Ok(Some(LearningNode::Reference(reference)))
    } else {
        let block: PreservedMarkdown =
            serde_json::from_value(Value::Object(attrs)).map_err(|_| "保留原文属性格式无效")?;
        if !uuid(&block.archive_item_id) || !label(&block.label) {
            return Err("保留原文标识或标题无效".into());
        }
        Ok(Some(LearningNode::Preserved(block)))
    }
}
pub fn validate_registered(db: &Connection, document: &Value) -> Result<(), String> {
    let mut nodes = vec![document];
    while let Some(node) = nodes.pop() {
        if let Some(LearningNode::Preserved(block)) = inspect_node(node)? {
            let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM learning_note_archive_items i JOIN learning_note_archives a ON a.id=i.archive_id WHERE i.id=?1)",[&block.archive_item_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if !exists {
                return Err("原文片段未登记在当前素材库，请使用包含原文的笔记包重新导入".into());
            }
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    Ok(())
}
pub fn archive_ids(document: &Value) -> Result<std::collections::HashSet<String>, String> {
    crate::knowledge::inspect_document(document)?;
    let mut ids = std::collections::HashSet::new();
    let mut nodes = vec![document];
    while let Some(node) = nodes.pop() {
        if let Some(LearningNode::Preserved(block)) = inspect_node(node)? {
            ids.insert(block.archive_item_id);
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    Ok(ids)
}
pub fn read_preserved(
    db: &Connection,
    base: &std::path::Path,
    note_id: &str,
    item_id: &str,
) -> Result<String, String> {
    let note = crate::knowledge::get(db, note_id)?;
    if !archive_ids(&note.document)?.contains(item_id) {
        let in_history:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_revisions r,json_tree(r.snapshot_json,'$.document') j WHERE r.note_id=?1 AND CASE WHEN j.type='object' THEN json_extract(j.value,'$.type')='preservedMarkdown' AND json_extract(j.value,'$.attrs.archiveItemId')=?2 ELSE 0 END)",rusqlite::params![note_id,item_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !in_history {
            return Err("原文片段不属于当前笔记或其历史".into());
        }
    }
    String::from_utf8(super::note_archive::read_archive_item(db, base, item_id)?)
        .map_err(|_| "原文片段不是有效UTF-8".into())
}
