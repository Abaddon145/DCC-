//! Course result wire format adapted from Yingliu c866508 (MIT, Copyright 2026 14582).
//! Runtime/library identity belongs to DCC, never to a source service or database.
use super::models::TranscriptCue;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseAnalysisResult {
    pub content_type: String,
    pub one_sentence_summary: String,
    pub overview: String,
    pub chapters: Vec<Chapter>,
    pub key_points: Vec<KeyPoint>,
    pub tutorial_steps: Vec<TutorialStep>,
    pub terms: Vec<Term>,
    pub action_items: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub sections: Vec<PurposeSection>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub section_validation: Vec<SectionIssue>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub keyframe_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyPoint {
    pub time_ms: u64,
    pub title: String,
    pub content: String,
    pub importance: u8,
    pub source_segment_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TutorialStep {
    pub order: u32,
    pub time_ms: u64,
    pub title: String,
    pub description: String,
    pub notes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Term {
    pub name: String,
    pub definition: String,
    pub time_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PurposeSection {
    pub id: String,
    pub title: String,
    pub items: Vec<PurposeItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PurposeItem {
    pub title: String,
    pub content: String,
    pub time_ms: Option<u64>,
    pub source_segment_ids: Vec<String>,
    pub keyframe_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionIssue {
    pub stage: String,
    pub scope_start_ms: u64,
    pub scope_end_ms: u64,
    pub section_id: String,
    pub item_index: usize,
    pub item_title: String,
    pub disposition: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisFrame {
    pub id: String,
    pub time_ms: u64,
    pub sha256: String,
}
pub struct CourseScope<'a> {
    pub start_ms: u64,
    pub end_ms: u64,
    pub cues: &'a [TranscriptCue],
    pub frames: &'a [AnalysisFrame],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub cues: Vec<TranscriptCue>,
    pub frames: Vec<AnalysisFrame>,
    pub input_hash: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseAnalysisOptions {
    pub subtitle_id: Option<String>,
    #[serde(default)]
    pub frame_times_ms: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CourseAnalysisPhase {
    Segments,
    Summary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseAnalysisRequest {
    pub course_id: String,
    pub preview_id: String,
    pub stage: CourseAnalysisPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseStagePreview {
    pub key: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub cached: bool,
    pub tokens_max: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseAnalysisPreview {
    pub preview_id: String,
    pub course_id: String,
    pub fingerprint: String,
    pub subtitle_id: Option<String>,
    pub frame_ids: Vec<String>,
    pub model: String,
    pub template_id: String,
    pub template_version: i64,
    pub prompt_version: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub stages: Vec<CourseStagePreview>,
    pub tokens_max: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseFrameImage {
    pub reference: AnalysisFrame,
    pub data_url: String,
}

/// Backend-only immutable snapshot. Never accepted as a command argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedCourseAnalysis {
    pub preview: CourseAnalysisPreview,
    pub library_id: String,
    pub root_hash: String,
    pub source_file_id: String,
    pub source_sha256: String,
    pub subtitle_sha256: Option<String>,
    pub title: String,
    pub duration_ms: u64,
    pub segments: Vec<CourseSegment>,
    pub frames: Vec<CourseFrameImage>,
    pub format_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseAnalysisStage {
    pub key: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub status: String,
    pub tokens_used: Option<u64>,
    pub error: String,
}

/// Stage metadata is paged; only the explicitly selected result is loaded.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseAnalysisDetail {
    pub id: String,
    pub course_id: String,
    pub preview_id: String,
    pub source_fingerprint: String,
    pub model: String,
    pub status: String,
    pub error: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub stale_reasons: Vec<String>,
    pub stages: crate::models::Page<CourseAnalysisStage>,
    pub selected_stage_key: Option<String>,
    pub result: Option<CourseAnalysisResult>,
}
