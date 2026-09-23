import { invoke } from "@tauri-apps/api/core";
import type { AssetDetail, AssetInput, AssetMedia, ImportMapping, ImportPreview, ImportReport, LibraryMeta, Page, AssetCard, SearchRequest, LibraryLocationState, StorageChangeRequest, ParsedShareText, DuplicateMatch, BatchAssetUpdate, BatchUpdateReport, HealthSummary, HealthIssueRequest, FabMetadata, FabDuplicateMatch, ContentLanguage, TranslationSettings, TranslationRequest, TranslationPreview, TranslationTestResult, TranslationTerm, TranslationTermInput, TranslationTermImportReport, LinkCheckReport, LinkCheckResult, MoveCategoryRequest, MoveResult, UndoMoveResult, ReferenceBoardSummary, ReferenceBoardDetail, ReferencePlacement, ReferenceImageAddReport, ReferenceBoardItem, ReferenceBoardChanges, ReferenceExportOptions, PersonalizationState, GlobalPreferences, LibraryPreferences, SmartCollection, SmartCollectionInput, TagUsage, TagMutationReport, AssetSelection, DeleteRequest, DeleteResult, TrashBatch, ProjectSummary, CreativeProject, ProjectInput, ProjectUnitInput, ProjectUnit, ProjectTaskInput, ProjectTask, ProjectAssetUpdate, ProjectBoardLink, ProjectPathInput, ProjectPathShortcut, ProjectPathCheck, ProjectTaskStatus, MediaEntry, MediaEntryDetail, MediaSearchRequest, MediaImportRequest, MediaImportReport, MediaFolder, MediaKind, MediaBatchUpdate } from "../types";

export const assetMediaUrl = (id: string, variant: "stream" | "thumbnail" | "original" = "stream") => `http://dcc-media.localhost/${encodeURIComponent(id)}/${variant}`;
export const mediaLibraryFileUrl = (fileId: string) => `http://dcc-media.localhost/library/file/${encodeURIComponent(fileId)}`;
export const mediaLibraryBundleUrl = (entryId: string, logicalPath: string) => `http://dcc-media.localhost/library/bundle/${encodeURIComponent(entryId)}/${logicalPath.split("/").map(encodeURIComponent).join("/")}`;

export const api = {
  getMeta: (contentLanguage: ContentLanguage) => invoke<LibraryMeta>("get_library_meta", { contentLanguage }),
  search: (request: SearchRequest) => invoke<Page<AssetCard>>("search_assets", { request }),
  selectAssetIds: (request: SearchRequest) => invoke<AssetSelection>("select_asset_ids", { request }),
  getAsset: (id: string, contentLanguage: ContentLanguage) => invoke<AssetDetail>("get_asset", { id, contentLanguage }),
  saveAsset: (input: AssetInput) => invoke<AssetDetail>("upsert_asset", { input }),
  deleteAsset: (id: string) => invoke<void>("delete_asset", { id }),
  setFavorite: (id: string, favorite: boolean) => invoke<void>("set_favorite", { id, favorite }),
  addCategory: (name: string, parentId: string | null) => invoke("upsert_category", { id: null, name, parentId }),
  renameCategory: (id: string, name: string) => invoke("upsert_category", { id, name, parentId: null, preserveParent: true }),
  deleteCategory: (id: string) => invoke<void>("delete_category", { id }),
  getDeleteImpact: (request: DeleteRequest) => invoke<DeleteResult>("get_delete_impact", { request }),
  deleteLibraryItems: (request: DeleteRequest) => invoke<DeleteResult>("delete_library_items", { request }),
  listTrash: (offset = 0, limit = 100) => invoke<Page<TrashBatch>>("list_trash", { offset, limit }),
  restoreTrashBatch: (batchId: string) => invoke<DeleteResult>("restore_trash_batch", { batchId }),
  purgeTrashBatch: (batchId: string) => invoke<void>("purge_trash_batch", { batchId }),
  emptyTrash: () => invoke<number>("empty_trash"),
  prepareReferenceCoverIds: (ids: string[]) => invoke<string[]>("prepare_reference_cover_ids", { ids }),
  imageData: (imageId: string, thumbnail = true) => invoke<string>("get_image_data", { imageId, thumbnail }),
  importAssetMedia: (assetId: string, paths: string[]) => invoke<AssetMedia[]>("import_asset_media", { assetId, paths }),
  deleteAssetMedia: (id: string) => invoke<void>("delete_asset_media", { id }),
  retryAssetMedia: (id: string) => invoke<AssetMedia>("retry_asset_media", { id }),
  reorderAssetMedia: (assetId: string, ids: string[]) => invoke<void>("reorder_asset_media", { assetId, ids }),
  setAssetCoverMedia: (assetId: string, mediaId: string | null, imageId: string | null) => invoke<void>("set_asset_cover_media", { assetId, mediaId, imageId }),
  searchMediaEntries: (request: MediaSearchRequest) => invoke<Page<MediaEntry>>("search_media_entries", { request }),
  selectMediaEntryIds: (request: MediaSearchRequest) => invoke<string[]>("select_media_entry_ids", { request }),
  getMediaEntry: (id: string) => invoke<MediaEntryDetail>("get_media_entry", { id }),
  importMediaEntries: (request: MediaImportRequest) => invoke<MediaImportReport>("import_media_entries", { request }),
  updateMediaEntry: (id: string, input: Pick<MediaEntry, "name" | "description" | "author" | "sourceUrl" | "license" | "tags">) => invoke<void>("update_media_entry", { id, ...input }),
  deleteMediaEntries: (ids: string[]) => invoke<number>("delete_media_entries", { ids }),
  batchUpdateMediaEntries: (update: MediaBatchUpdate) => invoke<BatchUpdateReport>("batch_update_media_entries", { update }),
  listMediaFolders: (kind: MediaKind) => invoke<MediaFolder[]>("list_media_folders", { kind }),
  saveMediaFolder: (kind: MediaKind, id: string | null, parentId: string | null, name: string) => invoke<MediaFolder>("upsert_media_folder", { kind, id, parentId, name }),
  moveMediaFolder: (id: string, targetParentId: string | null, targetIndex: number) => invoke<void>("move_media_folder", { id, targetParentId, targetIndex }),
  deleteMediaFolder: (id: string) => invoke<void>("delete_media_folder", { id }),
  linkMediaAssets: (entryIds: string[], assetIds: string[]) => invoke<number>("link_media_assets", { entryIds, assetIds }),
  unlinkMediaAssets: (entryIds: string[], assetIds: string[]) => invoke<number>("unlink_media_assets", { entryIds, assetIds }),
  linkProjectMedia: (entryIds: string[], projectIds: string[]) => invoke<number>("link_project_media", { entryIds, projectIds }),
  unlinkProjectMedia: (entryIds: string[], projectIds: string[]) => invoke<number>("unlink_project_media", { entryIds, projectIds }),
  collectAssetPreviewsToMedia: (assetId: string) => invoke<MediaImportReport>("collect_asset_previews_to_media", { assetId }),
  addMediaImagesToBoard: (boardId: string, entryIds: string[], placement: ReferencePlacement) => invoke<ReferenceImageAddReport>("add_media_images_to_board", { boardId, entryIds, placement }),
  openShare: (id: string) => invoke<void>("open_share_link", { id }),
  openExternal: (url: string) => invoke<void>("open_external_url", { url }),
  copyCode: (id: string) => invoke<void>("copy_extraction_code", { id }),
  inspectImport: (path: string, mapping: ImportMapping = {}) => invoke<ImportPreview>("preview_import", { path, mapping }),
  commitImport: (path: string, mapping: ImportMapping) => invoke<ImportReport>("import_assets", { path, mapping }),
  exportTemplate: (path: string) => invoke<void>("export_import_template", { path }),
  exportBackup: (path: string) => invoke<void>("export_backup", { path }),
  restoreBackup: (path: string) => invoke<void>("restore_backup", { path }),
  getLibraryLocations: () => invoke<LibraryLocationState>("get_library_locations"),
  changeLibrary: (request: StorageChangeRequest) => invoke<LibraryLocationState>("change_library", { request }),
  forgetRecentLibrary: (path: string) => invoke<void>("forget_recent_library", { path }),
  readClipboard: () => invoke<string>("read_clipboard_text"),
  parseShareText: (text: string) => invoke<ParsedShareText>("parse_share_text", { text }),
  fetchFabMetadata: (url: string) => invoke<FabMetadata>("fetch_fab_metadata", { url }),
  checkFabUrl: (url: string, excludeAssetId?: string) => invoke<FabDuplicateMatch | null>("check_fab_url", { url, excludeAssetId }),
  checkShareUrl: (url: string) => invoke<DuplicateMatch | null>("check_share_url", { url }),
  batchUpdate: (update: BatchAssetUpdate) => invoke<BatchUpdateReport>("batch_update_assets", { update }),
  moveAssetsToCategory: (ids: string[], categoryId: string | null) => invoke<MoveResult>("move_assets_to_category", { ids, categoryId }),
  moveCategory: (request: MoveCategoryRequest) => invoke<MoveResult>("move_category", { request }),
  undoLibraryMove: (token: string) => invoke<UndoMoveResult>("undo_library_move", { token }),
  getHealth: () => invoke<HealthSummary>("get_library_health"),
  listHealthIssues: (request: HealthIssueRequest) => invoke<Page<AssetCard>>("list_health_issues", { request }),
  checkAllShareLinks: () => invoke<LinkCheckReport>("check_all_share_links"),
  cancelShareLinkCheck: () => invoke<boolean>("cancel_share_link_check"),
  checkAssetShareLink: (id: string) => invoke<LinkCheckResult>("check_asset_share_link", { id }),
  getTranslationSettings: () => invoke<TranslationSettings>("get_translation_settings"),
  saveTranslationCredentials: (appId: string, secretKey: string) => invoke<void>("save_translation_credentials", { appId, secretKey }),
  deleteTranslationCredentials: () => invoke<void>("delete_translation_credentials"),
  setFabAutoTranslate: (enabled: boolean) => invoke<void>("set_fab_auto_translate", { enabled }),
  setContentLanguage: (language: ContentLanguage) => invoke<void>("set_content_language", { language }),
  testTranslationService: () => invoke<TranslationTestResult>("test_translation_service"),
  translateAssetFields: (request: TranslationRequest) => invoke<TranslationPreview>("translate_asset_fields", { request }),
  listTranslationTerms: (filters: { query?: string; sourceLanguage?: ContentLanguage; targetLanguage?: ContentLanguage; origin?: "builtin" | "custom"; enabled?: boolean } = {}) => invoke<TranslationTerm[]>("list_translation_terms", filters),
  upsertTranslationTerm: (input: TranslationTermInput) => invoke<TranslationTerm>("upsert_translation_term", { input }),
  deleteTranslationTerm: (id: string) => invoke<void>("delete_translation_term", { id }),
  setTranslationTermEnabled: (id: string, enabled: boolean) => invoke<void>("set_translation_term_enabled", { id, enabled }),
  resetTranslationTerms: () => invoke<void>("reset_translation_term_overrides"),
  importTranslationTerms: (path: string) => invoke<TranslationTermImportReport>("import_translation_terms", { path }),
  exportTranslationTerms: (path: string) => invoke<void>("export_translation_terms", { path }),
  listReferenceBoards: () => invoke<ReferenceBoardSummary[]>("list_reference_boards"),
  createReferenceBoard: (name: string) => invoke<ReferenceBoardDetail>("create_reference_board", { name }),
  renameReferenceBoard: (id: string, name: string) => invoke<void>("rename_reference_board", { id, name }),
  duplicateReferenceBoard: (id: string) => invoke<ReferenceBoardDetail>("duplicate_reference_board", { id }),
  deleteReferenceBoard: (id: string) => invoke<void>("delete_reference_board", { id }),
  getReferenceBoard: (id: string) => invoke<ReferenceBoardDetail>("get_reference_board", { id }),
  addAssetImagesToBoard: (boardId: string, imageIds: string[], placement: ReferencePlacement) => invoke<ReferenceImageAddReport>("add_asset_images_to_board", { boardId, imageIds, placement }),
  importReferenceImages: (boardId: string, paths: string[], placement: ReferencePlacement) => invoke<ReferenceImageAddReport>("import_reference_images", { boardId, paths, placement }),
  pasteReferenceClipboardImage: (boardId: string, placement: ReferencePlacement) => invoke<ReferenceBoardItem>("paste_reference_clipboard_image", { boardId, placement }),
  duplicateReferenceItems: (boardId: string, itemIds: string[]) => invoke<ReferenceBoardItem[]>("duplicate_reference_items", { boardId, itemIds }),
  saveReferenceBoardChanges: (changes: ReferenceBoardChanges) => invoke<void>("save_reference_board_changes", { changes }),
  referenceImageData: (itemId: string, thumbnail = true) => invoke<string>("get_reference_image_data", { itemId, thumbnail }),
  exportReferenceBoard: (boardId: string, path: string, options: ReferenceExportOptions) => invoke<void>("export_reference_board", { boardId, path, options }),
  openReferenceWindow: (boardId: string) => invoke<void>("open_reference_window", { boardId }),
  referenceWindowBoard: () => invoke<string>("get_reference_window_board"),
  referenceWindowReady: () => invoke<void>("reference_window_ready"),
  attachReferenceWindow: (boardId: string) => invoke<void>("attach_reference_window", { boardId }),
  setReferenceWindowAlwaysOnTop: (enabled: boolean) => invoke<void>("set_reference_window_always_on_top", { enabled }),
  isReferenceWindowOpen: () => invoke<boolean>("is_reference_window_open")
  ,getPersonalization: () => invoke<PersonalizationState>("get_personalization_settings")
  ,saveGlobalPreferences: (preferences: GlobalPreferences) => invoke<GlobalPreferences>("save_global_preferences", { preferences })
  ,saveLibraryPreferences: (preferences: LibraryPreferences) => invoke<LibraryPreferences>("save_library_preferences", { preferences })
  ,resetPersonalization: (scope: "global" | "library" | "all") => invoke<PersonalizationState>("reset_personalization_settings", { scope })
  ,listSmartCollections: () => invoke<SmartCollection[]>("list_smart_collections")
  ,createSmartCollection: (input: SmartCollectionInput) => invoke<SmartCollection>("create_smart_collection", { input })
  ,updateSmartCollection: (id: string, input: SmartCollectionInput) => invoke<SmartCollection>("update_smart_collection", { id, input })
  ,duplicateSmartCollection: (id: string) => invoke<SmartCollection>("duplicate_smart_collection", { id })
  ,deleteSmartCollection: (id: string) => invoke<void>("delete_smart_collection", { id })
  ,reorderSmartCollections: (ids: string[]) => invoke<void>("reorder_smart_collections", { ids })
  ,listTags: (locale: ContentLanguage, query = "", offset = 0, limit = 100) => invoke<Page<TagUsage>>("list_tags", { locale, query, offset, limit })
  ,renameTag: (id: string, name: string) => invoke<TagMutationReport>("rename_tag", { id, name })
  ,mergeTags: (sourceIds: string[], targetId: string) => invoke<TagMutationReport>("merge_tags", { sourceIds, targetId })
  ,deleteTags: (ids: string[]) => invoke<TagMutationReport>("delete_tags", { ids })
  ,deleteUnusedTags: (locale: ContentLanguage) => invoke<TagMutationReport>("delete_unused_tags", { locale })
  ,listProjects: (includeArchived = false) => invoke<ProjectSummary[]>("list_projects", { includeArchived })
  ,getProject: (id: string, contentLanguage: ContentLanguage, markOpened = true) => invoke<CreativeProject>("get_project", { id, contentLanguage, markOpened })
  ,saveProject: (input: ProjectInput) => invoke<CreativeProject>("upsert_project", { input })
  ,archiveProject: (id: string, archived: boolean) => invoke<void>("archive_project", { id, archived })
  ,deleteProject: (id: string) => invoke<void>("delete_project", { id })
  ,addProjectAssets: (projectId: string, assetIds: string[]) => invoke<number>("add_project_assets", { projectId, assetIds })
  ,updateProjectAssets: (update: ProjectAssetUpdate) => invoke<number>("update_project_assets", { update })
  ,removeProjectAssets: (projectId: string, assetIds: string[]) => invoke<number>("remove_project_assets", { projectId, assetIds })
  ,saveProjectUnit: (input: ProjectUnitInput) => invoke<ProjectUnit>("upsert_project_unit", { input })
  ,setProjectShotVideo: (shotId: string, entryId: string | null) => invoke<void>("set_project_shot_video", { shotId, entryId })
  ,deleteProjectUnit: (id: string) => invoke<[number, number]>("delete_project_unit", { id })
  ,reorderProjectUnits: (projectId: string, parentId: string | null, ids: string[]) => invoke<void>("reorder_project_units", { projectId, parentId, ids })
  ,saveProjectTask: (input: ProjectTaskInput) => invoke<ProjectTask>("upsert_project_task", { input })
  ,deleteProjectTask: (id: string) => invoke<void>("delete_project_task", { id })
  ,moveProjectTask: (id: string, status: ProjectTaskStatus, targetIndex: number) => invoke<void>("move_project_task", { id, status, targetIndex })
  ,setProjectTaskAssets: (taskId: string, assetIds: string[]) => invoke<void>("set_project_task_assets", { taskId, assetIds })
  ,linkProjectBoard: (projectId: string, boardId: string, main = false) => invoke<void>("link_project_board", { projectId, boardId, main })
  ,unlinkProjectBoard: (projectId: string, boardId: string) => invoke<void>("unlink_project_board", { projectId, boardId })
  ,setMainProjectBoard: (projectId: string, boardId: string) => invoke<void>("set_main_project_board", { projectId, boardId })
  ,saveProjectPath: (input: ProjectPathInput) => invoke<ProjectPathShortcut>("upsert_project_path", { input })
  ,deleteProjectPath: (id: string) => invoke<void>("delete_project_path", { id })
  ,checkProjectPath: (id: string) => invoke<ProjectPathCheck>("check_project_path", { id })
  ,openProjectPath: (id: string) => invoke<void>("open_project_path", { id })
};
