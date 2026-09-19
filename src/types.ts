export type SortMode = "relevance" | "updated" | "name" | "created" | "recent" | "favorite";
export type ViewMode = "library" | "favorites" | "recent" | "tagManager" | "health" | "reference" | "trash";
export type ModuleId = "library" | "smartCollections" | "favorites" | "recent" | "tagManager" | "health" | "reference" | "trash";
export type ThemeId = "graphite" | "ue-slate" | "midnight";
export type AssetViewMode = "grid" | "list";
export type CardSize = "small" | "medium" | "large";
export type CommandId = "commandPalette" | "focusSearch" | "addAsset" | "quickAdd" | "settings" | "toggleSelection" | "selectAll" | "saveAsset" | "deleteSelected";
export type ContentLanguage = "zh-CN" | "en";
export type LinkCheckStatus = "unknown" | "valid" | "invalid" | "error";

export interface LocalizedAssetText { name: string; description: string; tags: string[]; license: string }

export interface Category {
  id: string;
  name: string;
  parentId: string | null;
  sortOrder: number;
  assetCount: number;
}

export interface FilterOptions {
  tags: string[];
  dccTools: string[];
  versions: string[];
  formats: string[];
  licenses: string[];
}

export interface LibraryMeta {
  categories: Category[];
  filters: FilterOptions;
  totalAssets: number;
}

export interface SearchRequest {
  query: string;
  categoryIds: string[];
  tags: string[];
  tagIds?: string[];
  dccTools: string[];
  versions: string[];
  formats: string[];
  licenses: string[];
  favoriteOnly: boolean;
  recentOnly: boolean;
  healthIssue?: string | null;
  smartCollectionId?: string | null;
  sort: SortMode;
  offset: number;
  limit: number;
  contentLanguage: ContentLanguage;
}

export interface GlobalPreferences {
  theme: ThemeId;
  accentColor: string;
  density: "comfortable" | "compact";
  reduceMotion: boolean;
  sidebarWidth: number;
  detailWidth: number;
  shortcuts: Record<CommandId | string, string>;
}

export interface CardFieldVisibility { category: boolean; tags: boolean; software: boolean; version: boolean; format: boolean; linkStatus: boolean }
export interface LibraryPreferences {
  moduleOrder: string[];
  disabledModules: string[];
  startupModule: string;
  rememberLastContext: boolean;
  assetView: AssetViewMode;
  cardSize: CardSize;
  coverFit: "cover" | "contain";
  cardFields: CardFieldVisibility;
  defaultSort: SortMode;
  rememberSearch: boolean;
  categoryTreeExpanded: boolean;
  lastContext?: { view: ViewMode; request: SearchRequest } | null;
}
export interface PersonalizationState { global: GlobalPreferences; library: LibraryPreferences }

export interface SmartCollectionRule {
  query: string; categoryIds: string[]; tagIds: string[]; dccTools: string[]; versions: string[];
  formats: string[]; licenses: string[]; favoriteOnly: boolean; recentOnly: boolean;
  healthIssue: string | null; sort: SortMode;
}
export interface SmartCollectionInput { id?: string | null; name: string; icon: string; color: string; rule: SmartCollectionRule }
export interface SmartCollection extends SmartCollectionInput { id: string; sortOrder: number; invalidConditions: string[]; createdAt: string; updatedAt: string }
export interface TagUsage { id: string; locale: ContentLanguage; name: string; usageCount: number; assetCount: number }
export interface TagMutationReport { affectedAssets: number; removedTags: number }

export interface AssetCard {
  id: string;
  name: string;
  categoryName: string | null;
  tags: string[];
  dccTools: string[];
  versions: string[];
  formats: string[];
  favorite: boolean;
  updatedAt: string;
  lastViewedAt: string | null;
  coverImageId: string | null;
  contentLanguage: ContentLanguage;
  languageFallback: boolean;
  linkCheckStatus: LinkCheckStatus;
  linkCheckedAt: string | null;
  linkCheckMessage: string;
  hasShareLink?: boolean;
}

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface AssetImage {
  id: string;
  originalName: string;
  sortOrder: number;
  isCover: boolean;
}

export interface AssetDetail extends AssetCard {
  description: string;
  categoryId: string | null;
  sizeBytes: number | null;
  author: string;
  sourceUrl: string;
  license: string;
  shareUrl: string;
  extractionCode: string;
  images: AssetImage[];
  createdAt: string;
  localizations: Partial<Record<ContentLanguage, LocalizedAssetText>>;
}

export interface ImageInput {
  id?: string;
  sourcePath?: string;
  previewDataUrl?: string;
  originalName?: string;
  remoteUrl?: string;
  isCover: boolean;
  sortOrder: number;
}

export interface AssetInput {
  id?: string;
  name: string;
  description: string;
  categoryId: string | null;
  tags: string[];
  dccTools: string[];
  versions: string[];
  formats: string[];
  sizeBytes: number | null;
  author: string;
  sourceUrl: string;
  license: string;
  shareUrl: string;
  extractionCode: string;
  favorite: boolean;
  images: ImageInput[];
  localizations: Partial<Record<ContentLanguage, LocalizedAssetText>>;
  contentLanguage: ContentLanguage;
}

export interface ImportMapping {
  [canonicalField: string]: string;
}

export interface ImportRowResult {
  row: number;
  name: string;
  status: "valid" | "warning" | "error" | "skipped" | "imported";
  messages: string[];
}

export interface ImportPreview {
  headers: string[];
  suggestedMapping: ImportMapping;
  sample: Record<string, string>[];
  validCount: number;
  warningCount: number;
  errorCount: number;
  rows: ImportRowResult[];
}

export interface ImportReport {
  imported: number;
  skipped: number;
  failed: number;
  rows: ImportRowResult[];
}

export interface ImportProgress { current: number; total: number; imported: number; skipped: number; failed: number; currentName: string; phase: string }
export interface AssetSelection { ids: string[]; total: number }
export interface DeleteRequest { assetIds: string[]; categoryId: string | null }
export interface DeleteResult { batchId: string; label: string; assetCount: number; categoryCount: number }
export interface TrashBatch { id: string; kind: "assets" | "category"; label: string; assetCount: number; categoryCount: number; createdAt: string }
export interface BaiduSaveTask { id: string; name: string; shareUrl: string; extractionCode: string }

export interface LibraryLocation {
  path: string;
  name: string;
  libraryId: string;
  available: boolean;
  isCurrent: boolean;
  retainedCopy: boolean;
  sizeBytes: number;
  lastOpenedAt: string | null;
}

export interface LibraryLocationState {
  current: LibraryLocation;
  recent: LibraryLocation[];
  startupWarning: string | null;
}

export interface StorageChangeRequest {
  mode: "create" | "migrate" | "open";
  path: string;
  name?: string;
}

export interface ParsedShareText { shareUrl: string; extractionCode: string }
export interface DuplicateMatch { id: string; name: string; shareUrl: string }

export interface FabMetadata {
  canonicalUrl: string;
  name: string;
  description: string;
  author: string;
  category: string;
  tags: string[];
  dccTools: string[];
  versions: string[];
  formats: string[];
  license: string;
  previewImages: FabPreviewImage[];
  imageWarning: string | null;
}

export interface FabPreviewImage {
  sourcePath: string;
  previewDataUrl: string;
  originalName: string;
  remoteUrl: string;
}

export interface BatchAssetUpdate {
  ids: string[];
  categoryId: string | null;
  clearCategory: boolean;
  addTags: string[];
  removeTags: string[];
  favorite: boolean | null;
  contentLanguage: ContentLanguage;
}

export interface BatchUpdateReport { requested: number; updated: number }
export interface MoveCategoryRequest { id: string; targetParentId: string | null; targetIndex: number }
export interface MoveResult { moved: number; message: string; undoToken: string | null }
export interface UndoMoveResult { restored: number; message: string }
export type LibraryDragPayload = { kind: "assets"; ids: string[]; label: string } | { kind: "category"; id: string; label: string };
export interface HealthCount { issue: string; label: string; count: number }
export interface HealthSummary { totalIssues: number; counts: HealthCount[] }
export interface HealthIssueRequest { issue: string; offset: number; limit: number }
export interface LinkCheckResult { assetId: string; status: Exclude<LinkCheckStatus, "unknown">; checkedAt: string | null; message: string }
export interface LinkCheckProgress { checked: number; total: number; valid: number; invalid: number; error: number; currentUrl: string | null }
export interface LinkCheckReport { totalAssets: number; uniqueLinks: number; checkedAssets: number; valid: number; invalid: number; error: number; skipped: number; cancelled: boolean; stoppedReason: string | null }

export interface TranslationSettings { provider: "baidu"; configured: boolean; fabAutoTranslate: boolean; contentLanguage: ContentLanguage }
export interface TranslationRequest { sourceLanguage: ContentLanguage; targetLanguage: ContentLanguage; fields: LocalizedAssetText }
export interface TranslationPreview { fields: LocalizedAssetText; characterCount: number; warnings: string[]; failedFields: string[] }
export interface TranslationTestResult { success: boolean; message: string }

export interface ReferenceBoardSummary {
  id: string;
  name: string;
  background: string;
  itemCount: number;
  updatedAt: string;
  lastOpenedAt: string | null;
}

export interface ReferenceBoardItem {
  id: string;
  boardId: string;
  originalName: string;
  pixelWidth: number;
  pixelHeight: number;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  zIndex: number;
  sourceAssetId: string | null;
  sourceImageId: string | null;
}

export interface ReferenceBoardDetail {
  id: string;
  name: string;
  background: string;
  viewX: number;
  viewY: number;
  viewScale: number;
  createdAt: string;
  updatedAt: string;
  items: ReferenceBoardItem[];
}

export interface ReferenceBoardItemTransform {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  zIndex: number;
}

export interface ReferenceBoardChanges {
  boardId: string;
  name?: string | null;
  background?: string | null;
  viewX?: number | null;
  viewY?: number | null;
  viewScale?: number | null;
  items: ReferenceBoardItemTransform[];
  deletedIds: string[];
  restoredIds: string[];
}

export interface ReferencePlacement { x: number; y: number }
export interface ReferenceImageAddReport { items: ReferenceBoardItem[]; skipped: number; warnings: string[] }
export interface ReferenceExportOptions { scale: number }
