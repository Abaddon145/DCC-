use super::{transcription::*, transcription_runner::run_local};
use crate::{
    state::AppState,
    tasks::{LibraryTask, TaskRequest},
};
use serde_json::json;
use std::path::{Path, PathBuf};

fn fixture() -> (
    tempfile::TempDir,
    AppState,
    CourseTranscriptionRequest,
    LibraryTask,
    tempfile::TempDir,
    VerifiedTranscription,
) {
    let (root, db, input) = super::analysis_tests::stored_fixture();
    std::fs::write(
        root.path().join("fixture.mp4"),
        b"synthetic registered video",
    )
    .unwrap();
    drop(db);
    let state = crate::state::test_state(root.path());
    let request = CourseTranscriptionRequest {
        course_id: input.preview.course_id,
        model_id: "base".into(),
    };
    let (task, _) = state
        .tasks
        .submit(
            state.active_library_id().unwrap(),
            TaskRequest::CourseTranscription(request.clone()),
        )
        .unwrap();
    let (component_root, tools, models, spec) = super::transcription_tests::components();
    let components = verify_with_manifest(&tools, &models, "base", &spec, &|| false).unwrap();
    (root, state, request, task, component_root, components)
}

fn exchange(exe: &Path, args: &[String], cwd: &Path) -> Result<String, String> {
    match exe.file_name().unwrap().to_str().unwrap() {
        "ffmpeg.exe" => {
            for arg in [
                "-nostdin",
                "-n",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                "-map",
                "0:a:0",
                "-ar",
                "16000",
                "-ac",
                "1",
                "pcm_s16le",
                "audio.wav",
            ] {
                assert!(args.iter().any(|v| v == arg), "missing {arg}");
            }
            assert!(args.windows(2).any(|v| v == ["-rf64", "never"]));
            std::fs::write(cwd.join("audio.wav"), [0u8; 100]).unwrap();
            Ok(String::new())
        }
        "ffprobe.exe" => {
            assert_eq!(args.last().unwrap(), "audio.wav");
            Ok(json!({"format":{"duration":"60","format_name":"wav"},"streams":[{"codec_type":"audio","codec_name":"pcm_s16le","sample_rate":"16000","channels":1}]}).to_string())
        }
        "whisper-cli.exe" => {
            for arg in ["-ng", "-oj", "-np"] {
                assert!(args.iter().any(|v| v == arg));
            }
            for pair in [
                ["-m", "model.bin"],
                ["-f", "audio.wav"],
                ["-of", "transcript"],
                ["-l", "auto"],
            ] {
                assert!(args.windows(2).any(|v| v == pair));
            }
            assert_eq!(std::fs::read(cwd.join("model.bin")).unwrap().len(), 48);
            std::fs::write(cwd.join("transcript.json"),json!({"result":{"language":"zh"},"transcription":[{"offsets":{"from":1100,"to":2900},"text":"完整中文🦀"}]}).to_string()).unwrap();
            Ok(String::new())
        }
        _ => panic!("unexpected process"),
    }
}

#[test]
fn learning_transcription_runner_registers_real_cues_reuses_cache_and_cleans_runtime() {
    let (root, state, request, task, _component_root, components) = fixture();
    let mut calls = vec![];
    let mut work: Option<PathBuf> = None;
    let result = run_local(
        &state,
        &task,
        &request,
        &components,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |exe, args, cwd, _timeout, stop| {
            assert!(!stop());
            calls.push(exe.file_name().unwrap().to_str().unwrap().to_owned());
            work = Some(cwd.into());
            exchange(exe, args, cwd)
        },
    )
    .unwrap();
    assert_eq!(calls, ["ffmpeg.exe", "ffprobe.exe", "whisper-cli.exe"]);
    assert!(!work.unwrap().exists());
    let db = crate::tasks::open_media_worker_connection(&root.path().join("library.db")).unwrap();
    let subtitles = super::subtitles::list(&db, &request.course_id).unwrap();
    assert_eq!(subtitles.len(), 1);
    assert_eq!(result["subtitleId"], subtitles[0].id);
    let page = super::subtitles::page(
        &db,
        root.path(),
        &request.course_id,
        &subtitles[0].id,
        0,
        50,
    )
    .unwrap();
    assert_eq!(page.items[0].text, "完整中文🦀");
    assert_eq!((page.items[0].start_ms, page.items[0].end_ms), (1100, 2900));
    let cached = run_local(
        &state,
        &task,
        &request,
        &components,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |_, _, _, _, _| panic!("cache issued another process"),
    )
    .unwrap();
    assert_eq!(cached["subtitleId"], subtitles[0].id);
    assert_eq!(cached["cached"], true);
    assert_eq!(
        std::fs::read(root.path().join("fixture.mp4")).unwrap(),
        b"synthetic registered video"
    );
    std::fs::write(
        root.path().join("fixture.mp4"),
        b"new source with identical speech",
    )
    .unwrap();
    let updated = run_local(
        &state,
        &task,
        &request,
        &components,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |exe, args, cwd, _, _| exchange(exe, args, cwd),
    )
    .unwrap();
    assert_eq!(
        updated["subtitleId"], subtitles[0].id,
        "identical bytes retain existing subtitle identity"
    );
    let updated_cache = run_local(
        &state,
        &task,
        &request,
        &components,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |_, _, _, _, _| panic!("new source cache alias was lost"),
    )
    .unwrap();
    assert_eq!(updated_cache["cached"], true);
}

#[test]
fn learning_transcription_runner_failure_cancel_and_source_change_never_publish() {
    for reason in [
        "failed",
        "cancelled",
        "changed",
        "switched",
        "bad-json",
        "rollback",
        "audio-failed",
        "probe-invalid",
    ] {
        let (root, state, request, task, _component_root, components) = fixture();
        if reason == "rollback" {
            let db = crate::tasks::open_media_worker_connection(&root.path().join("library.db"))
                .unwrap();
            db.execute_batch("CREATE TRIGGER reject_transcript BEFORE INSERT ON learning_subtitles BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;").unwrap();
        }
        let mut work = None;
        let outcome = run_local(
            &state,
            &task,
            &request,
            &components,
            Path::new("ffmpeg.exe"),
            Path::new("ffprobe.exe"),
            |exe, args, cwd, _, _| {
                work = Some(cwd.to_owned());
                let value = exchange(exe, args, cwd)?;
                if reason == "audio-failed" && exe.file_name().unwrap() == "ffmpeg.exe" {
                    return Err("synthetic audio extraction failure".into());
                }
                if reason == "probe-invalid" && exe.file_name().unwrap() == "ffprobe.exe" {
                    return Ok(json!({"format":{"duration":"60","format_name":"wav"},"streams":[{"codec_type":"audio","codec_name":"pcm_s16le","sample_rate":"48000","channels":2}]}).to_string());
                }
                if exe.file_name().unwrap() == "whisper-cli.exe" {
                    match reason {
                        "failed" => return Err("synthetic process failure".into()),
                        "cancelled" => {
                            state.tasks.cancel(&task.id).unwrap();
                        }
                        "changed" => {
                            state
                                .with_library(|db, _| {
                                    db.execute(
                                        "UPDATE learning_courses SET duration_ms=30000 WHERE id=?1",
                                        [&request.course_id],
                                    )
                                    .map(|_| ())
                                    .map_err(|e| e.to_string())
                                })
                                .unwrap();
                        }
                        "switched" => {
                            state
                                .with_active_mut(|active| {
                                    active.marker.library_id = uuid::Uuid::new_v4().to_string();
                                    Ok(())
                                })
                                .unwrap();
                        }
                        "bad-json" => {
                            std::fs::write(cwd.join("transcript.json"), "{broken").unwrap();
                        }
                        _ => {}
                    }
                }
                Ok(value)
            },
        );
        assert!(outcome.is_err(), "{reason} published a subtitle");
        assert!(
            work.is_some(),
            "{reason} did not reach the process boundary"
        );
        if let Some(work) = work {
            assert!(!work.exists());
        }
        let db =
            crate::tasks::open_media_worker_connection(&root.path().join("library.db")).unwrap();
        assert!(super::subtitles::list(&db, &request.course_id)
            .unwrap()
            .is_empty());
        let folder = root
            .path()
            .join("learning-data/subtitles")
            .join(&request.course_id);
        assert!(!folder.exists() || std::fs::read_dir(folder).unwrap().next().is_none());
    }
}

#[test]
fn learning_transcription_runner_precancel_does_not_start_or_create_runtime() {
    let (root, state, request, task, _components, verified) = fixture();
    state.tasks.cancel(&task.id).unwrap();
    assert!(run_local(
        &state,
        &task,
        &request,
        &verified,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |_, _, _, _, _| panic!("cancelled task started a process")
    )
    .is_err());
    assert!(!root.path().join(".dcc/runtime/transcription").exists());
}

#[test]
fn learning_transcription_runner_cache_failure_keeps_a_valid_completed_subtitle() {
    let (root, state, request, task, _components, verified) = fixture();
    std::fs::create_dir_all(root.path().join("learning-data")).unwrap();
    std::fs::write(
        root.path().join("learning-data/transcriptions"),
        b"synthetic blocked cache directory",
    )
    .unwrap();
    let result = run_local(
        &state,
        &task,
        &request,
        &verified,
        Path::new("ffmpeg.exe"),
        Path::new("ffprobe.exe"),
        |exe, args, cwd, _, _| exchange(exe, args, cwd),
    )
    .unwrap();
    assert!(result["warning"].as_str().unwrap().contains("字幕已保存"));
    let db = crate::tasks::open_media_worker_connection(&root.path().join("library.db")).unwrap();
    let subtitles = super::subtitles::list(&db, &request.course_id).unwrap();
    assert_eq!(subtitles.len(), 1);
    assert!(super::subtitles::verified_cues(
        &db,
        root.path(),
        &request.course_id,
        &subtitles[0].id
    )
    .is_ok());
}
