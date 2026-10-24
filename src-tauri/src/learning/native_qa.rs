//! Debug-only target-process acceptance. Never initializes the normal app, native
//! messaging, credential cleanup, online services or an installed library.
use crate::state::AppState;
use std::path::PathBuf;
use tauri::Manager;

pub(crate) fn workspace_command_allowed(command: &str) -> bool {
    matches!(
        command,
        "get_workspace_context"
            | "save_workspace_context"
            | "get_personalization_settings"
            | "save_library_preferences"
            | "get_library_meta"
            | "list_smart_collections"
            | "search_assets"
            | "list_library_tasks"
            | "get_task_warning"
            | "get_automatic_backup_policy"
            | "get_upgrade_backup_status"
            | "get_interrupted_restore"
            | "search_media_entries"
            | "get_media_entry"
            | "list_media_folders"
            | "list_projects"
            | "get_project"
            | "list_reference_boards"
            | "get_learning_course"
            | "ensure_learning_course"
            | "list_learning_courses"
            | "list_learning_subtitles"
            | "get_learning_transcription_components"
            | "list_learning_series"
            | "list_course_analyses"
            | "get_course_analysis"
            | "prepare_course_analysis"
            | "prepare_course_summary"
            | "upsert_learning_series"
            | "set_learning_series_members"
            | "delete_learning_series"
            | "transcript_page"
            | "load_learning_subtitle"
            | "start_learning_playback"
            | "get_learning_playback"
            | "control_learning_playback"
            | "set_learning_viewport"
            | "stop_learning_playback"
            | "get_reference_board"
            | "open_reference_window"
            | "attach_reference_window"
            | "is_reference_window_open"
    )
}

pub fn run_workspace(root: PathBuf) -> Result<(), String> {
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
    let library = state.active_library_id()?;
    let source = root.join("workspace-synthetic.mp4");
    crate::runtime::process::capture(
        &crate::runtime::tools::require(None, "ffmpeg")?,
        &[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25:duration=60",
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
    let captions = root.join("workspace-captions.srt");
    let cues = (0..55)
        .map(|n| {
            format!(
                "{}\n00:00:{n:02},000 --> 00:00:{n:02},800\n完整中文字幕第{n}段\n第二行\n\n",
                n + 1
            )
        })
        .collect::<String>();
    std::fs::write(&captions, cues).map_err(|e| e.to_string())?;
    state.with_library_mut(|db, base| {
        super::subtitles::import(db, base, &course.id, &captions)?;
        let series = super::store::upsert_series(db, None, "隔离课程系列", "", None)?;
        super::store::set_series_members(db, &series.id, series.revision, &[course.id.clone()])?;
        Ok(())
    })?;
    // Fixed local fixture, not an AI/network result. Paid task commands remain
    // blocked by workspace_command_allowed throughout interactive acceptance.
    let analysis_preview = super::analysis::prepare(
        &state,
        &library,
        &course.id,
        &super::analysis_models::CourseAnalysisOptions::default(),
        None,
    )?;
    let analysis_input = super::analysis::load_confirmed(
        &state,
        &library,
        &course.id,
        &analysis_preview.preview_id,
    )?;
    let segment = analysis_input.segments.first().ok_or("QA缺少分析材料")?;
    let cue = segment.cues.get(1).ok_or("QA缺少来源字幕")?;
    let fixed = serde_json::json!({"contentType":"tutorial","oneSentenceSummary":"隔离合成课程分析（非 AI）","overview":"本机固定结果，仅用于核对界面、来源跳转与预览。未调用在线 AI。","chapters":[],"keyPoints":[{"timeMs":cue.start_ms,"title":"合成学习要点","content":"点击时间码应跳转到真实合成字幕位置。","importance":3,"sourceSegmentIds":[cue.id]}],"tutorialSteps":[],"terms":[],"actionItems":["核对字幕与分析面板切换"],"tags":["合成验收"],"sections":[{"id":"general","title":"用途","items":[{"title":"时间纠正示例","content":"应显示本机校验报告，不伪装为模型成功。","timeMs":0,"sourceSegmentIds":[cue.id],"keyframeIds":[]}]}],"warnings":["合成固定结果，非付费 AI 产物"]});
    let analysis_result = super::analysis_result::parse_course_result(
        &fixed.to_string(),
        &super::analysis_models::CourseScope {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            cues: &segment.cues,
            frames: &segment.frames,
        },
    )?;
    let analysis_id = state.with_library_mut(|db, base| {
        let id = super::analysis_store::begin_analysis(
            db,
            base,
            &analysis_input,
            chrono::Utc::now().timestamp(),
        )?;
        super::analysis_store::record_stage(
            db,
            &analysis_input,
            &id,
            &segment.input_hash,
            Ok(&analysis_result),
            Some(0),
        )?;
        super::analysis_store::finish_analysis(db, &analysis_input, &id, "completed", "")?;
        Ok(id)
    })?;
    state.with_library(|db, _| {
        let mut preferences = crate::organization::load_library_preferences(db)?;
        preferences.startup_module = "videoLibrary".into();
        preferences.remember_workspace = false;
        preferences
            .collapsed_module_groups
            .retain(|id| id != "mediaLibraries");
        crate::organization::save_library_preferences(db, preferences)?;
        Ok(())
    })?;
    std::fs::write(root.join("workspace.json"),serde_json::to_vec_pretty(&serde_json::json!({"libraryId":library,"courseId":course.id,"mediaId":media,"videoSha256":preview.sha256,"analysisId":analysis_id,"analysisFixture":"local-fixed-not-ai","kind":"isolated-real-workspace"})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    crate::application_builder(state, Some(root))
        .run(context)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod workspace_qa_tests {
    #[test]
    fn learning_workspace_qa_blocks_credentials_network_and_user_file_operations() {
        for command in [
            "start_learning_playback",
            "get_workspace_context",
            "get_media_entry",
            "list_course_analyses",
            "get_course_analysis",
            "prepare_course_analysis",
            "prepare_course_summary",
        ] {
            assert!(super::workspace_command_allowed(command));
        }
        for command in [
            "get_translation_settings",
            "save_deepseek_credentials",
            "generate_ai_session",
            "import_media_entries",
            "open_external_url",
            "restore_backup",
            "save_global_preferences",
            "run",
            "start_library_task",
            "retry_library_task",
        ] {
            assert!(!super::workspace_command_allowed(command));
        }
    }
}

pub fn run(root: PathBuf) -> Result<(), String> {
    run_host(root, false)
}
pub fn resume(root: PathBuf) -> Result<(), String> {
    run_host(root, true)
}
fn run_host(root: PathBuf, resuming: bool) -> Result<(), String> {
    let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../release-artifacts/yingliu-merge")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_absolute()
        || !canonical.starts_with(&allowed)
        || canonical == allowed
        || (!resuming
            && std::fs::read_dir(&root)
                .map_err(|e| e.to_string())?
                .next()
                .is_some())
    {
        return Err("QA只允许空的工作区证据子目录".into());
    }
    crate::media_access::reject_links(&root)?;
    if resuming {
        let config = root.join("local/DCCAssetLibrary");
        crate::media_access::reject_links(&config.join("app-settings.json"))?;
        crate::media_access::reject_links(&config.join("library.db"))?;
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(config.join("app-settings.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let active = PathBuf::from(settings["activeLibrary"].as_str().ok_or("QA缺少库路径")?);
        if active.canonicalize().map_err(|e| e.to_string())?
            != config.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("恢复QA不能打开隔离目录以外的库".into());
        }
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("results.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if report["postExitVerified"] != true
            || !root.join("local/DCCAssetLibrary/library.db").is_file()
        {
            return Err("只能恢复已完成退出验证的隔离QA库".into());
        }
    }
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
            let result=if resuming{check_resume(&handle,&root)}else{check(&handle,&root)};
            let report_file=if resuming{"resume-results.json"}else{"results.json"};
            let report=serde_json::json!({"native":result.is_ok(),"kind":"target-native-host-not-workspace-ui","result":result});
            let _=std::fs::write(root.join(report_file),serde_json::to_vec_pretty(&report).unwrap());
            if report["native"]==true {
                if let Err(error)=super::native_player::exit_application(&handle,false){
                    let _=std::fs::write(root.join(report_file),serde_json::to_vec_pretty(&serde_json::json!({"native":false,"exitError":error})).unwrap());
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
fn check_resume(
    app: &tauri::AppHandle,
    root: &std::path::Path,
) -> Result<serde_json::Value, String> {
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("results.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let id = report["result"]["Ok"]["exitCourseId"]
        .as_str()
        .ok_or("QA缺少恢复课程")?;
    let state = app.state::<AppState>();
    let library = state.active_library_id()?;
    let saved = state.with_library(|db, _| super::store::course(db, id))?;
    if saved.completed || saved.position_ms == 0 {
        return Err("QA未留下可恢复进度".into());
    }
    let opened = super::native_player::start_playback(app, &library, id)?;
    ready(app, &library, &opened.session_id)?;
    let actual = super::native_player::effective_position_ms(&state, &library, &opened.session_id)?;
    let error = actual.abs_diff(saved.position_ms);
    if error > 50 {
        return Err(format!("重启后恢复误差{error}ms"));
    }
    let (pid, _) = super::native_player::native_ids(&state, &library, &opened.session_id)?;
    Ok(
        serde_json::json!({"restoredPositionMs":saved.position_ms,"actualPositionMs":actual,"errorMs":error,"pids":[pid]}),
    )
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
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=10",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:a",
            "aac",
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
    let mut seek_errors = Vec::new();
    for target in [125, 1350, 7999] {
        super::native_player::control(
            app,
            &library,
            &snapshot.session_id,
            super::native_player::PlaybackAction::Seek {
                position_ms: target,
            },
        )?;
        let started = std::time::Instant::now();
        let initial =
            super::native_player::effective_position_ms(&state, &library, &snapshot.session_id)?;
        let actual = loop {
            let actual = super::native_player::effective_position_ms(
                &state,
                &library,
                &snapshot.session_id,
            )?;
            if actual.abs_diff(target) <= 50
                || started.elapsed() > std::time::Duration::from_secs(2)
            {
                break actual;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let error = actual.abs_diff(target);
        seek_errors.push(serde_json::json!({"targetMs":target,"initialMs":initial,"actualMs":actual,"errorMs":error,"settleMs":started.elapsed().as_millis()}));
        if error > 50 {
            return Err(format!("暂停seek误差{error}ms超过50ms"));
        }
    }
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        super::native_player::PlaybackAction::Mute { muted: true },
    )?;
    let audio_started = std::time::Instant::now();
    let audio = loop {
        let value = super::native_player::snapshot(&state, &library, &snapshot.session_id)?;
        let ids = value
            .tracks
            .iter()
            .filter(|t| t.kind == "audio")
            .map(|t| t.id)
            .collect::<Vec<_>>();
        if ids.len() == 2 {
            break ids;
        }
        if audio_started.elapsed() > std::time::Duration::from_secs(5) {
            return Err("双音轨fixture未被实际播放器识别".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    for id in &audio {
        super::native_player::control(
            app,
            &library,
            &snapshot.session_id,
            super::native_player::PlaybackAction::Audio {
                track_id: Some(*id),
            },
        )?;
        if super::native_player::effective_audio_id(&state, &library, &snapshot.session_id)? != *id
        {
            return Err("实际音轨没有切换".into());
        }
    }
    let subtitle_source = root.join("captions.srt");
    std::fs::write(
        &subtitle_source,
        "1\n00:00:01,000 --> 00:00:04,000\n完整中文字幕\n",
    )
    .map_err(|e| e.to_string())?;
    let subtitle = state.with_library(|db, base| {
        super::subtitles::import(db, base, &course.id, &subtitle_source)
    })?;
    super::native_player::load_subtitle(app, &library, &snapshot.session_id, &subtitle.id)?;
    let missing_source = root.join("missing-subtitle.srt");
    std::fs::write(
        &missing_source,
        "1\n00:00:01,000 --> 00:00:02,000\nmissing fixture\n",
    )
    .map_err(|e| e.to_string())?;
    let missing = state
        .with_library(|db, base| super::subtitles::import(db, base, &course.id, &missing_source))?;
    let missing_path =
        state.with_library(|db, base| super::subtitles::path(db, base, &course.id, &missing.id))?;
    std::fs::remove_file(missing_path).map_err(|e| e.to_string())?;
    if super::native_player::load_subtitle(app, &library, &snapshot.session_id, &missing.id).is_ok()
    {
        return Err("字幕缺失未报告错误".into());
    }
    if !super::native_player::snapshot(&state, &library, &snapshot.session_id)?.ready {
        return Err("缺失字幕破坏视频会话".into());
    }
    let subtitle_started = std::time::Instant::now();
    loop {
        let value = super::native_player::snapshot(&state, &library, &snapshot.session_id)?;
        if value.subtitle_id.is_some() && value.tracks.iter().any(|track| track.kind == "sub") {
            break;
        }
        if subtitle_started.elapsed() > std::time::Duration::from_secs(5) {
            return Err("实际播放器未加载已登记字幕".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
    use super::native_player::PlaybackAction;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::AbLoop {
            a_ms: 200,
            b_ms: 800,
        },
    )?;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Seek { position_ms: 200 },
    )?;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Pause { paused: false },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let looped = super::native_player::snapshot(&state, &library, &snapshot.session_id)?;
    if looped.position_ms > 850 {
        return Err("实际A-B循环未返回范围内".into());
    }
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::ClearAbLoop,
    )?;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Seek { position_ms: 9800 },
    )?;
    let eof_started = std::time::Instant::now();
    loop {
        if state
            .with_library(|db, _| super::store::course(db, &course.id))?
            .completed
        {
            break;
        }
        if eof_started.elapsed() > std::time::Duration::from_secs(5) {
            return Err("真实EOF后未持久化完成状态".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Completed { completed: false },
    )?;
    if state
        .with_library(|db, _| super::store::course(db, &course.id))?
        .completed
    {
        return Err("恢复学习未清除完成状态".into());
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
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Seek { position_ms: 1350 },
    )?;
    super::native_player::control(
        app,
        &library,
        &snapshot.session_id,
        PlaybackAction::Pause { paused: true },
    )?;
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
    super::native_player::control(
        app,
        &library,
        &second.session_id,
        PlaybackAction::Seek { position_ms: 2250 },
    )?;
    super::native_player::control(
        app,
        &library,
        &second.session_id,
        PlaybackAction::Pause { paused: true },
    )?;
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
    if saved.position_ms.abs_diff(1350) > 50 {
        return Err("停止后进度未持久保存".into());
    }
    let resumed = super::native_player::start_playback(app, &library, &course.id)?;
    let resumed = ready(app, &library, &resumed.session_id)?;
    let (resume_pid, _) = super::native_player::native_ids(&state, &library, &resumed.session_id)?;
    let resume_error = resumed.position_ms.abs_diff(saved.position_ms);
    if resume_error > 50 {
        return Err(format!("切课恢复误差{resume_error}ms超过50ms"));
    }
    super::native_player::stop_learning_session(&state, &library, &resumed.session_id)?;
    if crate::file_digest::digest_file(&source)?.1 != preview.sha256
        || crate::file_digest::digest_file(&other)?.1 != preview2.sha256
    {
        return Err("学习操作改变原视频".into());
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
    let final_ready = ready(app, &library, &final_session.session_id)?;
    if final_ready.position_ms.abs_diff(2250) > 50 {
        return Err("第二课程未恢复自己的播放位置".into());
    }
    let (pid3, _) = super::native_player::native_ids(&state, &library, &final_session.session_id)?;
    super::native_player::control(
        app,
        &library,
        &final_session.session_id,
        super::native_player::PlaybackAction::Pause { paused: false },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(450));
    Ok(
        serde_json::json!({"ready":first_ready,"played":played,"saved":saved,"pids":[pid,pid2,resume_pid,pid3],"seekErrors":seek_errors,"missingSubtitleRejected":true,"audioTracks":audio,"resumeErrorMs":resume_error,"secondResumeMs":final_ready.position_ms,"originalsUnchangedBeforeCorruptionTest":true,"hwnd":hwnd,"sha256":preview.sha256,"cold":true,"sameCourseReused":true,"switchReapsOld":true,"corruptionRejectedBeforeSpawn":true,"exitCourseId":course2.id,"exitBeforeRevision":exit_before}),
    )
}
