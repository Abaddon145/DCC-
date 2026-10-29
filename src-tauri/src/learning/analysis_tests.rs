use super::{analysis_models::*, analysis_result::parse_course_result, models::TranscriptCue};
use serde_json::{json, Value};

#[test]
fn learning_analysis_history_list_pages_metadata_without_loading_results_or_other_courses() {
    let (dir, mut db, input) = stored_fixture();
    for n in 0..3 {
        let mut preview = input.clone();
        preview.preview.preview_id = uuid::Uuid::new_v4().to_string();
        preview.preview.created_at = 1000 + n;
        preview.preview.expires_at = 87400 + n;
        super::analysis_store::save_preview(&db, dir.path(), &preview).unwrap();
        let id =
            super::analysis_store::begin_analysis(&mut db, dir.path(), &preview, 1000 + n).unwrap();
        db.execute("UPDATE learning_analyses SET result_json=zeroblob(1048577),prompt_version=?1 WHERE id=?2",rusqlite::params![if n==2 { super::analysis_summary::PROMPT_VERSION }else{ super::analysis_store::PROMPT_VERSION },id]).unwrap();
    }
    let page = super::analysis_store::list_history(&db, &input.preview.course_id, 0, 2).unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].stage, CourseAnalysisPhase::Summary);
    assert_eq!(
        (page.items[0].completed_stages, page.items[0].total_stages),
        (0, 1)
    );
    assert_eq!(page.items[0].created_at, 1002);
    assert!(serde_json::to_value(&page).unwrap()["items"][0]
        .get("result")
        .is_none());
    let last = super::analysis_store::list_history(&db, &input.preview.course_id, 2, 2).unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].created_at, 1000);
    assert!(
        super::analysis_store::list_history(&db, &uuid::Uuid::new_v4().to_string(), 0, 50).is_err()
    );
    let bounded =
        super::analysis_store::list_history(&db, &input.preview.course_id, -10, 5000).unwrap();
    assert_eq!((bounded.offset, bounded.limit), (0, 50));
}

#[test]
fn learning_analysis_history_remains_readable_without_reusing_expired_consent() {
    let (dir, mut db, input) = stored_fixture();
    let video = dir.path().join("fixture.mp4");
    std::fs::write(&video, b"registered managed video synthetic bytes").unwrap();
    super::analysis_store::save_preview(&db, dir.path(), &input).unwrap();
    let id = super::analysis_store::begin_analysis(&mut db, dir.path(), &input, 1001).unwrap();
    let (raw, _) = fixture();
    let segment = &input.segments[0];
    let scope = CourseScope {
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        cues: &segment.cues,
        frames: &segment.frames,
    };
    let saved =
        serde_json::to_string(&parse_course_result(&raw.to_string(), &scope).unwrap()).unwrap();
    db.execute("UPDATE learning_analysis_stages SET status='completed',result_json=?1,tokens_used=123 WHERE analysis_id=?2", rusqlite::params![saved,id]).unwrap();
    db.execute(
        "UPDATE learning_analyses SET status='completed' WHERE id=?1",
        [&id],
    )
    .unwrap();
    let state = crate::state::test_state(dir.path());
    let library = state.active_library_id().unwrap();
    let read = super::analysis::get(
        &state,
        &library,
        &input.preview.course_id,
        &id,
        Some(&segment.input_hash),
        0,
        1,
    )
    .expect("history is independent from expired/consumed approval");
    assert_eq!(read.id, id);
    assert_eq!(read.course_id, input.preview.course_id);
    assert_eq!((read.stages.total, read.stages.items.len()), (1, 1));
    assert_eq!(read.stages.items[0].tokens_used, Some(123));
    assert_eq!(
        read.result.unwrap().section_validation[0].disposition,
        "corrected"
    );
    assert!(
        !read.stale_reasons.is_empty(),
        "synthetic history source hash differs from actual bytes"
    );
    assert!(super::analysis::get(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        &input.preview.course_id,
        &id,
        None,
        0,
        50
    )
    .is_err());
    assert!(super::analysis::get(
        &state,
        &library,
        &uuid::Uuid::new_v4().to_string(),
        &id,
        None,
        0,
        50
    )
    .is_err());
    assert!(super::analysis::get(
        &state,
        &library,
        &input.preview.course_id,
        &id,
        Some(&"b".repeat(64)),
        0,
        50
    )
    .is_err());
    let page =
        super::analysis::get(&state, &library, &input.preview.course_id, &id, None, 1, 1).unwrap();
    assert_eq!(page.stages.items.len(), 0);
    assert!(page.result.is_none());
    db.execute(
        "UPDATE learning_analysis_stages SET result_json='broken' WHERE analysis_id=?1",
        [&id],
    )
    .unwrap();
    let broken = super::analysis::get(
        &state,
        &library,
        &input.preview.course_id,
        &id,
        Some(&segment.input_hash),
        0,
        1,
    )
    .unwrap();
    assert!(broken.result.is_none());
    assert!(!broken.stale_reasons.is_empty());
    assert!(super::analysis_store::load_preview(
        &db,
        dir.path(),
        &library,
        &input.preview.course_id,
        &input.preview.preview_id,
        chrono::Utc::now().timestamp()
    )
    .is_err());
    assert_eq!(
        state.translation_service.usage().unwrap().deepseek_tokens,
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
            [&input.preview.preview_id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1001,
        "history cannot renew approval"
    );
}

#[test]
fn learning_analysis_oversized_snapshot_is_rejected_without_publication() {
    let (dir, db, mut input) = stored_fixture();
    input.frames = (0..49)
        .map(|n| CourseFrameImage {
            reference: AnalysisFrame {
                id: format!("frame-{n}"),
                time_ms: n * 100,
                sha256: "a".repeat(64),
            },
            data_url: "a".repeat(350000),
        })
        .collect();
    let refs = input
        .frames
        .iter()
        .map(|f| f.reference.clone())
        .collect::<Vec<_>>();
    input.segments = super::analysis::plan_segments(&[], &refs, 60000).unwrap();
    input.preview.frame_ids = refs.iter().map(|r| r.id.clone()).collect();
    input.preview.stages = input
        .segments
        .iter()
        .map(|s| CourseStagePreview {
            key: s.input_hash.clone(),
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            cached: false,
            tokens_max: 5000,
        })
        .collect();
    input.preview.tokens_max = 45000;
    input.preview.fingerprint = super::analysis_store::fingerprint(&input).unwrap();
    let error = super::analysis_store::save_preview(&db, dir.path(), &input).unwrap_err();
    assert!(error.contains("16MiB"));
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analysis_previews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!dir.path().join("learning-data/analysis").exists());
}

#[test]
fn learning_analysis_v25_migration_and_competing_confirmation_keep_identity() {
    let (dir, db, input) = stored_fixture();
    let media_before: String = db
        .query_row(
            "SELECT media_entry_id FROM learning_courses WHERE id=?1",
            [&input.preview.course_id],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TABLE IF EXISTS learning_origin_map; DROP TABLE IF EXISTS learning_source_vault_bindings; DROP TABLE learning_source_profiles; DROP TABLE learning_analysis_stages; DROP TABLE learning_analyses; DROP TABLE learning_analysis_previews; UPDATE schema_meta SET value='25' WHERE key='schema_version';").unwrap();
    drop(db);
    let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    assert_eq!(
        super::store::course(&db, &input.preview.course_id)
            .unwrap()
            .media_id,
        media_before
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "36"
    );
    super::analysis_store::save_preview(&db, dir.path(), &input).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let run = || {
            let mut worker =
                crate::tasks::open_media_worker_connection(&dir.path().join("library.db")).unwrap();
            barrier.wait();
            super::analysis_store::begin_analysis(&mut worker, dir.path(), &input, 1001)
        };
        let a = scope.spawn(run);
        let b = scope.spawn(run);
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analyses", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn learning_analysis_prepare_real_frames_are_stable_and_failed_decode_publishes_nothing() {
    let (dir, db, old) = stored_fixture();
    let video = dir.path().join("fixture.mp4");
    let ffmpeg = crate::runtime::tools::require(None, "ffmpeg").unwrap();
    let args = vec![
        "-nostdin".into(),
        "-v".into(),
        "error".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "color=c=blue:s=64x48:r=10".into(),
        "-t".into(),
        "1".into(),
        "-c:v".into(),
        "mpeg4".into(),
        "-n".into(),
        video.to_string_lossy().into_owned(),
    ];
    crate::runtime::process::capture(
        &ffmpeg,
        &args,
        std::time::Duration::from_secs(20),
        &|| false,
        &mut |_| {},
    )
    .unwrap();
    db.execute(
        "UPDATE learning_courses SET duration_ms=1000 WHERE id=?1",
        [&old.preview.course_id],
    )
    .unwrap();
    let hash = crate::file_digest::digest_file(&video).unwrap();
    let modified = std::fs::metadata(&video).unwrap().modified().unwrap();
    let state = crate::state::test_state(dir.path());
    let library = state.active_library_id().unwrap();
    let options = CourseAnalysisOptions {
        subtitle_id: None,
        frame_times_ms: vec![0, 100, 200, 300, 400, 500, 600],
    };
    let first =
        super::analysis::prepare(&state, &library, &old.preview.course_id, &options, None).unwrap();
    assert_eq!(first.frame_ids.len(), 7);
    let loaded =
        super::analysis::load_confirmed(&state, &library, &first.course_id, &first.preview_id)
            .unwrap();
    assert_eq!(
        loaded
            .segments
            .iter()
            .map(|s| s.frames.len())
            .collect::<Vec<_>>(),
        vec![6, 1]
    );
    for segment in &loaded.segments {
        let body = super::analysis::request_body(&loaded, segment).unwrap();
        assert!(crate::translation_service::course_budget(&body, segment.frames.len()).is_ok());
    }
    assert!(loaded
        .frames
        .iter()
        .all(|f| f.data_url.starts_with("data:image/jpeg;base64,") && f.data_url.len() <= 350000));
    let mut worker =
        crate::tasks::open_media_worker_connection(&dir.path().join("library.db")).unwrap();
    let analysis_id = super::analysis_store::begin_analysis(
        &mut worker,
        dir.path(),
        &loaded,
        chrono::Utc::now().timestamp(),
    )
    .unwrap();
    let fresh = super::analysis::get(
        &state,
        &library,
        &first.course_id,
        &analysis_id,
        None,
        0,
        50,
    )
    .unwrap();
    assert!(
        fresh.stale_reasons.is_empty(),
        "unchanged real frames are current"
    );
    assert_eq!(fresh.stages.total, 2);
    let second =
        super::analysis::prepare(&state, &library, &first.course_id, &options, None).unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_ne!(first.preview_id, second.preview_id);
    assert_eq!(crate::file_digest::digest_file(&video).unwrap(), hash);
    assert_eq!(
        std::fs::metadata(&video).unwrap().modified().unwrap(),
        modified
    );
    std::fs::write(&video, b"corrupt fixture").unwrap();
    assert!(super::analysis::prepare(&state, &library, &first.course_id, &options, None).is_err());
    assert!(!super::analysis::get(
        &state,
        &library,
        &first.course_id,
        &analysis_id,
        None,
        0,
        50
    )
    .unwrap()
    .stale_reasons
    .is_empty());
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analysis_previews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn learning_analysis_restore_revokes_pending_consent_without_losing_snapshot() {
    let (dir, db, mut input) = stored_fixture();
    let now = chrono::Utc::now().timestamp();
    input.preview.created_at = now;
    input.preview.expires_at = now + 86400;
    super::analysis_store::save_preview(&db, dir.path(), &input).unwrap();
    let before: String = db
        .query_row(
            "SELECT input_json FROM learning_analysis_previews WHERE id=?1",
            [&input.preview.preview_id],
            |r| r.get(0),
        )
        .unwrap();
    let locator: Value = serde_json::from_str(&before).unwrap();
    let path = dir.path().join(locator["relativePath"].as_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let archive = dir.path().join("preview.dccassetlib");
    crate::backup::export_library(&db, dir.path(), &archive).unwrap();
    drop(db);
    let state = crate::state::test_state(dir.path());
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &archive))
        .unwrap();
    state
        .with_library(|db, base| {
            assert!(
                super::analysis_store::load_preview(
                    db,
                    base,
                    &input.library_id,
                    &input.preview.course_id,
                    &input.preview.preview_id,
                    now
                )
                .is_err(),
                "restoring a backup cannot restore paid approval"
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_analysis_previews", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        std::fs::read(path).unwrap(),
        bytes,
        "archival material remains available for review"
    );
}
fn fixture() -> (Value, Vec<TranscriptCue>) {
    let cues = vec![
        TranscriptCue {
            id: "cue-a".into(),
            subtitle_id: "subtitle".into(),
            start_ms: 10000,
            end_ms: 12000,
            text: "设置参数".into(),
        },
        TranscriptCue {
            id: "cue-b".into(),
            subtitle_id: "subtitle".into(),
            start_ms: 40000,
            end_ms: 42000,
            text: "验证结果".into(),
        },
    ];
    (
        json!({"contentType":"tutorial","oneSentenceSummary":"合成课程","overview":"独立合成样例，非用户远程响应","chapters":[],"keyPoints":[],"tutorialSteps":[],"terms":[],"actionItems":[],"tags":[],"sections":[{"id":"section-1","title":"操作步骤","items":[{"title":"合法项","content":"已验证来源","timeMs":11000,"sourceSegmentIds":["cue-a"],"keyframeIds":[]},{"title":"错配项","content":"唯一来源","timeMs":500,"sourceSegmentIds":["cue-b"],"keyframeIds":[]}]}]}),
        cues,
    )
}
#[test]
fn learning_analysis_preserves_valid_sections_and_generates_own_corrections() {
    let (mut raw, cues) = fixture();
    let scope = CourseScope {
        start_ms: 0,
        end_ms: 60000,
        cues: &cues,
        frames: &[],
    };
    raw["sectionValidation"] = json!([{"forged":"trust me"}]);
    let result =
        parse_course_result(&raw.to_string(), &scope).expect("retain valid source-bound items");
    assert_eq!(result.sections[0].items[0].time_ms, Some(11000));
    assert_eq!(result.sections[0].items[1].time_ms, Some(40000));
    assert_eq!(result.section_validation.len(), 1);
    assert_eq!(result.section_validation[0].disposition, "corrected");
    assert_eq!(result.section_validation[0].item_index, 1);
}
#[test]
fn learning_analysis_disjoint_sources_cannot_invent_middle_time() {
    let (mut raw, cues) = fixture();
    let scope = CourseScope {
        start_ms: 0,
        end_ms: 60000,
        cues: &cues,
        frames: &[],
    };
    raw["sections"][0]["items"][1]["timeMs"] = json!(25000);
    raw["sections"][0]["items"][1]["sourceSegmentIds"] = json!(["cue-a", "cue-b"]);
    let result = parse_course_result(&raw.to_string(), &scope).unwrap();
    assert_eq!(result.sections[0].items.len(), 1);
    assert_eq!(result.section_validation[0].disposition, "omitted");
    raw["sections"][0]["items"][0]["sourceSegmentIds"] = json!(["invented"]);
    assert!(parse_course_result(&raw.to_string(), &scope).is_err());
}
#[test]
fn learning_analysis_rejects_shape_units_and_text_limits() {
    let (raw, cues) = fixture();
    let scope = CourseScope {
        start_ms: 0,
        end_ms: 60000,
        cues: &cues,
        frames: &[],
    };
    assert!(parse_course_result(&raw.to_string(), &scope).is_ok());
    let mut wrong = raw.clone();
    wrong["sections"][0]["items"][0]["timeMs"] = json!("00:11");
    assert!(parse_course_result(&wrong.to_string(), &scope).is_err());
    let mut wrong = raw.clone();
    wrong["sections"][0]["items"][0]["content"] = json!("字".repeat(2001));
    assert!(parse_course_result(&wrong.to_string(), &scope).is_err());
    let mut wrong = raw.clone();
    wrong["runCommand"] = json!("no");
    assert!(parse_course_result(&wrong.to_string(), &scope).is_err());
    let mut wrong = raw.clone();
    wrong["sections"][0]["items"][0]["sourceSegmentIds"] = json!(["invented"]);
    wrong["sections"][0]["items"][1]["sourceSegmentIds"] = json!(["invented"]);
    assert!(parse_course_result(&wrong.to_string(), &scope).is_err());
}
#[test]
fn learning_analysis_frames_require_exact_time_and_unknown_points_fail_when_nothing_remains() {
    let (mut raw, cues) = fixture();
    let frames = vec![AnalysisFrame {
        id: "frame".into(),
        time_ms: 35000,
        sha256: "a".repeat(64),
    }];
    let scope = CourseScope {
        start_ms: 0,
        end_ms: 60000,
        cues: &cues,
        frames: &frames,
    };
    raw["sections"][0]["items"][1]["sourceSegmentIds"] = json!([]);
    raw["sections"][0]["items"][1]["keyframeIds"] = json!(["frame"]);
    raw["sections"][0]["items"][1]["timeMs"] = json!(35);
    let result = parse_course_result(&raw.to_string(), &scope).unwrap();
    assert_eq!(result.sections[0].items[1].time_ms, Some(35000));
    raw["sections"] = json!([]);
    raw["keyPoints"] = json!([{"timeMs":11000,"title":"虚构来源","content":"必须略过","importance":3,"sourceSegmentIds":["invented"]}]);
    assert!(
        parse_course_result(&raw.to_string(), &scope).is_err(),
        "all source-bearing entries invalid must not report full success"
    );
}

#[test]
fn learning_analysis_segments_keep_full_cues_and_split_bytes_and_time() {
    let (_, mut cues) = fixture();
    cues[0].text = "字".repeat(4000);
    cues[1].text = "文".repeat(4000);
    let segments =
        super::analysis::plan_segments(&cues, &[], 60000).expect("complete source segments");
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].cues[0].text, "字".repeat(4000));
    assert_eq!(segments[1].cues[0].text, "文".repeat(4000));
    assert_eq!((segments[0].start_ms, segments[0].end_ms), (10000, 12000));
    let before = segments[0].input_hash.clone();
    cues[0].text.push('变');
    assert_ne!(
        super::analysis::plan_segments(&cues, &[], 60000).unwrap()[0].input_hash,
        before
    );
    cues[1].text = "后段".into();
    cues[1].start_ms = 310000;
    cues[1].end_ms = 312000;
    assert_eq!(
        super::analysis::plan_segments(&cues, &[], 400000)
            .unwrap()
            .len(),
        2
    );
    cues[0].text = "过长".repeat(3000);
    assert!(
        super::analysis::plan_segments(&cues, &[], 400000).is_err(),
        "single oversized cue must fail without truncation"
    );
    cues[0].text = "完整长段".into();
    cues[0].end_ms = 350000;
    let long = super::analysis::plan_segments(&cues[..1], &[], 400000).unwrap();
    assert_eq!(
        (long.len(), long[0].cues[0].end_ms),
        (1, 350000),
        "five-minute target never cuts a cue"
    );
}

#[test]
fn learning_analysis_segments_bound_frames_and_reject_invalid_or_excessive_sources() {
    let frames = (0..7)
        .map(|n| AnalysisFrame {
            id: format!("frame-{n}"),
            time_ms: 1000 + n * 10,
            sha256: "a".repeat(64),
        })
        .collect::<Vec<_>>();
    let segments =
        super::analysis::plan_segments(&[], &frames, 60000).expect("frame-only source plan");
    assert_eq!(
        segments.iter().map(|s| s.frames.len()).collect::<Vec<_>>(),
        vec![6, 1]
    );
    assert!(super::analysis::plan_segments(&[], &[], 60000).is_err());
    let (_, mut cues) = fixture();
    cues[1].id = cues[0].id.clone();
    assert!(super::analysis::plan_segments(&cues, &frames, 60000).is_err());
    let mut bad = frames.clone();
    bad[0].time_ms = 60001;
    assert!(super::analysis::plan_segments(&[], &bad, 60000).is_err());
    bad = frames.clone();
    bad[0].sha256 = "b".repeat(64);
    assert_ne!(
        super::analysis::plan_segments(&[], &bad, 60000).unwrap()[0].input_hash,
        segments[0].input_hash
    );
    let too_many = (0..4097)
        .map(|n| TranscriptCue {
            id: format!("cue-{n}"),
            subtitle_id: "subtitle".into(),
            start_ms: n * 100,
            end_ms: n * 100 + 10,
            text: "字".repeat(4000),
        })
        .collect::<Vec<_>>();
    assert!(
        super::analysis::plan_segments(&too_many, &[], 500000).is_err(),
        "never return a truncated first4096 segments"
    );
}

pub(super) fn stored_fixture() -> (
    tempfile::TempDir,
    rusqlite::Connection,
    PreparedCourseAnalysis,
) {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(dir.path());
    let library = state.active_library_id().unwrap();
    let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','合成课程',?2,'now','now')",rusqlite::params![media,file]).unwrap();
    db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','fixture.mp4',60000,'now')",rusqlite::params![file,media]).unwrap();
    let course = super::store::ensure_course(&db, &media).unwrap();
    let (_, cues) = fixture();
    let segments = super::analysis::plan_segments(&cues, &[], 60000).unwrap();
    let root_hash = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(
            dir.path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_bytes()
        )
    );
    let mut input = PreparedCourseAnalysis {
        preview: CourseAnalysisPreview {
            preview_id: uuid::Uuid::new_v4().to_string(),
            course_id: course.id,
            fingerprint: "f".repeat(64),
            subtitle_id: None,
            frame_ids: vec![],
            model: crate::translation_service::MODEL.into(),
            template_id: "general".into(),
            template_version: 1,
            prompt_version: "dcc-course-v1".into(),
            created_at: 1000,
            expires_at: 87400,
            stages: segments
                .iter()
                .map(|s| CourseStagePreview {
                    key: s.input_hash.clone(),
                    start_ms: s.start_ms,
                    end_ms: s.end_ms,
                    cached: false,
                    tokens_max: 5000,
                })
                .collect(),
            tokens_max: 5000,
            warnings: vec![],
        },
        library_id: library,
        root_hash,
        source_file_id: file,
        source_sha256: "a".repeat(64),
        subtitle_sha256: None,
        title: "合成课程".into(),
        duration_ms: 60000,
        segments,
        frames: vec![],
        format_version: 1,
        summary: None,
    };
    input.preview.fingerprint = super::analysis_store::fingerprint(&input).unwrap();
    (dir, db, input)
}

#[test]
fn learning_analysis_stage_rejects_oversized_encoded_results_without_partial_update() {
    let (dir, mut db, input) = stored_fixture();
    super::analysis_store::save_preview(&db, dir.path(), &input).unwrap();
    let id = super::analysis_store::begin_analysis(&mut db, dir.path(), &input, 1001).unwrap();
    let (raw, _) = fixture();
    let segment = &input.segments[0];
    let mut result = parse_course_result(
        &raw.to_string(),
        &CourseScope {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            cues: &segment.cues,
            frames: &segment.frames,
        },
    )
    .unwrap();
    result.overview = "x".repeat(1048576);
    assert!(
        super::analysis_store::record_stage(
            &mut db,
            &input,
            &id,
            &segment.input_hash,
            Ok(&result),
            Some(101)
        )
        .is_err(),
        "encoded result must remain inside the cache read limit"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM learning_analysis_stages WHERE analysis_id=?1",
            [&id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending"
    );
}

#[test]
fn learning_analysis_preview_storage_is_scoped_expiring_and_single_use() {
    let (dir, mut db, input) = stored_fixture();
    super::analysis_store::save_preview(&db, dir.path(), &input).expect("save immutable preview");
    assert!(super::analysis_store::load_preview(
        &db,
        dir.path(),
        "foreign",
        &input.preview.course_id,
        &input.preview.preview_id,
        1001
    )
    .is_err());
    assert!(super::analysis_store::load_preview(
        &db,
        dir.path(),
        &input.library_id,
        &input.preview.course_id,
        &input.preview.preview_id,
        87400
    )
    .is_err());
    let loaded = super::analysis_store::load_preview(
        &db,
        dir.path(),
        &input.library_id,
        &input.preview.course_id,
        &input.preview.preview_id,
        1001,
    )
    .unwrap();
    assert_eq!(loaded.segments[0].cues[0].text, "设置参数");
    let mut changed = loaded.clone();
    changed.preview.tokens_max = 1;
    changed.preview.stages[0].tokens_max = 1;
    assert!(
        super::analysis_store::begin_analysis(&mut db, dir.path(), &changed, 1001).is_err(),
        "a matching material fingerprint alone cannot authorize a modified cost preview"
    );
    let id = super::analysis_store::begin_analysis(&mut db, dir.path(), &loaded, 1001)
        .expect("one confirmed execution");
    assert!(super::analysis_store::begin_analysis(&mut db, dir.path(), &loaded, 1002).is_err());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM learning_analyses WHERE id=?1",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analysis_stages", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        1
    );
}

#[test]
fn learning_analysis_preview_storage_rejects_copy_tamper_and_failed_publication() {
    let (dir, db, input) = stored_fixture();
    super::analysis_store::save_preview(&db, dir.path(), &input).expect("save preview");
    let locator: String = db
        .query_row(
            "SELECT input_json FROM learning_analysis_previews WHERE id=?1",
            [&input.preview.preview_id],
            |r| r.get(0),
        )
        .unwrap();
    let locator: Value = serde_json::from_str(&locator).unwrap();
    let path = dir.path().join(locator["relativePath"].as_str().unwrap());
    let copy = dir.path().join("relocated-copy");
    std::fs::create_dir(&copy).unwrap();
    db.execute(
        "VACUUM INTO ?1",
        [copy.join("library.db").to_string_lossy().as_ref()],
    )
    .unwrap();
    std::fs::copy(dir.path().join("library.json"), copy.join("library.json")).unwrap();
    let copied_file = copy.join(locator["relativePath"].as_str().unwrap());
    std::fs::create_dir_all(copied_file.parent().unwrap()).unwrap();
    std::fs::copy(&path, copied_file).unwrap();
    let copied_db = crate::db::open_database(&copy.join("library.db")).unwrap();
    assert!(
        super::analysis_store::load_preview(
            &copied_db,
            &copy,
            &input.library_id,
            &input.preview.course_id,
            &input.preview.preview_id,
            1001
        )
        .unwrap_err()
        .contains("身份不一致"),
        "same library UUID at another physical root cannot reuse approval"
    );
    std::fs::write(&path, b"{}").unwrap();
    assert!(super::analysis_store::load_preview(
        &db,
        dir.path(),
        &input.library_id,
        &input.preview.course_id,
        &input.preview.preview_id,
        1001
    )
    .is_err());
    let (other, other_db, _) = stored_fixture();
    assert!(super::analysis_store::load_preview(
        &other_db,
        other.path(),
        &input.library_id,
        &input.preview.course_id,
        &input.preview.preview_id,
        1001
    )
    .is_err());
    let mut next = input.clone();
    next.preview.preview_id = uuid::Uuid::new_v4().to_string();
    db.execute_batch("CREATE TRIGGER refuse_course_preview BEFORE INSERT ON learning_analysis_previews BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let before = walkdir::WalkDir::new(dir.path().join("learning-data/analysis"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count();
    assert!(super::analysis_store::save_preview(&db, dir.path(), &next).is_err());
    let after = walkdir::WalkDir::new(dir.path().join("learning-data/analysis"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count();
    assert_eq!(
        after, before,
        "failed DB registration must remove only its own new snapshot"
    );
}

#[test]
fn learning_analysis_prepare_checks_real_sources_and_keeps_consent_unused_on_failure() {
    let (dir, db, old) = stored_fixture();
    let video = dir.path().join("fixture.mp4");
    std::fs::write(&video, b"registered managed video synthetic bytes").unwrap();
    let subtitle = dir.path().join("source.srt");
    std::fs::write(&subtitle,b"1\n00:00:10,000 --> 00:00:12,000\nFirst complete line\nSecond line\n\n2\n00:00:40,000 --> 00:00:42,000\nFinal step\n").unwrap();
    let track =
        super::subtitles::import(&db, dir.path(), &old.preview.course_id, &subtitle).unwrap();
    let state = crate::state::test_state(dir.path());
    let library = state.active_library_id().unwrap();
    let options = CourseAnalysisOptions {
        subtitle_id: Some(track.id.clone()),
        frame_times_ms: vec![],
    };
    let p = super::analysis::prepare(&state, &library, &old.preview.course_id, &options, None)
        .expect("prepare only reads local sources");
    let input =
        super::analysis::load_confirmed(&state, &library, &old.preview.course_id, &p.preview_id)
            .unwrap();
    assert_eq!(
        input.segments[0].cues[0].text,
        "First complete line\nSecond line"
    );
    assert_eq!(
        input.source_sha256,
        format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(b"registered managed video synthetic bytes")
        )
    );
    assert!(p.tokens_max > 0);
    assert_eq!(
        state.translation_service.usage().unwrap().deepseek_tokens,
        0,
        "preview never calls a provider"
    );
    assert!(super::analysis::load_confirmed(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        &p.course_id,
        &p.preview_id
    )
    .is_err());
    std::fs::write(&video, b"replaced source").unwrap();
    assert!(
        super::analysis::load_confirmed(&state, &library, &p.course_id, &p.preview_id).is_err()
    );
    assert!(db
        .query_row(
            "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
            [&p.preview_id],
            |r| r.get::<_, Option<i64>>(0)
        )
        .unwrap()
        .is_none());
    std::fs::write(&video, b"registered managed video synthetic bytes").unwrap();
    let registered = super::subtitles::path(&db, dir.path(), &p.course_id, &track.id).unwrap();
    std::fs::write(registered, b"changed subtitle").unwrap();
    assert!(
        super::analysis::load_confirmed(&state, &library, &p.course_id, &p.preview_id).is_err()
    );
    assert!(
        super::analysis::prepare(
            &state,
            &library,
            &p.course_id,
            &CourseAnalysisOptions::default(),
            None
        )
        .is_err(),
        "no silent transcription or AI request when no materials selected"
    );
}

#[test]
fn learning_analysis_cache_preserves_local_diagnostics_and_binds_full_material_identity() {
    let (dir, mut db, input) = stored_fixture();
    super::analysis_store::save_preview(&db, dir.path(), &input).unwrap();
    let id = super::analysis_store::begin_analysis(&mut db, dir.path(), &input, 1001).unwrap();
    let (raw, _) = fixture();
    let segment = &input.segments[0];
    let scope = CourseScope {
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        cues: &segment.cues,
        frames: &[],
    };
    let result = parse_course_result(&raw.to_string(), &scope).unwrap();
    let saved = serde_json::to_string(&result).unwrap();
    let recovered = super::analysis_result::read_saved_result(&saved, &scope)
        .expect("validated local reports survive cache reads");
    assert_eq!(recovered.section_validation.len(), 1);
    assert_eq!(recovered.section_validation[0].disposition, "corrected");
    let mut broken: Value = serde_json::from_str(&saved).unwrap();
    broken["sectionValidation"][0]["disposition"] = json!("forged");
    assert!(super::analysis_result::read_saved_result(&broken.to_string(), &scope).is_err());
    db.execute("UPDATE learning_analysis_stages SET status='completed',result_json=?1 WHERE analysis_id=?2",rusqlite::params![saved,id]).unwrap();
    assert!(
        super::analysis_store::cached_stage(&db, &input, &input.preview.stages[0])
            .unwrap()
            .is_some()
    );
    let mut changed = input.clone();
    changed.source_sha256 = "b".repeat(64);
    changed.preview.fingerprint = super::analysis_store::fingerprint(&changed).unwrap();
    assert!(
        super::analysis_store::cached_stage(&db, &changed, &changed.preview.stages[0])
            .unwrap()
            .is_none()
    );
    let mut cached = input.clone();
    cached.preview.preview_id = uuid::Uuid::new_v4().to_string();
    cached.preview.stages[0].cached = true;
    cached.preview.tokens_max = 0;
    super::analysis_store::save_preview(&db, dir.path(), &cached).unwrap();
    let next = super::analysis_store::begin_analysis(&mut db, dir.path(), &cached, 1002)
        .expect("cached stages consume no new request budget");
    assert_eq!(
        db.query_row(
            "SELECT status FROM learning_analysis_stages WHERE analysis_id=?1",
            [next],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "completed"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analyses", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
