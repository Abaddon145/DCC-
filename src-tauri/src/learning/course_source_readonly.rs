//! Authorized synthetic course-source reads.
//!
//! Selection, isolated copy, and course preview run only when the sibling marker
//! `dcc-synthetic-course-source.ok` is present. An unmarked file, including a real
//! Yingliu Vault database, is refused before SQLite is opened. This is not a Tauri
//! command and does not accept a path from the frontend.
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
struct SelectedSynthetic {
    scope: LibraryScope,
    profile_id: String,
    epoch: String,
    selection_id: String,
    path: PathBuf,
    digest: [u8; 32],
}

/// In-memory selection for marked synthetic course databases.
#[derive(Default)]
pub(crate) struct SyntheticCourseSession {
    generation: String,
    selected: Option<SelectedSynthetic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyntheticCourseTicket {
    pub selection_id: String,
    pub profile_id: String,
    pub epoch: String,
}

fn session(state: &AppState) -> Result<std::sync::MutexGuard<'_, SyntheticCourseSession>, String> {
    state
        .synthetic_course_source
        .lock()
        .map_err(|_| "来源会话状态不可用".into())
}

fn load(state: &AppState, library: &str, ticket: &str) -> Result<SelectedSynthetic, String> {
    let scope = source_profiles::scope(state, library)?;
    let current = session(state)?;
    let selected = current
        .selected
        .as_ref()
        .ok_or("请重新选择课程来源数据库")?;
    if current.generation != ticket || selected.selection_id != ticket || selected.scope != scope {
        return Err("来源选择已变化，请重新选择".into());
    }
    Ok(selected.clone())
}

fn confirm_epoch(state: &AppState, selected: &SelectedSynthetic) -> Result<(), String> {
    let profile = source_profiles::get(state, &selected.scope, &selected.profile_id)?;
    if profile.access_epoch != selected.epoch {
        return Err("来源状态已变化，请重新选择".into());
    }
    Ok(())
}

/// Register a marked synthetic database. Does not open it with SQLite.
pub(super) fn select_synthetic_course_source(
    state: &AppState,
    library: &str,
    path: &Path,
) -> Result<SyntheticCourseTicket, String> {
    let ticket = uuid::Uuid::new_v4().to_string();
    {
        let mut current = session(state)?;
        current.generation = ticket.clone();
        current.selected = None;
    }
    let scope = source_profiles::scope(state, library)?;
    let digest = super::course_snapshot::fingerprint_synthetic(path)?;
    if session(state)?.generation != ticket {
        return Err("来源选择已变化，请重试".into());
    }
    let profile = source_profiles::create(state, &scope)?;
    let mut current = session(state)?;
    if current.generation != ticket {
        return Err("来源选择已变化，请重试".into());
    }
    current.selected = Some(SelectedSynthetic {
        scope,
        profile_id: profile.id.clone(),
        epoch: profile.access_epoch.clone(),
        selection_id: ticket.clone(),
        path: path.to_path_buf(),
        digest,
    });
    Ok(SyntheticCourseTicket {
        selection_id: ticket,
        profile_id: profile.id,
        epoch: profile.access_epoch,
    })
}

pub(super) fn release_synthetic_course_source(
    state: &AppState,
    library: &str,
    ticket: &str,
) -> Result<bool, String> {
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    if !current
        .selected
        .as_ref()
        .is_some_and(|selected| selected.selection_id == ticket && selected.scope == scope)
    {
        return Ok(false);
    }
    current.generation = uuid::Uuid::new_v4().to_string();
    current.selected = None;
    Ok(true)
}

/// Isolated snapshot plus course preview. The returned preview is not a commit grant.
pub(super) fn preview_synthetic_course_source(
    state: &AppState,
    library: &str,
    ticket: &str,
) -> Result<super::course_apply::CourseStagePreview, String> {
    let selected = load(state, library, ticket)?;
    confirm_epoch(state, &selected)?;
    let snapshot = super::course_snapshot::open_isolated_readonly(
        &selected.path,
        Duration::from_secs(15),
        || {},
    )?;
    let current = load(state, library, ticket)?;
    if current.digest != selected.digest
        || current.path != selected.path
        || current.epoch != selected.epoch
        || current.profile_id != selected.profile_id
    {
        return Err("来源选择已变化，请重新选择".into());
    }
    let digest = super::course_snapshot::fingerprint_synthetic(&selected.path)?;
    if digest != selected.digest {
        return Err("合成课程来源无法只读隔离，请重试".into());
    }
    confirm_epoch(state, &selected)?;
    super::course_apply::preview_course_batch(
        state,
        &selected.scope,
        &selected.profile_id,
        &snapshot,
        &[],
    )
}
