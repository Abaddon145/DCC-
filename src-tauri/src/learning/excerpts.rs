//! Local excerpt preparation; publishing and source read leases are separate gates.
use super::{models::TranscriptCue, note_nodes::TimeRange};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcerptSelection {
    pub subtitle_id: Option<String>,
    pub cue_ids: Vec<String>,
    pub ranges: Vec<TimeRange>,
    pub comment: String,
    pub frame_time_ms: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcerptText {
    pub course_id: String,
    pub media_id: String,
    pub title: String,
    pub subtitle_id: Option<String>,
    pub subtitle_sha256: Option<String>,
    pub ranges: Vec<TimeRange>,
    pub cues: Vec<TranscriptCue>,
    pub comment: String,
    pub frame_time_ms: Option<u64>,
}
pub fn prepare_text(
    db: &Connection,
    base: &Path,
    course: &str,
    selection: &ExcerptSelection,
) -> Result<ExcerptText, String> {
    use std::collections::HashSet;
    if selection.cue_ids.len() > 256
        || selection.ranges.len() > 256
        || selection.comment.len() > 20000
        || selection
            .comment
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("摘录段数或备注超限，或备注包含无效控制字符；未截断内容".into());
    }
    let requested: HashSet<_> = selection.cue_ids.iter().map(String::as_str).collect();
    if requested.len() != selection.cue_ids.len()
        || selection
            .cue_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 80 || id.chars().any(char::is_control))
        || selection.subtitle_id.is_some() == selection.cue_ids.is_empty()
    {
        return Err("字幕与选中段不一致，或包含重复/无效段ID".into());
    }
    let course = super::store::course(db, course)?;
    if !super::note_nodes::label(&course.title) {
        return Err("课程标题不能作为有效来源引用".into());
    }
    let duration = course
        .duration_ms
        .filter(|d| *d > 0 && *d <= 604800000)
        .ok_or("课程时长未知或超限，请先检查视频")?;
    let (subtitle_sha256, cues) = if let Some(id) = &selection.subtitle_id {
        let (sha, all) = super::subtitles::verified_cues(db, base, &course.id, id)?;
        let cues: Vec<_> = all
            .into_iter()
            .filter(|cue| requested.contains(cue.id.as_str()))
            .collect();
        if cues.len() != requested.len() {
            return Err("当前课程字幕中不存在部分选中段，未替换为其他原文".into());
        }
        if cues.iter().map(|cue| cue.text.len()).sum::<usize>() > 1024 * 1024 {
            return Err("选中字幕超过1MiB，请减少范围；未截断原文".into());
        }
        (Some(sha), cues)
    } else {
        (None, vec![])
    };
    let ranges = if cues.is_empty() {
        selection.ranges.clone()
    } else {
        let mut ranges: Vec<TimeRange> = Vec::new();
        for cue in &cues {
            if let Some(last) = ranges.last_mut().filter(|last| cue.start_ms <= last.end_ms) {
                last.end_ms = last.end_ms.max(cue.end_ms);
            } else {
                ranges.push(TimeRange {
                    start_ms: cue.start_ms,
                    end_ms: cue.end_ms,
                });
            }
        }
        if !selection.ranges.is_empty() && selection.ranges != ranges {
            return Err("字幕摘录范围必须与完整选中原文一致，不能扩大或截断".into());
        }
        ranges
    };
    if ranges.is_empty() {
        return Err("请选择实际时间范围或完整字幕段".into());
    }
    let mut end = 0;
    for range in &ranges {
        if range.start_ms < end || range.start_ms >= range.end_ms || range.end_ms > duration {
            return Err("摘录范围顺序、重叠或时长无效；未自动截断".into());
        }
        end = range.end_ms;
    }
    if selection.frame_time_ms.is_some_and(|at| {
        at >= duration
            || !ranges
                .iter()
                .any(|range| at >= range.start_ms && at < range.end_ms)
    }) {
        return Err("截图时间不在选中范围内".into());
    }
    Ok(ExcerptText {
        course_id: course.id,
        media_id: course.media_id,
        title: course.title,
        subtitle_id: selection.subtitle_id.clone(),
        subtitle_sha256,
        ranges,
        cues,
        comment: selection.comment.clone(),
        frame_time_ms: selection.frame_time_ms,
    })
}
// The caller must obtain this hash from a controlled media read lease, never IPC input.
pub fn document(
    text: &ExcerptText,
    verified_media_sha256: &str,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let reference = super::note_nodes::LearningReference {
        course_id: text.course_id.clone(),
        media_id: text.media_id.clone(),
        fingerprint: verified_media_sha256.into(),
        ranges: text.ranges.clone(),
        label: text.title.clone(),
    };
    reference.validate()?;
    let bodies = text
        .cues
        .iter()
        .map(|cue| cue.text.as_str())
        .chain(std::iter::once(text.comment.as_str()).filter(|s| !s.is_empty()));
    if bodies
        .clone()
        .map(|body| body.bytes().filter(|byte| *byte == b'\n').count())
        .sum::<usize>()
        > 4096
    {
        return Err("摘录换行超过4096处，请拆分摘录；文字草稿保留，未截断正文".into());
    }
    let mut content = vec![json!({"type":"learningReference","attrs":reference})];
    for body in bodies {
        let mut paragraph = vec![];
        for (index, line) in body.split('\n').enumerate() {
            if index > 0 {
                paragraph.push(json!({"type":"hardBreak"}));
            }
            if !line.is_empty() {
                paragraph.push(json!({"type":"text","text":line}));
            }
        }
        content.push(json!({"type":"paragraph","content":paragraph}));
    }
    Ok(json!({"type":"doc","content":content}))
}
