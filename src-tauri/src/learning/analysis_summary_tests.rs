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
fn learning_analysis_summary_lineage_derives_eight_layers_and_rejects_forged_request_counts() {
    use super::{analysis_models::CourseStagePreview, analysis_store as store};
    let (dir, mut db, mut root) = super::analysis_tests::stored_fixture();
    let cues = (0..4)
        .map(|n| {
            let mut cue = root.segments[0].cues[0].clone();
            cue.id = format!("cue-{n}");
            cue.start_ms = n * 10000;
            cue.end_ms = n * 10000 + 1000;
            cue.text = "x".repeat(14000);
            cue
        })
        .collect::<Vec<_>>();
    root.segments = super::analysis::plan_segments(&cues, &[], 60000).unwrap();
    root.preview.stages = root
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
    assert_eq!(root.preview.stages.len(), 4);
    root.preview.tokens_max = 20000;
    root.preview.fingerprint = store::fingerprint(&root).unwrap();
    store::save_preview(&db, dir.path(), &root).unwrap();
    let mut parent = store::begin_analysis(&mut db, dir.path(), &root, 1001).unwrap();
    for stage in &root.preview.stages {
        store::record_stage(
            &mut db,
            &root,
            &parent,
            &stage.key,
            Ok(&source(0, 20000).result),
            Some(101),
        )
        .unwrap();
    }
    store::finish_analysis(&db, &root, &parent, "completed", "").unwrap();
    for layer in 1..=8 {
        let input = draft_summary(
            &db,
            dir.path(),
            &root.library_id,
            &root.preview.course_id,
            &parent,
            1001 + layer as i64,
        )
        .unwrap();
        let context = input.summary.as_ref().unwrap();
        assert_eq!(context.previous_requests, (layer - 1) * 4);
        assert!(context.groups.iter().all(|g| g.layer == layer as u8));
        assert_eq!(context.groups.len(), if layer == 8 { 1 } else { 4 });
        if layer == 2 {
            let mut forged = input.clone();
            forged.preview.preview_id = uuid::Uuid::new_v4().to_string();
            forged.summary.as_mut().unwrap().previous_requests = 0;
            forged.preview.fingerprint = store::fingerprint(&forged).unwrap();
            store::save_preview(&db, dir.path(), &forged).unwrap();
            assert!(
                store::begin_analysis(&mut db, dir.path(), &forged, forged.preview.created_at)
                    .is_err(),
                "local shape/hash is not proof of backend-derived ancestry"
            );
            assert_eq!(
                db.query_row(
                    "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
                    [&forged.preview.preview_id],
                    |r| r.get::<_, Option<i64>>(0)
                )
                .unwrap(),
                None
            );
        }
        store::save_preview(&db, dir.path(), &input).unwrap();
        let id =
            store::begin_analysis(&mut db, dir.path(), &input, input.preview.created_at).unwrap();
        for stage in &input.preview.stages {
            store::record_stage(
                &mut db,
                &input,
                &id,
                &stage.key,
                Ok(&source(0, if layer >= 7 { 50 } else { 20000 }).result),
                Some(101),
            )
            .unwrap();
        }
        store::finish_analysis(&db, &input, &id, "completed", "").unwrap();
        parent = id;
    }
    assert!(
        draft_summary(
            &db,
            dir.path(),
            &root.library_id,
            &root.preview.course_id,
            &parent,
            1020
        )
        .is_err(),
        "no ninth layer or redundant final request"
    );
}

#[test]
fn learning_analysis_summary_live_preview_reuses_groups_and_derives_next_layer_from_history() {
    use super::{analysis, analysis_store as store};
    let (dir, state, first) = super::analysis_runner_tests::sources();
    let library = state.active_library_id().unwrap();
    let mut db =
        crate::tasks::open_media_worker_connection(&dir.path().join("library.db")).unwrap();
    let root =
        analysis::load_confirmed(&state, &library, &first.course_id, &first.preview_id).unwrap();
    let now = chrono::Utc::now().timestamp();
    let parent = store::begin_analysis(&mut db, dir.path(), &root, now).unwrap();
    for (n, stage) in root.preview.stages.iter().enumerate() {
        store::record_stage(
            &mut db,
            &root,
            &parent,
            &stage.key,
            Ok(&source(n as u64, 10000).result),
            Some(101),
        )
        .unwrap();
    }
    store::finish_analysis(&db, &root, &parent, "completed", "").unwrap();
    let p = analysis::prepare_summary(&state, &library, &first.course_id, &parent)
        .expect("local source-checked summary preview");
    assert_eq!(p.stages.len(), 2);
    let input = analysis::load_confirmed(&state, &library, &p.course_id, &p.preview_id).unwrap();
    assert_eq!(input.summary.as_ref().unwrap().previous_requests, 0);
    assert!(input
        .summary
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .all(|g| g.layer == 1));
    let partial = store::begin_analysis(&mut db, dir.path(), &input, p.created_at).unwrap();
    store::record_stage(
        &mut db,
        &input,
        &partial,
        &p.stages[0].key,
        Ok(&source(0, 50).result),
        Some(101),
    )
    .unwrap();
    store::finish_analysis(
        &db,
        &input,
        &partial,
        "partial",
        "fixture failed second group",
    )
    .unwrap();
    assert!(
        analysis::prepare_summary(&state, &library, &p.course_id, &partial).is_err(),
        "partial groups must not be passed off as a full course"
    );
    let retry = analysis::prepare_summary(&state, &library, &first.course_id, &parent).unwrap();
    assert_eq!(
        retry.stages.iter().map(|s| s.cached).collect::<Vec<_>>(),
        vec![true, false]
    );
    assert_eq!(retry.tokens_max, retry.stages[1].tokens_max);
    let retry_input =
        analysis::load_confirmed(&state, &library, &retry.course_id, &retry.preview_id).unwrap();
    let complete =
        store::begin_analysis(&mut db, dir.path(), &retry_input, retry.created_at).unwrap();
    store::record_stage(
        &mut db,
        &retry_input,
        &complete,
        &retry.stages[1].key,
        Ok(&source(1, 50).result),
        Some(101),
    )
    .unwrap();
    store::finish_analysis(&db, &retry_input, &complete, "completed", "").unwrap();
    let read = analysis::get(
        &state,
        &library,
        &retry.course_id,
        &complete,
        Some(&retry.stages[0].key),
        0,
        1,
    )
    .unwrap();
    assert_eq!(read.result.unwrap().overview, "字".repeat(50));
    assert!(
        read.stale_reasons.is_empty(),
        "a summary must not be marked stale merely for having its own prompt version"
    );
    let next = analysis::prepare_summary(&state, &library, &retry.course_id, &complete).unwrap();
    let next_input =
        analysis::load_confirmed(&state, &library, &next.course_id, &next.preview_id).unwrap();
    let context = next_input.summary.as_ref().unwrap();
    assert_eq!(context.previous_requests, 2);
    assert_eq!(context.groups.len(), 1);
    assert_eq!(context.groups[0].layer, 2);
    let final_id =
        store::begin_analysis(&mut db, dir.path(), &next_input, next.created_at).unwrap();
    store::record_stage(
        &mut db,
        &next_input,
        &final_id,
        &next.stages[0].key,
        Ok(&source(0, 40).result),
        Some(101),
    )
    .unwrap();
    store::finish_analysis(&db, &next_input, &final_id, "completed", "").unwrap();
    assert!(
        analysis::prepare_summary(&state, &library, &next.course_id, &final_id).is_err(),
        "no paid repetition after the terminal merge"
    );
    let unused = analysis::prepare_summary(&state, &library, &retry.course_id, &complete).unwrap();
    db.execute(
        "UPDATE learning_analysis_stages SET result_json=?1 WHERE analysis_id=?2 AND stage_key=?3",
        rusqlite::params![
            serde_json::to_string(&source(0, 51).result).unwrap(),
            parent,
            root.preview.stages[0].key
        ],
    )
    .unwrap();
    assert!(
        analysis::load_confirmed(&state, &library, &unused.course_id, &unused.preview_id).is_err(),
        "an ancestor changed, not only the direct parent"
    );
    let historical = analysis::get(
        &state,
        &library,
        &next.course_id,
        &final_id,
        Some(&next.stages[0].key),
        0,
        1,
    )
    .unwrap();
    assert!(!historical.stale_reasons.is_empty());
    assert_eq!(
        historical.result.unwrap().overview,
        "字".repeat(40),
        "history remains available but cannot renew approval"
    );
    assert_eq!(
        state.translation_service.usage().unwrap().deepseek_tokens,
        0,
        "no provider request was made by previews or local history"
    );
}

#[test]
fn learning_analysis_summary_approval_is_separate_single_use_and_parent_bound() {
    use super::analysis_store as store;
    let (dir, mut db, root) = super::analysis_tests::stored_fixture();
    let old_bytes = serde_json::to_vec(&root).unwrap();
    assert!(serde_json::to_value(&root)
        .unwrap()
        .get("summary")
        .is_none());
    store::save_preview(&db, dir.path(), &root).unwrap();
    let parent = store::begin_analysis(&mut db, dir.path(), &root, 1001).unwrap();
    let result = source(0, 20).result;
    store::record_stage(
        &mut db,
        &root,
        &parent,
        &root.preview.stages[0].key,
        Ok(&result),
        Some(101),
    )
    .unwrap();
    store::finish_analysis(&db, &root, &parent, "completed", "").unwrap();
    let draft = draft_summary(
        &db,
        dir.path(),
        &root.library_id,
        &root.preview.course_id,
        &parent,
        1002,
    )
    .expect("completed stages produce a separate summary preview, not another segment approval");
    assert_ne!(draft.preview.preview_id, root.preview.preview_id);
    assert_ne!(draft.preview.fingerprint, root.preview.fingerprint);
    assert_eq!(draft.preview.prompt_version, "dcc-course-summary-v1");
    assert_eq!(draft.preview.stages.len(), 1);
    assert_eq!(draft.preview.stages[0].start_ms, 10000);
    assert_eq!(draft.preview.stages[0].end_ms, 42000);
    assert!(draft.preview.tokens_max > 0);
    assert!(
        store::load_preview(
            &db,
            dir.path(),
            &root.library_id,
            &root.preview.course_id,
            &draft.preview.preview_id,
            1002
        )
        .is_err(),
        "a draft is not registered approval"
    );
    store::save_preview(&db, dir.path(), &draft).unwrap();
    let loaded = store::load_preview(
        &db,
        dir.path(),
        &root.library_id,
        &root.preview.course_id,
        &draft.preview.preview_id,
        1002,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&loaded).unwrap(),
        serde_json::to_vec(&draft).unwrap()
    );
    let child = store::begin_analysis(&mut db, dir.path(), &loaded, 1003).unwrap();
    assert_ne!(child, parent);
    assert!(
        store::finish_analysis(&db, &loaded, &child, "completed", "").is_err(),
        "pending groups cannot be promoted to a completed course summary"
    );
    assert!(store::begin_analysis(&mut db, dir.path(), &loaded, 1004).is_err());
    assert!(store::load_preview(
        &db,
        dir.path(),
        &root.library_id,
        &root.preview.course_id,
        &loaded.preview.preview_id,
        1004
    )
    .is_err());
    let old_loaded =
        serde_json::from_slice::<super::analysis_models::PreparedCourseAnalysis>(&old_bytes)
            .unwrap();
    assert_eq!(
        serde_json::to_vec(&old_loaded).unwrap(),
        old_bytes,
        "legacy snapshots must retain their exact digest"
    );

    let retry = draft_summary(
        &db,
        dir.path(),
        &root.library_id,
        &root.preview.course_id,
        &parent,
        1005,
    )
    .unwrap();
    store::save_preview(&db, dir.path(), &retry).unwrap();
    db.execute(
        "UPDATE learning_analysis_stages SET result_json=?1 WHERE analysis_id=?2",
        rusqlite::params![
            serde_json::to_string(&source(0, 21).result).unwrap(),
            parent
        ],
    )
    .unwrap();
    assert!(
        store::begin_analysis(&mut db, dir.path(), &retry, 1006).is_err(),
        "changed parent material cannot consume the old approval"
    );
    let consumed: Option<i64> = db
        .query_row(
            "SELECT consumed_at FROM learning_analysis_previews WHERE id=?1",
            [&retry.preview.preview_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(consumed, None);
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_analyses", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2,
        "rejection must not publish partial analysis rows"
    );
}

#[test]
fn learning_analysis_summary_loads_only_complete_bound_and_verified_stage_results() {
    use super::{
        analysis_models::CourseScope, analysis_result::parse_course_result, analysis_store as store,
    };
    let (dir, mut db, input) = super::analysis_tests::stored_fixture();
    store::save_preview(&db, dir.path(), &input).unwrap();
    let id = store::begin_analysis(&mut db, dir.path(), &input, 1001).unwrap();
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
