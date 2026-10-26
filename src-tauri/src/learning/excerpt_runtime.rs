//! Backend-only prepared material; persistence and user confirmation follow separately.
use super::excerpts::{ExcerptSelection, ExcerptText};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcerptFrame {
    pub time_ms: u64,
    pub sha256: String,
    pub data_url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedExcerpt {
    pub library_id: String,
    pub root_hash: String,
    pub source_file_id: String,
    pub source_sha256: String,
    pub duration_ms: u64,
    pub text: ExcerptText,
    pub document: serde_json::Value,
    pub frame: Option<ExcerptFrame>,
    pub warnings: Vec<String>,
}
pub fn prepare_in_library(
    state: &AppState,
    library: &str,
    course: &str,
    selection: &ExcerptSelection,
    resources: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<PreparedExcerpt, String> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let stopped = || {
        cancelled()
            || state.tasks.maintenance().is_some()
            || super::analysis::source_scope_changed(state, library, None)
    };
    if stopped() {
        return Err("摘录准备已停止或素材库正在维护".into());
    }
    let live =
        super::analysis::live_source_controlled(state, library, course, None, false, cancelled)?;
    let text = super::excerpts::prepare_text(&live.db, &live.base, course, selection)?;
    let document = super::excerpts::document(&text, &live.sha)?;
    let (identity, root_hash) = super::analysis_store::library_identity(&live.base)?;
    if identity != library {
        return Err("素材库身份已变化".into());
    }
    let mut warnings = vec![];
    let frame = if let Some(at) = text.frame_time_ms {
        let result = (|| {
            let ffmpeg = crate::runtime::tools::require(resources, "ffmpeg")?;
            let data_url =
                crate::media_engine::frame::sample_jpeg(&ffmpeg, &live.lease, at, &stopped)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(
                    data_url
                        .strip_prefix("data:image/jpeg;base64,")
                        .ok_or("截图编码无效")?,
                )
                .map_err(|_| "截图编码无效")?;
            Ok::<_, String>(ExcerptFrame {
                time_ms: at,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                data_url,
            })
        })();
        match result {
            Ok(frame) => Some(frame),
            Err(error) => {
                warnings.push(format!("截图失败，文字草稿已保留：{error}"));
                None
            }
        }
    } else {
        None
    };
    if stopped() {
        return Err("摘录准备已停止，未发布结果".into());
    }
    if let Some(id) = &text.subtitle_id {
        if Some(super::subtitles::verified_cues(&live.db, &live.base, course, id)?.0)
            != text.subtitle_sha256
        {
            return Err("摘录字幕已变化，请重新选择".into());
        }
    }
    // No large reads or source leases while waiting for the active-library lock.
    drop(live.lease);
    state
        .with_library_if_id(library, |db, base| {
            let current = super::store::course(db, course)?;
            let file = super::analysis::source_file_id(db, &current.media_id)?;
            let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
            if base != live.base
                || current.media_id != text.media_id
                || current.title != text.title
                || current.duration_ms != live.course.duration_ms
                || file != live.file_id
                || !live.prepared.matches_current(db, base, &file, &grants)
                || super::analysis_store::library_identity(base)?
                    != (library.to_owned(), root_hash.clone())
                || state.tasks.maintenance().is_some()
            {
                return Err("素材库、课程或原件登记已变化，未返回旧结果".into());
            }
            if let Some(id) = &text.subtitle_id {
                let sha: String = db
                    .query_row(
                        "SELECT sha256 FROM learning_subtitles WHERE id=?1 AND course_id=?2",
                        rusqlite::params![id, course],
                        |r| r.get(0),
                    )
                    .map_err(|_| "字幕登记已变化")?;
                if Some(sha) != text.subtitle_sha256 {
                    return Err("字幕登记已变化".into());
                }
            }
            Ok(())
        })?
        .ok_or("素材库已切换，未返回旧结果")?;
    if stopped() {
        return Err("摘录准备已停止，未返回结果".into());
    }
    Ok(PreparedExcerpt {
        library_id: library.into(),
        root_hash,
        source_file_id: live.file_id,
        source_sha256: live.sha,
        duration_ms: live.course.duration_ms.ok_or("课程时长未知")?,
        text,
        document,
        frame,
        warnings,
    })
}
