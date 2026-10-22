use crate::{
    db,
    glossary::GlossaryStore,
    models::*,
    upgrade_backup::{self, UpgradeBackupRecord, UpgradeBackupStatus},
};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;
use walkdir::WalkDir;

const SETTINGS_FILE: &str = "app-settings.json";
const MARKER_FILE: &str = "library.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMarker {
    pub format_version: u32,
    pub library_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RecentLibrary {
    path: String,
    last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    #[serde(default)]
    download_settings: crate::downloader::models::DownloadSettings,
    active_library: String,
    recent_libraries: Vec<RecentLibrary>,
    retained_copies: Vec<String>,
    #[serde(default = "default_language")]
    content_language: String,
    #[serde(default = "default_true")]
    fab_auto_translate: bool,
    #[serde(default)]
    global_preferences: GlobalPreferences,
    #[serde(default)]
    translation_preferences: crate::translation_service::TranslationPreferences,
    #[serde(default)]
    note_appearance: serde_json::Value,
    #[serde(default)]
    last_backup_app_version: String,
    #[serde(default = "default_auto_backup_keep")]
    auto_backup_keep: i64,
    #[serde(default)]
    upgrade_backups: HashMap<String, UpgradeBackupRecord>,
}

fn default_auto_backup_keep() -> i64 {
    5
}

fn default_language() -> String {
    "zh-CN".into()
}
fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_settings: crate::downloader::models::DownloadSettings::default(),
            active_library: String::new(),
            recent_libraries: Vec::new(),
            retained_copies: Vec::new(),
            content_language: default_language(),
            fab_auto_translate: true,
            global_preferences: GlobalPreferences::default(),
            translation_preferences: crate::translation_service::TranslationPreferences::default(),
            note_appearance: serde_json::Value::Null,
            last_backup_app_version: String::new(),
            auto_backup_keep: default_auto_backup_keep(),
            upgrade_backups: HashMap::new(),
        }
    }
}

pub struct ActiveLibrary {
    pub base_dir: PathBuf,
    pub db_path: PathBuf,
    pub marker: LibraryMarker,
    pub connection: Option<Connection>,
}

pub struct AppState {
    pub external_videos: Mutex<crate::media_access::ExternalVideoRegistry>,
    pub knowledge_editing: AtomicBool,
    pub tasks: crate::tasks::TaskRegistry,
    pub translation_service: crate::translation_service::TranslationService,
    settings_path: PathBuf,
    settings: Mutex<AppSettings>,
    startup_warning: Mutex<Option<String>>,
    pub active: Mutex<ActiveLibrary>,
    link_check_job: Mutex<Option<Arc<AtomicBool>>>,
    pending_move_undo: Mutex<Option<PendingMoveUndo>>,
    reference_window_board: Mutex<Option<String>>,
    glossary: Mutex<GlossaryStore>,
}

#[derive(Clone)]
enum MoveUndoOperation {
    Assets(db::AssetMoveUndo),
    Category(db::CategoryMoveUndo),
}

#[derive(Clone)]
struct PendingMoveUndo {
    token: String,
    library_id: String,
    created_at: Instant,
    operation: MoveUndoOperation,
}

impl AppState {
    pub fn recover_interrupted_restore(&self) -> Result<(), String> {
        self.with_active_mut(|active| {
            if self.tasks.busy() {
                return Err("请先等待当前任务结束".into());
            }
            let mut settings = self.settings.lock().map_err(|_| "设置锁已损坏")?;
            let mut warning = self
                .startup_warning
                .lock()
                .map_err(|_| "启动提示锁已损坏")?;
            crate::backup::recover_interrupted_restore(active, |path| {
                protect_existing_library(path, &self.settings_path, &mut settings, &mut warning);
            })
        })
    }

    pub fn upgrade_backup_status(&self, library_id: &str) -> Result<UpgradeBackupStatus, String> {
        let active = self.active.lock().map_err(|_| "素材库锁已损坏")?;
        if active.marker.library_id != library_id {
            return Err("素材库已改变，请重新打开设置".into());
        }
        let settings = self.settings.lock().map_err(|_| "设置锁已损坏")?;
        let key = upgrade_backup::library_key(&active.base_dir)?;
        upgrade_backup::status(
            &active.base_dir,
            settings.auto_backup_keep,
            settings
                .upgrade_backups
                .get(&key)
                .unwrap_or(&UpgradeBackupRecord::default()),
        )
    }

    pub fn save_upgrade_backup_keep(
        &self,
        library_id: &str,
        keep: i64,
    ) -> Result<UpgradeBackupStatus, String> {
        if !(1..=20).contains(&keep) {
            return Err("升级快照保留数量必须为 1–20 的整数".into());
        }
        let active = self.active.lock().map_err(|_| "素材库锁已损坏")?;
        if active.marker.library_id != library_id {
            return Err("素材库已改变，未保存升级保护设置".into());
        }
        if active.connection.is_none()
            || active.base_dir.join(crate::backup::RESTORE_MARKER).exists()
            || self.tasks.maintenance().is_some()
        {
            return Err("素材库正在安全维护，未保存升级保护设置".into());
        }
        let mut settings = self.settings.lock().map_err(|_| "设置锁已损坏")?;
        let mut next = settings.clone();
        next.auto_backup_keep = keep;
        let key = upgrade_backup::library_key(&active.base_dir)?;
        let status = upgrade_backup::status(
            &active.base_dir,
            keep,
            next.upgrade_backups
                .get(&key)
                .unwrap_or(&UpgradeBackupRecord::default()),
        )?;
        write_settings(&self.settings_path, &next)?;
        *settings = next;
        Ok(status)
    }

    pub fn config_directory(&self) -> PathBuf {
        self.settings_path
            .parent()
            .expect("fixed settings path")
            .to_path_buf()
    }
    pub fn note_appearance(&self) -> Result<serde_json::Value, String> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| "设置锁损坏")?
            .note_appearance
            .clone())
    }
    pub fn save_note_appearance(&self, value: serde_json::Value) -> Result<(), String> {
        if !value.is_object() || value.to_string().len() > 4096 {
            return Err("知识库外观设置无效".into());
        }
        for key in ["background", "text", "accent"] {
            let color = value[key].as_str().ok_or("颜色无效")?;
            if color.len() != 7
                || !color.starts_with('#')
                || !color[1..].chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err("颜色仅支持 #RRGGBB".into());
            }
        }
        let mut settings = self.settings.lock().map_err(|_| "设置锁损坏")?;
        let previous = settings.note_appearance.clone();
        settings.note_appearance = value;
        if let Err(error) = write_settings(&self.settings_path, &settings) {
            settings.note_appearance = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn initialize() -> Result<Self, String> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("无法定位 LOCALAPPDATA")?;
        let config_dir = local.join("DCCAssetLibrary");
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
        let settings_path = config_dir.join(SETTINGS_FILE);
        let mut settings = read_settings(&settings_path).unwrap_or_default();
        if settings.translation_preferences.validate().is_err() {
            settings.translation_preferences = Default::default();
        }
        let default_path = config_dir.clone();
        let desired = if settings.active_library.trim().is_empty() {
            default_path.clone()
        } else {
            PathBuf::from(&settings.active_library)
        };
        let mut warning = None;
        let prepare = if desired == default_path {
            ensure_library(&desired, "默认素材库", true)
        } else if desired.join(crate::backup::RESTORE_MARKER).exists() {
            Ok(())
        } else {
            validate_library(&desired)
        };
        let active = match prepare
            .and_then(|_| open_library(&desired, &settings_path, &mut settings, &mut warning))
        {
            Ok(active) => active,
            Err(error) if desired != default_path => {
                merge_warning(
                    &mut warning,
                    format!("上次使用的素材库无法打开，已回到默认库：{error}"),
                );
                ensure_library(&default_path, "默认素材库", true)?;
                open_library(&default_path, &settings_path, &mut settings, &mut warning)?
            }
            Err(error) => return Err(error),
        };
        if active.connection.is_none() {
            merge_warning(&mut warning, "上次素材库恢复中断，已暂停库内操作。请在设置中心→数据与素材库→稳定性与恢复，确认恢复原库。未自动重试。".into());
        }
        settings.active_library = path_text(&active.base_dir);
        touch_recent(&mut settings, &active.base_dir);
        if let Err(error) = write_settings(&settings_path, &settings) {
            merge_warning(
                &mut warning,
                format!("应用设置未保存，下次打开会重试升级前保护：{error}"),
            );
        }
        Ok(Self {
            external_videos: Mutex::default(),
            tasks: crate::tasks::TaskRegistry::load(config_dir.join("task-journal.json")),
            knowledge_editing: AtomicBool::new(false),
            translation_service: crate::translation_service::TranslationService::open(
                &config_dir.join("translation-cache.sqlite"),
            )?,
            settings_path,
            settings: Mutex::new(settings),
            startup_warning: Mutex::new(warning),
            active: Mutex::new(active),
            link_check_job: Mutex::new(None),
            pending_move_undo: Mutex::new(None),
            reference_window_board: Mutex::new(None),
            glossary: Mutex::new(GlossaryStore::load(
                config_dir.join("translation-glossary.json"),
            )),
        })
    }

    pub fn list_translation_terms(
        &self,
        query: Option<&str>,
        source_language: Option<&str>,
        target_language: Option<&str>,
        origin: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Vec<TranslationTerm>, String> {
        Ok(self
            .glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .list(query, source_language, target_language, origin, enabled))
    }

    pub fn effective_translation_terms(
        &self,
        source_language: &str,
        target_language: &str,
    ) -> Result<Vec<TranslationTerm>, String> {
        Ok(self
            .glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .effective(source_language, target_language))
    }

    pub fn upsert_translation_term(
        &self,
        input: TranslationTermInput,
    ) -> Result<TranslationTerm, String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .upsert(input)
    }

    pub fn delete_translation_term(&self, id: &str) -> Result<(), String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .delete(id)
    }

    pub fn set_translation_term_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .set_enabled(id, enabled)
    }

    pub fn reset_translation_terms(&self) -> Result<(), String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .reset()
    }

    pub fn import_translation_terms(
        &self,
        path: &Path,
    ) -> Result<TranslationTermImportReport, String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .import_xlsx(path)
    }

    pub fn export_translation_terms(&self, path: &Path) -> Result<(), String> {
        self.glossary
            .lock()
            .map_err(|_| "术语库状态锁已损坏".to_string())?
            .export_xlsx(path)
    }

    pub fn set_reference_window_board(&self, board_id: Option<String>) -> Result<(), String> {
        *self
            .reference_window_board
            .lock()
            .map_err(|_| "参考板窗口状态锁已损坏".to_string())? = board_id;
        Ok(())
    }

    pub fn reference_window_board(&self) -> Result<String, String> {
        self.reference_window_board_id()?
            .ok_or_else(|| "悬浮参考板尚未初始化".to_string())
    }

    pub fn reference_window_board_id(&self) -> Result<Option<String>, String> {
        Ok(self
            .reference_window_board
            .lock()
            .map_err(|_| "参考板窗口状态锁已损坏".to_string())?
            .clone())
    }

    fn replace_move_undo(
        &self,
        library_id: String,
        operation: Option<MoveUndoOperation>,
    ) -> Result<Option<String>, String> {
        let mut pending = self
            .pending_move_undo
            .lock()
            .map_err(|_| "撤销状态锁已损坏".to_string())?;
        *pending = operation.map(|operation| PendingMoveUndo {
            token: Uuid::new_v4().to_string(),
            library_id,
            created_at: Instant::now(),
            operation,
        });
        Ok(pending.as_ref().map(|value| value.token.clone()))
    }

    pub fn move_assets_to_category(
        &self,
        ids: Vec<String>,
        category_id: Option<String>,
    ) -> Result<MoveResult, String> {
        let (library_id, target_name, outcome) = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "素材库锁已损坏".to_string())?;
            let library_id = active.marker.library_id.clone();
            let target_name = if let Some(category_id) = category_id.as_deref() {
                active
                    .connection
                    .as_ref()
                    .ok_or("数据库正在维护，请稍后重试")?
                    .query_row(
                        "SELECT name FROM categories WHERE id=?1",
                        [category_id],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(|_| "目标分类不存在".to_string())?
            } else {
                "未分类".into()
            };
            let connection = active
                .connection
                .as_mut()
                .ok_or("数据库正在维护，请稍后重试")?;
            let outcome = db::move_assets_to_category(connection, ids, category_id)?;
            (library_id, target_name, outcome)
        };
        let undo_token =
            self.replace_move_undo(library_id, outcome.undo.map(MoveUndoOperation::Assets))?;
        Ok(MoveResult {
            moved: outcome.moved,
            message: if outcome.moved == 0 {
                "素材已经位于目标分类".into()
            } else {
                format!("已将 {} 项素材移动到“{}”", outcome.moved, target_name)
            },
            undo_token,
        })
    }

    pub fn move_category(&self, request: MoveCategoryRequest) -> Result<MoveResult, String> {
        let (library_id, outcome) = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "素材库锁已损坏".to_string())?;
            let library_id = active.marker.library_id.clone();
            let connection = active
                .connection
                .as_mut()
                .ok_or("数据库正在维护，请稍后重试")?;
            let outcome = db::move_category(connection, request)?;
            (library_id, outcome)
        };
        let undo_token =
            self.replace_move_undo(library_id, outcome.undo.map(MoveUndoOperation::Category))?;
        Ok(MoveResult {
            moved: usize::from(outcome.changed),
            message: if outcome.changed {
                format!("已移动分类“{}”", outcome.name)
            } else {
                "分类位置没有变化".into()
            },
            undo_token,
        })
    }

    pub fn undo_library_move(&self, token: &str) -> Result<UndoMoveResult, String> {
        let pending = self
            .pending_move_undo
            .lock()
            .map_err(|_| "撤销状态锁已损坏".to_string())?
            .clone()
            .ok_or("没有可以撤销的拖拽操作")?;
        if pending.token != token {
            return Err("该撤销操作已经失效".into());
        }
        if pending.created_at.elapsed() > Duration::from_secs(300) {
            return Err("撤销操作已经过期".into());
        }
        let restored = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "素材库锁已损坏".to_string())?;
            if active.marker.library_id != pending.library_id {
                return Err("已切换素材库，不能撤销之前的操作".into());
            }
            let connection = active
                .connection
                .as_mut()
                .ok_or("数据库正在维护，请稍后重试")?;
            match &pending.operation {
                MoveUndoOperation::Assets(undo) => db::undo_asset_move(connection, undo)?,
                MoveUndoOperation::Category(undo) => db::undo_category_move(connection, undo)?,
            }
        };
        let mut current = self
            .pending_move_undo
            .lock()
            .map_err(|_| "撤销状态锁已损坏".to_string())?;
        if current
            .as_ref()
            .is_some_and(|value| value.token == pending.token)
        {
            *current = None;
        }
        Ok(UndoMoveResult {
            restored,
            message: "已撤销上一次拖拽移动".into(),
        })
    }

    pub fn active_library_id(&self) -> Result<String, String> {
        self.active
            .lock()
            .map(|active| active.marker.library_id.clone())
            .map_err(|_| "素材库锁已损坏".to_string())
    }

    pub fn begin_link_check(&self) -> Result<Arc<AtomicBool>, String> {
        let mut job = self
            .link_check_job
            .lock()
            .map_err(|_| "链接检查状态锁已损坏".to_string())?;
        if job.is_some() {
            return Err("素材库链接检查正在进行中".into());
        }
        let token = Arc::new(AtomicBool::new(false));
        *job = Some(token.clone());
        Ok(token)
    }

    pub fn finish_link_check(&self, token: &Arc<AtomicBool>) {
        if let Ok(mut job) = self.link_check_job.lock() {
            if job
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
            {
                *job = None;
            }
        }
    }

    pub fn cancel_link_check(&self) -> Result<bool, String> {
        let job = self
            .link_check_job
            .lock()
            .map_err(|_| "链接检查状态锁已损坏".to_string())?;
        if let Some(token) = job.as_ref() {
            token.store(true, Ordering::SeqCst);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn with_library<T>(
        &self,
        f: impl FnOnce(&Connection, &Path) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        let connection = guard
            .connection
            .as_ref()
            .ok_or("数据库正在维护，请稍后重试")?;
        f(connection, &guard.base_dir)
    }

    pub fn with_library_mut<T>(
        &self,
        f: impl FnOnce(&mut Connection, &Path) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        let base_dir = guard.base_dir.clone();
        let connection = guard
            .connection
            .as_mut()
            .ok_or("数据库正在维护，请稍后重试")?;
        f(connection, &base_dir)
    }

    pub fn with_library_mut_if_id<T>(
        &self,
        library_id: &str,
        f: impl FnOnce(&mut Connection, &Path) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        if guard.marker.library_id != library_id {
            return Ok(None);
        }
        let base_dir = guard.base_dir.clone();
        let connection = guard
            .connection
            .as_mut()
            .ok_or("数据库正在维护，请稍后重试")?;
        f(connection, &base_dir).map(Some)
    }

    pub fn with_library_if_id<T>(
        &self,
        library_id: &str,
        f: impl FnOnce(&Connection, &Path) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        let guard = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        if guard.marker.library_id != library_id {
            return Ok(None);
        }
        let connection = guard
            .connection
            .as_ref()
            .ok_or("数据库正在维护，请稍后重试")?;
        f(connection, &guard.base_dir).map(Some)
    }

    pub fn with_active_mut<T>(
        &self,
        f: impl FnOnce(&mut ActiveLibrary) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        f(&mut guard)
    }

    pub fn locations(&self) -> Result<LibraryLocationState, String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        let settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        let current_path = path_text(&active.base_dir);
        let current = location_for(
            &active.base_dir,
            Some(&active.marker),
            true,
            settings
                .retained_copies
                .iter()
                .any(|path| path == &current_path),
            settings
                .recent_libraries
                .iter()
                .find(|item| item.path == current_path)
                .map(|item| item.last_opened_at.clone()),
        );
        let recent = settings
            .recent_libraries
            .iter()
            .filter(|item| item.path != current_path)
            .map(|item| {
                let path = PathBuf::from(&item.path);
                let marker = read_marker(&path).ok();
                location_for(
                    &path,
                    marker.as_ref(),
                    false,
                    settings
                        .retained_copies
                        .iter()
                        .any(|value| value == &item.path),
                    Some(item.last_opened_at.clone()),
                )
            })
            .collect();
        Ok(LibraryLocationState {
            current,
            recent,
            startup_warning: self
                .startup_warning
                .lock()
                .map_err(|_| "启动状态锁已损坏".to_string())?
                .clone(),
        })
    }

    pub fn change_library(
        &self,
        request: StorageChangeRequest,
    ) -> Result<LibraryLocationState, String> {
        self.change_library_inner(request, None)
    }

    pub(crate) fn change_library_for_migration(
        &self,
        task_id: &str,
    ) -> Result<LibraryLocationState, String> {
        let task = self.tasks.get(task_id)?;
        let crate::tasks::TaskRequest::Migration(request) = task.payload else {
            return Err("不是迁移任务，未切换素材库".into());
        };
        self.change_library_inner(request, Some(task_id))
    }

    fn change_library_inner(
        &self,
        request: StorageChangeRequest,
        migration_task: Option<&str>,
    ) -> Result<LibraryLocationState, String> {
        let target = absolute_clean_path(&request.path)?;
        validate_not_root(&target)?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
        // Enqueue takes this same lock: a switch cannot pass an idle check while a task submits.
        self.tasks
            .ensure_library_change_allowed(&active.marker.library_id, migration_task)?;
        self.cancel_link_check()?;
        if let Ok(mut pending) = self.pending_move_undo.lock() {
            *pending = None;
        }
        let current = absolute_clean_path(&path_text(&active.base_dir))?;
        if target == current {
            return Err("所选目录已经是当前素材库".into());
        }
        if target.starts_with(&current) || current.starts_with(&target) {
            return Err("素材库目录之间不能互相嵌套".into());
        }
        match request.mode.as_str() {
            "create" => {
                require_empty_directory(&target)?;
                ensure_library(
                    &target,
                    request.name.as_deref().unwrap_or("新素材库"),
                    false,
                )?;
            }
            "migrate" => {
                require_empty_directory(&target)?;
                migrate_library(&mut active, &target, request.name.as_deref())?;
            }
            "open" => validate_library(&target)?,
            _ => return Err("未知的素材库切换方式".into()),
        }
        let old_path = path_text(&active.base_dir);
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        let next = {
            let mut warning = self
                .startup_warning
                .lock()
                .map_err(|_| "启动状态锁已损坏")?;
            open_library(&target, &self.settings_path, &mut settings, &mut warning)?
        };
        if request.mode == "migrate" && !settings.retained_copies.contains(&old_path) {
            settings.retained_copies.push(old_path.clone());
        }
        settings.active_library = path_text(&target);
        touch_recent(&mut settings, &PathBuf::from(&old_path));
        touch_recent(&mut settings, &target);
        write_settings(&self.settings_path, &settings)?;
        *active = next;
        crate::note_moves::clear_undo();
        drop(settings);
        drop(active);
        self.locations()
    }

    pub fn forget_recent(&self, path: &str) -> Result<(), String> {
        let active_path = {
            let active = self
                .active
                .lock()
                .map_err(|_| "素材库锁已损坏".to_string())?;
            path_text(&active.base_dir)
        };
        if path == active_path {
            return Err("不能移除当前素材库".into());
        }
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        settings.recent_libraries.retain(|item| item.path != path);
        settings.retained_copies.retain(|item| item != path);
        write_settings(&self.settings_path, &settings)
    }

    pub fn translation_preferences(
        &self,
    ) -> Result<crate::translation_service::TranslationPreferences, String> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏")?
            .translation_preferences
            .clone())
    }

    pub fn save_translation_preferences(
        &self,
        value: crate::translation_service::TranslationPreferences,
    ) -> Result<(), String> {
        value.validate()?;
        let mut settings = self.settings.lock().map_err(|_| "设置锁已损坏")?;
        let mut next = settings.clone();
        next.translation_preferences = value;
        write_settings(&self.settings_path, &next)?;
        *settings = next;
        self.translation_service.clear_blocks();
        Ok(())
    }

    pub fn preferences(&self) -> Result<(String, bool), String> {
        let settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        Ok((
            settings.content_language.clone(),
            settings.fab_auto_translate,
        ))
    }

    pub fn download_settings(&self) -> Result<crate::downloader::models::DownloadSettings, String> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏")?
            .download_settings
            .clone())
    }
    pub fn save_download_settings(
        &self,
        value: crate::downloader::models::DownloadSettings,
    ) -> Result<(), String> {
        value.validate()?;
        let mut settings = self.settings.lock().map_err(|_| "设置锁已损坏")?;
        let mut next = settings.clone();
        next.download_settings = value;
        write_settings(&self.settings_path, &next)?;
        *settings = next;
        Ok(())
    }

    pub fn global_preferences(&self) -> Result<GlobalPreferences, String> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?
            .global_preferences
            .clone())
    }

    pub fn save_global_preferences(
        &self,
        value: GlobalPreferences,
    ) -> Result<GlobalPreferences, String> {
        let value = crate::organization::normalize_global(value)?;
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        settings.global_preferences = value.clone();
        write_settings(&self.settings_path, &settings)?;
        Ok(value)
    }

    pub fn set_content_language(&self, language: &str) -> Result<(), String> {
        if !matches!(language, "zh-CN" | "en") {
            return Err("不支持的内容语言".into());
        }
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        settings.content_language = language.into();
        write_settings(&self.settings_path, &settings)
    }

    pub fn set_fab_auto_translate(&self, enabled: bool) -> Result<(), String> {
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        settings.fab_auto_translate = enabled;
        write_settings(&self.settings_path, &settings)
    }
}

fn ensure_library(path: &Path, name: &str, allow_legacy: bool) -> Result<(), String> {
    if path.join(crate::backup::RESTORE_MARKER).exists() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|e| format!("创建素材库目录失败：{e}"))?;
    fs::create_dir_all(path.join("images/originals")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("images/thumbnails")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("reference-boards")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("media-library")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("safety-backups")).map_err(|e| e.to_string())?;
    let marker_path = path.join(MARKER_FILE);
    if !marker_path.exists() {
        if !allow_legacy && path.join("library.db").exists() {
            return Err("目录中存在未识别的数据库，已停止写入".into());
        }
        let marker = LibraryMarker {
            format_version: 1,
            library_id: Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        write_json_atomic(&marker_path, &marker)?;
    }
    Ok(())
}

fn validate_library(path: &Path) -> Result<(), String> {
    if !path.is_dir() || !path.join(MARKER_FILE).is_file() || !path.join("library.db").is_file() {
        return Err("所选目录不是有效的栈藏素材库".into());
    }
    let marker = read_marker(path)?;
    if marker.format_version != 1 {
        return Err("素材库格式版本不受支持".into());
    }
    let connection = Connection::open(path.join("library.db")).map_err(|e| e.to_string())?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("数据库完整性检查失败：{integrity}"));
    }
    Ok(())
}

fn open_library(
    path: &Path,
    settings_path: &Path,
    settings: &mut AppSettings,
    warning: &mut Option<String>,
) -> Result<ActiveLibrary, String> {
    if path.join(crate::backup::RESTORE_MARKER).exists() {
        return Ok(ActiveLibrary {
            base_dir: path.to_path_buf(),
            db_path: path.join("library.db"),
            marker: read_marker(path)?,
            connection: None,
        });
    }
    let marker = read_marker(path)?;
    if marker.format_version != 1 {
        return Err("素材库格式版本不受支持".into());
    }
    let db_path = path.join("library.db");
    if db_path.exists() {
        validate_library(path)?;
        protect_existing_library(path, settings_path, settings, warning);
    }
    let connection = db::open_database(&db_path)?;
    Ok(ActiveLibrary {
        base_dir: path.to_path_buf(),
        db_path,
        marker,
        connection: Some(connection),
    })
}

fn merge_warning(warning: &mut Option<String>, message: String) {
    match warning {
        Some(existing) if !existing.contains(&message) => {
            existing.push('\n');
            existing.push_str(&message);
        }
        None => *warning = Some(message),
        _ => {}
    }
}

fn protect_existing_library(
    path: &Path,
    settings_path: &Path,
    settings: &mut AppSettings,
    warning: &mut Option<String>,
) {
    let key = match upgrade_backup::library_key(path) {
        Ok(key) => key,
        Err(error) => {
            merge_warning(warning, format!("升级前数据库保护未完成：{error}"));
            return;
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    let previous = settings
        .upgrade_backups
        .get(&key)
        .cloned()
        .unwrap_or_default();
    if previous.last_success_app_version == version {
        return;
    }
    let mut record = previous.clone();
    let result = upgrade_backup::run_upgrade_backup(
        path,
        version,
        settings.auto_backup_keep.clamp(1, 20) as usize,
        &mut record,
    );
    match &result {
        Ok(status)
            if status
                .message
                .as_ref()
                .is_some_and(|s| s.contains("未清理")) =>
        {
            merge_warning(warning, status.message.clone().unwrap())
        }
        Err(error) => merge_warning(
            warning,
            format!("升级前数据库保护未完成，已继续打开；请检查目录权限和磁盘空间：{error}"),
        ),
        _ => {}
    }
    let mut next = settings.clone();
    next.upgrade_backups.insert(key.clone(), record.clone());
    if result.is_ok() {
        next.last_backup_app_version = version.into();
    }
    match write_settings(settings_path, &next) {
        Ok(()) => *settings = next,
        Err(error) => {
            // A verified file is kept, but cannot suppress retry until its record is durable.
            record.last_success_app_version = previous.last_success_app_version;
            record.snapshots = previous.snapshots;
            record.result = Some("failed".into());
            let message =
                format!("升级前保护记录未能保存到设置，下次打开会重试；已生成的文件保留：{error}");
            record.message = Some(message.clone());
            settings.upgrade_backups.insert(key, record);
            merge_warning(warning, message);
        }
    }
}

fn migrate_library(
    active: &mut ActiveLibrary,
    target: &Path,
    name: Option<&str>,
) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    let stage = target.join(format!(".migration-{}", Uuid::new_v4()));
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let connection = active.connection.as_ref().ok_or("数据库正在维护")?;
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| format!("整理数据库失败：{e}"))?;
        let staged_db = stage.join("library.db");
        connection
            .execute("VACUUM INTO ?1", [path_text(&staged_db)])
            .map_err(|e| format!("创建数据库快照失败：{e}"))?;
        copy_tree_verified(&active.base_dir.join("images"), &stage.join("images"))?;
        copy_tree_verified(
            &active.base_dir.join("reference-boards"),
            &stage.join("reference-boards"),
        )?;
        copy_tree_verified(
            &active.base_dir.join("media-library"),
            &stage.join("media-library"),
        )?;
        copy_tree_verified(&active.base_dir.join("media"), &stage.join("media"))?;
        copy_tree_verified(
            &active.base_dir.join("note-attachments"),
            &stage.join("note-attachments"),
        )?;
        copy_tree_verified(
            &active.base_dir.join(".dcc/cache/thumbnails"),
            &stage.join(".dcc/cache/thumbnails"),
        )?;
        fs::create_dir_all(stage.join("safety-backups")).map_err(|e| e.to_string())?;
        let marker = LibraryMarker {
            format_version: 1,
            library_id: Uuid::new_v4().to_string(),
            name: name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&active.marker.name)
                .to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        write_json_atomic(&stage.join(MARKER_FILE), &marker)?;
        validate_library(&stage)?;
        for entry in fs::read_dir(&stage).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            fs::rename(entry.path(), target.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
        fs::remove_dir(&stage).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() && stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn copy_tree_verified(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        fs::create_dir_all(destination).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for entry in WalkDir::new(source) {
        let entry = entry.map_err(|e| e.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| e.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            if digest_file(entry.path())? != digest_file(&target)? {
                return Err(format!("迁移文件校验失败：{}", relative.display()));
            }
        }
    }
    Ok(())
}

fn digest_file(path: &Path) -> Result<String, String> {
    crate::file_digest::digest_file(path).map(|(_, digest)| digest)
}

fn require_empty_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        if !path.is_dir() {
            return Err("目标路径不是文件夹".into());
        }
        if fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
        {
            return Err("目标目录必须为空；已有素材库请使用“打开已有库”".into());
        }
    }
    Ok(())
}

fn validate_not_root(path: &Path) -> Result<(), String> {
    if path.parent().is_none()
        || path
            .parent()
            .is_some_and(|parent| parent.as_os_str().is_empty())
    {
        Err("不能把磁盘根目录作为素材库".into())
    } else {
        Ok(())
    }
}
fn absolute_clean_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err("请选择绝对路径".into());
    }
    if path.exists() {
        path.canonicalize().map_err(|e| e.to_string())
    } else {
        Ok(path)
    }
}
fn read_marker(path: &Path) -> Result<LibraryMarker, String> {
    serde_json::from_slice(&fs::read(path.join(MARKER_FILE)).map_err(|e| e.to_string())?)
        .map_err(|e| format!("素材库标记无效：{e}"))
}
fn read_settings(path: &Path) -> Result<AppSettings, String> {
    let mut settings: AppSettings =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("应用设置无效：{e}"))?;
    settings.auto_backup_keep = settings.auto_backup_keep.clamp(1, 20);
    Ok(settings)
}
fn write_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    write_json_atomic(path, settings)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("配置目录无效")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary
        .write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn touch_recent(settings: &mut AppSettings, path: &Path) {
    let value = path_text(path);
    settings.recent_libraries.retain(|item| item.path != value);
    settings.recent_libraries.insert(
        0,
        RecentLibrary {
            path: value,
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    settings.recent_libraries.truncate(12);
}

fn location_for(
    path: &Path,
    marker: Option<&LibraryMarker>,
    is_current: bool,
    retained_copy: bool,
    last_opened_at: Option<String>,
) -> LibraryLocation {
    LibraryLocation {
        path: path_text(path),
        name: marker
            .map(|value| value.name.clone())
            .unwrap_or_else(|| "不可用的素材库".into()),
        library_id: marker
            .map(|value| value.library_id.clone())
            .unwrap_or_default(),
        available: marker.is_some() && path.join("library.db").is_file(),
        is_current,
        retained_copy,
        size_bytes: directory_size(path),
        last_opened_at,
    }
}

fn directory_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok().map(|meta| meta.len()))
        .sum()
}
fn path_text(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
pub(crate) fn test_state(path: &Path) -> AppState {
    ensure_library(path, "测试库", false).unwrap();
    let mut settings = AppSettings::default();
    let mut warning = None;
    let active = open_library(
        path,
        &path.join("settings.json"),
        &mut settings,
        &mut warning,
    )
    .unwrap();
    AppState {
        external_videos: Mutex::default(),
        knowledge_editing: AtomicBool::new(false),
        tasks: crate::tasks::TaskRegistry::load(path.join("task-journal.json")),
        translation_service: crate::translation_service::TranslationService::for_test(
            &path.join("translation-cache.sqlite"),
            false,
        ),
        settings_path: path.join("settings.json"),
        settings: Mutex::new(settings),
        startup_warning: Mutex::new(warning),
        active: Mutex::new(active),
        link_check_job: Mutex::new(None),
        pending_move_undo: Mutex::new(None),
        reference_window_board: Mutex::new(None),
        glossary: Mutex::new(GlossaryStore::load(path.join("translation-glossary.json"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn upgrade_backup_precedes_migrations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old-library");
        ensure_library(&path, "旧测试库", false).unwrap();
        let connection = db::open_database(&path.join("library.db")).unwrap();
        connection
            .execute(
                "UPDATE schema_meta SET value='12' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        drop(connection);
        let active = open_library(
            &path,
            &dir.path().join("settings.json"),
            &mut AppSettings::default(),
            &mut None,
        )
        .unwrap();
        let backup_root = dir.path().join("升级前自动备份");
        assert!(
            backup_root.exists(),
            "migration must have a pre-upgrade database snapshot"
        );
        let database = WalkDir::new(&backup_root)
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|s| s == "db"))
            .unwrap();
        let snapshot = Connection::open(database.path()).unwrap();
        assert_eq!(
            snapshot
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "12"
        );
        assert_ne!(
            active
                .connection
                .as_ref()
                .unwrap()
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "12"
        );
    }

    #[test]
    fn upgrade_backup_second_library_and_copied_uuid_are_protected() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        ensure_library(&first, "第一库", false).unwrap();
        drop(db::open_database(&first.join("library.db")).unwrap());
        fs::create_dir(&second).unwrap();
        fs::copy(first.join("library.db"), second.join("library.db")).unwrap();
        fs::copy(first.join(MARKER_FILE), second.join(MARKER_FILE)).unwrap();
        let mut settings = AppSettings::default();
        let settings_path = dir.path().join("settings.json");
        let mut warning = None;
        drop(open_library(&first, &settings_path, &mut settings, &mut warning).unwrap());
        drop(open_library(&second, &settings_path, &mut settings, &mut warning).unwrap());
        assert_eq!(
            settings.upgrade_backups.len(),
            2,
            "copied UUID must not bypass path-specific protection"
        );
        assert!(settings
            .upgrade_backups
            .values()
            .all(|r| r.snapshots.len() == 1));
        drop(open_library(&first, &settings_path, &mut settings, &mut warning).unwrap());
        assert!(settings
            .upgrade_backups
            .values()
            .all(|r| r.snapshots.len() == 1));
        assert_eq!(
            read_settings(&settings_path).unwrap().upgrade_backups.len(),
            2
        );
    }

    #[test]
    fn upgrade_backup_new_library_skips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("new");
        ensure_library(&path, "新库", false).unwrap();
        assert!(
            !path.join("library.db").exists(),
            "directory preparation must not migrate or initialize a DB"
        );
        let mut settings = AppSettings::default();
        let active = open_library(
            &path,
            &dir.path().join("settings.json"),
            &mut settings,
            &mut None,
        )
        .unwrap();
        assert!(active.connection.is_some());
        assert!(settings.upgrade_backups.is_empty());
        assert!(!dir.path().join("升级前自动备份").exists());
    }

    #[test]
    fn upgrade_backup_restore_interruption_skips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("restore");
        ensure_library(&path, "恢复库", false).unwrap();
        // The interrupted DB is deliberately unreadable; only its marker may be read.
        fs::write(path.join("library.db"), b"unfinished-restore").unwrap();
        fs::write(path.join(crate::backup::RESTORE_MARKER), b"{}").unwrap();
        let mut settings = AppSettings::default();
        let active = open_library(
            &path,
            &dir.path().join("settings.json"),
            &mut settings,
            &mut None,
        )
        .unwrap();
        assert!(active.connection.is_none());
        assert!(settings.upgrade_backups.is_empty());
        assert_eq!(
            fs::read(path.join("library.db")).unwrap(),
            b"unfinished-restore"
        );
        assert!(!dir.path().join("升级前自动备份").exists());
    }

    #[test]
    fn upgrade_backup_explicit_restore_precedes_migrations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library");
        let state = test_state(&path);
        let id = state.active_library_id().unwrap();
        state
            .with_library(|db, _| {
                db.execute(
                    "UPDATE schema_meta SET value='12' WHERE key='schema_version'",
                    [],
                )
                .map_err(|e| e.to_string())?;
                db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        drop(state.active.lock().unwrap().connection.take());
        let old = path.join(".restore-old-test");
        fs::create_dir(&old).unwrap();
        fs::rename(path.join("library.db"), old.join("library.db")).unwrap();
        fs::write(path.join("library.db"), b"partial restore").unwrap();
        crate::reliability::write_json(
            &path.join(crate::backup::RESTORE_MARKER),
            &serde_json::json!({"rollback": ".restore-old-test", "originals": ["library.db"]}),
        )
        .unwrap();

        state.recover_interrupted_restore().unwrap();
        let status = state.upgrade_backup_status(&id).unwrap();
        assert_eq!(
            status.result.as_deref(),
            Some("success"),
            "explicit recovery must protect the restored original before migration"
        );
        let database = WalkDir::new(&status.backup_directory)
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|s| s == "db"))
            .unwrap();
        let snapshot = Connection::open_with_flags(
            database.path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            snapshot
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "12"
        );
        state
            .with_library(|db, _| {
                assert_ne!(
                    db.query_row(
                        "SELECT value FROM schema_meta WHERE key='schema_version'",
                        [],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                    "12"
                );
                Ok(())
            })
            .unwrap();
        assert!(!path.join(crate::backup::RESTORE_MARKER).exists());
        assert_eq!(
            read_settings(&state.settings_path)
                .unwrap()
                .last_backup_app_version,
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn upgrade_backup_failure_does_not_block_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old");
        ensure_library(&path, "旧库", false).unwrap();
        drop(db::open_database(&path.join("library.db")).unwrap());
        fs::write(dir.path().join("升级前自动备份"), b"blocked-directory").unwrap();
        let mut settings = AppSettings::default();
        let mut warning = Some("原有启动提示".into());
        let active = open_library(
            &path,
            &dir.path().join("settings.json"),
            &mut settings,
            &mut warning,
        )
        .unwrap();
        assert!(active.connection.is_some());
        assert!(warning.as_ref().unwrap().contains("原有启动提示"));
        assert!(warning.as_ref().unwrap().contains("升级前"));
        let record = settings.upgrade_backups.values().next().unwrap();
        assert!(record.last_success_app_version.is_empty());
        assert_eq!(record.result.as_deref(), Some("failed"));
    }

    #[test]
    fn upgrade_backup_settings_failure_retries_next_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old");
        ensure_library(&path, "旧库", false).unwrap();
        drop(db::open_database(&path.join("library.db")).unwrap());
        let settings_path = dir.path().join("unwritable-settings.json");
        fs::create_dir(&settings_path).unwrap();
        let mut settings = AppSettings::default();
        let mut warning = None;
        assert!(
            open_library(&path, &settings_path, &mut settings, &mut warning)
                .unwrap()
                .connection
                .is_some()
        );
        assert!(warning.as_ref().unwrap().contains("设置"));
        let record = settings.upgrade_backups.values().next().unwrap();
        assert!(record.last_success_app_version.is_empty());
        assert_ne!(record.result.as_deref(), Some("success"));
        let before = WalkDir::new(dir.path().join("升级前自动备份"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count();
        drop(
            open_library(
                &path,
                &dir.path().join("good-settings.json"),
                &mut settings,
                &mut warning,
            )
            .unwrap(),
        );
        let after = WalkDir::new(dir.path().join("升级前自动备份"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count();
        assert_eq!(after, before + 1);
        assert_eq!(
            settings
                .upgrade_backups
                .values()
                .next()
                .unwrap()
                .last_success_app_version,
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn upgrade_backup_old_settings_defaults_and_invalid_keep_clamps() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            br#"{"activeLibrary":"old", "recentLibraries":[], "retainedCopies":[]}"#,
        )
        .unwrap();
        let defaults = read_settings(&path).unwrap();
        assert_eq!(defaults.auto_backup_keep, 5);
        assert!(defaults.last_backup_app_version.is_empty());
        assert!(defaults.upgrade_backups.is_empty());
        for (input, expected) in [(i64::MIN, 1), (0, 1), (21, 20), (i64::MAX, 20)] {
            let value = AppSettings {
                auto_backup_keep: input,
                ..defaults.clone()
            };
            write_settings(&path, &value).unwrap();
            assert_eq!(read_settings(&path).unwrap().auto_backup_keep, expected);
        }
    }

    #[test]
    fn upgrade_backup_commands_are_bound_and_keep_validation_is_atomic() {
        let dir = tempdir().unwrap();
        let state = test_state(&dir.path().join("library"));
        let id = state.active_library_id().unwrap();
        assert_eq!(state.upgrade_backup_status(&id).unwrap().keep, 5);
        assert!(state.upgrade_backup_status("previous-library").is_err());
        assert!(state
            .save_upgrade_backup_keep("previous-library", 2)
            .is_err());
        for keep in [0, 21, -1, i64::MAX] {
            assert!(state.save_upgrade_backup_keep(&id, keep).is_err());
        }
        assert_eq!(state.upgrade_backup_status(&id).unwrap().keep, 5);
        assert_eq!(state.save_upgrade_backup_keep(&id, 2).unwrap().keep, 2);
        assert_eq!(
            read_settings(&state.settings_path)
                .unwrap()
                .auto_backup_keep,
            2
        );
    }

    #[test]
    fn upgrade_backup_restore_allows_read_but_rejects_keep_changes() {
        let dir = tempdir().unwrap();
        let state = test_state(&dir.path().join("library"));
        let id = state.active_library_id().unwrap();
        fs::write(
            state
                .active
                .lock()
                .unwrap()
                .base_dir
                .join(crate::backup::RESTORE_MARKER),
            b"{}",
        )
        .unwrap();
        assert_eq!(state.upgrade_backup_status(&id).unwrap().keep, 5);
        assert!(state.save_upgrade_backup_keep(&id, 2).is_err());
        assert_eq!(state.upgrade_backup_status(&id).unwrap().keep, 5);
    }
    #[test]
    fn creates_and_validates_library_marker() {
        let dir = tempdir().unwrap();
        let library = dir.path().join("library");
        ensure_library(&library, "测试库", false).unwrap();
        drop(
            open_library(
                &library,
                &dir.path().join("settings.json"),
                &mut AppSettings::default(),
                &mut None,
            )
            .unwrap(),
        );
        validate_library(&library).unwrap();
        assert_eq!(read_marker(&library).unwrap().name, "测试库");
    }

    #[test]
    fn enqueue_after_idle_preflight_prevents_library_switch_under_shared_lock() {
        let dir = tempdir().unwrap();
        let library = dir.path().join("library");
        let target = dir.path().join("target");
        let state = test_state(&library);
        let original_id = state.active_library_id().unwrap();
        std::thread::scope(|scope| {
            let guard = state.active.lock().unwrap();
            let (preflight, ready) = std::sync::mpsc::channel();
            let state = &state;
            let target = &target;
            let switch = scope.spawn(move || {
                assert!(!state.tasks.busy());
                preflight.send(()).unwrap();
                state.change_library(StorageChangeRequest {
                    mode: "create".into(),
                    path: path_text(&target),
                    name: None,
                })
            });
            ready.recv().unwrap();
            state
                .tasks
                .submit(
                    guard.marker.library_id.clone(),
                    crate::tasks::TaskRequest::BrowserCapture {
                        id: Uuid::new_v4().to_string(),
                    },
                )
                .unwrap();
            drop(guard);
            assert!(switch.join().unwrap().unwrap_err().contains("任务仍在执行"));
        });
        assert_eq!(state.active_library_id().unwrap(), original_id);
        assert!(!target.exists());
        assert!(!library.join("settings.json").exists());
    }

    #[test]
    fn queued_or_non_migration_task_cannot_authorize_library_change() {
        let dir = tempdir().unwrap();
        let library = dir.path().join("library");
        let target = dir.path().join("target");
        let state = test_state(&library);
        let id = state.active_library_id().unwrap();
        let (capture, _) = state
            .tasks
            .submit(
                id.clone(),
                crate::tasks::TaskRequest::BrowserCapture {
                    id: Uuid::new_v4().to_string(),
                },
            )
            .unwrap();
        assert!(state
            .change_library_for_migration(&capture.id)
            .unwrap_err()
            .contains("不是迁移任务"));
        state.tasks.cancel(&capture.id).unwrap();
        let (migration, _) = state
            .tasks
            .submit(
                id,
                crate::tasks::TaskRequest::Migration(StorageChangeRequest {
                    mode: "migrate".into(),
                    path: path_text(&target),
                    name: None,
                }),
            )
            .unwrap();
        assert!(state
            .change_library_for_migration(&migration.id)
            .unwrap_err()
            .contains("迁移任务已失效"));
        assert!(!target.exists());
        state.tasks.cancel(&migration.id).unwrap();
        state
            .change_library(StorageChangeRequest {
                mode: "create".into(),
                path: path_text(&target),
                name: None,
            })
            .unwrap();
        assert_eq!(state.active.lock().unwrap().base_dir, target);
    }

    #[test]
    fn migration_keeps_source_and_validates_copy() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        ensure_library(&source, "源库", false).unwrap();
        fs::create_dir_all(source.join("media-library/image/demo")).unwrap();
        fs::write(
            source.join("media-library/image/demo/original.png"),
            b"media",
        )
        .unwrap();
        let mut active = open_library(
            &source,
            &dir.path().join("settings.json"),
            &mut AppSettings::default(),
            &mut None,
        )
        .unwrap();
        active
            .connection
            .as_ref()
            .unwrap()
            .execute(
                "INSERT INTO categories(id,name,sort_order,created_at) VALUES('c1','环境',0,'now')",
                [],
            )
            .unwrap();
        migrate_library(&mut active, &target, Some("迁移库")).unwrap();
        assert!(source.join("library.db").is_file());
        validate_library(&target).unwrap();
        let copied = Connection::open(target.join("library.db")).unwrap();
        let count: i64 = copied
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            fs::read(target.join("media-library/image/demo/original.png")).unwrap(),
            b"media"
        );
    }

    #[test]
    fn move_undo_is_latest_only_and_bound_to_library() {
        let dir = tempdir().unwrap();
        let library = dir.path().join("library");
        let state = test_state(&library);
        state.with_library(|connection, _| {
            connection.execute_batch("INSERT INTO categories(id,name,sort_order,created_at) VALUES('c1','环境',0,'now'); INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES('a1','素材','https://pan.baidu.com/s/a1','now','now');").map_err(|e| e.to_string())?;
            Ok(())
        }).unwrap();

        let first = state
            .move_assets_to_category(vec!["a1".into()], Some("c1".into()))
            .unwrap();
        let first_token = first.undo_token.unwrap();
        let second = state
            .move_assets_to_category(vec!["a1".into()], None)
            .unwrap();
        let second_token = second.undo_token.unwrap();
        assert!(state.undo_library_move(&first_token).is_err());
        assert_eq!(state.undo_library_move(&second_token).unwrap().restored, 1);

        let third = state
            .move_assets_to_category(vec!["a1".into()], None)
            .unwrap();
        let third_token = third.undo_token.unwrap();
        state.active.lock().unwrap().marker.library_id = "another-library".into();
        assert!(state.undo_library_move(&third_token).is_err());
    }
}
