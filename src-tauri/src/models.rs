use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn default_content_language() -> String {
    "zh-CN".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedAssetText {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub license: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub asset_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub tags: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub licenses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMeta {
    pub categories: Vec<Category>,
    pub filters: FilterOptions,
    pub total_assets: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub category_ids: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub licenses: Vec<String>,
    pub favorite_only: bool,
    pub recent_only: bool,
    #[serde(default)]
    pub health_issue: Option<String>,
    #[serde(default)]
    pub smart_collection_id: Option<String>,
    pub sort: String,
    pub offset: i64,
    pub limit: i64,
    #[serde(default = "default_content_language")]
    pub content_language: String,
}

pub fn default_module_order() -> Vec<String> {
    [
        "library",
        "smartCollections",
        "favorites",
        "recent",
        "tagManager",
        "health",
        "reference",
        "trash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_theme() -> String {
    "graphite".into()
}
fn default_accent() -> String {
    "#D99A42".into()
}
fn default_density() -> String {
    "comfortable".into()
}
fn default_sidebar_width() -> i64 {
    248
}
fn default_detail_width() -> i64 {
    420
}
fn default_true_value() -> bool {
    true
}
fn default_asset_view() -> String {
    "grid".into()
}
fn default_card_size() -> String {
    "medium".into()
}
fn default_cover_fit() -> String {
    "cover".into()
}
fn default_sort() -> String {
    "updated".into()
}
fn default_startup_module() -> String {
    "library".into()
}

pub fn default_shortcuts() -> HashMap<String, String> {
    [
        ("commandPalette", "Ctrl+P"),
        ("focusSearch", "Ctrl+K"),
        ("addAsset", "Ctrl+N"),
        ("quickAdd", "Ctrl+Shift+N"),
        ("settings", "Ctrl+,"),
        ("toggleSelection", "Ctrl+M"),
        ("selectAll", "Ctrl+A"),
        ("saveAsset", "Ctrl+S"),
        ("deleteSelected", "Delete"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalPreferences {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_density")]
    pub density: String,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: i64,
    #[serde(default = "default_detail_width")]
    pub detail_width: i64,
    #[serde(default = "default_shortcuts")]
    pub shortcuts: HashMap<String, String>,
}

impl Default for GlobalPreferences {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            accent_color: default_accent(),
            density: default_density(),
            reduce_motion: false,
            sidebar_width: default_sidebar_width(),
            detail_width: default_detail_width(),
            shortcuts: default_shortcuts(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFieldVisibility {
    #[serde(default = "default_true_value")]
    pub category: bool,
    #[serde(default = "default_true_value")]
    pub tags: bool,
    #[serde(default = "default_true_value")]
    pub software: bool,
    #[serde(default = "default_true_value")]
    pub version: bool,
    #[serde(default = "default_true_value")]
    pub format: bool,
    #[serde(default = "default_true_value")]
    pub link_status: bool,
}

impl Default for CardFieldVisibility {
    fn default() -> Self {
        Self {
            category: true,
            tags: true,
            software: true,
            version: true,
            format: true,
            link_status: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPreferences {
    #[serde(default = "default_module_order")]
    pub module_order: Vec<String>,
    #[serde(default)]
    pub disabled_modules: Vec<String>,
    #[serde(default = "default_startup_module")]
    pub startup_module: String,
    #[serde(default)]
    pub remember_last_context: bool,
    #[serde(default = "default_asset_view")]
    pub asset_view: String,
    #[serde(default = "default_card_size")]
    pub card_size: String,
    #[serde(default = "default_cover_fit")]
    pub cover_fit: String,
    #[serde(default)]
    pub card_fields: CardFieldVisibility,
    #[serde(default = "default_sort")]
    pub default_sort: String,
    #[serde(default)]
    pub remember_search: bool,
    #[serde(default = "default_true_value")]
    pub category_tree_expanded: bool,
    #[serde(default)]
    pub last_context: Option<serde_json::Value>,
}

impl Default for LibraryPreferences {
    fn default() -> Self {
        Self {
            module_order: default_module_order(),
            disabled_modules: vec![],
            startup_module: default_startup_module(),
            remember_last_context: false,
            asset_view: default_asset_view(),
            card_size: default_card_size(),
            cover_fit: default_cover_fit(),
            card_fields: CardFieldVisibility::default(),
            default_sort: default_sort(),
            remember_search: false,
            category_tree_expanded: true,
            last_context: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizationState {
    pub global: GlobalPreferences,
    pub library: LibraryPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionRule {
    pub query: String,
    pub category_ids: Vec<String>,
    pub tag_ids: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub licenses: Vec<String>,
    pub favorite_only: bool,
    pub recent_only: bool,
    pub health_issue: Option<String>,
    pub sort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionInput {
    pub id: Option<String>,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub rule: SmartCollectionRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollection {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub rule: SmartCollectionRule,
    pub sort_order: i64,
    pub invalid_conditions: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagUsage {
    pub id: String,
    pub locale: String,
    pub name: String,
    pub usage_count: i64,
    pub asset_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagMutationReport {
    pub affected_assets: usize,
    pub removed_tags: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLocation {
    pub path: String,
    pub name: String,
    pub library_id: String,
    pub available: bool,
    pub is_current: bool,
    pub retained_copy: bool,
    pub size_bytes: u64,
    pub last_opened_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLocationState {
    pub current: LibraryLocation,
    pub recent: Vec<LibraryLocation>,
    pub startup_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageChangeRequest {
    pub mode: String,
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParsedShareText {
    pub share_url: String,
    pub extraction_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FabMetadata {
    pub canonical_url: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub category: String,
    pub tags: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub license: String,
    pub preview_images: Vec<FabPreviewImage>,
    pub image_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FabPreviewImage {
    pub source_path: String,
    pub preview_data_url: String,
    pub original_name: String,
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMatch {
    pub id: String,
    pub name: String,
    pub share_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAssetUpdate {
    pub ids: Vec<String>,
    pub category_id: Option<String>,
    #[serde(default)]
    pub clear_category: bool,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
    pub favorite: Option<bool>,
    #[serde(default = "default_content_language")]
    pub content_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchUpdateReport {
    pub requested: usize,
    pub updated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveCategoryRequest {
    pub id: String,
    pub target_parent_id: Option<String>,
    pub target_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub moved: usize,
    pub message: String,
    pub undo_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoMoveResult {
    pub restored: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCount {
    pub issue: String,
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSummary {
    pub total_issues: i64,
    pub counts: Vec<HealthCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssueRequest {
    pub issue: String,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCheckTarget {
    pub id: String,
    pub share_url: String,
    pub normalized_share_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCheckResult {
    pub asset_id: String,
    pub status: String,
    pub checked_at: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkCheckProgress {
    pub checked: usize,
    pub total: usize,
    pub valid: usize,
    pub invalid: usize,
    pub error: usize,
    pub current_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkCheckReport {
    pub total_assets: usize,
    pub unique_links: usize,
    pub checked_assets: usize,
    pub valid: usize,
    pub invalid: usize,
    pub error: usize,
    pub skipped: usize,
    pub cancelled: bool,
    pub stopped_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCard {
    pub id: String,
    pub name: String,
    pub category_name: Option<String>,
    pub tags: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub favorite: bool,
    pub updated_at: String,
    pub last_viewed_at: Option<String>,
    pub cover_image_id: Option<String>,
    pub content_language: String,
    pub language_fallback: bool,
    pub link_check_status: String,
    pub link_checked_at: Option<String>,
    pub link_check_message: String,
    pub has_share_link: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImage {
    pub id: String,
    pub original_name: String,
    pub sort_order: i64,
    pub is_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDetail {
    #[serde(flatten)]
    pub card: AssetCard,
    pub description: String,
    pub category_id: Option<String>,
    pub size_bytes: Option<i64>,
    pub author: String,
    pub source_url: String,
    pub license: String,
    pub share_url: String,
    pub extraction_code: String,
    pub images: Vec<AssetImage>,
    pub created_at: String,
    pub localizations: HashMap<String, LocalizedAssetText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub id: Option<String>,
    pub source_path: Option<String>,
    #[serde(default)]
    pub original_name: Option<String>,
    pub is_cover: bool,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub category_id: Option<String>,
    pub tags: Vec<String>,
    pub dcc_tools: Vec<String>,
    pub versions: Vec<String>,
    pub formats: Vec<String>,
    pub size_bytes: Option<i64>,
    pub author: String,
    pub source_url: String,
    pub license: String,
    pub share_url: String,
    pub extraction_code: String,
    pub favorite: bool,
    pub images: Vec<ImageInput>,
    #[serde(default)]
    pub localizations: HashMap<String, LocalizedAssetText>,
    #[serde(default = "default_content_language")]
    pub content_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettings {
    pub provider: String,
    pub configured: bool,
    pub fab_auto_translate: bool,
    pub content_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRequest {
    pub source_language: String,
    pub target_language: String,
    pub fields: LocalizedAssetText,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TranslationPreview {
    pub fields: LocalizedAssetText,
    pub character_count: usize,
    pub warnings: Vec<String>,
    pub failed_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationTestResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRowResult {
    pub row: usize,
    pub name: String,
    pub status: String,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub headers: Vec<String>,
    pub suggested_mapping: HashMap<String, String>,
    pub sample: Vec<HashMap<String, String>>,
    pub valid_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub rows: Vec<ImportRowResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub rows: Vec<ImportRowResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub current: usize,
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_name: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSelection {
    pub ids: Vec<String>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    #[serde(default)]
    pub asset_ids: Vec<String>,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub batch_id: String,
    pub label: String,
    pub asset_count: usize,
    pub category_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashBatch {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub asset_count: i64,
    pub category_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaiduSaveTask {
    pub id: String,
    pub name: String,
    pub share_url: String,
    pub extraction_code: String,
}

#[derive(Debug, Clone)]
pub struct ParsedImportRow {
    pub row: usize,
    pub values: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBoardSummary {
    pub id: String,
    pub name: String,
    pub background: String,
    pub item_count: i64,
    pub updated_at: String,
    pub last_opened_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBoardItem {
    pub id: String,
    pub board_id: String,
    pub original_name: String,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub z_index: i64,
    pub source_asset_id: Option<String>,
    pub source_image_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBoardDetail {
    pub id: String,
    pub name: String,
    pub background: String,
    pub view_x: f64,
    pub view_y: f64,
    pub view_scale: f64,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<ReferenceBoardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBoardItemTransform {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub z_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBoardChanges {
    pub board_id: String,
    pub name: Option<String>,
    pub background: Option<String>,
    pub view_x: Option<f64>,
    pub view_y: Option<f64>,
    pub view_scale: Option<f64>,
    #[serde(default)]
    pub items: Vec<ReferenceBoardItemTransform>,
    #[serde(default)]
    pub deleted_ids: Vec<String>,
    #[serde(default)]
    pub restored_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencePlacement {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImageAddReport {
    pub items: Vec<ReferenceBoardItem>,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceExportOptions {
    pub scale: f64,
}
