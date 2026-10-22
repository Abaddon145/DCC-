//! Debug-only target-process acceptance. Never initializes the normal app, native
//! messaging, credential cleanup, online services or an installed library.
use crate::state::AppState;
use std::path::PathBuf;
use tauri::Manager;

pub fn run(root: PathBuf) -> Result<(), String> {
    let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../release-artifacts/yingliu-merge")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_absolute()
        || !canonical.starts_with(&allowed)
        || canonical == allowed
        || std::fs::read_dir(&root)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("QA只允许空的工作区证据子目录".into());
    }
    crate::media_access::reject_links(&root)?;
    std::env::set_var("LOCALAPPDATA", root.join("local"));
    let state = AppState::initialize()?;
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    tauri::Builder::default().manage(state).setup(move |app| {
        tauri::WebviewWindowBuilder::new(app,"main",tauri::WebviewUrl::External("about:blank".parse().unwrap()))
            .data_directory(root.join("webview")).title("栈藏 · 隔离原生播放验收").inner_size(1000.0,700.0).build()?;
        let handle=app.handle().clone();
        let root=root.clone();
        std::thread::spawn(move || {
            let result=check(&handle,&root);
            let report=serde_json::json!({"native":result.is_ok(),"kind":"target-native-host-not-workspace-ui","result":result});
            let _=std::fs::write(root.join("results.json"),serde_json::to_vec_pretty(&report).unwrap());
            if report["native"]==true {
                if let Err(error)=super::native_player::exit_application(&handle,false){
                    let _=std::fs::write(root.join("results.json"),serde_json::to_vec_pretty(&serde_json::json!({"native":false,"exitError":error})).unwrap());
                    handle.exit(1);
                }
            }else{handle.exit(1);}
        });
        Ok(())
    }).run(context).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn verify(output: PathBuf) -> Result<(), String> {
    let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../release-artifacts/yingliu-merge")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    if !output.starts_with(&allowed) || output == allowed {
        return Err("QA验证目录不在工作区证据目录内".into());
    }
    crate::media_access::reject_links(&output)?;
    let mut report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(output.join("results.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if report["native"] != true {
        return Err(report.to_string());
    }
    let check = &report["result"]["Ok"];
    for pid in check["pids"].as_array().ok_or("QA缺少进程记录")? {
        if running(pid.as_u64().ok_or("QA进程ID无效")? as u32) {
            return Err("退出后存在未回收的原生进程".into());
        }
    }
    let db = rusqlite::Connection::open_with_flags(
        output.join("local/DCCAssetLibrary/library.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let revision: i64 = db
        .query_row(
            "SELECT progress_revision FROM learning_courses WHERE id=?1",
            [check["exitCourseId"].as_str().ok_or("QA缺少退出课程")?],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if revision
        <= check["exitBeforeRevision"]
            .as_i64()
            .ok_or("QA缺少退出前修订号")?
    {
        return Err("真实退出未提交最后播放进度".into());
    }
    report["postExitVerified"] = serde_json::json!(true);
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn running(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(handle, 5000) == WAIT_TIMEOUT;
        CloseHandle(handle);
        alive
    }
}
fn ready(
    app: &tauri::AppHandle,
    library: &str,
    id: &str,
) -> Result<super::native_player::PlaybackSnapshot, String> {
    let started = std::time::Instant::now();
    loop {
        let value = super::native_player::snapshot(&app.state::<AppState>(), library, id)?;
        if let Some(error) = value.error {
            return Err(error);
        }
        if value.ready {
            return Ok(value);
        }
        if started.elapsed() > std::time::Duration::from_secs(12) {
            return Err("实际目标未进入就绪状态".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
fn check(app: &tauri::AppHandle, root: &std::path::Path) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let library = state.active_library_id()?;
    let source = root.join("synthetic.mp4");
    crate::runtime::process::capture(
        &crate::runtime::tools::require(None, "ffmpeg")?,
        &[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25:duration=10",
            "-c:v",
            "mpeg4",
            "-n",
            source.to_str().ok_or("QA路径无效")?,
        ]
        .map(str::to_owned),
        std::time::Duration::from_secs(30),
        &|| false,
        &mut |_| {},
    )?;
    let preview = crate::media_access::preview_external_video_with_resources(
        &state,
        &library,
        &source,
        &|| false,
        None,
    )?;
    let media = crate::media_access::commit_external_video(&state, &library, &preview.preview_id)?;
    let course = state.with_library(|db, _| super::store::ensure_course(db, &media))?;
    let other = root.join("second.mp4");
    crate::runtime::process::capture(
        &crate::runtime::tools::require(None, "ffmpeg")?,
        &[
            "-nostdin",
            "-v",
            "error",
            "-i",
            source.to_str().unwrap(),
            "-c",
            "copy",
            "-metadata",
            "title=Second",
            "-n",
            other.to_str().unwrap(),
        ]
        .map(str::to_owned),
        std::time::Duration::from_secs(30),
        &|| false,
        &mut |_| {},
    )?;
    let preview2 = crate::media_access::preview_external_video_with_resources(
        &state,
        &library,
        &other,
        &|| false,
        None,
    )?;
    let media2 =
        crate::media_access::commit_external_video(&state, &library, &preview2.preview_id)?;
    let course2 = state.with_library(|db, _| super::store::ensure_course(db, &media2))?;
    let snapshot = super::native_player::start_playback(app, &library, &course.id)?;
    super::native_player::set_viewport(
        &state,
        &library,
        &snapshot.session_id,
        super::native_player::NativeViewport {
            left: 20.0,
            top: 20.0,
            width: 640.0,
            height: 360.0,
            visible: true,
        },
    )?;
    let first_ready = ready(app, &library, &snapshot.session_id)?;
    let (pid, hwnd) = super::native_player::native_ids(&state, &library, &snapshot.session_id)?;
    if pid == 0 || hwnd == 0 {
        return Err("缺少实际原生进程/子窗口".into());
    }
    let again = super::native_player::start_playback(app, &library, &course.id)?;
    if again.session_id != snapshot.session_id
        || super::native_player::native_ids(&state, &library, &again.session_id)? != (pid, hwnd)
    {
        return Err("连点产生重复内核".into());
    }
    let bindings = (1..=12)
        .map(|n| serde_json::json!({"id":"player.fullscreen","key":format!("F{n}")}))
        .chain([
            serde_json::json!({"id":"player.toggle","key":"Ctrl++"}),
            serde_json::json!({"id":"player.toggle","key":"Ctrl+!"}),
        ])
        .collect::<Vec<_>>();
    for bindings in [bindings, vec![]] {
        let action =
            serde_json::from_value(serde_json::json!({"type":"shortcuts","bindings":bindings}))
                .map_err(|e| e.to_string())?;
        super::native_player::control(app, &library, &snapshot.session_id, action)?;
    }
    if super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        super::native_player::PlaybackAction::Audio {
            track_id: Some(999),
        },
    )
    .is_ok()
    {
        return Err("允许了未登记轨道".into());
    }
    for enabled in [true, false] {
        super::native_player::control(
            app,
            &library,
            &snapshot.session_id,
            super::native_player::PlaybackAction::Fullscreen { enabled },
        )?;
        if super::native_player::native_ids(&state, &library, &snapshot.session_id)? != (pid, hwnd)
        {
            return Err("全屏重新创建了播放内核".into());
        }
    }
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        super::native_player::PlaybackAction::Seek { position_ms: 1250 },
    )?;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        super::native_player::PlaybackAction::Pause { paused: false },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(450));
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        super::native_player::PlaybackAction::Pause { paused: true },
    )?;
    let played = super::native_player::snapshot(&state, &library, &snapshot.session_id)?;
    if played.position_ms < 1400 {
        return Err("实际播放时间未推进".into());
    }
    state
        .knowledge_editing
        .store(true, std::sync::atomic::Ordering::Release);
    if super::native_player::stop_learning_for_exit(&state).is_ok() {
        return Err("未遵守笔记保存屏障".into());
    }
    state
        .knowledge_editing
        .store(false, std::sync::atomic::Ordering::Release);
    let second = super::native_player::start_playback(app, &library, &course2.id)?;
    super::native_player::set_viewport(
        &state,
        &library,
        &second.session_id,
        super::native_player::NativeViewport {
            left: 20.0,
            top: 20.0,
            width: 640.0,
            height: 360.0,
            visible: true,
        },
    )?;
    ready(app, &library, &second.session_id)?;
    let (pid2, _) = super::native_player::native_ids(&state, &library, &second.session_id)?;
    if pid2 == pid || running(pid) {
        return Err("切课未停止旧内核".into());
    }
    if super::native_player::stop_learning_session(&state, &library, &snapshot.session_id).is_ok() {
        return Err("旧停止请求影响了新课程".into());
    }
    super::native_player::stop_learning_session(&state, &library, &second.session_id)?;
    super::native_player::stop_learning_session(&state, &library, &second.session_id)?;
    let saved = state.with_library(|db, _| super::store::course(db, &course.id))?;
    if saved.position_ms < 1400 {
        return Err("停止后进度未持久保存".into());
    }
    std::fs::write(&source, b"corrupted fixture").map_err(|e| e.to_string())?;
    let broken = super::native_player::start_playback(app, &library, &course.id)?;
    let started = std::time::Instant::now();
    loop {
        let value = super::native_player::snapshot(&state, &library, &broken.session_id)?;
        if value.error.is_some() {
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(8) {
            return Err("损坏原件未被拒绝".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if super::native_player::native_ids(&state, &library, &broken.session_id).is_ok() {
        return Err("为损坏原件创建了播放内核".into());
    }
    super::native_player::stop_learning_session(&state, &library, &broken.session_id)?;
    let exit_before = state
        .with_library(|db, _| super::store::course(db, &course2.id))?
        .progress_revision;
    let final_session = super::native_player::start_playback(app, &library, &course2.id)?;
    super::native_player::set_viewport(
        &state,
        &library,
        &final_session.session_id,
        super::native_player::NativeViewport {
            left: 20.0,
            top: 20.0,
            width: 640.0,
            height: 360.0,
            visible: true,
        },
    )?;
    ready(app, &library, &final_session.session_id)?;
    let (pid3, _) = super::native_player::native_ids(&state, &library, &final_session.session_id)?;
    super::native_player::control(
        app,
        &library,
        &final_session.session_id,
        super::native_player::PlaybackAction::Pause { paused: false },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(450));
    Ok(
        serde_json::json!({"ready":first_ready,"played":played,"saved":saved,"pids":[pid,pid2,pid3],"hwnd":hwnd,"sha256":preview.sha256,"cold":true,"sameCourseReused":true,"switchReapsOld":true,"corruptionRejectedBeforeSpawn":true,"exitCourseId":course2.id,"exitBeforeRevision":exit_before}),
    )
}
