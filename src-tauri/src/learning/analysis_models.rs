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
#[derive(Debug, Clone)]
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
