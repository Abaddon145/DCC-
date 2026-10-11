use super::models::Asset;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationInput {
    pub source_asset_id: String,
    pub target_asset_id: String,
    pub relation_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRelation {
    pub id: String,
    pub source_asset_id: String,
    pub target_asset_id: String,
    pub relation_type: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphRequest {
    pub asset_id: String,
    pub locale: String,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetGraph {
    pub center: Asset,
    pub nodes: Vec<Asset>,
    pub relations: Vec<AssetRelation>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisMode {
    Text,
    Visual,
}
impl AnalysisMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Visual => "visual",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisRequest {
    pub asset_id: String,
    pub preview_id: String,
    pub force: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisImage {
    pub data_url: String,
    pub timestamp: Option<f64>,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisInputPreview {
    pub id: String,
    pub asset_id: String,
    pub model: String,
    pub mode: AnalysisMode,
    pub summary: String,
    pub fingerprint: String,
    pub images: Vec<AnalysisImage>,
    pub estimated_tokens: u64,
    pub warnings: Vec<String>,
    pub cached_analysis_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisAnswer {
    pub description: String,
    pub keywords: Vec<String>,
    pub scene: String,
    pub style: String,
    pub camera: String,
    pub lighting: String,
    pub mood: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetAnalysis {
    pub id: String,
    pub asset_id: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub scene: String,
    pub style: String,
    pub camera: String,
    pub lighting: String,
    pub mood: String,
    pub model: String,
    pub mode: AnalysisMode,
    pub input_fingerprint: String,
    pub format_version: i64,
    pub created_at: String,
    pub actual_tokens: Option<u64>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptInput {
    pub asset_id: Option<String>,
    pub expected_revision: Option<i64>,
    pub name: String,
    pub description: String,
    pub content: String,
    pub model: String,
    pub parameters: serde_json::Value,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAsset {
    pub asset: super::models::Asset,
    pub content: String,
    pub model: String,
    pub parameters: serde_json::Value,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}
