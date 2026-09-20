use crate::{db, glossary::GlossaryStore, models::*};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
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
    active_library: String,
    recent_libraries: Vec<RecentLibrary>,
    retained_copies: Vec<String>,
    #[serde(default = "default_language")]
    content_language: String,
    #[serde(default = "default_true")]
    fab_auto_translate: bool,
    #[serde(default)]
    global_preferences: GlobalPreferences,
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
            active_library: String::new(),
            recent_libraries: Vec::new(),
            retained_copies: Vec::new(),
            content_language: default_language(),
            fab_auto_translate: true,
            global_preferences: GlobalPreferences::default(),
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
    pub fn initialize() -> Result<Self, String> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("无法定位 LOCALAPPDATA")?;
        let config_dir = local.join("DCCAssetLibrary");
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
        let settings_path = config_dir.join(SETTINGS_FILE);
        let mut settings = read_settings(&settings_path).unwrap_or_default();
        let default_path = config_dir.clone();
        ensure_library(&default_path, "默认素材库", true)?;
        let desired = if settings.active_library.trim().is_empty() {
            default_path.clone()
        } else {
            PathBuf::from(&settings.active_library)
        };
        let (active, warning) = match open_library(&desired) {
            Ok(active) => (active, None),
            Err(error) if desired != default_path => (
                open_library(&default_path)?,
                Some(format!("上次使用的素材库无法打开，已回到默认库：{error}")),
            ),
            Err(error) => return Err(error),
        };
        settings.active_library = path_text(&active.base_dir);
        touch_recent(&mut settings, &active.base_dir);
        write_settings(&settings_path, &settings)?;
        Ok(Self {
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
        self.cancel_link_check()?;
        if let Ok(mut pending) = self.pending_move_undo.lock() {
            *pending = None;
        }
        let target = absolute_clean_path(&request.path)?;
        validate_not_root(&target)?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| "素材库锁已损坏".to_string())?;
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
        let next = open_library(&target)?;
        let old_path = path_text(&active.base_dir);
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| "设置锁已损坏".to_string())?;
        if request.mode == "migrate" && !settings.retained_copies.contains(&old_path) {
            settings.retained_copies.push(old_path.clone());
        }
        settings.active_library = path_text(&target);
        touch_recent(&mut settings, &PathBuf::from(&old_path));
        touch_recent(&mut settings, &target);
        write_settings(&self.settings_path, &settings)?;
        *active = next;
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
    fs::create_dir_all(path).map_err(|e| format!("创建素材库目录失败：{e}"))?;
    fs::create_dir_all(path.join("images/originals")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("images/thumbnails")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("reference-boards")).map_err(|e| e.to_string())?;
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
    drop(db::open_database(&path.join("library.db"))?);
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

fn open_library(path: &Path) -> Result<ActiveLibrary, String> {
    validate_library(path)?;
    let marker = read_marker(path)?;
    let db_path = path.join("library.db");
    let connection = db::open_database(&db_path)?;
    Ok(ActiveLibrary {
        base_dir: path.to_path_buf(),
        db_path,
        marker,
        connection: Some(connection),
    })
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
    let data = fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(data)))
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
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("应用设置无效：{e}"))
}
fn write_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    write_json_atomic(path, settings)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    fs::rename(temporary, path).map_err(|e| e.to_string())
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_state(path: &Path) -> AppState {
        ensure_library(path, "测试库", false).unwrap();
        AppState {
            settings_path: path.join("settings.json"),
            settings: Mutex::new(AppSettings::default()),
            startup_warning: Mutex::new(None),
            active: Mutex::new(open_library(path).unwrap()),
            link_check_job: Mutex::new(None),
            pending_move_undo: Mutex::new(None),
            reference_window_board: Mutex::new(None),
            glossary: Mutex::new(GlossaryStore::load(path.join("translation-glossary.json"))),
        }
    }
    #[test]
    fn creates_and_validates_library_marker() {
        let dir = tempdir().unwrap();
        let library = dir.path().join("library");
        ensure_library(&library, "测试库", false).unwrap();
        validate_library(&library).unwrap();
        assert_eq!(read_marker(&library).unwrap().name, "测试库");
    }

    #[test]
    fn migration_keeps_source_and_validates_copy() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        ensure_library(&source, "源库", false).unwrap();
        let mut active = open_library(&source).unwrap();
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
