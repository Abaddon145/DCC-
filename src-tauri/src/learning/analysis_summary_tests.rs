use super::{analysis_models::CourseAnalysisResult, analysis_summary::*};

fn source(n: u64, chars: usize) -> SummarySource {
    let result:CourseAnalysisResult=serde_json::from_value(serde_json::json!({"contentType":"tutorial","oneSentenceSummary":"学习摘要","overview":"字".repeat(chars),"chapters":[],"keyPoints":[],"tutorialSteps":[],"terms":[],"actionItems":[format!("行动{n}")],"tags":[],"sections":[],"warnings":[]})).unwrap();
    SummarySource {
        key: format!("{n:064x}"),
        start_ms: n * 1000,
        end_ms: n * 1000 + 1000,
        result,
    }
}

#[test]
fn learning_analysis_summary_loads_only_complete_bound_and_verified_stage_results() {
    use super::{
        analysis_models::CourseScope, analysis_result::parse_course_result, analysis_store as store,
    };
    let (dir, mut db, input) = super::analysis_tests::stored_fixture();
    store::save_preview(&db, dir.path(), &input).unwrap();
    let id = store::begin_analysis(&mut db, &input, 1001).unwrap();
    assert!(
        plan_saved_segments(
            &db,
            dir.path(),
            &input.library_id,
            &input.preview.course_id,
            &id
        )
        .is_err(),
        "pending is not complete source material"
    );
    let segment = &input.segments[0];
    let mut raw = serde_json::to_value(source(0, 20).result).unwrap();
    raw["keyPoints"] = serde_json::json!([{"timeMs":11000,"title":"重点","content":"内容","importance":3,"sourceSegmentIds":[segment.cues[0].id]}]);
    raw["sections"] = serde_json::json!([{"id":"general","title":"用途","items":[{"title":"校正","content":"按真实引用定位","timeMs":500,"sourceSegmentIds":[segment.cues[0].id],"keyframeIds":[]}]}]);
    let result = parse_course_result(
        &raw.to_string(),
        &CourseScope {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            cues: &segment.cues,
            frames: &segment.frames,
        },
    )
    .unwrap();
    store::record_stage(
        &mut db,
        &input,
        &id,
        &segment.input_hash,
        Ok(&result),
        Some(101),
    )
    .unwrap();
    store::finish_analysis(&db, &input, &id, "completed", "").unwrap();
    let (snapshot, groups) = plan_saved_segments(
        &db,
        dir.path(),
        &input.library_id,
        &input.preview.course_id,
        &id,
    )
    .expect("load entire verified completed results");
    assert_eq!(snapshot.preview.fingerprint, input.preview.fingerprint);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].sources[0].result.overview, "字".repeat(20));
    assert_eq!(groups[0].sources[0].result.key_points[0].time_ms, 11000);
    assert_eq!(groups[0].sources[0].result.section_validation.len(), 1);
    assert_eq!(
        groups[0].sources[0].result.sections[0].items[0].time_ms,
        Some(10000)
    );
    assert!(plan_saved_segments(
        &db,
        dir.path(),
        &uuid::Uuid::new_v4().to_string(),
        &input.preview.course_id,
        &id
    )
    .is_err());
    assert!(plan_saved_segments(
        &db,
        dir.path(),
        &input.library_id,
        &uuid::Uuid::new_v4().to_string(),
        &id
    )
    .is_err());
    db.execute(
        "UPDATE learning_analysis_stages SET result_json='broken' WHERE analysis_id=?1",
        [&id],
    )
    .unwrap();
    assert!(
        plan_saved_segments(
            &db,
            dir.path(),
            &input.library_id,
            &input.preview.course_id,
            &id
        )
        .is_err(),
        "corrupt stage is not silently skipped"
    );
    db.execute(
        "UPDATE learning_analysis_stages SET result_json=?1,end_ms=59000 WHERE analysis_id=?2",
        rusqlite::params![serde_json::to_string(&result).unwrap(), id],
    )
    .unwrap();
    assert!(
        plan_saved_segments(
            &db,
            dir.path(),
            &input.library_id,
            &input.preview.course_id,
            &id
        )
        .is_err(),
        "stage bounds must match immutable source snapshot"
    );
    db.execute(
        "UPDATE learning_analysis_stages SET end_ms=?1,status='pending' WHERE analysis_id=?2",
        rusqlite::params![segment.end_ms, id],
    )
    .unwrap();
    assert!(
        plan_saved_segments(
            &db,
            dir.path(),
            &input.library_id,
            &input.preview.course_id,
            &id
        )
        .is_err(),
        "completed header cannot promote a pending stage"
    );
    db.execute(
        "DELETE FROM learning_analysis_stages WHERE analysis_id=?1",
        [&id],
    )
    .unwrap();
    assert!(
        plan_saved_segments(
            &db,
            dir.path(),
            &input.library_id,
            &input.preview.course_id,
            &id
        )
        .is_err(),
        "missing rows cannot form a whole-course summary"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analysis_previews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1,
        "drafting cannot create fresh approval"
    );
}

#[test]
fn learning_analysis_summary_groups_whole_results_without_truncating_or_network() {
    let groups = plan_summary("UE学习", 60000, 1, 0, (0..4).map(|n| Ok(source(n, 10000))))
        .expect("whole source grouping");
    assert_eq!(
        groups.iter().map(|g| g.sources.len()).collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert_eq!((groups[0].start_ms, groups[0].end_ms), (0, 3000));
    assert_eq!((groups[1].start_ms, groups[1].end_ms), (3000, 4000));
    assert_eq!(
        groups
            .iter()
            .flat_map(|g| &g.sources)
            .map(|s| s.result.overview.chars().count())
            .collect::<Vec<_>>(),
        vec![10000; 4]
    );
    for group in &groups {
        let body = request_body("UE学习", group).unwrap();
        assert_eq!(
            crate::translation_service::course_budget(&body, 0).unwrap(),
            group.tokens_max
        );
        let material: serde_json::Value =
            serde_json::from_str(body["messages"][1]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(
            material["sources"].as_array().unwrap().len(),
            group.sources.len()
        );
        assert!(body.to_string().len() < 120000);
    }
}

#[test]
fn learning_analysis_summary_rejects_unapproved_depth_capacity_and_partial_input() {
    assert!(
        plan_summary("课程", 70000, 1, 0, (0..64).map(|n| Ok(source(n, 20000)))).is_err(),
        "nonterminal groups need capacity for the final merge"
    );
    assert!(
        plan_summary(
            "课程",
            60000,
            8,
            0,
            [Ok(source(0, 20000)), Ok(source(1, 20000))]
        )
        .is_err(),
        "the final layer cannot promise a ninth-layer merge"
    );
    for layer in [0, 9] {
        assert!(plan_summary("课程", 60000, layer, 0, [Ok(source(0, 10))]).is_err());
    }
    assert!(plan_summary("课程", 60000, 1, 64, [Ok(source(0, 10))]).is_err());
    assert!(
        plan_summary("课程", 70000, 1, 0, (0..65).map(|n| Ok(source(n, 20000)))).is_err(),
        "65 groups cannot silently return64"
    );
    assert!(plan_summary(
        "课程",
        60000,
        1,
        63,
        [Ok(source(0, 20000)), Ok(source(1, 20000))]
    )
    .is_err());
    assert!(
        plan_summary("课程", 60000, 1, 0, [Ok(source(0, 40000))]).is_err(),
        "a single oversize result cannot be cut"
    );
    assert!(
        plan_summary(
            "课程",
            60000,
            1,
            0,
            [Ok(source(0, 10)), Err("source cache damaged".into())]
        )
        .is_err(),
        "source errors cannot produce a partial success plan"
    );
    assert!(plan_summary("课程", 60000, 1, 0, []).is_err());
    assert!(plan_summary("课程", 60000, 1, 0, [Ok(source(0, 10)), Ok(source(0, 10))]).is_err());
    let last = plan_summary("课程", 60000, 8, 63, [Ok(source(0, 10))])
        .expect("bounded final layer still works");
    assert_eq!(last.len(), 1);
}

#[test]
fn learning_analysis_summary_fingerprint_tracks_complete_result_and_layer() {
    let original = plan_summary("课程", 60000, 1, 0, [Ok(source(0, 10))]).unwrap();
    let mut changed = source(0, 10);
    changed.result.action_items.push("后续练习".into());
    assert_ne!(
        original[0].key,
        plan_summary("课程", 60000, 1, 0, [Ok(changed)]).unwrap()[0].key
    );
    assert_ne!(
        original[0].key,
        plan_summary("课程", 60000, 2, 1, [Ok(source(0, 10))]).unwrap()[0].key
    );
    assert!(plan_summary("课程", 60000, 1, 0, [Ok(source(1, 10)), Ok(source(0, 10))]).is_err());
    let mut escaped = source(0, 10);
    escaped.result.overview = "\"\\\n".repeat(4000);
    let title = format!("课程{}", "\"\\\n".repeat(1000));
    for group in plan_summary(&title, 60000, 1, 0, [Ok(escaped)]).unwrap() {
        let body = request_body(&title, &group).unwrap();
        assert!(
            body.to_string().len() <= 120000,
            "encoded request, not only visible characters, is bounded"
        );
    }
}
