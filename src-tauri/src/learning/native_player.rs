//! Native playback boundary, adapted from Yingliu c866508 (+8e5147b key contract).
//! No source LearningService, database, task queue or arbitrary mpv command crosses this API.
use crate::{
    learning::models::{Progress, ProgressInput},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    Arc, Mutex,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackTrack {
    pub id: i64,
    pub kind: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub session_id: String,
    pub library_id: String,
    pub course_id: String,
    pub ready: bool,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub paused: bool,
    pub rate: f64,
    pub volume: u8,
    pub muted: bool,
    pub ended: bool,
    pub tracks: Vec<PlaybackTrack>,
    pub audio_id: Option<i64>,
    pub subtitle_id: Option<i64>,
    pub fullscreen: bool,
    pub error: Option<String>,
    pub save_error: Option<String>,
}
pub(crate) struct PlaybackLease {
    library_id: String,
    course_id: String,
    session_id: String,
    epoch: u64,
    valid: AtomicBool,
    last_sequence: AtomicU64,
    revision: AtomicI64,
}
pub(crate) struct PlaybackSession {
    lease: PlaybackLease,
    snapshot: Mutex<PlaybackSnapshot>,
    closed: AtomicBool,
    operations: Mutex<()>,
    viewport: Mutex<NativeViewport>,
    #[cfg(windows)]
    engine: Mutex<Option<Arc<NativeEngine>>>,
}
#[cfg(windows)]
struct NativeEngine {
    _source: crate::media_access::CheckedMediaFile,
    window: super::native_window::NativeWindow,
    ipc: super::native_ipc::NativeIpc,
    process: Arc<crate::runtime::process::OwnedProcess>,
    bindings: Mutex<(String, Vec<NativeBinding>)>,
}
#[cfg(windows)]
impl Drop for NativeEngine {
    fn drop(&mut self) {
        let _ = self.process.terminate();
    }
}
#[derive(Default)]
pub struct NativeRuntime {
    slot: Mutex<Option<Arc<PlaybackSession>>>,
    transition: Mutex<()>,
    epoch: AtomicU64,
    exiting: AtomicBool,
}
#[cfg(windows)]
pub(crate) fn start_playback(
    app: &tauri::AppHandle,
    library: &str,
    course: &str,
) -> Result<PlaybackSnapshot, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    if state.learning_runtime.exiting.load(Ordering::Acquire) {
        return Err("正在保存并退出，请稍候".into());
    }
    let _transition = state
        .learning_runtime
        .transition
        .lock()
        .map_err(|_| "播放切换锁损坏")?;
    state
        .with_library_if_id(library, |db, _| super::store::course(db, course))?
        .ok_or("素材库已切换")?;
    let old = state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if let Some(old) = old {
        if !old.closed.load(Ordering::Acquire)
            && old.lease.library_id == library
            && old.lease.course_id == course
            && old
                .snapshot
                .lock()
                .map_err(|_| "播放状态锁损坏")?
                .error
                .is_none()
        {
            return snapshot(&state, library, &old.lease.session_id);
        }
        finish_session(&state, &old)?;
        detach_session(&state, &old)?;
    }
    let session = claim_session_locked(&state, library, course)?;
    let initial = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = boot_engine(&app, &session) {
            if let Ok(_operation) = session.operations.lock() {
                if !session.closed.load(Ordering::Acquire) {
                    if let Ok(mut value) = session.snapshot.lock() {
                        value.ready = false;
                        value.paused = true;
                        value.error = Some(error.chars().take(512).collect());
                    }
                    session.lease.valid.store(false, Ordering::Release);
                    if let Ok(mut engine) = session.engine.lock() {
                        engine.take();
                    }
                    emit_snapshot(&app, &session);
                }
            }
        }
    });
    Ok(initial)
}
fn active_session(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<Arc<PlaybackSession>, String> {
    if state.active_library_id()? != library {
        return Err("素材库已切换".into());
    }
    state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .as_ref()
        .filter(|s| {
            !s.closed.load(Ordering::Acquire)
                && s.lease.library_id == library
                && s.lease.session_id == id
        })
        .cloned()
        .ok_or_else(|| "播放会话已过期".into())
}
pub(crate) fn snapshot(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<PlaybackSnapshot, String> {
    let session = active_session(state, library, id)?;
    let value = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    Ok(value)
}
#[cfg(windows)]
pub(crate) fn load_subtitle(
    app: &tauri::AppHandle,
    library: &str,
    id: &str,
    subtitle: &str,
) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let session = active_session(&state, library, id)?;
    let _operation = session.operations.lock().map_err(|_| "播放操作锁损坏")?;
    active_session(&state, library, id)?;
    if !session.snapshot.lock().map_err(|_| "播放状态锁损坏")?.ready {
        return Err("播放内核尚未就绪".into());
    }
    let path = state
        .with_library_if_id(library, |db, base| {
            super::subtitles::path(db, base, &session.lease.course_id, subtitle)
        })?
        .ok_or("素材库已切换")?;
    let engine = session
        .engine
        .lock()
        .map_err(|_| "播放内核锁损坏")?
        .clone()
        .ok_or("播放内核不可用")?;
    engine.ipc.request(vec![
        json!("sub-add"),
        json!(path.to_string_lossy()),
        json!("select"),
    ])?;
    Ok(())
}
#[cfg(windows)]
fn emit_snapshot(app: &tauri::AppHandle, session: &PlaybackSession) {
    use tauri::Emitter;
    if let Ok(value) = session.snapshot.lock() {
        let _ = app.emit("learning-playback-state", value.clone());
    }
}
#[cfg(windows)]
pub(crate) fn set_viewport(
    state: &AppState,
    library: &str,
    id: &str,
    viewport: NativeViewport,
) -> Result<(), String> {
    viewport.physical(1.0, 100000, 100000)?;
    let session = active_session(state, library, id)?;
    let _operation = session.operations.lock().map_err(|_| "播放操作锁损坏")?;
    if session.closed.load(Ordering::Acquire) {
        return Err("播放会话已关闭".into());
    }
    *session.viewport.lock().map_err(|_| "播放区域锁损坏")? = viewport;
    if let Some(engine) = session.engine.lock().map_err(|_| "播放内核锁损坏")?.clone() {
        engine.window.update(viewport)?;
    }
    Ok(())
}
#[cfg(windows)]
pub(crate) fn control(
    app: &tauri::AppHandle,
    library: &str,
    id: &str,
    action: PlaybackAction,
) -> Result<PlaybackSnapshot, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护，播放控制已暂停".into());
    }
    let session = active_session(&state, library, id)?;
    let _operation = session.operations.lock().map_err(|_| "播放操作锁损坏")?;
    active_session(&state, library, id)?;
    let old = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if !old.ready || old.error.is_some() {
        return Err("播放内核未就绪，请稍候或重试".into());
    }
    if let PlaybackAction::Completed { completed } = action {
        set_completed(&state, &session, completed)?;
        emit_snapshot(app, &session);
        return snapshot(&state, library, id);
    }
    match &action {
        PlaybackAction::Audio {
            track_id: Some(track),
        }
        | PlaybackAction::Subtitle {
            track_id: Some(track),
        } => {
            let kind = if matches!(action, PlaybackAction::Audio { .. }) {
                "audio"
            } else {
                "sub"
            };
            if !old.tracks.iter().any(|t| t.kind == kind && t.id == *track) {
                return Err("此轨道不属于当前视频".into());
            }
        }
        _ => {}
    }
    let engine = session
        .engine
        .lock()
        .map_err(|_| "播放内核锁损坏")?
        .clone()
        .ok_or("播放内核未就绪")?;
    if let PlaybackAction::Shortcuts { bindings } = &action {
        checked_bindings(bindings)?;
        let mut current = engine.bindings.lock().map_err(|_| "快捷键配置锁损坏")?;
        let nonce = uuid::Uuid::new_v4().to_string();
        let (_, old) = std::mem::replace(&mut *current, (nonce.clone(), vec![]));
        for old in old {
            engine
                .ipc
                .request(vec![json!("keybind"), json!(old.key), json!("ignore")])?;
        }
        for binding in bindings {
            engine.ipc.request(vec![
                json!("keybind"),
                json!(binding.key),
                json!(format!(
                    "script-message stackdcc-command:{nonce}:{}",
                    binding.id
                )),
            ])?;
        }
        current.1 = bindings.clone();
        return snapshot(&state, library, id);
    }
    if let PlaybackAction::Fullscreen { enabled } = action {
        engine.window.set_fullscreen(enabled)?;
        session
            .snapshot
            .lock()
            .map_err(|_| "播放状态锁损坏")?
            .fullscreen = enabled;
    } else {
        for command in action.commands(old.duration_ms)? {
            engine.ipc.request(command)?;
        }
        {
            let mut value = session.snapshot.lock().map_err(|_| "播放状态锁损坏")?;
            match action {
                PlaybackAction::Pause { paused } => value.paused = paused,
                PlaybackAction::Seek { position_ms } => {
                    value.position_ms = position_ms;
                    value.ended = false;
                }
                PlaybackAction::Rate { rate } => value.rate = rate,
                PlaybackAction::Volume { volume } => value.volume = volume,
                PlaybackAction::Mute { muted } => value.muted = muted,
                PlaybackAction::Audio { track_id } => value.audio_id = track_id,
                PlaybackAction::Subtitle { track_id } => value.subtitle_id = track_id,
                _ => {}
            }
        }
        if matches!(action, PlaybackAction::Pause { paused: true }) {
            let time = engine
                .ipc
                .request(vec![json!("get_property"), json!("time-pos")])?;
            apply_event(
                &mut *session.snapshot.lock().map_err(|_| "播放状态锁损坏")?,
                &json!({"event":"property-change","name":"time-pos","data":time}),
            )?;
            persist_snapshot(&state, &session)?;
        }
    }
    emit_snapshot(app, &session);
    snapshot(&state, library, id)
}
#[cfg(all(windows, debug_assertions))]
pub(super) fn native_ids(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<(u32, usize), String> {
    let session = active_session(state, library, id)?;
    let engine = session
        .engine
        .lock()
        .map_err(|_| "播放内核锁损坏")?
        .clone()
        .ok_or("播放内核未就绪")?;
    Ok((engine.process.id(), engine.window.id()))
}
#[cfg(all(windows, debug_assertions))]
pub(super) fn effective_audio_id(state: &AppState, library: &str, id: &str) -> Result<i64, String> {
    let session = active_session(state, library, id)?;
    let engine = session
        .engine
        .lock()
        .map_err(|_| "播放内核锁损坏")?
        .clone()
        .ok_or("播放内核未就绪")?;
    engine
        .ipc
        .request(vec![json!("get_property"), json!("aid")])?
        .as_i64()
        .ok_or_else(|| "实际音轨ID无效".into())
}
#[cfg(all(windows, debug_assertions))]
pub(super) fn effective_position_ms(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<u64, String> {
    let session = active_session(state, library, id)?;
    let engine = session
        .engine
        .lock()
        .map_err(|_| "播放内核锁损坏")?
        .clone()
        .ok_or("播放内核未就绪")?;
    let seconds = engine
        .ipc
        .request(vec![json!("get_property"), json!("time-pos")])?
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or("实际播放时间无效")?;
    Ok((seconds * 1000.0).round() as u64)
}
#[cfg(windows)]
fn boot_engine(app: &tauri::AppHandle, session: &Arc<PlaybackSession>) -> Result<(), String> {
    use std::io::{BufReader, Write};
    use tauri::{Emitter, Manager};
    let state = app.state::<AppState>();
    let (source, prepared, file_id) = checked_session_source(&state, session)?;
    let resources = app.path().resource_dir().ok();
    let exe = crate::runtime::tools::require(resources.as_deref(), "mpv")?;
    if session.closed.load(Ordering::Acquire) {
        return Err("播放准备已取消".into());
    }
    let window = super::native_window::NativeWindow::create(
        app,
        &session.lease.library_id,
        &session.lease.session_id,
        &session.lease.course_id,
    )?;
    let pipe = format!(r"\\.\pipe\stackdcc-{}", uuid::Uuid::new_v4());
    let mut args = [
        "--no-config",
        "--load-scripts=no",
        "--load-auto-profiles=no",
        "--load-console=no",
        "--load-stats-overlay=no",
        "--load-select=no",
        "--demuxer-lavf-o=protocol_whitelist=file",
        "--osc=no",
        "--osd-level=0",
        "--input-default-bindings=no",
        "--input-terminal=no",
        "--idle=yes",
        "--force-window=yes",
        "--keep-open=yes",
        "--access-references=no",
        "--sub-auto=no",
        "--ytdl=no",
        "--hwdec=auto-safe",
        "--vo=gpu",
        "--gpu-api=d3d11",
        "--gpu-context=d3d11",
        "--terminal=no",
        "--input-media-keys=no",
        "--input-doubleclick-time=300",
    ]
    .map(str::to_owned)
    .to_vec();
    args.extend([
        format!("--wid={}", window.id()),
        format!("--input-ipc-server={pipe}"),
    ]);
    let (process, mut out, mut err) = crate::runtime::process::spawn_owned(&exe, &args)?;
    let process = Arc::new(process);
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut out, &mut std::io::sink());
    });
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut err, &mut std::io::sink());
    });
    let stop = || session.closed.load(Ordering::Acquire);
    let ipc = super::native_ipc::NativeIpc::connect(process.clone(), &pipe, &stop)?;
    let mut events = super::native_ipc::open_pipe(&process, &pipe, &stop)?;
    for (id, name) in [
        "time-pos",
        "duration",
        "speed",
        "pause",
        "volume",
        "mute",
        "sid",
        "aid",
        "track-list",
        "eof-reached",
    ]
    .iter()
    .enumerate()
    {
        writeln!(
            events,
            "{}",
            json!({"command":["observe_property",id,name]})
        )
        .map_err(|e| e.to_string())?;
    }
    let engine = Arc::new(NativeEngine {
        _source: source,
        window,
        ipc,
        process,
        bindings: Mutex::new((String::new(), vec![])),
    });
    {
        let _operation = session.operations.lock().map_err(|_| "播放操作锁损坏")?;
        active_session(&state, &session.lease.library_id, &session.lease.session_id)?;
        if state.tasks.maintenance().is_some() {
            return Err("素材库正在维护".into());
        }
        state
            .with_library_if_id(&session.lease.library_id, |db, base| {
                let course = super::store::course(db, &session.lease.course_id)?;
                let current: String = db
                    .query_row(
                        "SELECT primary_file_id FROM media_entries WHERE id=?1",
                        [course.media_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if current != file_id
                    || !prepared.matches_current(
                        db,
                        base,
                        &file_id,
                        &crate::media_access::LocalMediaGrants::new(&state.config_directory()),
                    )
                {
                    return Err("视频登记或访问批准已变化，未发布播放会话".into());
                }
                Ok(())
            })?
            .ok_or("素材库已切换")?;
        *session.engine.lock().map_err(|_| "播放内核锁损坏")? = Some(engine.clone());
        engine
            .window
            .update(*session.viewport.lock().map_err(|_| "播放区域锁损坏")?)?;
    }
    let weak = Arc::downgrade(session);
    let event_app = app.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(events);
        loop {
            let event = super::native_ipc::read_line(&mut reader).and_then(|line| {
                serde_json::from_str::<Value>(&line).map_err(|_| "播放内核返回无效事件".to_string())
            });
            let Some(session) = weak.upgrade() else { break };
            let Ok(_operation) = session.operations.lock() else {
                break;
            };
            if session.closed.load(Ordering::Acquire) {
                break;
            }
            let Ok(mut value) = session.snapshot.lock() else {
                break;
            };
            match event {
                Ok(event) => {
                    if event["event"] == "client-message" {
                        if let Some((nonce, id)) = event["args"][0]
                            .as_str()
                            .and_then(|v| v.strip_prefix("stackdcc-command:"))
                            .and_then(|v| v.split_once(':'))
                        {
                            if let Ok(engine) = session.engine.lock() {
                                if let Some(engine) = engine.as_ref() {
                                    if let Ok(bindings) = engine.bindings.lock() {
                                        if bindings.0 == nonce
                                            && bindings.1.iter().any(|binding| binding.id == id)
                                        {
                                            let _=event_app.emit("learning-playback-shortcut",json!({"libraryId":session.lease.library_id,"sessionId":session.lease.session_id,"courseId":session.lease.course_id,"id":id}));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Err(error) = apply_event(&mut value, &event) {
                        value.error = Some(error);
                    }
                    if value.ready {
                        session.lease.valid.store(true, Ordering::Release);
                    }
                }
                Err(_) => {
                    value.error = Some("播放内核已退出，请重试".into());
                    value.ready = false;
                    break;
                }
            }
        }
    });
    let value = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    for command in [
        vec![json!("set_property"), json!("pause"), json!(true)],
        vec![json!("set_property"), json!("speed"), json!(value.rate)],
        vec![json!("set_property"), json!("volume"), json!(value.volume)],
        vec![json!("set_property"), json!("sid"), json!("no")],
        vec![
            json!("loadfile"),
            json!(engine._source.to_string_lossy()),
            json!("replace"),
            json!(-1),
            json!({"start":(value.position_ms as f64/1000.0).to_string()}),
        ],
    ] {
        if session.closed.load(Ordering::Acquire) {
            return Err("播放准备已取消".into());
        }
        engine.ipc.request(command)?;
    }
    let started = std::time::Instant::now();
    while !session.snapshot.lock().map_err(|_| "播放状态锁损坏")?.ready {
        if session.closed.load(Ordering::Acquire) {
            return Err("播放准备已取消".into());
        }
        if engine.process.exited() || started.elapsed() > std::time::Duration::from_secs(8) {
            return Err("播放内核未能出画，请检查视频格式或修复播放组件".into());
        }
        if let Some(error) = session
            .snapshot
            .lock()
            .map_err(|_| "播放状态锁损坏")?
            .error
            .clone()
        {
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    emit_snapshot(app, session);
    let weak = Arc::downgrade(session);
    let app = app.clone();
    std::thread::spawn(move || {
        let mut persisted = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let Some(session) = weak.upgrade() else { break };
            let Ok(_operation) = session.operations.lock() else {
                break;
            };
            if session.closed.load(Ordering::Acquire) {
                break;
            }
            let state = app.state::<AppState>();
            if active_session(&state, &session.lease.library_id, &session.lease.session_id).is_err()
            {
                break;
            }
            if persisted.elapsed() >= std::time::Duration::from_secs(2) {
                let _ = persist_snapshot(&state, &session);
                persisted = std::time::Instant::now();
            }
            emit_snapshot(&app, &session);
        }
    });
    Ok(())
}
fn persist_snapshot(state: &AppState, session: &PlaybackSession) -> Result<(), String> {
    let snapshot = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if !snapshot.ready || !session.lease.valid.load(Ordering::Acquire) {
        return Ok(());
    }
    let completed = state
        .with_library_if_id(&session.lease.library_id, |db, _| {
            Ok(super::store::course(db, &session.lease.course_id)?.completed)
        })?
        .ok_or("素材库已切换")?;
    let sequence = session
        .lease
        .last_sequence
        .load(Ordering::Acquire)
        .checked_add(1)
        .ok_or("播放进度序列已满")?;
    let result = save_learning_progress(
        state,
        &session.lease,
        sequence,
        ProgressInput {
            position_ms: snapshot
                .duration_ms
                .map_or(snapshot.position_ms, |d| snapshot.position_ms.min(d)),
            duration_ms: snapshot.duration_ms,
            rate: snapshot.rate,
            completed: completed || snapshot.ended,
        },
    );
    session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .save_error = result.as_ref().err().cloned();
    result.map(|_| ())
}
fn set_completed(
    state: &AppState,
    session: &PlaybackSession,
    completed: bool,
) -> Result<(), String> {
    let value = session
        .snapshot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if !value.ready {
        return Err("播放内核未就绪".into());
    }
    let seq = session
        .lease
        .last_sequence
        .load(Ordering::Acquire)
        .checked_add(1)
        .ok_or("播放进度序列已满")?;
    let result = save_learning_progress(
        state,
        &session.lease,
        seq,
        ProgressInput {
            position_ms: value.position_ms,
            duration_ms: value.duration_ms,
            rate: value.rate,
            completed,
        },
    );
    let mut snapshot = session.snapshot.lock().map_err(|_| "播放状态锁损坏")?;
    snapshot.save_error = result.as_ref().err().cloned();
    if result.is_ok() && !completed {
        snapshot.ended = false;
    }
    result.map(|_| ())
}
#[cfg(test)]
pub(crate) fn claim_session(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<Arc<PlaybackSession>, String> {
    let _transition = state
        .learning_runtime
        .transition
        .lock()
        .map_err(|_| "播放切换锁损坏")?;
    claim_session_locked(state, library, id)
}
fn claim_session_locked(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<Arc<PlaybackSession>, String> {
    if state.learning_runtime.exiting.load(Ordering::Acquire) {
        return Err("正在保存并退出，请稍候".into());
    }
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护，未启动播放器".into());
    }
    let course = state
        .with_library_if_id(library, |db, _| super::store::course(db, id))?
        .ok_or("素材库已切换")?;
    let mut slot = state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?;
    if let Some(active) = slot.as_ref().filter(|s| !s.closed.load(Ordering::Acquire)) {
        if active.lease.library_id == library && active.lease.course_id == id {
            return Ok(active.clone());
        }
        return Err("请先停止当前播放会话".into());
    }
    let epoch = state
        .learning_runtime
        .epoch
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
        .map_err(|_| "播放会话序列已满")?
        + 1;
    let session_id = uuid::Uuid::new_v4().to_string();
    let snapshot = PlaybackSnapshot {
        session_id: session_id.clone(),
        library_id: library.into(),
        course_id: id.into(),
        ready: false,
        position_ms: if course.completed {
            0
        } else {
            course.position_ms
        },
        duration_ms: course.duration_ms,
        paused: true,
        rate: course.rate,
        volume: 80,
        muted: false,
        ended: false,
        tracks: vec![],
        audio_id: None,
        subtitle_id: None,
        fullscreen: false,
        error: None,
        save_error: None,
    };
    let session = Arc::new(PlaybackSession {
        lease: PlaybackLease {
            library_id: library.into(),
            course_id: id.into(),
            session_id,
            epoch,
            valid: AtomicBool::new(false),
            last_sequence: AtomicU64::new(0),
            revision: AtomicI64::new(course.progress_revision),
        },
        snapshot: Mutex::new(snapshot),
        closed: AtomicBool::new(false),
        operations: Mutex::new(()),
        viewport: Mutex::new(NativeViewport {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            visible: false,
        }),
        #[cfg(windows)]
        engine: Mutex::new(None),
    });
    *slot = Some(session.clone());
    Ok(session)
}
pub(crate) fn save_learning_progress(
    state: &AppState,
    lease: &PlaybackLease,
    sequence: u64,
    input: ProgressInput,
) -> Result<Progress, String> {
    if !input.rate.is_finite()
        || !(0.25..=4.0).contains(&input.rate)
        || input.position_ms > i64::MAX as u64
        || input
            .duration_ms
            .is_some_and(|d| d > i64::MAX as u64 || input.position_ms > d)
    {
        return Err("播放进度范围无效".into());
    }
    state.with_library_if_id(&lease.library_id,|db,_| {
        let current=state.learning_runtime.slot.lock().map_err(|_|"播放状态锁损坏")?;
        if !lease.valid.load(Ordering::Acquire)||!current.as_ref().is_some_and(|s|s.lease.epoch==lease.epoch&&s.lease.session_id==lease.session_id&&s.lease.course_id==lease.course_id&&!s.closed.load(Ordering::Acquire)) {return Err("播放会话已过期，未写入旧进度".into());}
        if sequence==0||sequence<=lease.last_sequence.load(Ordering::Acquire) {return Err("旧进度事件已忽略".into());}
        let old=super::store::course(db,&lease.course_id)?;
        let expected=lease.revision.load(Ordering::Acquire);
        if old.progress_revision!=expected {return Err("课程进度已被其他操作修改，未覆盖".into());}
        let changed=old.position_ms!=input.position_ms||old.duration_ms!=input.duration_ms||old.rate!=input.rate||old.completed!=input.completed;
        let revision=if changed {
            let next=expected.checked_add(1).ok_or("进度修订号已满")?;
            if db.execute("UPDATE learning_courses SET position_ms=?1,duration_ms=?2,rate=?3,completed=?4,progress_revision=?5,updated_at=?6 WHERE id=?7 AND progress_revision=?8",rusqlite::params![input.position_ms as i64,input.duration_ms.map(|d|d as i64),input.rate,input.completed,next,chrono::Utc::now().to_rfc3339(),lease.course_id,expected]).map_err(|e|e.to_string())?!=1 {return Err("课程进度已变化，未覆盖".into());}
            next
        } else {expected};
        lease.last_sequence.store(sequence,Ordering::Release);lease.revision.store(revision,Ordering::Release);
        Ok(Progress{input,revision})
    })?.ok_or_else(||"素材库已切换，未写入旧进度".into())
}
fn finish_session(state: &AppState, session: &PlaybackSession) -> Result<(), String> {
    let _operation = session.operations.lock().map_err(|_| "播放操作锁损坏")?;
    if session.closed.load(Ordering::Acquire) {
        return Ok(());
    }
    #[cfg(windows)]
    let engine = session.engine.lock().map_err(|_| "播放内核锁损坏")?.clone();
    #[cfg(windows)]
    if let Some(engine) = &engine {
        let _ = engine
            .ipc
            .request(vec![json!("set_property"), json!("pause"), json!(true)]);
        if let Ok(value) = engine
            .ipc
            .request(vec![json!("get_property"), json!("time-pos")])
        {
            if let Some(position) = value
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0 && *n <= (i64::MAX as f64 / 1000.0))
            {
                session
                    .snapshot
                    .lock()
                    .map_err(|_| "播放状态锁损坏")?
                    .position_ms = (position * 1000.0).round() as u64;
            }
        }
    }
    persist_snapshot(state, session)
        .map_err(|error| format!("进度未保存，保留会话以便重试：{error}"))?;
    session.closed.store(true, Ordering::Release);
    session.lease.valid.store(false, Ordering::Release);
    #[cfg(windows)]
    if let Some(engine) = engine {
        engine.process.terminate()?;
        engine.window.update(NativeViewport {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            visible: false,
        })?;
        session.engine.lock().map_err(|_| "播放内核锁损坏")?.take();
    }
    Ok(())
}
fn detach_session(state: &AppState, session: &PlaybackSession) -> Result<(), String> {
    let mut slot = state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?;
    if slot.as_ref().is_some_and(|current| {
        current.lease.epoch == session.lease.epoch
            && current.lease.session_id == session.lease.session_id
    }) {
        slot.take();
    }
    Ok(())
}
pub(crate) fn stop_learning_session(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<(), String> {
    if state.active_library_id()? != library {
        return Err("素材库已切换".into());
    }
    let _transition = state
        .learning_runtime
        .transition
        .lock()
        .map_err(|_| "播放切换锁损坏")?;
    let session = state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if let Some(session) = session {
        if session.lease.library_id != library || session.lease.session_id != id {
            return Err("播放会话已切换，未停止新会话".into());
        }
        finish_session(state, &session)?;
        detach_session(state, &session)?;
    }
    Ok(())
}
pub(crate) fn quiesce_for_library_change(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, ()>, String> {
    let transition = state
        .learning_runtime
        .transition
        .lock()
        .map_err(|_| "播放切换锁损坏")?;
    let session = state
        .learning_runtime
        .slot
        .lock()
        .map_err(|_| "播放状态锁损坏")?
        .clone();
    if let Some(session) = session {
        finish_session(state, &session)?;
        detach_session(state, &session)?;
    }
    Ok(transition)
}
pub(crate) fn stop_learning_for_exit(state: &AppState) -> Result<(), String> {
    if state.knowledge_editing.load(Ordering::Acquire) {
        return Err("请先保存笔记或取消退出".into());
    }
    if state.tasks.busy() {
        return Err("请先等待或安全停止后台任务".into());
    }
    let _transition = quiesce_for_library_change(state)?;
    Ok(())
}
pub(crate) fn exit_application(app: &tauri::AppHandle, notes_saved: bool) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    if state.learning_runtime.exiting.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let was_editing = notes_saved && state.knowledge_editing.swap(false, Ordering::AcqRel);
    let result = (|| {
        stop_learning_for_exit(&state)?;
        if state.tasks.busy() || state.knowledge_editing.load(Ordering::Acquire) {
            return Err("退出准备期间有新任务或笔记修改，已取消退出".into());
        }
        app.exit(0);
        Ok(())
    })();
    if result.is_err() {
        if was_editing {
            state.knowledge_editing.store(true, Ordering::Release);
        }
        state
            .learning_runtime
            .exiting
            .store(false, Ordering::Release);
    }
    result
}
fn apply_event(snapshot: &mut PlaybackSnapshot, value: &Value) -> Result<(), String> {
    fn milliseconds(value: &Value) -> Result<Option<u64>, String> {
        if value.is_null() {
            return Ok(None);
        }
        value
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0 && *n < i64::MAX as f64 / 1000.0)
            .map(|n| Some((n * 1000.0).round() as u64))
            .ok_or_else(|| "播放内核时间无效".into())
    }
    match value["event"].as_str() {
        Some("playback-restart") => {
            snapshot.ready = true;
            snapshot.error = None;
        }
        Some("end-file") if value["reason"] == "error" => {
            snapshot.ready = false;
            snapshot.error = Some("无法解码此视频，请检查源文件或重新定位".into());
        }
        Some("property-change") => {
            let data = &value["data"];
            match value["name"].as_str() {
                Some("time-pos") => {
                    if let Some(position) = milliseconds(data)? {
                        snapshot.position_ms =
                            snapshot.duration_ms.map_or(position, |d| position.min(d));
                    }
                }
                Some("duration") => {
                    snapshot.duration_ms = milliseconds(data)?.filter(|d| *d > 0);
                    if let Some(d) = snapshot.duration_ms {
                        snapshot.position_ms = snapshot.position_ms.min(d);
                    }
                }
                Some("pause") => {
                    if let Some(paused) = data.as_bool() {
                        snapshot.paused = paused;
                    }
                }
                Some("eof-reached") => {
                    if let Some(ended) = data.as_bool() {
                        snapshot.ended = ended;
                    }
                }
                Some("mute") => {
                    if let Some(muted) = data.as_bool() {
                        snapshot.muted = muted;
                    }
                }
                Some("speed") => {
                    if let Some(rate) = data
                        .as_f64()
                        .filter(|v| v.is_finite() && (0.25..=4.0).contains(v))
                    {
                        snapshot.rate = rate;
                    }
                }
                Some("volume") => {
                    if let Some(volume) = data
                        .as_f64()
                        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
                    {
                        snapshot.volume = volume.round() as u8;
                    }
                }
                Some("aid") => snapshot.audio_id = data.as_i64().filter(|v| *v >= 0),
                Some("sid") => snapshot.subtitle_id = data.as_i64().filter(|v| *v >= 0),
                Some("track-list") => {
                    snapshot.tracks = data
                        .as_array()
                        .map(|rows| {
                            rows.iter()
                                .take(128)
                                .filter_map(|row| {
                                    let id = row["id"].as_i64().filter(|v| *v >= 0)?;
                                    let kind = row["type"]
                                        .as_str()
                                        .filter(|v| matches!(*v, "audio" | "sub"))?;
                                    let label = row["title"]
                                        .as_str()
                                        .or_else(|| row["lang"].as_str())
                                        .unwrap_or(kind)
                                        .chars()
                                        .filter(|c| !c.is_control())
                                        .take(128)
                                        .collect();
                                    Some(PlaybackTrack {
                                        id,
                                        kind: kind.into(),
                                        label,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}
fn checked_session_source(
    state: &AppState,
    session: &PlaybackSession,
) -> Result<
    (
        crate::media_access::CheckedMediaFile,
        crate::media_access::PreparedMediaFile,
        String,
    ),
    String,
> {
    let current = || {
        !session.closed.load(Ordering::Acquire)
            && state.tasks.maintenance().is_none()
            && state.learning_runtime.slot.lock().is_ok_and(|slot| {
                slot.as_ref().is_some_and(|s| {
                    s.lease.epoch == session.lease.epoch
                        && s.lease.session_id == session.lease.session_id
                })
            })
    };
    if !current() {
        return Err("播放准备已取消或素材库正在维护".into());
    }
    let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
    let (id, prepared) = state
        .with_library_if_id(&session.lease.library_id, |db, base| {
            let course = super::store::course(db, &session.lease.course_id)?;
            let id: String = db
                .query_row(
                    "SELECT primary_file_id FROM media_entries WHERE id=?1",
                    [course.media_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            Ok((
                id.clone(),
                crate::media_access::prepare_media_file(db, base, &id, &grants)?,
            ))
        })?
        .ok_or("素材库已切换")?;
    // The source read/hash and Windows read lease must not hold the library mutex.
    let checked = prepared.open_for_playback(&|| !current())?;
    let valid = state
        .with_library_if_id(&session.lease.library_id, |db, base| {
            super::store::course(db, &session.lease.course_id)?;
            Ok(prepared.matches_current(db, base, &id, &grants))
        })?
        .unwrap_or(false);
    if !valid || !current() {
        return Err("访问批准或播放会话已变化，未启动播放器".into());
    }
    Ok((checked, prepared, id))
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeViewport {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub visible: bool,
}
#[derive(Debug)]
pub struct PhysicalViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
}
impl NativeViewport {
    pub fn physical(
        self,
        scale: f64,
        client_width: i32,
        client_height: i32,
    ) -> Result<PhysicalViewport, String> {
        if ![self.left, self.top, self.width, self.height, scale]
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 100000.0)
            || self.width < 0.0
            || self.height < 0.0
            || scale <= 0.0
            || scale > 16.0
        {
            return Err("播放区域无效".into());
        }
        let (x, y, width, height) = (
            (self.left * scale).round() as i32,
            (self.top * scale).round() as i32,
            (self.width * scale).round() as i32,
            (self.height * scale).round() as i32,
        );
        Ok(PhysicalViewport {
            x,
            y,
            width,
            height,
            visible: self.visible
                && x >= 0
                && y >= 0
                && width > 0
                && height > 0
                && x + width <= client_width.saturating_add(1)
                && y + height <= client_height.saturating_add(1),
        })
    }
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlaybackAction {
    Completed { completed: bool },
    Pause { paused: bool },
    Seek { position_ms: u64 },
    Rate { rate: f64 },
    Volume { volume: u8 },
    Mute { muted: bool },
    Subtitle { track_id: Option<i64> },
    Audio { track_id: Option<i64> },
    FrameStep { direction: String },
    Aspect { ratio: String },
    SubtitleDelay { delay_ms: i64 },
    AbLoop { a_ms: u64, b_ms: u64 },
    ClearAbLoop,
    Fullscreen { enabled: bool },
    Shortcuts { bindings: Vec<NativeBinding> },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeBinding {
    id: String,
    key: String,
}
fn checked_bindings(bindings: &[NativeBinding]) -> Result<(), String> {
    const ACTIONS: &[&str] = &[
        "player.toggle",
        "player.seek-back",
        "player.seek-forward",
        "player.fullscreen",
        "player.seek-back-arrow",
        "player.seek-forward-arrow",
        "player.seek-back-large",
        "player.seek-forward-large",
        "player.mute",
        "player.fullscreen-legacy",
        "player.next",
        "player.previous",
        "player.exit-fullscreen",
    ];
    if bindings.len() > 128 {
        return Err("播放快捷键过多".into());
    }
    let mut seen = std::collections::HashSet::new();
    for binding in bindings {
        if !ACTIONS.contains(&binding.id.as_str())
            || binding.key.len() > 64
            || !seen.insert(&binding.key)
        {
            return Err("快捷键动作未注册或按键重复".into());
        }
        let (raw, key) = if binding.key == "+" {
            ("", "+")
        } else if let Some(raw) = binding.key.strip_suffix("++") {
            (raw, "+")
        } else {
            binding.key.rsplit_once('+').unwrap_or(("", &binding.key))
        };
        let modifiers = if raw.is_empty() {
            vec![]
        } else {
            raw.split('+').collect::<Vec<_>>()
        };
        if modifiers
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != modifiers.len()
            || !modifiers
                .iter()
                .all(|m| ["Ctrl", "Alt", "Shift"].contains(m))
            || !(key.len() == 1
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "!@#$%^&*()_+".contains(c))
                || [
                    "SPACE", "LEFT", "RIGHT", "UP", "DOWN", "ESC", "ENTER", "TAB", "HOME", "END",
                    "BS", "DEL", "PGUP", "PGDWN",
                ]
                .contains(&key)
                || key
                    .strip_prefix('F')
                    .and_then(|n| n.parse::<u8>().ok())
                    .is_some_and(|n| (1..=12).contains(&n)))
        {
            return Err("播放快捷键名称无效".into());
        }
    }
    Ok(())
}
impl PlaybackAction {
    fn commands(&self, duration: Option<u64>) -> Result<Vec<Vec<Value>>, String> {
        let set =
            |property: &str, value: Value| vec![json!("set_property"), json!(property), value];
        Ok(match self {
            Self::Pause { paused } => vec![set("pause", json!(paused))],
            Self::Seek { position_ms }
                if *position_ms <= i64::MAX as u64
                    && duration.is_none_or(|d| *position_ms <= d) =>
            {
                vec![vec![
                    json!("seek"),
                    json!(*position_ms as f64 / 1000.0),
                    json!("absolute+exact"),
                ]]
            }
            Self::Rate { rate } if rate.is_finite() && (0.25..=4.0).contains(rate) => {
                vec![set("speed", json!(rate))]
            }
            Self::Volume { volume } if *volume <= 100 => vec![set("volume", json!(volume))],
            Self::Mute { muted } => vec![set("mute", json!(muted))],
            Self::Subtitle { track_id } if track_id.is_none_or(|n| n >= 0) => {
                vec![set("sid", track_id.map_or(json!("no"), |n| json!(n)))]
            }
            Self::Audio { track_id } if track_id.is_none_or(|n| n >= 0) => {
                vec![set("aid", track_id.map_or(json!("no"), |n| json!(n)))]
            }
            Self::FrameStep { direction } if direction == "forward" => {
                vec![vec![json!("frame-step")]]
            }
            Self::FrameStep { direction } if direction == "backward" => {
                vec![vec![json!("frame-back-step")]]
            }
            Self::Aspect { ratio }
                if ["source", "16:9", "4:3", "fill"].contains(&ratio.as_str()) =>
            {
                vec![
                    set("keepaspect", json!(ratio != "fill")),
                    set(
                        "video-aspect-override",
                        match ratio.as_str() {
                            "16:9" => json!(16.0 / 9.0),
                            "4:3" => json!(4.0 / 3.0),
                            _ => json!("no"),
                        },
                    ),
                ]
            }
            Self::SubtitleDelay { delay_ms } if delay_ms.unsigned_abs() <= 600000 => {
                vec![set("sub-delay", json!(*delay_ms as f64 / 1000.0))]
            }
            Self::AbLoop { a_ms, b_ms } if a_ms < b_ms && duration.is_some_and(|d| *b_ms <= d) => {
                vec![
                    set("ab-loop-a", json!(*a_ms as f64 / 1000.0)),
                    set("ab-loop-b", json!(*b_ms as f64 / 1000.0)),
                ]
            }
            Self::ClearAbLoop => vec![set("ab-loop-a", json!("no")), set("ab-loop-b", json!("no"))],
            _ => return Err("播放操作参数无效，或必须使用窗口专用入口".into()),
        })
    }
}
#[cfg(test)]
mod learning_native_tests {
    use super::*;
    use crate::learning::models::Course;
    fn course_fixture(state: &AppState) -> Course {
        state.with_library(|db,_| {
            let id=uuid::Uuid::new_v4().to_string();let file=uuid::Uuid::new_v4().to_string();
            db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','lease fixture',?2,'now','now')",rusqlite::params![id,file]).unwrap();
            db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4',?3,1000,'now')",rusqlite::params![file,id,format!("media-library/video/{id}/video.mp4")]).unwrap();
            super::super::store::ensure_course(db,&id)
        }).unwrap()
    }
    #[test]
    fn learning_native_session_is_lazy_scoped_and_owned() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        assert!(state.learning_runtime.slot.lock().unwrap().is_none());
        let course = course_fixture(&state);
        assert!(claim_session(&state, &uuid::Uuid::new_v4().to_string(), &course.id).is_err());
        let session = claim_session(&state, &library, &course.id).unwrap();
        assert!(
            !session.snapshot.lock().unwrap().ready,
            "claim is metadata only, not proof of a running native renderer"
        );
        assert_eq!(
            claim_session(&state, &library, &course.id)
                .unwrap()
                .lease
                .session_id,
            session.lease.session_id
        );
        assert!(claim_session(&state, &library, &course_fixture(&state).id).is_err());
    }
    #[test]
    fn learning_progress_ignores_late_epoch_and_sequence() {
        // Authorization/storage model only; the separate IPC component test runs actual mpv.
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let course = course_fixture(&state);
        let old = claim_session(&state, &library, &course.id).unwrap();
        old.lease.valid.store(true, Ordering::Release);
        let input = ProgressInput {
            position_ms: 150,
            duration_ms: Some(1000),
            rate: 1.0,
            completed: false,
        };
        assert_eq!(
            save_learning_progress(&state, &old.lease, 1, input.clone())
                .unwrap()
                .revision,
            1
        );
        assert!(save_learning_progress(
            &state,
            &old.lease,
            1,
            ProgressInput {
                position_ms: 900,
                ..input.clone()
            }
        )
        .is_err());
        assert!(save_learning_progress(
            &state,
            &old.lease,
            2,
            ProgressInput {
                position_ms: 1100,
                ..input.clone()
            }
        )
        .is_err());
        state.learning_runtime.slot.lock().unwrap().take();
        let fresh = claim_session(&state, &library, &course.id).unwrap();
        fresh.lease.valid.store(true, Ordering::Release);
        assert!(save_learning_progress(&state, &old.lease, 2, input.clone()).is_err());
        let saved = save_learning_progress(
            &state,
            &fresh.lease,
            1,
            ProgressInput {
                position_ms: 200,
                ..input
            },
        )
        .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(
            state
                .with_library(|db, _| super::super::store::course(db, &course.id))
                .unwrap()
                .position_ms,
            200
        );
    }
    #[test]
    fn learning_progress_saves_current_position_and_eof_without_a_fake_duration() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let course = course_fixture(&state);
        let session = claim_session(&state, &library, &course.id).unwrap();
        session.lease.valid.store(true, Ordering::Release);
        {
            let mut value = session.snapshot.lock().unwrap();
            value.ready = true;
            value.position_ms = 800;
        }
        persist_snapshot(&state, &session).unwrap();
        session.snapshot.lock().unwrap().position_ms = 200;
        persist_snapshot(&state, &session).unwrap();
        let saved = state
            .with_library(|db, _| super::super::store::course(db, &course.id))
            .unwrap();
        assert_eq!(saved.position_ms, 200);
        assert!(!saved.completed);
        {
            let mut value = session.snapshot.lock().unwrap();
            apply_event(
                &mut value,
                &json!({"event":"property-change","name":"eof-reached","data":true}),
            )
            .unwrap();
        }
        persist_snapshot(&state, &session).unwrap();
        assert!(
            state
                .with_library(|db, _| super::super::store::course(db, &course.id))
                .unwrap()
                .completed
        );
        assert!(PlaybackAction::AbLoop {
            a_ms: 0,
            b_ms: 1000
        }
        .commands(None)
        .is_err());
        assert!(PlaybackAction::Seek { position_ms: 200 }
            .commands(None)
            .is_ok());
        stop_learning_session(&state, &library, &session.lease.session_id).unwrap();
        let resumed = claim_session(&state, &library, &course.id).unwrap();
        assert_eq!(
            resumed.snapshot.lock().unwrap().position_ms,
            0,
            "completed course explicitly restarts from beginning"
        );
    }
    #[test]
    fn learning_progress_explicit_completion_preserves_position_and_can_be_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let course = course_fixture(&state);
        let session = claim_session(&state, &library, &course.id).unwrap();
        session.lease.valid.store(true, Ordering::Release);
        {
            let mut value = session.snapshot.lock().unwrap();
            value.ready = true;
            value.position_ms = 300;
        }
        set_completed(&state, &session, true).expect("explicit completion");
        let saved = state
            .with_library(|db, _| super::super::store::course(db, &course.id))
            .unwrap();
        assert!(saved.completed);
        assert_eq!(saved.position_ms, 300);
        session.snapshot.lock().unwrap().ended = true;
        set_completed(&state, &session, false).unwrap();
        persist_snapshot(&state, &session).unwrap();
        assert!(
            !state
                .with_library(|db, _| super::super::store::course(db, &course.id))
                .unwrap()
                .completed
        );
        session.lease.valid.store(false, Ordering::Release);
        assert!(set_completed(&state, &session, true).is_err());
    }
    #[test]
    fn learning_native_exit_respects_note_save_and_does_not_stop_new_owner() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let course = course_fixture(&state);
        let old = claim_session(&state, &library, &course.id).unwrap();
        state.knowledge_editing.store(true, Ordering::Release);
        assert!(stop_learning_for_exit(&state).is_err());
        assert_eq!(
            claim_session(&state, &library, &course.id)
                .unwrap()
                .lease
                .session_id,
            old.lease.session_id
        );
        state.knowledge_editing.store(false, Ordering::Release);
        stop_learning_for_exit(&state).unwrap();
        stop_learning_session(&state, &library, &old.lease.session_id).unwrap();
        let new = claim_session(&state, &library, &course.id).unwrap();
        assert!(stop_learning_session(&state, &library, &old.lease.session_id).is_err());
        assert_eq!(
            claim_session(&state, &library, &course.id)
                .unwrap()
                .lease
                .session_id,
            new.lease.session_id
        );
        state
            .learning_runtime
            .exiting
            .store(true, Ordering::Release);
        assert!(
            claim_session(&state, &library, &course.id).is_err(),
            "an approved exit must not accept a new playback session"
        );
    }
    #[test]
    fn learning_native_events_keep_integer_time_and_untrusted_tracks_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let session = claim_session(&state, &library, &course_fixture(&state).id).unwrap();
        let mut value = session.snapshot.lock().unwrap();
        apply_event(&mut value, &json!({"event":"file-loaded"})).unwrap();
        assert!(
            !value.ready,
            "loading metadata alone is not a presented video frame"
        );
        apply_event(&mut value, &json!({"event":"playback-restart"})).unwrap();
        assert!(value.ready);
        apply_event(
            &mut value,
            &json!({"event":"property-change","name":"duration","data":2.125}),
        )
        .unwrap();
        apply_event(
            &mut value,
            &json!({"event":"property-change","name":"time-pos","data":1.2346}),
        )
        .unwrap();
        assert_eq!((value.position_ms, value.duration_ms), (1235, Some(2125)));
        assert!(apply_event(
            &mut value,
            &json!({"event":"property-change","name":"time-pos","data":-1})
        )
        .is_err());
        assert_eq!(value.position_ms, 1235);
        apply_event(
            &mut value,
            &json!({"event":"property-change","name":"duration","data":null}),
        )
        .unwrap();
        assert_eq!(value.duration_ms, None);
        apply_event(&mut value, &json!({"event":"property-change","name":"track-list","data":[{"id":2,"type":"audio","title":"中文"},{"id":-1,"type":"sub"},{"id":3,"type":"file"}]})).unwrap();
        assert_eq!(value.tracks.len(), 1);
        assert_eq!(value.tracks[0].id, 2);
        assert_eq!(value.tracks[0].label, "中文");
        apply_event(
            &mut value,
            &json!({"event":"end-file","reason":"error","file_error":"secret path"}),
        )
        .unwrap();
        assert!(!value.ready);
        assert!(!value.error.as_ref().unwrap().contains("secret"));
    }
    #[test]
    fn learning_native_library_change_flush_failure_keeps_original_library() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(&dir.path().join("old"));
        let library = state.active_library_id().unwrap();
        let course = course_fixture(&state);
        let session = claim_session(&state, &library, &course.id).unwrap();
        session.lease.valid.store(true, Ordering::Release);
        session.snapshot.lock().unwrap().ready = true;
        // A stale progress revision must block the actual library-changing entry point.
        state
            .with_library(|db, _| {
                db.execute(
                    "UPDATE learning_courses SET progress_revision=9 WHERE id=?1",
                    [&course.id],
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
        let request = crate::models::StorageChangeRequest {
            mode: "create".into(),
            path: dir.path().join("new").to_string_lossy().into(),
            name: Some("new".into()),
        };
        assert!(state.change_library(request).is_err());
        assert_eq!(state.active_library_id().unwrap(), library);
        assert!(!session.closed.load(Ordering::Acquire));
        assert!(session.snapshot.lock().unwrap().save_error.is_some());
    }
    #[test]
    fn learning_native_invalid_library_target_does_not_stop_playback() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let library = state.active_library_id().unwrap();
        let session = claim_session(&state, &library, &course_fixture(&state).id).unwrap();
        let request = crate::models::StorageChangeRequest {
            mode: "open".into(),
            path: dir.path().to_string_lossy().into(),
            name: None,
        };
        assert!(state.change_library(request).is_err());
        assert!(
            !session.closed.load(Ordering::Acquire),
            "an invalid library target must not stop the current session"
        );
    }
    #[test]
    fn learning_native_source_validation_holds_read_lease_and_rejects_stale_owner() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let source = dir.path().join("course.mp4");
        crate::runtime::process::capture(
            &crate::runtime::tools::require(None, "ffmpeg").unwrap(),
            &[
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=0.1",
                "-c:v",
                "mpeg4",
                "-n",
                source.to_str().unwrap(),
            ]
            .map(str::to_owned),
            std::time::Duration::from_secs(20),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        let library = state.active_library_id().unwrap();
        let preview =
            crate::media_access::preview_external_video(&state, &library, &source, &|| false)
                .unwrap();
        let media =
            crate::media_access::commit_external_video(&state, &library, &preview.preview_id)
                .unwrap();
        let course = state
            .with_library(|db, _| super::super::store::ensure_course(db, &media))
            .unwrap();
        let session = claim_session(&state, &library, &course.id).unwrap();
        let (checked, _, _) = checked_session_source(&state, &session).unwrap();
        assert!(checked.is_file());
        assert!(std::fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .is_err());
        assert!(
            state.active.try_lock().is_ok(),
            "source IO must release database mutex"
        );
        drop(checked);
        stop_learning_session(&state, &library, &session.lease.session_id).unwrap();
        assert!(checked_session_source(&state, &session).is_err());
        let fresh = claim_session(&state, &library, &course.id).unwrap();
        std::fs::write(&source, b"changed").unwrap();
        assert!(checked_session_source(&state, &fresh).is_err());
    }
    #[test]
    fn learning_native_controls_have_bounded_fixed_commands() {
        let seek: PlaybackAction =
            serde_json::from_value(json!({"type":"seek","positionMs":1250})).unwrap();
        assert_eq!(
            seek.commands(Some(2000)).unwrap(),
            vec![vec![json!("seek"), json!(1.25), json!("absolute+exact")]]
        );
        assert!(seek.commands(Some(1000)).is_err());
        for value in [
            json!({"type":"volume","volume":101}),
            json!({"type":"rate","rate":5}),
            json!({"type":"audio","trackId":-1}),
            json!({"type":"subtitleDelay","delayMs":600001}),
            json!({"type":"frameStep","direction":"run"}),
            json!({"type":"pause","paused":true,"command":["run","evil"]}),
            json!({"type":"loadSubtitle","path":"C:/secret"}),
        ] {
            assert!(serde_json::from_value::<PlaybackAction>(value)
                .map_or(true, |v| v.commands(Some(2000)).is_err()));
        }
        let loop_action = PlaybackAction::AbLoop {
            a_ms: 100,
            b_ms: 1900,
        };
        assert!(loop_action.commands(None).is_err());
        assert_eq!(loop_action.commands(Some(2000)).unwrap().len(), 2);
        assert!(PlaybackAction::Rate { rate: f64::NAN }
            .commands(None)
            .is_err());
    }
    #[test]
    fn learning_native_bindings_keep_function_keys_and_reject_injected_commands() {
        let keys = [
            "F1",
            "F2",
            "F3",
            "F4",
            "F5",
            "F6",
            "F7",
            "F8",
            "F9",
            "F10",
            "F11",
            "F12",
            "Ctrl+Shift+F11",
            "Ctrl++",
            "Ctrl+!",
            "Shift+LEFT",
            "M",
            "SPACE",
        ];
        for key in keys {
            checked_bindings(&[NativeBinding {
                id: "player.fullscreen".into(),
                key: key.into(),
            }])
            .unwrap();
        }
        for key in [
            "f11",
            "F13",
            "Ctrl+Ctrl+F11",
            "Ctrl+F11\nrun evil",
            "Meta+F11",
            "",
            "Ctrl+; run",
        ] {
            assert!(checked_bindings(&[NativeBinding {
                id: "player.fullscreen".into(),
                key: key.into()
            }])
            .is_err());
        }
        assert!(checked_bindings(&[NativeBinding {
            id: "player.fullscreen; run evil".into(),
            key: "F11".into()
        }])
        .is_err());
        assert!(checked_bindings(&[NativeBinding {
            id: "knowledge.delete".into(),
            key: "DEL".into()
        }])
        .is_err());
        assert!(checked_bindings(&[
            NativeBinding {
                id: "player.fullscreen".into(),
                key: "F11".into()
            },
            NativeBinding {
                id: "player.toggle".into(),
                key: "F11".into()
            }
        ])
        .is_err());
    }
    #[test]
    fn learning_native_viewport_scales_once_and_hides_out_of_bounds() {
        let v = NativeViewport {
            left: 10.0,
            top: 20.0,
            width: 300.0,
            height: 200.0,
            visible: true,
        };
        let p = v.physical(1.25, 1000, 800).unwrap();
        assert_eq!((p.x, p.y, p.width, p.height), (13, 25, 375, 250));
        assert!(p.visible);
        assert!(
            !NativeViewport { left: -1.0, ..v }
                .physical(1.0, 1000, 800)
                .unwrap()
                .visible
        );
        assert!(NativeViewport {
            width: f64::NAN,
            ..v
        }
        .physical(1.0, 1000, 800)
        .is_err());
    }
}
