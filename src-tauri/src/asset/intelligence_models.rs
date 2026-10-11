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
