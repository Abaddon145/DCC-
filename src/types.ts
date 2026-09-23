export type SortMode = "relevance" | "updated" | "name" | "created" | "recent" | "favorite";
export type MediaKind = "image" | "model" | "audio" | "video";
export type ViewMode = "library" | "imageLibrary" | "modelLibrary" | "audioLibrary" | "videoLibrary" | "projects" | "favorites" | "recent" | "tagManager" | "health" | "reference" | "trash";
export type ModuleId = ViewMode | "smartCollections";
export type ModuleGroupId = "content" | "libraryContext" | "creation" | "manage";
export type ContextPaneId = "library" | "imageLibrary" | "modelLibrary" | "audioLibrary" | "videoLibrary";
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
  projectId?: string | null;
  projectAssetStatus?: ProjectAssetStatus | null;
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
  contextPaneWidths: Record<string, number>;
  collapsedContextPanes: string[];
  collapsedModuleGroups: string[];
  lastContext?: { view: ViewMode; request: SearchRequest } | null;
}
export interface PersonalizationState { global: GlobalPreferences; library: LibraryPreferences }

export type ProjectType = "still" | "scene" | "animation";
export type ProjectStatus = "planning" | "active" | "paused" | "completed" | "archived";
export type ProjectAssetStatus = "candidate" | "selected" | "used" | "rejected";
export type ProjectTaskStatus = "todo" | "in_progress" | "review" | "done";
export type ProjectTaskPriority = "low" | "normal" | "high" | "urgent";

export interface ProjectInput {
  id?: string | null; name: string; description: string; projectType: ProjectType; status: ProjectStatus;
  targetTools: string[]; versions: string[]; resolutionWidth: number | null; resolutionHeight: number | null;
  frameRate: number | null; coverAssetId: string | null;
}
export interface ProjectSummary extends Omit<ProjectInput, "id"> {
  id: string; coverImageId: string | null; assetCount: number; unavailableAssetCount: number; taskCount: number;
  completedTaskCount: number; reviewTaskCount: number; progress: number; mainBoardId: string | null;
  createdAt: string; updatedAt: string; lastOpenedAt: string | null; archivedAt: string | null;
}
export interface ProjectUnitInput {
  id?: string | null; projectId: string; parentId: string | null; kind: "scene" | "shot"; name: string; description: string;
  startFrame: number | null; endFrame: number | null; resolutionWidth: number | null; resolutionHeight: number | null;
  frameRate: number | null; sortOrder?: number | null;
}
export interface ProjectUnit extends Omit<ProjectUnitInput, "id" | "sortOrder"> { id: string; sortOrder: number; createdAt: string; updatedAt: string }
export interface ProjectTaskInput {
  id?: string | null; projectId: string; unitId: string | null; title: string; description: string; status: ProjectTaskStatus;
  priority: ProjectTaskPriority; dueDate: string | null; sortOrder?: number | null; assetIds: string[];
}
export interface ProjectTask extends Omit<ProjectTaskInput, "id" | "sortOrder"> { id: string; sortOrder: number; createdAt: string; updatedAt: string }
export interface ProjectAssetLink { assetId: string; name: string; coverImageId: string | null; categoryName: string | null; status: ProjectAssetStatus; purpose: string; note: string; unitIds: string[]; unavailable: boolean; updatedAt: string }
export interface ProjectAssetUpdate { projectId: string; assetIds: string[]; status?: ProjectAssetStatus | null; purpose?: string | null; note?: string | null; unitIds?: string[] | null }
export interface ProjectBoardLink { boardId: string; name: string; isMain: boolean; sortOrder: number; itemCount: number }
export interface ProjectPathInput { id?: string | null; projectId: string; kind: "root" | "project_file" | "output" | "custom"; label: string; path: string; pathType: "file" | "directory"; sortOrder?: number | null }
export interface ProjectPathShortcut extends Omit<ProjectPathInput, "id" | "sortOrder"> { id: string; sortOrder: number; available: boolean; createdAt: string; updatedAt: string }
export interface ProjectPathCheck { id: string; available: boolean; message: string }
export interface CreativeProject extends ProjectSummary { units: ProjectUnit[]; tasks: ProjectTask[]; assets: ProjectAssetLink[]; boards: ProjectBoardLink[]; paths: ProjectPathShortcut[] }

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
  coverMediaId?: string | null;
  coverMediaKind?: AssetMediaKind | null;
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

export type AssetMediaKind = "video" | "audio" | "model";
export type MediaProcessingStatus = "pending" | "processing" | "ready" | "error";
export interface AssetMedia {
  id: string; assetId: string; kind: AssetMediaKind; originalName: string; mimeType: string; fileSize: number;
  durationMs: number | null; pixelWidth: number | null; pixelHeight: number | null; sortOrder: number; isCover: boolean;
  processingStatus: MediaProcessingStatus; processingMessage: string; hasProxy: boolean; hasThumbnail: boolean;
}

export interface AssetDetail extends AssetCard {
  description: string;
  categoryId: string | null;
  sizeBytes: number | null;
  author: string;
  sourceUrl: string;
  fabListingId: string;
  license: string;
  shareUrl: string;
  extractionCode: string;
  images: AssetImage[];
  media?: AssetMedia[];
  mediaLibrary?: LinkedMediaPreview[];
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
  fabListingId?: string | null;
  autoCategoryPath?: string[];
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
  status: "valid" | "warning" | "error" | "duplicate" | "skipped" | "imported";
  messages: string[];
  suggestedCategoryPath?: string | null;
  actualCategoryPath?: string | null;
  duplicateAssetId?: string | null;
  duplicateSource?: "file" | "library" | "trash" | null;
}

export interface ImportPreview {
  headers: string[];
  suggestedMapping: ImportMapping;
  sample: Record<string, string>[];
  validCount: number;
  warningCount: number;
  errorCount: number;
  duplicateCount: number;
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
export interface DeleteResult { batchId: string; label: string; assetCount: number; categoryCount: number; mediaCount: number; mediaFolderCount: number }
export interface TrashBatch { id: string; kind: "assets" | "category" | "media" | "mediaFolder"; label: string; assetCount: number; categoryCount: number; mediaCount: number; mediaFolderCount: number; createdAt: string }

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
  listingId: string;
  categoryPath: string;
  listingType: string;
  suggestedCategoryPath: string[];
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

export interface FabDuplicateMatch {
  assetId: string;
  assetName: string;
  categoryPath: string;
  location: "library" | "trash";
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
export interface AdvancedSearchError { message: string; position?: number }
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

export interface MediaFolder { id: string; kind: MediaKind; parentId: string | null; name: string; sortOrder: number; entryCount: number }
export interface MediaFile { id: string; entryId: string; role: "main" | "dependency" | "proxy" | "thumbnail" | "waveform"; logicalPath: string; originalName: string; mimeType: string; fileSize: number; checksum: string; width: number | null; height: number | null; durationMs: number | null }
export interface MediaEntry { id: string; kind: MediaKind; folderId: string | null; name: string; description: string; author: string; sourceUrl: string; license: string; favorite: boolean; processingStatus: string; processingMessage: string; format: string; fileSize: number; tags: string[]; thumbnailFileId: string | null; primaryFileId: string; createdAt: string; updatedAt: string }
export interface MediaEntryDetail extends MediaEntry { files: MediaFile[]; assetIds: string[]; projectIds: string[] }
export interface LinkedMediaPreview { id: string; kind: MediaKind; name: string; originalName: string; logicalPath: string; primaryFileId: string; streamFileId: string; thumbnailFileId: string | null; processingStatus: string }
export interface MediaSearchRequest { kind: MediaKind; query: string; folderId: string | null; includeChildFolders: boolean; tags: string[]; formats: string[]; favoriteOnly: boolean; sort: "updated" | "name" | "created" | "favorite"; offset: number; limit: number }
export interface MediaImportRequest { kind: MediaKind; paths: string[]; folderId: string | null; tags: string[]; favorite: boolean }
export interface MediaImportRow { path: string; status: "imported" | "duplicate" | "error"; message: string; entryId: string | null; duplicateEntryId: string | null }
export interface MediaImportReport { imported: number; skipped: number; failed: number; rows: MediaImportRow[] }
export interface MediaBatchUpdate { ids: string[]; folderId: string | null; clearFolder: boolean; addTags: string[]; removeTags: string[]; favorite: boolean | null }

export interface TranslationSettings { provider: "baidu"; configured: boolean; fabAutoTranslate: boolean; contentLanguage: ContentLanguage }
export interface TranslationRequest { sourceLanguage: ContentLanguage; targetLanguage: ContentLanguage; fields: LocalizedAssetText }
export type TranslationTermMode = "translate" | "preserve";
export interface TranslationTermInput { id?: string | null; sourceLanguage: ContentLanguage; targetLanguage: ContentLanguage; source: string; target: string; mode: TranslationTermMode; caseSensitive: boolean; enabled: boolean }
export interface TranslationTerm extends TranslationTermInput { id: string; origin: "builtin" | "custom" }
export interface TranslationTermApplication { field: string; source: string; target: string; mode: TranslationTermMode; origin: "builtin" | "custom"; count: number }
export interface TranslationTermImportReport { imported: number; updated: number; skipped: number; warnings: string[] }
export interface TranslationPreview { fields: LocalizedAssetText; characterCount: number; warnings: string[]; failedFields: string[]; appliedTerms: TranslationTermApplication[]; protectedTokenCount: number }
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
