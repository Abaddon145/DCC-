use super::{analysis_models::*, analysis_result::parse_course_result, models::TranscriptCue};
use serde_json::{json, Value};
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
