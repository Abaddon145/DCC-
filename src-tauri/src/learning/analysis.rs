use super::{analysis_models::*, models::TranscriptCue};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

enum Source<'a> {
    Cue(&'a TranscriptCue),
    Frame(&'a AnalysisFrame),
}
impl Source<'_> {
    fn start(&self) -> u64 {
        match self {
            Self::Cue(c) => c.start_ms,
            Self::Frame(f) => f.time_ms,
        }
    }
    fn id(&self) -> &str {
        match self {
            Self::Cue(c) => &c.id,
            Self::Frame(f) => &f.id,
        }
    }
}

pub fn plan_segments(
    cues: &[TranscriptCue],
    frames: &[AnalysisFrame],
    duration_ms: u64,
) -> Result<Vec<CourseSegment>, String> {
    if duration_ms == 0
        || duration_ms > 604800000
        || cues.len() > 200000
        || frames.len() > 4096 * 6
        || (cues.is_empty() && frames.is_empty())
    {
        return Err("没有可分析材料，或课程范围超限".into());
    }
    let mut ids = HashSet::new();
    let mut previous = 0;
    for cue in cues {
        if cue.id.is_empty()
            || cue.id.len() > 80
            || !ids.insert(&cue.id)
            || cue.subtitle_id.is_empty()
            || cue.start_ms < previous
            || cue.start_ms >= cue.end_ms
            || cue.end_ms > duration_ms
            || cue.text.trim().is_empty()
            || cue.text.len() > 16000
            || cue.text.contains('\0')
        {
            return Err("字幕身份、时间、顺序或正文无效，未截断分段".into());
        }
        previous = cue.start_ms;
    }
    ids.clear();
    for frame in frames {
        if frame.id.is_empty()
            || frame.id.len() > 80
            || !ids.insert(&frame.id)
            || frame.time_ms >= duration_ms
            || frame.sha256.len() != 64
            || !frame.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("画面身份、时间或校验值无效".into());
        }
    }
    let mut sources = cues
        .iter()
        .map(Source::Cue)
        .chain(frames.iter().map(Source::Frame))
        .collect::<Vec<_>>();
    sources.sort_by(|a, b| a.start().cmp(&b.start()).then_with(|| a.id().cmp(b.id())));
    let mut segments: Vec<CourseSegment> = Vec::new();
    let mut text_bytes = 0;
    let mut json_bytes = 0;
    for source in sources {
        let (body_bytes, wire_bytes, end, is_frame) = match &source {
            Source::Cue(cue) => (
                cue.text.len(),
                serde_json::to_vec(cue).map_err(|_| "字幕无法编码")?.len(),
                cue.end_ms,
                false,
            ),
            Source::Frame(frame) => (
                0,
                serde_json::to_vec(frame).map_err(|_| "画面无法编码")?.len(),
                frame.time_ms + 1,
                true,
            ),
        };
        if wire_bytes > 100000 {
            return Err("单项材料无法安全编码，未截断正文".into());
        }
        if segments.last().is_none_or(|last| {
            source.start() >= last.start_ms + 300000
                || text_bytes + body_bytes > 18000
                || json_bytes + wire_bytes + 1 > 100000
                || (is_frame && last.frames.len() == 6)
        }) {
            if segments.len() == 4096 {
                return Err("课程分段超过4096，未返回不完整计划".into());
            }
            segments.push(CourseSegment {
                start_ms: source.start(),
                end_ms: end,
                cues: vec![],
                frames: vec![],
                input_hash: String::new(),
            });
            text_bytes = 0;
            json_bytes = 0;
        }
        let segment = segments.last_mut().ok_or("课程分段未创建")?;
        segment.end_ms = segment.end_ms.max(end);
        text_bytes += body_bytes;
        json_bytes += wire_bytes + 1;
        match source {
            Source::Cue(cue) => segment.cues.push(cue.clone()),
            Source::Frame(frame) => segment.frames.push(frame.clone()),
        }
    }
    for segment in &mut segments {
        // This hashes only complete stage material; the preview/cache identity must
        // additionally bind library, original source, model, template and versions.
        let bytes = serde_json::to_vec(&(
            segment.start_ms,
            segment.end_ms,
            &segment.cues,
            &segment.frames,
        ))
        .map_err(|_| "课程材料无法编码")?;
        segment.input_hash = format!("{:x}", Sha256::digest(bytes));
    }
    Ok(segments)
}
