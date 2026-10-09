import { invoke } from "@tauri-apps/api/core";
import { withKnowledgeBarrier } from "./noteSaveGuard";
import type { AssetDetail, AssetInput, AssetMedia, ImportMapping, ImportPreview, ImportReport, BatchTranslationReport, LibraryMeta, Page, AssetCard, SearchRequest, LibraryLocationState, StorageChangeRequest, ParsedShareText, DuplicateMatch, BatchAssetUpdate, BatchUpdateReport, HealthSummary, HealthIssueRequest, FabMetadata, FabDuplicateMatch, ContentLanguage, TranslationSettings, TranslationRequest, TranslationPreview, TranslationTestResult, TranslationTerm, TranslationTermInput, TranslationTermImportReport, LinkCheckReport, LinkCheckResult, MoveCategoryRequest, MoveResult, UndoMoveResult, ReferenceBoardSummary, ReferenceBoardDetail, ReferencePlacement, ReferenceImageAddReport, ReferenceBoardItem, ReferenceBoardChanges, ReferenceExportOptions, PersonalizationState, GlobalPreferences, LibraryPreferences, SmartCollection, SmartCollectionInput, TagUsage, TagMutationReport, AssetSelection, DeleteRequest, DeleteResult, TrashBatch, ProjectSummary, CreativeProject, ProjectInput, ProjectUnitInput, ProjectTaskInput, ProjectTask, ProjectAssetUpdate, ProjectBoardLink, ProjectPathInput, ProjectPathShortcut, ProjectPathCheck, ProjectTaskStatus, MediaEntry, MediaEntryDetail, MediaSearchRequest, MediaImportRequest, MediaImportReport, MediaFolder, MediaKind, MediaBatchUpdate, ModelThumbnailJob } from "../types";
import type { ProjectUnit } from "../types";
import type { LibraryTask, LibraryTaskRequest } from "../types";
import type { FabUeOptions, UeProjectTarget, EpicOpenResult } from "../types";

async function runLibraryTask<T>(payload: LibraryTaskRequest, expectedLibraryId?: string): Promise<T> {
  const created=await invoke<LibraryTask>("start_library_task",{payload, ...(expectedLibraryId ? { expectedLibraryId } : {})});
  window.dispatchEvent(new Event("library-task-started"));
  for (;;) {
    const task=await invoke<LibraryTask>("get_library_task",{id:created.id});
    if(task.status==="completed")return task.result as T;
    if(task.status==="failed")throw new Error(task.message||"任务失败，请查看任务中心");
    if(task.status==="stopped"||task.status==="interrupted")throw new Error("任务已停止，已完成结果保留；请在任务中心检查并手动继续");
    await new Promise(resolve=>window.setTimeout(resolve,500));
  }
}

export const assetMediaUrl = (id: string, variant: "stream" | "thumbnail" | "original" = "stream") => `http://dcc-media.localhost/${encodeURIComponent(id)}/${variant}`;
export const mediaLibraryFileUrl = (fileId: string) => `http://dcc-media.localhost/library/file/${encodeURIComponent(fileId)}`;
export const mediaLibraryBundleUrl = (entryId: string, logicalPath: string) => `http://dcc-media.localhost/library/bundle/${encodeURIComponent(entryId)}/${logicalPath.split("/").map(encodeURIComponent).join("/")}`;

export const api = {
  getFabUeOptions: (assetId: string, expectedLibraryId: string) => invoke<FabUeOptions>("get_fab_ue_options", { assetId, expectedLibraryId }),
  listUeProjectTargets: (expectedLibraryId: string) => invoke<UeProjectTarget[]>("list_ue_project_targets", { expectedLibraryId }),
  inspectUeProject: (path: string, expectedLibraryId: string) => invoke<UeProjectTarget>("inspect_ue_project", { path, expectedLibraryId }),
  openFabInEpic: (assetId: string, expectedLibraryId: string, recover = false) => invoke<EpicOpenResult>("open_fab_in_epic", { assetId, expectedLibraryId, recover }),
  listBrowserCaptures: (libraryId:string,offset=0) => invoke<import("../types").Page<import("../types").BrowserCapture>>("list_browser_captures",{libraryId,offset}),
  registerBrowserCaptureHost: (extensionId:string) => invoke<string>("register_browser_capture_host",{extensionId}),
  organizeBrowserCapture: (libraryId:string,id:string) => invoke<void>("organize_browser_capture",{libraryId,id}),
  createBrowserCaptureNote: (libraryId:string,id:string) => invoke<string>("create_browser_capture_note",{libraryId,id}),
  queueModelThumbnailTasks: () => invoke<number>("queue_model_thumbnail_tasks"),
  getModelThumbnailTask: (id:string) => invoke<ModelThumbnailJob>("get_model_thumbnail_task",{id}),
  finishModelThumbnailTask: (id:string,bytes:number[]|null,error:string|null) => invoke<void>("finish_model_thumbnail_task",{id,bytes,error}),
  listLibraryTasks: () => invoke<LibraryTask[]>("list_library_tasks"),
  getTaskWarning: () => invoke<string|null>("get_task_warning"),
  getLibraryTask: (id:string) => invoke<LibraryTask>("get_library_task",{id}),
  cancelLibraryTask: (id:string) => invoke<void>("cancel_library_task",{id}),
  retryLibraryTask: (id:string) => invoke<LibraryTask>("retry_library_task",{id}),
  exitWhenTasksIdle: () => invoke<void>("exit_when_tasks_idle"),
  getSelectionLabels: (ids: string[], media: boolean) => invoke<{id:string;name:string}[]>("get_selection_labels", { ids, media }),
  getWorkspaceContext: (moduleId: string) => invoke<{ libraryId: string; enabled: boolean; value: unknown }>("get_workspace_context", { moduleId }),
  saveWorkspaceContext: (libraryId: string, moduleId: string, value: unknown) => invoke<boolean>("save_workspace_context", { libraryId, moduleId, value }),
  getMeta: (contentLanguage: ContentLanguage) => invoke<LibraryMeta>("get_library_meta", { contentLanguage }),
  search: (request: SearchRequest) => invoke<Page<AssetCard>>("search_assets", { request }),
  selectAssetIds: (request: SearchRequest) => invoke<AssetSelection>("select_asset_ids", { request }),
  getAsset: (id: string, contentLanguage: ContentLanguage) => invoke<AssetDetail>("get_asset", { id, contentLanguage }),
  saveAsset: (input: AssetInput, expectedLibraryId: string, options?: import("../types").SingleAssetSaveOptions) => invoke<AssetDetail>("upsert_asset", { input, expectedLibraryId, ...options }),
  deleteAsset: (id: string) => invoke<void>("delete_asset", { id }),
  setFavorite: (id: string, favorite: boolean) => invoke<void>("set_favorite", { id, favorite }),
  addCategory: (name: string, parentId: string | null) => invoke<string>("upsert_category", { id: null, name, parentId }),
  renameCategory: (id: string, name: string) => invoke<string>("upsert_category", { id, name, parentId: null, preserveParent: true }),
  deleteCategory: (id: string) => invoke<void>("delete_category", { id }),
  getDeleteImpact: (request: DeleteRequest) => invoke<DeleteResult>("get_delete_impact", { request }),
  deleteLibraryItems: (request: DeleteRequest) => invoke<DeleteResult>("delete_library_items", { request }),
  listTrash: (offset = 0, limit = 100) => invoke<Page<TrashBatch>>("list_trash", { offset, limit }),
  restoreTrashBatch: (batchId: string) => invoke<DeleteResult>("restore_trash_batch", { batchId }),
  purgeTrashBatch: (batchId: string) => invoke<void>("purge_trash_batch", { batchId }),
  emptyTrash: () => invoke<number>("empty_trash"),
  prepareReferenceCoverIds: (ids: string[]) => invoke<string[]>("prepare_reference_cover_ids", { ids }),
  imageData: (imageId: string, thumbnail = true) => invoke<string>("get_image_data", { imageId, thumbnail }),
  importAssetMedia: (assetId: string, paths: string[]) => runLibraryTask<AssetMedia[]>({kind:"assetMediaImport",request:{assetId,paths}}),
  deleteAssetMedia: (id: string) => invoke<void>("delete_asset_media", { id }),
  retryAssetMedia: (id: string) => runLibraryTask<AssetMedia>({kind:"assetMediaRetry",request:{id}}),
  reorderAssetMedia: (assetId: string, ids: string[]) => invoke<void>("reorder_asset_media", { assetId, ids }),
  setAssetCoverMedia: (assetId: string, mediaId: string | null, imageId: string | null) => invoke<void>("set_asset_cover_media", { assetId, mediaId, imageId }),
  searchMediaEntries: (request: MediaSearchRequest) => invoke<Page<MediaEntry>>("search_media_entries", { request }),
  selectMediaEntryIds: (request: MediaSearchRequest) => invoke<string[]>("select_media_entry_ids", { request }),
  getMediaEntry: (id: string) => invoke<MediaEntryDetail>("get_media_entry", { id }),
  openModelLocation: (id: string) => invoke<void>("open_model_location", { id }),
  exportModelPackage: (id: string, destination: string) => invoke<string>("export_model_package", { id, destination }),
  pendingModelThumbnails: () => invoke<ModelThumbnailJob[]>("pending_model_thumbnails"),
  saveModelThumbnail: (entryId: string, bytes: number[], expectedLibraryId?: string) => invoke<string>("save_model_thumbnail", { entryId, bytes, expectedLibraryId }),
  reportModelThumbnailFailure: (entryId: string, message: string) => invoke<void>("report_model_thumbnail_failure", { entryId, message }),
  importMediaEntries: (request: MediaImportRequest) => runLibraryTask<MediaImportReport>({kind:"mediaImport",request}),
  updateMediaEntry: (id: string, input: Pick<MediaEntry, "name" | "description" | "author" | "sourceUrl" | "license" | "tags">, expectedLibraryId?: string) => invoke<void>("update_media_entry", { id, ...input, expectedLibraryId }),
  deleteMediaEntries: (ids: string[]) => invoke<number>("delete_media_entries", { ids }),
  batchUpdateMediaEntries: (update: MediaBatchUpdate, expectedLibraryId?: string) => invoke<BatchUpdateReport>("batch_update_media_entries", { update, expectedLibraryId }),
  listMediaFolders: (kind: MediaKind) => invoke<MediaFolder[]>("list_media_folders", { kind }),
  saveMediaFolder: (kind: MediaKind, id: string | null, parentId: string | null, name: string) => invoke<MediaFolder>("upsert_media_folder", { kind, id, parentId, name }),
  moveMediaFolder: (id: string, targetParentId: string | null, targetIndex: number) => invoke<void>("move_media_folder", { id, targetParentId, targetIndex }),
  deleteMediaFolder: (id: string) => invoke<void>("delete_media_folder", { id }),
  linkMediaAssets: (entryIds: string[], assetIds: string[]) => invoke<number>("link_media_assets", { entryIds, assetIds }),
  unlinkMediaAssets: (entryIds: string[], assetIds: string[]) => invoke<number>("unlink_media_assets", { entryIds, assetIds }),
  linkProjectMedia: (entryIds: string[], projectIds: string[], expectedLibraryId?: string) => invoke<number>("link_project_media", { entryIds, projectIds, expectedLibraryId }),
  unlinkProjectMedia: (entryIds: string[], projectIds: string[]) => invoke<number>("unlink_project_media", { entryIds, projectIds }),
  collectAssetPreviewsToMedia: (assetId: string) => invoke<MediaImportReport>("collect_asset_previews_to_media", { assetId }),
  addMediaImagesToBoard: (boardId: string, entryIds: string[], placement: ReferencePlacement, expectedLibraryId?: string) => invoke<ReferenceImageAddReport>("add_media_images_to_board", { boardId, entryIds, placement, expectedLibraryId }),
  openShare: (id: string) => invoke<void>("open_share_link", { id }),
  openExternal: (url: string) => invoke<void>("open_external_url", { url }),
  copyCode: (id: string) => invoke<void>("copy_extraction_code", { id }),
  inspectImport: (path: string, mapping: ImportMapping = {}) => invoke<ImportPreview>("preview_import", { path, mapping }),
  commitImport: (path: string, mapping: ImportMapping, autoTranslate: boolean, expectedLibraryId?: string) => runLibraryTask<ImportReport>({kind:"fabImport",request:{path,mapping,autoTranslate}}, expectedLibraryId),
  batchTranslateMissingAssets: (ids: string[], expectedLibraryId?: string) => runLibraryTask<BatchTranslationReport>({kind:"translation",request:{ids}}, expectedLibraryId),
  exportTemplate: (path: string) => invoke<void>("export_import_template", { path }),
  exportBackup: (path: string) => withKnowledgeBarrier(() => runLibraryTask<void>({kind:"backup",request:{path}})),
  restoreBackup: (path: string) => withKnowledgeBarrier(() => runLibraryTask<void>({kind:"restore",request:{path}})),
  getLibraryLocations: () => invoke<LibraryLocationState>("get_library_locations"),
  changeLibrary: (request: StorageChangeRequest) => withKnowledgeBarrier(() => request.mode === "migrate" ? runLibraryTask<LibraryLocationState>({kind:"migration",request}) : invoke<LibraryLocationState>("change_library", { request })),
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
  checkAllShareLinks: () => runLibraryTask<LinkCheckReport>({kind:"links"}),
  cancelShareLinkCheck: () => invoke<boolean>("cancel_share_link_check"),
  checkAssetShareLink: (id: string) => invoke<LinkCheckResult>("check_asset_share_link", { id }),
  getTranslationSettings: () => invoke<TranslationSettings>("get_translation_settings"),
  saveDeepseekCredentials: (apiKey: string) => invoke<void>("save_deepseek_credentials", { apiKey }),
  deleteDeepseekCredentials: () => invoke<void>("delete_deepseek_credentials"),
  saveTranslationPreferences: (preferences: import("../types").TranslationPreferences) => invoke<void>("save_translation_preferences", { preferences }),
  clearTranslationCache: () => invoke<void>("clear_translation_cache"),
  estimateMissingTranslations: (ids: string[]) => invoke<import("../types").TranslationEstimate>("estimate_missing_translations", { ids }),
  estimateTranslationCost: (request: import("../types").TranslationCostRequest, expectedLibraryId: string) => invoke<import("../types").TranslationCostEstimate>("estimate_translation_cost", { request, expectedLibraryId }),
  estimateTranslation: (request: TranslationRequest) => invoke<import("../types").TranslationEstimate>("estimate_translation", { request }),
  saveTranslationCredentials: (appId: string, secretKey: string) => invoke<void>("save_translation_credentials", { appId, secretKey }),
  deleteTranslationCredentials: () => invoke<void>("delete_translation_credentials"),
  setFabAutoTranslate: (enabled: boolean) => invoke<void>("set_fab_auto_translate", { enabled }),
  setContentLanguage: (language: ContentLanguage) => invoke<void>("set_content_language", { language }),
  testTranslationService: (provider: import("../types").TranslationProvider = "baidu") => invoke<TranslationTestResult>("test_translation_service", { provider }),
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
  ,getProject: (id: string, contentLanguage: ContentLanguage, markOpened = true) => invoke<CreativeProject>("get_project", { id, contentLanguage, markOpened, paged:true })
  ,listProjectAssets:(id:string,contentLanguage:ContentLanguage,query:string,status:string|null,offset=0,limit=60)=>invoke<Page<import("../types").ProjectAssetLink>>("list_project_assets",{id,contentLanguage,query,status,offset,limit})
  ,listProjectTasks:(id:string,status:ProjectTaskStatus|null,offset=0,limit=40)=>invoke<Page<ProjectTask>>("list_project_tasks",{id,status,offset,limit})
  ,getProjectUnitImpact:(id:string)=>invoke<{shots:number;tasks:number}>("get_project_unit_impact",{id})
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
