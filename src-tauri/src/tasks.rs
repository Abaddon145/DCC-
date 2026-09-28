use crate::media_library::ModelThumbnailJob;
use crate::{commands, media_library, models::*, state::AppState};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "camelCase")]
pub enum TaskRequest {
    MediaImport(MediaImportRequest),
    ModelThumbnail {
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    FabImport {
        path: String,
        mapping: HashMap<String, String>,
        #[serde(rename = "autoTranslate")]
        auto_translate: bool,
    },
    Translation {
        ids: Vec<String>,
    },
    Links,
    Backup {
        path: String,
    },
    Restore {
        path: String,
    },
    Migration(StorageChangeRequest),
}
impl TaskRequest {
    fn label(&self) -> &str {
        match self {
            Self::MediaImport(_) => "媒体导入",
            Self::ModelThumbnail { .. } => "模型缩略图",
            Self::FabImport { .. } => "Fab / 表格导入",
            Self::Translation { .. } => "双语补译",
            Self::Links => "网盘链接检查",
            Self::Backup { .. } => "备份导出",
            Self::Restore { .. } => "备份恢复",
            Self::Migration(_) => "素材库迁移",
        }
    }
    fn cancellable(&self) -> bool {
        !matches!(
            self,
            Self::Restore { .. } | Self::Migration(_) | Self::Backup { .. }
        )
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::MediaImport(r) if r.paths.len() > 500 => Err("单次媒体导入最多 500 项".into()),
            Self::Translation { ids } if ids.len() > 5000 => Err("单次翻译最多 5000 项".into()),
            Self::Migration(r) if r.mode != "migrate" => Err("任务中心只接受复制迁移操作".into()),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTask {
    pub id: String,
    pub library_id: String,
    pub label: String,
    pub status: String,
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub current_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub cancellable: bool,
    pub message: String,
    pub result: Option<Value>,
    pub payload: TaskRequest,
    #[serde(default)]
    pub stop_requested: bool,
}
fn active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}
struct TaskStore {
    entries: Vec<LibraryTask>,
    dispatching: bool,
    external: HashMap<String, Result<Value, String>>,
}
pub struct TaskRegistry {
    path: PathBuf,
    store: Mutex<TaskStore>,
}
impl TaskRegistry {
    pub fn load(path: PathBuf) -> Self {
        let mut entries: Vec<LibraryTask> = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for task in &mut entries {
            if active(&task.status) {
                task.status = "interrupted".into();
                task.message = "上次运行中断，请检查结果后手动继续".into();
                task.stop_requested = false;
            }
        }
        Self {
            path,
            store: Mutex::new(TaskStore {
                entries,
                dispatching: false,
                external: HashMap::new(),
            }),
        }
    }
    fn persist(&self, store: &TaskStore) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = self.path.with_extension("tmp");
        fs::write(
            &temp,
            serde_json::to_vec(&store.entries).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temp, &self.path).map_err(|e| e.to_string())
    }
    pub fn complete_external(&self, id: &str, value: Result<Value, String>) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        let task = store
            .entries
            .iter()
            .find(|t| t.id == id)
            .ok_or("任务不存在")?;
        if task.status != "running" || task.stop_requested {
            return Err("任务已失效".into());
        }
        store.external.insert(id.into(), value);
        Ok(())
    }
    fn take_external(&self, id: &str) -> Option<Result<Value, String>> {
        self.store.lock().ok()?.external.remove(id)
    }
    pub fn maintenance(&self) -> Option<(String, bool)> {
        self.store
            .lock()
            .ok()?
            .entries
            .iter()
            .find(|t| t.status == "running" && !t.cancellable)
            .map(|task| {
                (
                    task.library_id.clone(),
                    matches!(task.payload, TaskRequest::Backup { .. }),
                )
            })
    }
    pub fn busy(&self) -> bool {
        self.store
            .lock()
            .map(|store| store.entries.iter().any(|t| active(&t.status)))
            .unwrap_or(true)
    }
    pub fn list(&self, library_id: &str) -> Result<Vec<LibraryTask>, String> {
        Ok(self
            .store
            .lock()
            .map_err(|_| "任务状态锁已损坏")?
            .entries
            .iter()
            .filter(|t| t.library_id == library_id)
            .rev()
            .scan(0usize, |history, task| {
                if active(&task.status) {
                    Some(Some(task))
                } else {
                    *history += 1;
                    Some(if *history <= 200 { Some(task) } else { None })
                }
            })
            .flatten()
            .map(|task| LibraryTask {
                id: task.id.clone(),
                library_id: task.library_id.clone(),
                label: task.label.clone(),
                status: task.status.clone(),
                phase: task.phase.clone(),
                current: task.current,
                total: task.total,
                current_name: task.current_name.clone(),
                created_at: task.created_at.clone(),
                updated_at: task.updated_at.clone(),
                cancellable: task.cancellable,
                message: task.message.clone(),
                result: None,
                payload: task.payload.clone(),
                stop_requested: task.stop_requested,
            })
            .collect())
    }
    pub fn has_manual_thumbnail_task(&self, library_id: &str, entry_id: &str) -> bool {
        self.store.lock().map(|store|store.entries.iter().any(|task|task.library_id==library_id&&matches!(task.status.as_str(),"queued"|"running"|"interrupted"|"stopped"|"failed")&&matches!(&task.payload,TaskRequest::ModelThumbnail{entry_id:id} if id==entry_id))).unwrap_or(true)
    }
    pub fn get(&self, id: &str) -> Result<LibraryTask, String> {
        self.store
            .lock()
            .map_err(|_| "任务状态锁已损坏")?
            .entries
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or("任务不存在".into())
    }
    pub fn submit(
        &self,
        library_id: String,
        payload: TaskRequest,
    ) -> Result<(LibraryTask, bool), String> {
        payload.validate()?;
        let timestamp = Utc::now().to_rfc3339();
        let task = LibraryTask {
            id: Uuid::new_v4().to_string(),
            library_id,
            label: payload.label().into(),
            status: "queued".into(),
            phase: "等待执行".into(),
            current: 0,
            total: 0,
            current_name: String::new(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            cancellable: payload.cancellable(),
            message: String::new(),
            result: None,
            payload,
            stop_requested: false,
        };
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        if let TaskRequest::ModelThumbnail { entry_id } = &task.payload {
            if let Some(existing)=store.entries.iter().find(|value|active(&value.status)&&matches!(&value.payload,TaskRequest::ModelThumbnail{entry_id:id} if id==entry_id)){return Ok((existing.clone(),false))}
        }
        if store
            .entries
            .iter()
            .any(|t| active(&t.status) && !t.cancellable)
            || (!task.cancellable && store.entries.iter().any(|t| active(&t.status)))
        {
            return Err("有任务执行或等待中，请安全停止或等待完成后再维护素材库".into());
        }
        store.entries.push(task.clone());
        if let Err(error) = self.persist(&store) {
            store.entries.pop();
            return Err(error);
        }
        let launch = !store.dispatching;
        store.dispatching = true;
        Ok((task, launch))
    }
    fn claim(&self) -> Option<LibraryTask> {
        let mut store = self.store.lock().ok()?;
        let Some(task) = store.entries.iter_mut().find(|t| t.status == "queued") else {
            store.dispatching = false;
            return None;
        };
        task.status = "running".into();
        task.phase = "开始处理".into();
        let task = task.clone();
        let _ = self.persist(&store);
        Some(task)
    }
    pub fn update(
        &self,
        id: &str,
        current: usize,
        total: usize,
        name: String,
        phase: String,
        result: Option<Value>,
    ) {
        if let Ok(mut store) = self.store.lock() {
            if let Some(task) = store.entries.iter_mut().find(|t| t.id == id) {
                task.current = current;
                task.total = total;
                task.current_name = name;
                task.phase = phase;
                task.updated_at = Utc::now().to_rfc3339();
                if result.is_some() {
                    task.result = result;
                }
            }
            let _ = self.persist(&store);
        }
    }
    pub fn progress(&self, label: &str, current: usize, total: usize, name: String, phase: String) {
        let id = self.store.lock().ok().and_then(|store| {
            store
                .entries
                .iter()
                .find(|t| t.status == "running" && t.label == label)
                .map(|t| t.id.clone())
        });
        if let Some(id) = id {
            self.update(&id, current, total, name, phase, None)
        }
    }
    pub fn checkpoint(&self, label: &str, result: Value) {
        if let Ok(mut store) = self.store.lock() {
            if let Some(task) = store
                .entries
                .iter_mut()
                .find(|t| t.status == "running" && t.label == label)
            {
                task.result = Some(result);
            }
            let _ = self.persist(&store);
        }
    }
    pub fn should_stop(&self, id: &str) -> bool {
        self.get(id).map(|t| t.stop_requested).unwrap_or(true)
    }
    pub fn stop_label(&self, label: &str) -> bool {
        self.store
            .lock()
            .map(|store| {
                store
                    .entries
                    .iter()
                    .any(|t| t.status == "running" && t.label == label && t.stop_requested)
            })
            .unwrap_or(true)
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        let task = store
            .entries
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("任务不存在")?;
        if task.status == "queued" {
            task.status = "stopped".into();
            task.message = "尚未开始，已停止".into();
        } else if task.status == "running" {
            if !task.cancellable {
                return Err("此任务处于数据维护阶段，请等待安全完成".into());
            }
            task.stop_requested = true;
            task.phase = "正在安全停止，当前步骤完成后生效".into();
        }
        self.persist(&store)
    }
    fn finish(&self, id: &str, result: Result<Value, String>) {
        if let Ok(mut store) = self.store.lock() {
            store.external.remove(id);
            if let Some(task) = store.entries.iter_mut().find(|t| t.id == id) {
                task.updated_at = Utc::now().to_rfc3339();
                task.status = if task.stop_requested {
                    "stopped"
                } else if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                task.phase = task.status.clone();
                match result {
                    Ok(value) => task.result = Some(value),
                    Err(error) => task.message = error,
                };
            }
            let _ = self.persist(&store);
        }
    }
}

#[tauri::command]
pub fn list_library_tasks(state: tauri::State<AppState>) -> Result<Vec<LibraryTask>, String> {
    let id = state
        .tasks
        .maintenance()
        .map(|v| v.0)
        .map(Ok)
        .unwrap_or_else(|| state.active_library_id())?;
    state.tasks.list(&id)
}
#[tauri::command]
pub fn exit_when_tasks_idle(app: AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    if state.tasks.busy() {
        return Err("仍有任务执行，请先等待或安全停止".into());
    }
    app.exit(0);
    Ok(())
}
#[tauri::command]
pub fn get_library_task(state: tauri::State<AppState>, id: String) -> Result<LibraryTask, String> {
    state.tasks.get(&id)
}
#[tauri::command]
pub fn queue_model_thumbnail_tasks(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<usize, String> {
    let id = state.active_library_id()?;
    let jobs =
        state.with_library(|connection, _| media_library::pending_model_thumbnails(connection))?;
    let mut count = 0;
    for job in jobs {
        if state.tasks.has_manual_thumbnail_task(&id, &job.id) {
            continue;
        }
        start_library_task(
            app.clone(),
            state.clone(),
            TaskRequest::ModelThumbnail { entry_id: job.id },
        )?;
        count += 1;
    }
    Ok(count)
}
#[tauri::command]
pub fn get_model_thumbnail_task(
    state: tauri::State<AppState>,
    id: String,
) -> Result<ModelThumbnailJob, String> {
    let task = state.tasks.get(&id)?;
    if task.status != "running" || task.library_id != state.active_library_id()? {
        return Err("任务不在当前素材库运行".into());
    }
    let TaskRequest::ModelThumbnail { entry_id } = task.payload else {
        return Err("不是模型任务".into());
    };
    state.with_library(|connection, _| {
        let detail = media_library::get(connection, &entry_id)?;
        let main = detail
            .files
            .iter()
            .find(|f| f.id == detail.entry.primary_file_id)
            .ok_or("原模型缺失")?;
        Ok(ModelThumbnailJob {
            id: entry_id,
            logical_path: main.logical_path.clone(),
            original_name: main.original_name.clone(),
        })
    })
}
#[tauri::command]
pub fn finish_model_thumbnail_task(
    state: tauri::State<AppState>,
    id: String,
    bytes: Option<Vec<u8>>,
    error: Option<String>,
) -> Result<(), String> {
    let task = state.tasks.get(&id)?;
    if task.status != "running"
        || task.stop_requested
        || task.library_id != state.active_library_id()?
    {
        return Err("缩略图任务已失效".into());
    }
    let TaskRequest::ModelThumbnail { entry_id } = task.payload else {
        return Err("不是模型任务".into());
    };
    let result = if let Some(bytes) = bytes {
        state.with_library_mut(|connection, base| {
            if media_library::get(connection, &entry_id)?
                .entry
                .thumbnail_file_id
                .is_some()
            {
                return Ok(serde_json::json!({"message":"未覆盖已有封面"}));
            }
            let file_id = media_library::save_model_thumbnail(connection, base, &entry_id, &bytes)?;
            Ok(serde_json::json!({"fileId":file_id}))
        })
    } else {
        let message = error.unwrap_or_else(|| "模型无法生成缩略图".into());
        let _ = state.with_library(|connection, _| {
            media_library::report_model_thumbnail_failure(connection, &entry_id, &message)
        });
        Err(message)
    };
    state.tasks.complete_external(&id, result)
}
#[tauri::command]
pub fn cancel_library_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let task = state.tasks.get(&id)?;
    state.tasks.cancel(&id)?;
    if matches!(task.payload, TaskRequest::Links) {
        state.cancel_link_check()?;
    }
    Ok(())
}
#[tauri::command]
pub fn start_library_task(
    app: AppHandle,
    state: tauri::State<AppState>,
    payload: TaskRequest,
) -> Result<LibraryTask, String> {
    if matches!(
        payload,
        TaskRequest::Restore { .. } | TaskRequest::Migration(_)
    ) && state.reference_window_board_id()?.is_some()
    {
        return Err("请先收回悬浮参考板，再维护素材库".into());
    }
    let (task, launch) = state.tasks.submit(state.active_library_id()?, payload)?;
    if launch {
        tauri::async_runtime::spawn(dispatch(app));
    }
    Ok(task)
}
#[tauri::command]
pub fn retry_library_task(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<LibraryTask, String> {
    let previous = state.tasks.get(&id)?;
    if previous.library_id != state.active_library_id()? {
        return Err("请先打开任务所属的素材库".into());
    }
    if active(&previous.status) {
        return Err("任务仍在执行".into());
    }
    let mut payload = previous.payload;
    if let TaskRequest::MediaImport(request) = &mut payload {
        if let Some(rows) = previous
            .result
            .and_then(|v| v.get("rows").cloned())
            .and_then(|v| v.as_array().cloned())
        {
            request.paths.retain(|path| {
                !rows.iter().any(|row| {
                    row["path"].as_str() == Some(path.as_str())
                        && matches!(row["status"].as_str(), Some("imported" | "duplicate"))
                })
            });
        }
    }
    start_library_task(app, state, payload)
}
async fn dispatch(app: AppHandle) {
    loop {
        let Some(task) = app.state::<AppState>().tasks.claim() else {
            break;
        };
        let _ = app.emit("library-task-updated", &task);
        let result = execute(app.clone(), task.clone()).await;
        app.state::<AppState>().tasks.finish(&task.id, result);
        if let Ok(updated) = app.state::<AppState>().tasks.get(&task.id) {
            let _ = app.emit("library-task-updated", updated);
        }
    }
}
fn open_media_worker_connection(path: &Path) -> Result<rusqlite::Connection, String> {
    let connection = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("配置媒体导入数据库失败：{e}"))?;
    // busy_timeout PRAGMA returns a row; execute_batch rejects it under extra_check.
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("配置媒体导入等待超时失败：{e}"))?;
    Ok(connection)
}

async fn execute(app: AppHandle, task: LibraryTask) -> Result<Value, String> {
    let state = app.state::<AppState>();
    if state.active_library_id()? != task.library_id {
        return Err("任务所属素材库已改变，未执行".into());
    }
    match task.payload.clone() {
        TaskRequest::ModelThumbnail { entry_id } => {
            let started = std::time::Instant::now();
            loop {
                if state.tasks.should_stop(&task.id) {
                    return Err("缩略图生成已停止".into());
                }
                if let Some(result) = state.tasks.take_external(&task.id) {
                    return result;
                }
                if started.elapsed() > std::time::Duration::from_secs(90) {
                    return Err("模型缩略图生成超时，请打开模型检查后手动重试".into());
                }
                let exists = state
                    .with_library_if_id(&task.library_id, |connection, _| {
                        Ok(media_library::get(connection, &entry_id)?
                            .entry
                            .thumbnail_file_id
                            .is_some())
                    })?
                    .unwrap_or(false);
                if exists {
                    return Ok(serde_json::json!({"message":"已有封面，未覆盖"}));
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
        TaskRequest::MediaImport(request) => {
            let worker_app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = worker_app.state::<AppState>();
                let (base, path) = state.with_active_mut(|active| {
                    Ok((active.base_dir.clone(), active.db_path.clone()))
                })?;
                let mut connection = open_media_worker_connection(&path)?;
                let resource = worker_app.path().resource_dir().ok();
                let stop_app = worker_app.clone();
                let stop_id = task.id.clone();
                let _process_control = crate::media::ProcessControl::new(move || {
                    stop_app.state::<AppState>().tasks.should_stop(&stop_id)
                });
                let report = media_library::import_controlled(
                    &mut connection,
                    &base,
                    resource.as_deref(),
                    request,
                    &|current, total, name, report| {
                        state.tasks.update(
                            &task.id,
                            current,
                            total,
                            name,
                            "校验 / 复制 / 生成预览".into(),
                            serde_json::to_value(report).ok(),
                        );
                    },
                    &|| state.tasks.should_stop(&task.id),
                )?;
                serde_json::to_value(report).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        }
        TaskRequest::FabImport {
            path,
            mapping,
            auto_translate,
        } => serde_json::to_value(
            commands::import_assets(app.clone(), state, path, mapping, auto_translate).await?,
        )
        .map_err(|e| e.to_string()),
        TaskRequest::Translation { ids } => serde_json::to_value(
            commands::batch_translate_missing_assets(app.clone(), state, ids).await?,
        )
        .map_err(|e| e.to_string()),
        TaskRequest::Links => {
            serde_json::to_value(commands::check_all_share_links(app.clone(), state).await?)
                .map_err(|e| e.to_string())
        }
        payload => {
            drop(state);
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                state.tasks.update(
                    &task.id,
                    0,
                    0,
                    String::new(),
                    "创建快照 / 复制 / 校验（安全维护中）".into(),
                    None,
                );
                match payload {
                    TaskRequest::Backup { path } => {
                        let (base, db_path) = state.with_active_mut(|active| {
                            Ok((active.base_dir.clone(), active.db_path.clone()))
                        })?;
                        let connection =
                            rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
                        connection
                            .busy_timeout(std::time::Duration::from_secs(5))
                            .map_err(|e| e.to_string())?;
                        crate::backup::export_library_with_progress(
                            &connection,
                            &base,
                            Path::new(&path),
                            &|current, total, name, phase| {
                                state
                                    .tasks
                                    .update(&task.id, current, total, name, phase, None)
                            },
                        )?;
                        Ok(serde_json::json!({"outputPath":path}))
                    }
                    TaskRequest::Restore { path } => {
                        state.with_active_mut(|active| {
                            crate::backup::restore_library(active, Path::new(&path))
                        })?;
                        let safety = state.with_active_mut(|active| {
                            Ok(active
                                .base_dir
                                .join("safety-backups")
                                .to_string_lossy()
                                .to_string())
                        })?;
                        Ok(serde_json::json!({"safetyBackupDirectory":safety}))
                    }
                    TaskRequest::Migration(request) => {
                        serde_json::to_value(state.change_library(request)?)
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("未知维护任务".into()),
                }
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_busy_timeout_assignment_returns_a_result_row() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let timeout: i64 = connection
            .query_row("PRAGMA busy_timeout=5000", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5000);
    }
    #[test]
    fn background_media_import_initializes_connection_and_commits_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let active = crate::db::open_database(&path).unwrap();
        let source = dir.path().join("triangle.obj");
        fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let mut worker = open_media_worker_connection(&path).expect("后台导入连接应成功初始化");
        assert_eq!(
            worker
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            worker
                .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            5000
        );
        let report = media_library::import_controlled(
            &mut worker,
            dir.path(),
            None,
            MediaImportRequest {
                kind: MediaKind::Model,
                paths: vec![source.to_string_lossy().into_owned()],
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.failed, 0);
        let id = report.rows[0].entry_id.as_ref().unwrap();
        assert_eq!(
            media_library::get(&active, id).unwrap().entry.name,
            "triangle"
        );
    }
    #[test]
    fn background_image_import_keeps_existing_entries_and_skips_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let active = crate::db::open_database(&path).unwrap();
        let source = dir.path().join("reference.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([20, 70, 120, 255]))
            .save(&source)
            .unwrap();
        let mut worker = open_media_worker_connection(&path).unwrap();
        for expected in [1, 0] {
            let report = media_library::import_controlled(
                &mut worker,
                dir.path(),
                None,
                MediaImportRequest {
                    kind: MediaKind::Image,
                    paths: vec![source.to_string_lossy().into_owned()],
                    folder_id: None,
                    tags: vec![],
                    favorite: false,
                },
                &|_, _, _, _| {},
                &|| false,
            )
            .unwrap();
            assert_eq!(report.imported, expected);
            assert_eq!(report.failed, 0);
            if expected == 0 {
                assert_eq!(report.skipped, 1);
            }
        }
        assert_eq!(
            active
                .query_row("SELECT COUNT(*) FROM media_entries", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn maintenance_is_exclusive_and_thumbnail_tasks_are_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (first, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::ModelThumbnail {
                    entry_id: "model".into(),
                },
            )
            .unwrap();
        let (second, launch) = registry
            .submit(
                "lib".into(),
                TaskRequest::ModelThumbnail {
                    entry_id: "model".into(),
                },
            )
            .unwrap();
        assert_eq!(first.id, second.id);
        assert!(!launch);
        assert!(registry
            .submit(
                "lib".into(),
                TaskRequest::Restore {
                    path: "backup".into()
                }
            )
            .is_err());
        registry.cancel(&first.id).unwrap();
        let (restore, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::Restore {
                    path: "backup".into(),
                },
            )
            .unwrap();
        assert!(registry.submit("lib".into(), TaskRequest::Links).is_err());
        registry.cancel(&restore.id).unwrap();
    }
    #[test]
    fn partial_results_survive_restart_without_network_resume() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (task, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::Translation {
                    ids: vec!["a".into(), "b".into()],
                },
            )
            .unwrap();
        registry.claim().unwrap();
        registry.update(
            &task.id,
            1,
            2,
            "素材B".into(),
            "补译".into(),
            Some(serde_json::json!({"rows":[{"assetId":"a","status":"translated"}]})),
        );
        let restored = TaskRegistry::load(path);
        let task = restored.get(&task.id).unwrap();
        assert_eq!(task.status, "interrupted");
        assert_eq!(task.current, 1);
        assert_eq!(task.result.unwrap()["rows"][0]["assetId"], "a");
        assert!(!restored.busy());
    }

    #[test]
    fn long_history_keeps_running_model_visible_and_does_not_restart_paused_models() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (first, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::ModelThumbnail {
                    entry_id: "first".into(),
                },
            )
            .unwrap();
        registry.claim().unwrap();
        for index in 0..205 {
            registry
                .submit(
                    "lib".into(),
                    TaskRequest::ModelThumbnail {
                        entry_id: format!("model-{index}"),
                    },
                )
                .unwrap();
        }
        assert!(registry
            .list("lib")
            .unwrap()
            .iter()
            .any(|task| task.id == first.id && task.status == "running"));
        registry.cancel(&first.id).unwrap();
        registry.finish(&first.id, Err("停止".into()));
        assert!(registry.has_manual_thumbnail_task("lib", "first"));
        assert!(!registry.has_manual_thumbnail_task("other", "first"));
    }
    #[test]
    fn journal_recovers_as_interrupted_and_does_not_autorun() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (task, _) = registry
            .submit("library".into(), TaskRequest::Translation { ids: vec![] })
            .unwrap();
        assert!(registry.busy());
        let restored = TaskRegistry::load(path);
        assert_eq!(restored.get(&task.id).unwrap().status, "interrupted");
        assert!(!restored.busy());
        assert!(restored.list("other").unwrap().is_empty());
    }
    #[test]
    fn stopped_queue_is_not_claimed_and_maintenance_cannot_be_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (task, _) = registry
            .submit("library".into(), TaskRequest::Links)
            .unwrap();
        registry.cancel(&task.id).unwrap();
        assert!(registry.claim().is_none());
        let (backup, _) = registry
            .submit(
                "library".into(),
                TaskRequest::Backup {
                    path: "output".into(),
                },
            )
            .unwrap();
        registry.claim().unwrap();
        assert!(registry.cancel(&backup.id).is_err());
    }
}
