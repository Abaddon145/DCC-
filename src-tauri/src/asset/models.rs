use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreOrigin {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFileRef {
    pub kind: String,
    pub id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFile {
    pub id: String,
    pub asset_id: String,
    pub source_ref: SourceFileRef,
    pub role: String,
    pub filename: String,
    pub extension: String,
    pub size: u64,
    pub hash: String,
    pub created_at: String,
    pub available: bool,
    pub preview: Option<SourceFileRef>,
    pub media_entry_id: Option<String>,
    pub logical_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadata {
    pub file_id: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub duration: Option<f64>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMetadataValues {
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub duration: Option<f64>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub asset_type: String,
    pub description: String,
    pub thumbnail: Option<SourceFileRef>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub origin: CoreOrigin,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSource {
    pub id: String,
    pub asset_id: String,
    pub source_type: String,
    pub url: String,
    pub author: String,
    pub created_at: String,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersion {
    pub id: String,
    pub asset_id: String,
    pub file_id: String,
    pub version: String,
    pub note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLink {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreVersionInput {
    pub asset_id: String,
    pub file_id: String,
    pub version: String,
    #[serde(default)]
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreSourceInput {
    pub asset_id: String,
    pub source_type: String,
    pub url: String,
    #[serde(default)]
    pub author: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreTag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreAssetDetail {
    pub asset: Asset,
    pub files: Vec<AssetFile>,
    pub metadata: Vec<AssetMetadata>,
    pub sources: Vec<AssetSource>,
    pub versions: Vec<AssetVersion>,
    pub projects: Vec<AssetLink>,
    pub knowledge_links: Vec<AssetLink>,
    pub related_assets: Vec<AssetLink>,
    pub processing_history: Vec<serde_json::Value>,
    pub tag_details: Vec<CoreTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreSearchRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub asset_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_locale() -> String {
    "zh-CN".into()
}
fn default_limit() -> usize {
    80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRow {
    pub id: String,
    pub path: String,
    pub relative_path: String,
    pub kind: Option<String>,
    pub size: u64,
    pub hash: String,
    pub status: String,
    pub message: String,
    pub duplicate_asset_id: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub total: usize,
    pub importable: usize,
    pub duplicate: usize,
    pub unsupported: usize,
    pub failed: usize,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreImportRequest {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub scan_task_id: Option<String>,
    #[serde(default)]
    pub scan_row_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreImportRow {
    #[serde(default)]
    pub scan_row_id: Option<String>,
    pub path: String,
    pub status: String,
    pub message: String,
    pub asset_id: Option<String>,
    pub duplicate_asset_id: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub rows: Vec<CoreImportRow>,
}
