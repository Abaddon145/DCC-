use super::{excerpt_runtime::*, excerpts::ExcerptSelection, note_nodes::TimeRange};
use sha2::{Digest, Sha256};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
fn fixture(
    real: bool,
) -> (
    tempfile::TempDir,
    crate::state::AppState,
    String,
    String,
    ExcerptSelection,
) {
    let (root, db, input) = super::analysis_tests::stored_fixture();
    let path = root.path().join("fixture.mp4");
    if real {
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
            path.to_string_lossy().into_owned(),
        ];
        crate::runtime::process::capture(
            &ffmpeg,
            &args,
            std::time::Duration::from_secs(20),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
    } else {
        fs::write(&path, b"synthetic invalid video, not a playable source").unwrap();
    }
    let sha = crate::file_digest::digest_file(&path).unwrap().1;
    db.execute(
        "UPDATE media_files SET checksum=?1,duration_ms=1000 WHERE id=?2",
        rusqlite::params![sha, input.source_file_id],
    )
    .unwrap();
    db.execute(
        "UPDATE learning_courses SET duration_ms=1000 WHERE id=?1",
        [&input.preview.course_id],
    )
    .unwrap();
    drop(db);
    let state = crate::state::test_state(root.path());
    let library = state.active_library_id().unwrap();
    let selection = ExcerptSelection {
        ranges: vec![TimeRange {
            start_ms: 0,
            end_ms: 1000,
        }],
        comment: "原始文字草稿\n保留第二行".into(),
        ..Default::default()
    };
    (root, state, library, input.preview.course_id, selection)
}
#[test]
fn learning_excerpt_runtime_text_uses_actual_media_digest_without_writing_notes() {
    let (root, state, library, course, selection) = fixture(false);
    let bytes = fs::read(root.path().join("fixture.mp4")).unwrap();
    let ready = prepare_in_library(&state, &library, &course, &selection, None, &|| false).unwrap();
    assert_eq!(ready.source_sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(
        ready.document["content"][0]["attrs"]["fingerprint"],
        ready.source_sha256
    );
    assert_eq!(ready.text.comment, selection.comment);
    assert_eq!(ready.duration_ms, 1000);
    assert!(ready.frame.is_none());
    assert!(ready.warnings.is_empty());
    assert_eq!(ready.library_id, library);
    assert_eq!(ready.root_hash.len(), 64);
    assert_eq!(fs::read(root.path().join("fixture.mp4")).unwrap(), bytes);
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_attachments", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_excerpt_runtime_real_single_frame_is_bounded_and_does_not_modify_source() {
    use base64::Engine;
    let (root, state, library, course, mut selection) = fixture(true);
    selection.frame_time_ms = Some(400);
    let original = crate::file_digest::digest_file(&root.path().join("fixture.mp4")).unwrap();
    let ready = prepare_in_library(&state, &library, &course, &selection, None, &|| false).unwrap();
    let frame = ready.frame.unwrap();
    assert!(ready.warnings.is_empty());
    assert_eq!(frame.time_ms, 400);
    assert!(frame.data_url.len() <= 350000);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            frame
                .data_url
                .strip_prefix("data:image/jpeg;base64,")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(frame.sha256, format!("{:x}", Sha256::digest(&bytes)));
    let picture = image::load_from_memory(&bytes).unwrap();
    assert!(picture.width() > 0 && picture.width() <= 1024);
    assert!(picture.height() > 0 && picture.height() <= 1024);
    assert_eq!(
        crate::file_digest::digest_file(&root.path().join("fixture.mp4")).unwrap(),
        original
    );
}
#[test]
fn learning_excerpt_runtime_failed_frame_preserves_text_without_fabricating_an_image() {
    let (_root, state, library, course, mut selection) = fixture(false);
    selection.frame_time_ms = Some(400);
    let ready = prepare_in_library(&state, &library, &course, &selection, None, &|| false).unwrap();
    assert!(ready.frame.is_none());
    assert!(!ready.warnings.is_empty());
    assert_eq!(ready.text.comment, selection.comment);
    assert!(ready.document.to_string().contains("保留第二行"));
}
#[test]
fn learning_excerpt_runtime_scope_cancellation_maintenance_and_changed_bytes_are_errors() {
    let (root, state, library, course, selection) = fixture(false);
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| false).is_ok());
    assert!(prepare_in_library(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        &course,
        &selection,
        None,
        &|| false
    )
    .is_err());
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| true).is_err());
    let calls = AtomicUsize::new(0);
    assert!(
        prepare_in_library(&state, &library, &course, &selection, None, &|| calls
            .fetch_add(1, Ordering::SeqCst)
            > 1)
        .is_err()
    );
    let db = state
        .with_active_mut(|active| Ok(active.connection.take()))
        .unwrap();
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| false).is_err());
    state
        .with_active_mut(|active| {
            active.connection = db;
            Ok(())
        })
        .unwrap();
    fs::write(root.path().join("fixture.mp4"), b"changed source").unwrap();
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| false).is_err());
}

#[test]
fn learning_excerpt_runtime_rejects_a_primary_file_owned_by_a_different_medium() {
    let (root, state, library, course, selection) = fixture(false);
    let bytes = b"another medium, not this course";
    fs::write(root.path().join("other.mp4"), bytes).unwrap();
    state.with_library(|db,_|{
        let media=uuid::Uuid::new_v4().to_string();let file=uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','other',?2,'now','now')",rusqlite::params![media,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,checksum,duration_ms,created_at) VALUES(?1,?2,'main','other.mp4','other.mp4','other.mp4',?3,1000,'now')",rusqlite::params![file,media,format!("{:x}",Sha256::digest(bytes))]).unwrap();
        db.execute("UPDATE media_entries SET primary_file_id=?1 WHERE id=(SELECT media_entry_id FROM learning_courses WHERE id=?2)",rusqlite::params![file,course]).unwrap();Ok(())
    }).unwrap();
    assert!(super::analysis::live_source_controlled(
        &state,
        &library,
        &course,
        None,
        false,
        &|| false
    )
    .is_err());
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| false).is_err());
}

#[test]
fn learning_excerpt_runtime_verifies_subtitle_bytes_and_retains_the_complete_selected_cue() {
    let (root, state, library, course, mut selection) = fixture(false);
    let source = root.path().join("source.srt");
    fs::write(
        &source,
        "1\n00:00:00,100 --> 00:00:00,400\n完整原文\n第二行\n",
    )
    .unwrap();
    let subtitle = state
        .with_library(|db, base| super::subtitles::import(db, base, &course, &source))
        .unwrap();
    selection.subtitle_id = Some(subtitle.id.clone());
    selection.cue_ids = vec![format!("{}:0", subtitle.id)];
    selection.ranges.clear();
    let ready = prepare_in_library(&state, &library, &course, &selection, None, &|| false).unwrap();
    assert_eq!(ready.text.cues[0].text, "完整原文\n第二行");
    assert_eq!(ready.text.subtitle_sha256, Some(subtitle.sha256));
    let relative = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT relative_path FROM learning_subtitles WHERE id=?1",
                [&subtitle.id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
        .unwrap();
    fs::write(root.path().join(relative), "changed subtitle").unwrap();
    assert!(prepare_in_library(&state, &library, &course, &selection, None, &|| false).is_err());
}

#[test]
fn learning_excerpt_runtime_discards_results_if_the_same_library_id_moves_during_source_read() {
    let (root, state, library, course, selection) = fixture(false);
    let calls = AtomicUsize::new(0);
    let result = prepare_in_library(&state, &library, &course, &selection, None, &|| {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            state
                .with_active_mut(|active| {
                    active.base_dir = root.path().join("other-location");
                    Ok(())
                })
                .unwrap();
        }
        false
    });
    assert!(result.is_err());
    assert!(calls.load(Ordering::SeqCst) > 1);
}

#[cfg(windows)]
#[test]
fn learning_excerpt_runtime_source_lease_never_waits_for_a_library_lock_waiting_to_replace_the_file(
) {
    use std::{
        sync::{atomic::AtomicBool, mpsc},
        time::{Duration, Instant},
    };
    let (root, state, library, course, selection) = fixture(false);
    let (request_tx, request_rx) = mpsc::channel();
    let (locked_tx, locked_rx) = mpsc::channel();
    let timed_out = AtomicBool::new(false);
    let calls = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let current = &state;
        let timeout = &timed_out;
        let path = root.path().join("fixture.mp4");
        let writer = scope.spawn(move || {
            request_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            let active = current.active.lock().unwrap();
            locked_tx.send(()).unwrap();
            let started = Instant::now();
            let replacement = b"replacement while library metadata is locked";
            loop {
                if fs::write(&path, replacement).is_ok() {
                    active
                        .connection
                        .as_ref()
                        .unwrap()
                        .execute(
                            "UPDATE media_files SET checksum=?1",
                            [format!("{:x}", Sha256::digest(replacement))],
                        )
                        .unwrap();
                    break;
                }
                if started.elapsed() > Duration::from_secs(1) {
                    timeout.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let result = prepare_in_library(&state, &library, &course, &selection, None, &|| {
            if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                request_tx.send(()).unwrap();
                locked_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            false
        });
        writer.join().unwrap();
        assert!(
            !timed_out.load(Ordering::SeqCst),
            "read lease and library lock formed a wait cycle"
        );
        assert!(
            result.is_err(),
            "changed media must not produce a prepared excerpt"
        );
    });
}
