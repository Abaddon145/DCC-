use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub id: String,
    pub media_id: String,
    pub title: String,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub rate: f64,
    pub completed: bool,
    pub progress_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub id: String,
    pub name: String,
    pub description: String,
    pub revision: i64,
    pub ordered_course_ids: Vec<String>,
    pub category_id: Option<String>,
    #[serde(default)]
    pub cover_file_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningCategory {
    pub id: String,
    pub name: String,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressInput {
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub rate: f64,
    pub completed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    #[serde(flatten)]
    pub input: ProgressInput,
    pub revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeriesInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub expected_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSubtitle {
    pub id: String,
    pub course_id: String,
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub cue_count: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptCue {
    pub id: String,
    pub subtitle_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}
