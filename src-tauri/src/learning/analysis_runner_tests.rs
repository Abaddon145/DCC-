use super::{analysis_models::*, analysis_runner::run_segments};
use crate::{state::AppState, tasks::TaskRequest};
use serde_json::{json, Value};
use std::io::{Read, Write};
// These tests exercise the real app-wide single-request AI lane.
static RUNNER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn one_click_runs_segments_and_whole_course_summary_using_fixture_service() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    let (_dir, state, preview) = sources();
    let library = state.active_library_id().unwrap();
    let request = CourseAnalysisRequest {
        course_id: preview.course_id.clone(),
        preview_id: preview.preview_id.clone(),
        stage: CourseAnalysisPhase::Segments,
        auto_summary: true,
    };
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::CourseAnalysis(request.clone()),
        )
        .unwrap();
    let (url, server) = server(5, None);
    let report = tauri::async_runtime::block_on(super::analysis_runner::run_full(
        &state,
        &task,
        &request,
        |body, images, cancel| {
            let service = &state.translation_service;
            let url = &url;
            async move {
                crate::translation_service::course_fixture_exchange(
                    service, &body, 1000000, &cancel, images, url,
                )
                .await
            }
        },
    ))
    .unwrap();
    assert_eq!(report["stage"], "summary");
    let detail = super::analysis::get(
        &state,
        &library,
        &preview.course_id,
        report["analysisId"].as_str().unwrap(),
        None,
        0,
        50,
    )
    .unwrap();
    assert_eq!(detail.status, "completed");
    assert!(
        detail.result.is_some(),
        "one-click result is the whole-course summary"
    );
    assert_eq!(server.join().unwrap().len(), 5);
}

#[test]
fn one_click_summary_failure_keeps_completed_segments() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    let (_dir, state, preview) = sources();
    let library = state.active_library_id().unwrap();
    let request = CourseAnalysisRequest {
        course_id: preview.course_id.clone(),
        preview_id: preview.preview_id.clone(),
        stage: CourseAnalysisPhase::Segments,
        auto_summary: true,
    };
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::CourseAnalysis(request.clone()),
        )
        .unwrap();
    let (url, server) = server(5, Some(4));
    let outcome = tauri::async_runtime::block_on(super::analysis_runner::run_full(
        &state,
        &task,
        &request,
        |body, images, cancel| {
            let service = &state.translation_service;
            let url = &url;
            async move {
                crate::translation_service::course_fixture_exchange(
                    service, &body, 1000000, &cancel, images, url,
                )
                .await
            }
        },
    ));
    assert!(outcome.unwrap_err().contains("已完成的结果已保留"));
    assert_eq!(server.join().unwrap().len(), 5);
    let base = state
        .with_library_if_id(&library, |_, base| Ok(base.to_owned()))
        .unwrap()
        .unwrap();
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db")).unwrap();
    let completed: i64 = db
        .query_row(
            "SELECT count(*) FROM learning_analyses WHERE course_id=?1 AND status='completed'",
            [&preview.course_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        completed >= 1,
        "completed segments remain available after summary failure"
    );
}

#[test]
fn one_click_completes_multiple_summary_layers() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    let (dir, state, first) = sources();
    let library = state.active_library_id().unwrap();
    complete_segments(dir.path(), &state, &first, 10000);
    let preview = super::analysis::prepare(
        &state,
        &library,
        &first.course_id,
        &CourseAnalysisOptions::default(),
        None,
    )
    .unwrap();
    assert!(preview.stages.iter().all(|stage| stage.cached));
    let request = CourseAnalysisRequest {
        course_id: preview.course_id.clone(),
        preview_id: preview.preview_id.clone(),
        stage: CourseAnalysisPhase::Segments,
        auto_summary: true,
    };
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::CourseAnalysis(request.clone()),
        )
        .unwrap();
    let (url, server) = server(3, None);
    let report = tauri::async_runtime::block_on(super::analysis_runner::run_full(
        &state,
        &task,
        &request,
        |body, images, cancel| {
            let service = &state.translation_service;
            let url = &url;
            async move {
                crate::translation_service::course_fixture_exchange(
                    service, &body, 1000000, &cancel, images, url,
                )
                .await
            }
        },
    ))
    .unwrap();
    let detail = super::analysis::get(
        &state,
        &library,
        &preview.course_id,
        report["analysisId"].as_str().unwrap(),
        None,
        0,
        50,
    )
    .unwrap();
    assert!(detail.result.is_some());
    assert_eq!(server.join().unwrap().len(), 3);
}

pub(super) fn sources() -> (tempfile::TempDir, AppState, CourseAnalysisPreview) {
    let (dir, db, old) = super::analysis_tests::stored_fixture();
    std::fs::write(
        dir.path().join("fixture.mp4"),
        b"registered managed video synthetic bytes",
    )
    .unwrap();
    db.execute(
        "UPDATE learning_courses SET duration_ms=1200000 WHERE id=?1",
        [&old.preview.course_id],
    )
    .unwrap();
    let subtitle = dir.path().join("four.srt");
    std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\n第一段\n\n2\n00:05:00,000 --> 00:05:01,000\n第二段\n\n3\n00:10:00,000 --> 00:10:01,000\n第三段\n\n4\n00:15:00,000 --> 00:15:01,000\n第四段\n").unwrap();
    super::subtitles::import(&db, dir.path(), &old.preview.course_id, &subtitle).unwrap();
    let state = crate::state::test_state(dir.path());
    let library = state.active_library_id().unwrap();
    let preview = super::analysis::prepare(
        &state,
        &library,
        &old.preview.course_id,
        &CourseAnalysisOptions::default(),
        None,
    )
    .unwrap();
    assert_eq!(preview.stages.len(), 4);
    (dir, state, preview)
}

/// A bounded localhost HTTP fixture, not a fake budget or persistence layer.
fn server(count: usize, bad_index: Option<usize>) -> (String, std::thread::JoinHandle<Vec<u64>>) {
    server_controlled(count, bad_index, None)
}
fn server_controlled(
    count: usize,
    bad_index: Option<usize>,
    action: Option<Box<dyn Fn(usize) + Send>>,
) -> (String, std::thread::JoinHandle<Vec<u64>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        let mut starts = vec![];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        for index in 0..count {
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(_) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(_) => return starts,
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            let (end, length) = loop {
                let mut block = [0; 8192];
                let read = socket.read(&mut block).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&block[..read]);
                assert!(request.len() < 128 * 1024);
                if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                    let length = String::from_utf8_lossy(&request[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break (end + 4, length);
                    }
                }
            };
            let body: Value = serde_json::from_slice(&request[end..end + length]).unwrap();
            let source: Value =
                serde_json::from_str(body["messages"][1]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            let start = source["startMs"].as_u64().unwrap();
            starts.push(start);
            if let Some(action) = &action {
                action(index);
            }
            let raw = if bad_index == Some(index) {
                "{\"unexpected\":true}".into()
            } else if source.get("sources").is_some() {
                source["sources"][0]["result"].to_string()
            } else {
                json!({"contentType":"tutorial","oneSentenceSummary":"课程摘要","overview":"保留真实来源的学习结果","chapters":[],"keyPoints":[{"timeMs":start,"title":"重点","content":"已校验的重点","importance":3,"sourceSegmentIds":[source["cues"][0]["id"]]}],"tutorialSteps":[],"terms":[],"actionItems":[],"tags":[],"sections":[],"warnings":[]}).to_string()
            };
            let response=json!({"usage":{"total_tokens":101},"choices":[{"finish_reason":"stop","message":{"content":raw}}]}).to_string();
            // Cancellation may close the socket while this bounded fixture replies.
            let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len());
        }
        starts
    });
    (url, handle)
}

fn complete_segments(
    dir: &std::path::Path,
    state: &AppState,
    first: &CourseAnalysisPreview,
    chars: usize,
) -> String {
    let library = state.active_library_id().unwrap();
    let root =
        super::analysis::load_confirmed(&state, &library, &first.course_id, &first.preview_id)
            .unwrap();
    let mut db = crate::tasks::open_media_worker_connection(&dir.join("library.db")).unwrap();
    let parent =
        super::analysis_store::begin_analysis(&mut db, dir, &root, chrono::Utc::now().timestamp())
            .unwrap();
    let result: CourseAnalysisResult=serde_json::from_value(json!({"contentType":"tutorial","oneSentenceSummary":"课程摘要","overview":"字".repeat(chars),"chapters":[],"keyPoints":[],"tutorialSteps":[],"terms":[],"actionItems":["练习"],"tags":[],"sections":[],"warnings":[]})).unwrap();
    for stage in &root.preview.stages {
        super::analysis_store::record_stage(
            &mut db,
            &root,
            &parent,
            &stage.key,
            Ok(&result),
            Some(101),
        )
        .unwrap();
    }
    super::analysis_store::finish_analysis(&db, &root, &parent, "completed", "").unwrap();
    parent
}

#[test]
fn learning_analysis_runner_summary_uses_its_own_consent_and_retries_only_missing_groups() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    let (dir, state, first) = sources();
    let library = state.active_library_id().unwrap();
    let parent = complete_segments(dir.path(), &state, &first, 10000);
    let db = crate::tasks::open_media_worker_connection(&dir.path().join("library.db")).unwrap();
    let p = super::analysis::prepare_summary(&state, &library, &first.course_id, &parent).unwrap();
    assert_eq!(p.stages.len(), 2);
    let wrong = CourseAnalysisRequest {
        course_id: p.course_id.clone(),
        preview_id: p.preview_id.clone(),
        auto_summary: false,
        stage: CourseAnalysisPhase::Segments,
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseAnalysis(wrong.clone()))
        .unwrap();
    assert!(
        tauri::async_runtime::block_on(run_segments(&state, &task, &wrong, |_, _, _| async {
            panic!("wrong phase cannot send a request")
        }))
        .is_err()
    );
    state.tasks.cancel(&task.id).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
            [&p.preview_id],
            |r| r.get::<_, Option<i64>>(0)
        )
        .unwrap(),
        None,
        "wrong phase must not consume summary consent"
    );
    let (url, server) = server(4, Some(1));
    let execute = |preview: &CourseAnalysisPreview| {
        let request = CourseAnalysisRequest {
            course_id: preview.course_id.clone(),
            preview_id: preview.preview_id.clone(),
            auto_summary: false,
            stage: CourseAnalysisPhase::Summary,
        };
        let (task, _) = state
            .tasks
            .submit(
                library.clone(),
                TaskRequest::CourseAnalysis(request.clone()),
            )
            .expect("summary enters the same task registry");
        let result = tauri::async_runtime::block_on(run_segments(
            &state,
            &task,
            &request,
            |body, images, cancel| {
                assert_eq!(images, 0, "summary must not re-upload frame JPEGs");
                let service = &state.translation_service;
                let url = &url;
                async move {
                    crate::translation_service::course_fixture_exchange(
                        service, &body, 1000000, &cancel, images, url,
                    )
                    .await
                }
            },
        ));
        state.tasks.cancel(&task.id).unwrap();
        result
    };
    assert!(execute(&p).is_err());
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        202
    );
    let retry = super::analysis::prepare_summary(&state, &library, &p.course_id, &parent).unwrap();
    assert_eq!(
        retry.stages.iter().map(|s| s.cached).collect::<Vec<_>>(),
        vec![true, false]
    );
    let completed = execute(&retry).expect("new summary approval pays only for missing group");
    assert_eq!(completed["stage"], "summary");
    let next = super::analysis::prepare_summary(
        &state,
        &library,
        &p.course_id,
        completed["analysisId"].as_str().unwrap(),
    )
    .unwrap();
    let final_report = execute(&next).unwrap();
    let detail = super::analysis::get(
        &state,
        &library,
        &p.course_id,
        final_report["analysisId"].as_str().unwrap(),
        None,
        0,
        1,
    )
    .unwrap();
    assert!(
        detail.result.is_some(),
        "one terminal group is published as the full summary result"
    );
    assert_eq!(detail.status, "completed");
    assert_eq!(
        detail.parent_analysis_id.as_deref(),
        completed["analysisId"].as_str(),
        "a summary retry must derive its actual parent, not reuse the partial summary as parent"
    );
    assert!(detail.stale_reasons.is_empty());
    assert_eq!(server.join().unwrap(), vec![0, 900000, 900000, 0]);
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        404
    );
    assert!(
        execute(&next).is_err(),
        "old approval cannot issue another request"
    );
    let cached = super::analysis::prepare_summary(
        &state,
        &library,
        &p.course_id,
        completed["analysisId"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(cached.tokens_max, 0);
    execute(&cached).expect("terminal cached result works with fixture server closed");
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        404
    );
}

#[test]
fn learning_analysis_runner_cancellation_and_source_change_do_not_commit_a_response() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    for summary in [false, true] {
        for case in ["cancel", "source", "library", "ancestor"] {
            if !summary && case == "ancestor" {
                continue;
            }
            let (dir, state, mut preview) = sources();
            let parent = if summary {
                let id = complete_segments(dir.path(), &state, &preview, 10);
                preview = super::analysis::prepare_summary(
                    &state,
                    &state.active_library_id().unwrap(),
                    &preview.course_id,
                    &id,
                )
                .unwrap();
                id
            } else {
                String::new()
            };
            let state = std::sync::Arc::new(state);
            let library = state.active_library_id().unwrap();
            let request = CourseAnalysisRequest {
                course_id: preview.course_id.clone(),
                preview_id: preview.preview_id.clone(),
                auto_summary: false,
                stage: if summary {
                    CourseAnalysisPhase::Summary
                } else {
                    CourseAnalysisPhase::Segments
                },
            };
            let (task, _) = state
                .tasks
                .submit(
                    library.clone(),
                    TaskRequest::CourseAnalysis(request.clone()),
                )
                .unwrap();
            let target = state.clone();
            let task_id = task.id.clone();
            let video = dir.path().join("fixture.mp4");
            let database = dir.path().join("library.db");
            let (url, server) = server_controlled(
                1,
                None,
                Some(Box::new(move |_| match case {
                    "cancel" => {
                        target.tasks.cancel(&task_id).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    "source" => std::fs::write(&video, b"changed original").unwrap(),
                    "ancestor" => {
                        let db = crate::tasks::open_media_worker_connection(&database).unwrap();
                        db.execute("UPDATE learning_analysis_stages SET result_json='broken' WHERE analysis_id=?1",[&parent]).unwrap();
                    }
                    "library" => target
                        .with_active_mut(|active| {
                            active.marker.library_id = uuid::Uuid::new_v4().to_string();
                            Ok(())
                        })
                        .unwrap(),
                    _ => unreachable!(),
                })),
            );
            let result = tauri::async_runtime::block_on(run_segments(
                &state,
                &task,
                &request,
                |body, images, cancel| {
                    let service = &state.translation_service;
                    let url = &url;
                    async move {
                        crate::translation_service::course_fixture_exchange(
                            service, &body, 1000000, &cancel, images, url,
                        )
                        .await
                    }
                },
            ));
            assert!(result.is_err(), "{case} must reject response");
            assert_eq!(server.join().unwrap(), vec![0]);
            let db =
                crate::tasks::open_media_worker_connection(&dir.path().join("library.db")).unwrap();
            assert_eq!(
            db.query_row(
                "SELECT count(*) FROM learning_analysis_stages s JOIN learning_analyses a ON a.id=s.analysis_id WHERE s.status='completed' AND a.preview_id=?1",
                [&preview.preview_id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "{case}: no invalid response committed"
        );
            let status: String = db
                .query_row(
                    "SELECT status FROM learning_analyses WHERE preview_id=?1",
                    [&preview.preview_id],
                    |r| r.get(0),
                )
                .unwrap();
            if case == "cancel" {
                assert_eq!(status, "cancelled");
            }
            if ["source", "ancestor"].contains(&case) {
                assert_eq!(status, "failed");
                assert_eq!(
                    state.translation_service.usage().unwrap().course_tokens,
                    101,
                    "known use survives rejected source changes"
                );
            }
            assert!(
                state.translation_service.usage().unwrap().course_tokens > 0,
                "possibly billed request is not free"
            );
        }
    }
}

#[test]
fn learning_analysis_runner_quota_or_precancel_preserves_unused_approval() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    for summary in [false, true] {
        for precancel in [false, true] {
            let (dir, state, mut preview) = sources();
            let library = state.active_library_id().unwrap();
            if summary {
                let parent = complete_segments(dir.path(), &state, &preview, 10);
                preview =
                    super::analysis::prepare_summary(&state, &library, &preview.course_id, &parent)
                        .unwrap();
            }
            let request = CourseAnalysisRequest {
                course_id: preview.course_id.clone(),
                preview_id: preview.preview_id.clone(),
                auto_summary: false,
                stage: if summary {
                    CourseAnalysisPhase::Summary
                } else {
                    CourseAnalysisPhase::Segments
                },
            };
            let (task, _) = state
                .tasks
                .submit(
                    library.clone(),
                    TaskRequest::CourseAnalysis(request.clone()),
                )
                .unwrap();
            if precancel {
                state.tasks.cancel(&task.id).unwrap();
            } else {
                let mut preferences = state.translation_preferences().unwrap();
                preferences.deepseek_monthly_limit = 1;
                state.save_translation_preferences(preferences).unwrap();
            }
            let result = tauri::async_runtime::block_on(run_segments(
                &state,
                &task,
                &request,
                |_, _, _| async { panic!("quota/precancel must not reach transport") },
            ));
            assert!(result.is_err());
            state
                .with_library(|db, _| {
                    assert!(db
                        .query_row(
                            "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
                            [&preview.preview_id],
                            |r| r.get::<_, Option<i64>>(0)
                        )
                        .unwrap()
                        .is_none());
                    assert_eq!(
                        db.query_row(
                            "SELECT count(*) FROM learning_analyses WHERE preview_id=?1",
                            [&preview.preview_id],
                            |r| r.get::<_, i64>(0)
                        )
                        .unwrap(),
                        0
                    );
                    Ok(())
                })
                .unwrap();
            assert_eq!(state.translation_service.usage().unwrap().course_tokens, 0);
        }
    }
}

#[test]
fn learning_analysis_runner_keeps_three_successes_and_new_consent_sends_only_fourth() {
    let _serial = RUNNER_TEST_LOCK.lock().unwrap();
    let (_dir, state, first) = sources();
    let library = state.active_library_id().unwrap();
    let (url, server) = server(5, Some(3));
    let execute = |preview: &CourseAnalysisPreview| {
        let request = CourseAnalysisRequest {
            course_id: preview.course_id.clone(),
            preview_id: preview.preview_id.clone(),
            auto_summary: false,
            stage: CourseAnalysisPhase::Segments,
        };
        let (task, _) = state
            .tasks
            .submit(
                library.clone(),
                TaskRequest::CourseAnalysis(request.clone()),
            )
            .unwrap();
        let result = tauri::async_runtime::block_on(run_segments(
            &state,
            &task,
            &request,
            |body, images, cancel| {
                let url = &url;
                let service = &state.translation_service;
                async move {
                    crate::translation_service::course_fixture_exchange(
                        service, &body, 1000000, &cancel, images, url,
                    )
                    .await
                }
            },
        ));
        state.tasks.cancel(&task.id).unwrap();
        result
    };
    assert!(execute(&first).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_analysis_stages WHERE status='completed'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                3,
                "successful stages survive the fourth response failure"
            );
            assert_eq!(
                db.query_row("SELECT status FROM learning_analyses", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "partial"
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        404
    );
    assert!(
        execute(&first).is_err(),
        "consumed consent cannot issue another HTTP request"
    );
    let next = super::analysis::prepare(
        &state,
        &library,
        &first.course_id,
        &CourseAnalysisOptions::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        next.stages.iter().map(|s| s.cached).collect::<Vec<_>>(),
        vec![true, true, true, false]
    );
    let result = execute(&next).expect("new confirmation continues only missing stages");
    assert!(result["analysisId"].as_str().is_some());
    assert_eq!(
        server.join().unwrap(),
        vec![0, 300000, 600000, 900000, 900000]
    );
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        505
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_analysis_stages WHERE status='completed'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                7
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_analyses WHERE status='completed'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
    let cached = super::analysis::prepare(
        &state,
        &library,
        &first.course_id,
        &CourseAnalysisOptions::default(),
        None,
    )
    .unwrap();
    assert_eq!(cached.tokens_max, 0);
    assert!(cached.stages.iter().all(|s| s.cached));
    execute(&cached).expect("complete cache works with the localhost server already closed");
    assert_eq!(
        state.translation_service.usage().unwrap().course_tokens,
        505
    );
}
