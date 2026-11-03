use super::{excerpts::*, note_nodes::TimeRange};
use crate::knowledge::{self, NoteInput};
use serde_json::json;
use std::fs;

const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\n<script>alert(1)</script>\n完整第二行\n\n2\n00:00:02,000 --> 00:00:03,000\n相邻段\n\n3\n00:00:05,000 --> 00:00:06,000\n未选中的内容\n\n4\n00:00:08,000 --> 00:00:10,000\n远处段落\n完整原文\n\n5\n00:00:09,000 --> 00:00:11,000\n重叠原文\n";
fn fixture() -> (
    tempfile::TempDir,
    rusqlite::Connection,
    String,
    ExcerptSelection,
) {
    let (root, db, input) = super::analysis_tests::stored_fixture();
    let path = root.path().join("excerpt.srt");
    fs::write(&path, SRT).unwrap();
    let subtitle =
        super::subtitles::import(&db, root.path(), &input.preview.course_id, &path).unwrap();
    let selection = ExcerptSelection {
        subtitle_id: Some(subtitle.id.clone()),
        cue_ids: [4, 1, 3, 0]
            .map(|n| format!("{}:{n}", subtitle.id))
            .to_vec(),
        comment: "保留备注\n第二行".into(),
        frame_time_ms: Some(9000),
        ..Default::default()
    };
    (root, db, input.preview.course_id, selection)
}
fn range(start_ms: u64, end_ms: u64) -> TimeRange {
    TimeRange { start_ms, end_ms }
}

#[test]
fn learning_excerpt_keeps_full_verified_cues_and_disjoint_ranges_without_writes() {
    let (root, db, course, selection) = fixture();
    let changes = db.total_changes();
    let draft = prepare_text(&db, root.path(), &course, &selection).unwrap();
    assert_eq!(draft.ranges, vec![range(1000, 3000), range(8000, 11000)]);
    assert_eq!(
        draft.cues.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        [0, 1, 3, 4].map(|n| format!("{}:{n}", selection.subtitle_id.as_ref().unwrap()))
    );
    assert_eq!(draft.cues[0].text, "<script>alert(1)</script>\n完整第二行");
    assert_eq!(draft.cues[2].text, "远处段落\n完整原文");
    assert_eq!(draft.comment, selection.comment);
    assert_eq!(draft.frame_time_ms, Some(9000));
    assert_eq!(draft.subtitle_sha256.as_ref().unwrap().len(), 64);
    assert_eq!(db.total_changes(), changes);
    assert_eq!(
        fs::read(root.path().join("excerpt.srt")).unwrap(),
        SRT.as_bytes()
    );
    let mut exact = selection.clone();
    exact.ranges = draft.ranges;
    assert!(prepare_text(&db, root.path(), &course, &exact).is_ok());
    exact.ranges = vec![range(1000, 11000)];
    assert!(prepare_text(&db, root.path(), &course, &exact).is_err());
}
#[test]
fn learning_excerpt_reparses_original_subtitles_instead_of_trusting_cached_text() {
    let (root, db, course, selection) = fixture();
    db.execute("UPDATE learning_subtitle_cues SET text='forged cache'", [])
        .unwrap();
    let draft = prepare_text(&db, root.path(), &course, &selection).unwrap();
    assert!(draft.cues.iter().all(|c| c.text != "forged cache"));
    let relative: String = db
        .query_row(
            "SELECT relative_path FROM learning_subtitles WHERE id=?1",
            [selection.subtitle_id.as_ref().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    fs::write(root.path().join(relative), "changed source").unwrap();
    assert!(prepare_text(&db, root.path(), &course, &selection).is_err());
}
#[test]
fn learning_excerpt_rejects_missing_duplicate_cross_course_and_oversized_selections() {
    let (root, db, course, selection) = fixture();
    assert!(prepare_text(&db, root.path(), &course, &selection).is_ok());
    let mut bad = selection.clone();
    bad.cue_ids.push(bad.cue_ids[0].clone());
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    bad = selection.clone();
    bad.cue_ids[0] = "unknown-cue".into();
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    bad = selection.clone();
    bad.cue_ids = (0..257).map(|n| format!("unknown:{n}")).collect();
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    bad = selection.clone();
    bad.subtitle_id = None;
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    bad = selection.clone();
    bad.comment = "文".repeat(6667);
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    bad = selection.clone();
    bad.comment = "bad\0comment".into();
    assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    let media = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) SELECT ?1,kind,'other',primary_file_id,created_at,updated_at FROM media_entries LIMIT 1",[&media]).unwrap();
    let other = super::store::ensure_course(&db, &media).unwrap();
    assert!(prepare_text(&db, root.path(), &other.id, &selection).is_err());
    // Simulate an older client changing media outside the current guard.
    db.execute_batch("DROP TRIGGER media_workspace_protect_learning_trash")
        .unwrap();
    db.execute("UPDATE media_entries SET deleted_at='now' WHERE id=(SELECT media_entry_id FROM learning_courses WHERE id=?1)",[&course]).unwrap();
    assert!(prepare_text(&db, root.path(), &course, &selection).is_err());
}
#[test]
fn learning_excerpt_rejects_excess_selected_text_without_truncating_source() {
    let (root, db, course, _) = fixture();
    let body = "x".repeat(16000);
    let source = (1..=67)
        .map(|n| format!("{n}\n00:00:01,000 --> 00:00:02,000\n{body}\n\n"))
        .collect::<String>();
    let path = root.path().join("large.srt");
    fs::write(&path, &source).unwrap();
    let subtitle = super::subtitles::import(&db, root.path(), &course, &path).unwrap();
    let selection = ExcerptSelection {
        subtitle_id: Some(subtitle.id.clone()),
        cue_ids: (0..67).map(|n| format!("{}:{n}", subtitle.id)).collect(),
        ..Default::default()
    };
    let error = prepare_text(&db, root.path(), &course, &selection).unwrap_err();
    assert!(error.contains("1MiB"));
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
}
#[test]
fn learning_excerpt_manual_ranges_and_frame_must_be_real_and_within_known_duration() {
    let (root, db, course, _) = fixture();
    let good = ExcerptSelection {
        ranges: vec![range(1000, 2000), range(5000, 6000)],
        comment: "备注".into(),
        frame_time_ms: Some(5500),
        ..Default::default()
    };
    assert!(prepare_text(&db, root.path(), &course, &good).is_ok());
    for ranges in [
        vec![],
        vec![range(1000, 1000)],
        vec![range(2000, 1000)],
        vec![range(5000, 6000), range(1000, 2000)],
        vec![range(1000, 2000), range(1500, 2500)],
        vec![range(59999, 60001)],
    ] {
        let mut bad = good.clone();
        bad.ranges = ranges;
        assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    }
    for time in [0, 3000, 60000, u64::MAX] {
        let mut bad = good.clone();
        bad.frame_time_ms = Some(time);
        assert!(prepare_text(&db, root.path(), &course, &bad).is_err());
    }
    db.execute(
        "UPDATE learning_courses SET duration_ms=NULL WHERE id=?1",
        [&course],
    )
    .unwrap();
    assert!(prepare_text(&db, root.path(), &course, &good).is_err());
}
#[test]
fn learning_excerpt_document_uses_existing_inert_note_model_and_media_backlinks() {
    let (root, mut db, course, selection) = fixture();
    let draft = prepare_text(&db, root.path(), &course, &selection).unwrap();
    let doc = document(&draft, &"a".repeat(64)).unwrap();
    assert_eq!(doc["content"][0]["type"], "learningReference");
    assert!(document(&draft, "not-a-sha").is_err());
    let note = knowledge::save(
        &mut db,
        NoteInput {
            id: None,
            expected_revision: None,
            notebook_id: None,
            title: "摘录".into(),
            document: doc,
            tags: vec![],
            favorite: false,
            journal_date: None,
        },
    )
    .unwrap();
    assert!(note.plain_text.contains("<script>alert(1)</script>"));
    assert!(note.plain_text.contains("完整第二行"));
    assert!(note.plain_text.contains("保留备注"));
    assert!(!note.plain_text.contains("未选中的内容"));
    assert_eq!(
        knowledge::search(&db, "完整第二行", None, false, false, 0, 20)
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        knowledge::references(&db, &note.id).unwrap()[0].target_id,
        draft.media_id
    );
    let mut value = serde_json::to_value(&selection).unwrap();
    value["text"] = json!("forged body");
    assert!(serde_json::from_value::<ExcerptSelection>(value).is_err());
}

#[test]
fn learning_excerpt_rejects_excess_break_nodes_before_building_a_large_editor_document() {
    let (root, db, course, _) = fixture();
    let selection = ExcerptSelection {
        ranges: vec![range(0, 1000)],
        comment: "行\n".repeat(5000),
        ..Default::default()
    };
    let draft = prepare_text(&db, root.path(), &course, &selection).unwrap();
    let result = document(&draft, &"a".repeat(64));
    assert!(
        result.is_err(),
        "excess break nodes must be rejected before document allocation"
    );
    let error = result.unwrap_err();
    assert!(error.contains("4096"));
    assert_eq!(draft.comment, selection.comment);
}
