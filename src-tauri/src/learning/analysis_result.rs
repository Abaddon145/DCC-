//! Source-reference behavior adapted from Yingliu c866508; see YINGLIU-LICENSE.txt.
use super::analysis_models::*;
use std::collections::HashSet;
pub fn read_saved_result(
    raw: &str,
    scope: &CourseScope<'_>,
) -> Result<CourseAnalysisResult, String> {
    if raw.len() > 1024 * 1024 {
        return Err("课程缓存超过限制".into());
    }
    let saved: CourseAnalysisResult = serde_json::from_str(raw).map_err(|_| "课程缓存结构无效")?;
    let mut checked = parse_course_result(raw, scope)?;
    if serde_json::to_vec(&saved.sections).map_err(|e| e.to_string())?
        != serde_json::to_vec(&checked.sections).map_err(|e| e.to_string())?
        || serde_json::to_vec(&saved.key_points).map_err(|e| e.to_string())?
            != serde_json::to_vec(&checked.key_points).map_err(|e| e.to_string())?
    {
        return Err("课程缓存包含失效来源，不会静默重写旧结果".into());
    }
    if saved.section_validation.len() > 240 {
        return Err("课程缓存诊断超限".into());
    }
    let mut seen = HashSet::new();
    for report in &saved.section_validation {
        if report.stage != "parse"
            || report.scope_start_ms != scope.start_ms
            || report.scope_end_ms != scope.end_ms
            || report.item_index >= 20
            || !["corrected", "omitted"].contains(&report.disposition.as_str())
            || !seen.insert((&report.section_id, report.item_index))
            || !saved.sections.iter().any(|s| {
                s.id == report.section_id
                    && (report.disposition != "corrected"
                        || s.items.iter().any(|i| i.title == report.item_title))
            })
        {
            return Err("课程缓存本机诊断无效".into());
        }
        text(&report.item_title, 200, true)?;
        text(&report.reason, 500, true)?;
    }
    checked.section_validation = saved.section_validation;
    Ok(checked)
}
fn text(value: &str, max: usize, required: bool) -> Result<(), String> {
    if (required && value.trim().is_empty()) || value.chars().count() > max || value.contains('\0')
    {
        return Err("课程结果文本为空、含无效字符或超过限制".into());
    }
    Ok(())
}
fn strings(values: &[String], max: usize, len: usize) -> Result<(), String> {
    if values.len() > max {
        return Err("课程结果条目过多".into());
    }
    for value in values {
        text(value, len, true)?;
    }
    Ok(())
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn time(at: u64, s: &CourseScope<'_>) -> Result<(), String> {
    if at < s.start_ms || at > s.end_ms {
        Err("课程结果时间超出真实范围".into())
    } else {
        Ok(())
    }
}
fn refs_at(item: &PurposeItem, s: &CourseScope<'_>) -> bool {
    (!item.source_segment_ids.is_empty() || !item.keyframe_ids.is_empty())
        && item
            .source_segment_ids
            .iter()
            .all(|id| s.cues.iter().any(|c| &c.id == id))
        && item
            .keyframe_ids
            .iter()
            .all(|id| s.frames.iter().any(|f| &f.id == id))
        && item.time_ms.is_none_or(|at| {
            time(at, s).is_ok()
                && (s.cues.iter().any(|c| {
                    item.source_segment_ids.contains(&c.id) && at >= c.start_ms && at <= c.end_ms
                }) || s
                    .frames
                    .iter()
                    .any(|f| item.keyframe_ids.contains(&f.id) && at == f.time_ms))
        })
}
pub fn parse_course_result(raw: &str, s: &CourseScope<'_>) -> Result<CourseAnalysisResult, String> {
    if raw.len() > 1024 * 1024 || s.start_ms >= s.end_ms || s.end_ms > 604800000 {
        return Err("课程结果大小或真实范围无效".into());
    }
    let mut ids = HashSet::new();
    if s.cues.iter().any(|c| {
        c.id.is_empty()
            || !ids.insert(&c.id)
            || c.start_ms >= c.end_ms
            || c.start_ms >= s.end_ms
            || c.end_ms <= s.start_ms
    }) {
        return Err("课程字幕来源范围或身份无效".into());
    }
    let mut frames = HashSet::new();
    if s.frames.iter().any(|f| {
        f.id.is_empty()
            || !frames.insert(&f.id)
            || time(f.time_ms, s).is_err()
            || f.sha256.len() != 64
            || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return Err("课程画面来源无效".into());
    }
    let mut json: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "课程分析未返回有效JSON")?;
    if let Some(object) = json.as_object_mut() {
        object.remove("sectionValidation");
        if let Some(steps) = object
            .get_mut("tutorialSteps")
            .and_then(|v| v.as_array_mut())
        {
            for step in steps {
                if let Some(notes) = step.get_mut("notes") {
                    if notes.is_string() {
                        *notes = serde_json::Value::Array(vec![notes.take()]);
                    }
                }
            }
        }
    }
    let mut result: CourseAnalysisResult =
        serde_json::from_value(json).map_err(|_| "课程分析结构或字段类型无效")?;
    if !["tutorial", "lecture", "interview", "documentary", "other"]
        .contains(&result.content_type.as_str())
    {
        return Err("课程内容类型无效".into());
    }
    text(&result.one_sentence_summary, 1000, true)?;
    text(&result.overview, 20000, true)?;
    strings(&result.action_items, 100, 2000)?;
    strings(&result.tags, 32, 64)?;
    strings(&result.warnings, 8, 200)?;
    if result.chapters.len() > 200
        || result.key_points.len() > 200
        || result.tutorial_steps.len() > 200
        || result.terms.len() > 200
        || result.sections.len() > 12
    {
        return Err("课程结果条目过多".into());
    }
    let mut last = s.start_ms;
    for c in &result.chapters {
        time(c.start_ms, s)?;
        time(c.end_ms, s)?;
        if c.start_ms >= c.end_ms || c.start_ms < last {
            return Err("课程章节范围或顺序无效".into());
        }
        last = c.end_ms;
        text(&c.title, 200, true)?;
        text(&c.summary, 6000, true)?;
        strings(&c.keywords, 24, 100)?;
        strings(&c.keyframe_ids, 60, 200)?;
        if c.keyframe_ids
            .iter()
            .any(|id| !s.frames.iter().any(|f| &f.id == id))
        {
            return Err("章节引用了未提供的画面".into());
        }
    }
    for p in &result.key_points {
        text(&p.title, 200, true)?;
        text(&p.content, 6000, true)?;
        strings(&p.source_segment_ids, 32, 200)?;
        if !(1..=5).contains(&p.importance) {
            return Err("重点重要程度无效".into());
        }
    }
    let mut last = s.start_ms;
    for (n, step) in result.tutorial_steps.iter().enumerate() {
        time(step.time_ms, s)?;
        if step.order as usize != n + 1 || step.time_ms < last {
            return Err("教程步骤顺序无效".into());
        }
        last = step.time_ms;
        text(&step.title, 200, true)?;
        text(&step.description, 6000, true)?;
        strings(&step.notes, 32, 2000)?;
    }
    for term in &result.terms {
        time(term.time_ms, s)?;
        text(&term.name, 200, true)?;
        text(&term.definition, 4000, true)?;
    }
    let mut section_ids = HashSet::new();
    let mut bytes = 0;
    let original = result.sections.iter().map(|s| s.items.len()).sum::<usize>();
    let original_points = result.key_points.len();
    for section in &result.sections {
        if !id(&section.id) || !section_ids.insert(&section.id) || section.items.len() > 20 {
            return Err("用途栏目身份重复、无效或条目过多".into());
        }
        text(&section.title, 80, true)?;
        for item in &section.items {
            text(&item.title, 200, true)?;
            text(&item.content, 2000, true)?;
            strings(&item.source_segment_ids, 16, 200)?;
            strings(&item.keyframe_ids, 6, 200)?;
            bytes += item.title.len() + item.content.len();
        }
    }
    if bytes > 96000 {
        return Err("用途栏目总正文超过限制".into());
    }
    let mut report = Vec::new();
    let mut kept = 0;
    for section in &mut result.sections {
        let mut items = Vec::new();
        for (index, mut item) in std::mem::take(&mut section.items).into_iter().enumerate() {
            if !refs_at(&item, s) {
                let known = item
                    .source_segment_ids
                    .iter()
                    .all(|id| s.cues.iter().any(|c| &c.id == id))
                    && item
                        .keyframe_ids
                        .iter()
                        .all(|id| s.frames.iter().any(|f| &f.id == id));
                let mut anchors = s
                    .cues
                    .iter()
                    .filter(|c| item.source_segment_ids.contains(&c.id))
                    .map(|c| c.start_ms.max(s.start_ms))
                    .chain(
                        s.frames
                            .iter()
                            .filter(|f| item.keyframe_ids.contains(&f.id))
                            .map(|f| f.time_ms),
                    )
                    .collect::<Vec<_>>();
                anchors.sort_unstable();
                anchors.dedup();
                let corrected = known && item.time_ms.is_some() && anchors.len() == 1;
                if corrected {
                    item.time_ms = Some(anchors[0]);
                }
                let corrected = corrected && refs_at(&item, s);
                report.push(SectionIssue {
                    stage: "parse".into(),
                    scope_start_ms: s.start_ms,
                    scope_end_ms: s.end_ms,
                    section_id: section.id.clone(),
                    item_index: index,
                    item_title: item.title.clone(),
                    disposition: if corrected { "corrected" } else { "omitted" }.into(),
                    reason: if corrected {
                        "时间已依据唯一真实来源校正"
                    } else {
                        "来源缺失或时间不在引用范围内"
                    }
                    .into(),
                });
                if !corrected {
                    continue;
                }
            }
            kept += 1;
            items.push(item);
        }
        section.items = items;
    }
    result.key_points.retain(|p| {
        let valid = time(p.time_ms, s).is_ok()
            && p.source_segment_ids
                .iter()
                .all(|id| s.cues.iter().any(|c| &c.id == id))
            && (if p.source_segment_ids.is_empty() {
                s.frames.iter().any(|f| f.time_ms == p.time_ms)
            } else {
                s.cues.iter().any(|c| {
                    p.source_segment_ids.contains(&c.id)
                        && p.time_ms >= c.start_ms
                        && p.time_ms <= c.end_ms
                })
            });
        if !valid && result.warnings.len() < 8 {
            result
                .warnings
                .push("部分完成：重点来源或时间无法核对，已略过".into());
        }
        valid
    });
    if (original > 0 || original_points > 0)
        && kept == 0
        && result.chapters.is_empty()
        && result.key_points.is_empty()
        && result.tutorial_steps.is_empty()
        && result.terms.is_empty()
    {
        return Err("用途栏目没有可核对的条目，已成功分段仍可保留".into());
    }
    for issue in &report {
        if result.warnings.len() < 8 {
            result.warnings.push(format!(
                "部分完成：栏目 {} 第 {} 项{}",
                issue.section_id,
                issue.item_index + 1,
                if issue.disposition == "corrected" {
                    "已校正"
                } else {
                    "已略过"
                }
            ));
        }
    }
    result.section_validation = report;
    Ok(result)
}
