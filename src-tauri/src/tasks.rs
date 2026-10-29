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

const UE_TASK_HISTORY_ONLY: &str =
    "此 UE 任务来自较新版本，当前版本只保留历史，不能提交、继续或重新执行";

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "camelCase")]
pub enum TaskRequest {
    CourseExcerpt(crate::learning::excerpt_runner::ExcerptTaskRequest),
    LearningImport(crate::learning::import_runner::ImportTaskRequest),
    CourseTranscription(crate::learning::transcription::CourseTranscriptionRequest),
    CourseAnalysis(crate::learning::analysis_models::CourseAnalysisRequest),
    #[serde(rename = "aiAnalyze")]
    AIAnalyze(crate::asset::intelligence_models::AnalysisRequest),
    Download(crate::downloader::models::DownloadRequest),
    AssetMediaImport {
        #[serde(rename = "assetId")]
        asset_id: String,
        paths: Vec<String>,
    },
    AssetMediaRetry {
        id: String,
    },
    MediaImport(MediaImportRequest),
    AssetScan {
        root: String,
    },
    AssetImport(crate::asset::models::CoreImportRequest),
    AssetProcess {
        #[serde(rename = "assetId")]
        asset_id: String,
        #[serde(default)]
        force: bool,
    },
    BrowserCapture {
        id: String,
    },
    ModelThumbnail {
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    // Keep v0.21 requests intact when opening its journal in v0.20.
    UeAssets(Value),
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
            Self::CourseExcerpt(request) => request.label(),
            Self::LearningImport(request) => request.label(),
            Self::CourseTranscription(_) => "本地课程转写",
            Self::CourseAnalysis(r) => {
                if r.stage == crate::learning::analysis_models::CourseAnalysisPhase::Summary {
                    "课程汇总分析"
                } else {
                    "课程分段分析"
                }
            }
            Self::AIAnalyze(_) => "资产 AI 分析",
            Self::Download(_) => "下载",
            Self::AssetMediaImport { .. } => "素材预览附件导入",
            Self::AssetMediaRetry { .. } => "素材预览附件重试",
            Self::MediaImport(_) => "媒体导入",
            Self::AssetScan { .. } => "资产目录扫描",
            Self::AssetImport(_) => "资产批量导入",
            Self::AssetProcess { .. } => "资产预览与元数据",
            Self::BrowserCapture { .. } => "浏览器采集",
            Self::ModelThumbnail { .. } => "模型缩略图",
            Self::UeAssets(_) => "UE 原生资产（只读历史）",
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
            Self::CourseExcerpt(request) => request.validate(),
            Self::LearningImport(request) => request.validate(),
            Self::CourseTranscription(r) => {
                Uuid::parse_str(&r.course_id).map_err(|_| "课程标识无效")?;
                if !matches!(r.model_id.as_str(), "base" | "small") {
                    return Err("模型选项无效".into());
                }
                Ok(())
            }
            Self::CourseAnalysis(r) => {
                Uuid::parse_str(&r.course_id).map_err(|_| "课程标识无效")?;
                Uuid::parse_str(&r.preview_id).map_err(|_| "课程预览标识无效")?;
                Ok(())
            }
            Self::AIAnalyze(r) => {
                Uuid::parse_str(&r.asset_id).map_err(|_| "分析资产标识无效")?;
                Uuid::parse_str(&r.preview_id)
                    .map(|_| ())
                    .map_err(|_| "分析资料标识无效".into())
            }
            Self::Download(request) => crate::downloader::domain::validate_request(request),
            Self::AssetImport(request) => crate::asset::service::validate_import(request),
            Self::AssetProcess { asset_id, .. } => Uuid::parse_str(asset_id)
                .map(|_| ())
                .map_err(|_| "资产 ID 无效".into()),
            Self::AssetScan { root } if root.trim().is_empty() || root.len() > 32768 => {
                Err("扫描目录无效".into())
            }
            Self::UeAssets(_) => Err(UE_TASK_HISTORY_ONLY.into()),
            Self::AssetMediaImport { paths, .. } if paths.len() > 100 => {
                Err("单次预览附件导入最多 100 项".into())
            }
            Self::MediaImport(r) if r.paths.len() > 500 => Err("单次媒体导入最多 500 项".into()),
            Self::Translation { ids } if ids.len() > 5000 => Err("单次翻译最多 5000 项".into()),
            Self::Migration(r) if r.mode != "migrate" => Err("任务中心只接受复制迁移操作".into()),
            _ => Ok(()),
        }
    }
    fn ensure_retryable(&self) -> Result<(), String> {
        match self {
            Self::CourseAnalysis(_) => Err("课程分析必须重新预览并确认费用，不能重放旧批准".into()),
            Self::UeAssets(_) => Err(UE_TASK_HISTORY_ONLY.into()),
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
    #[serde(default)]
    pub pause_requested: bool,
}
fn active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}
struct TaskStore {
    persistence_warning: Option<String>,
    entries: Vec<LibraryTask>,
    dispatching: bool,
    external: HashMap<String, Result<Value, String>>,
}
pub struct TaskRegistry {
    path: PathBuf,
    store: Mutex<TaskStore>,
}
impl TaskRegistry {
    fn claim_with_limit(&self, limit: usize) -> Option<LibraryTask> {
        let mut store = self.store.lock().ok()?;
        let Some(index) = store.entries.iter().position(|t| t.status == "queued") else {
            store.dispatching = store.entries.iter().any(|t| t.status == "running");
            return None;
        };
        let running = store
            .entries
            .iter()
            .filter(|t| t.status == "running")
            .collect::<Vec<_>>();
        if !running.is_empty()
            && (!matches!(store.entries[index].payload, TaskRequest::Download(_))
                || running
                    .iter()
                    .any(|t| !matches!(t.payload, TaskRequest::Download(_)))
                || running.len() >= limit.clamp(1, 4))
        {
            return None;
        }
        let task = &mut store.entries[index];
        task.status = "running".into();
        task.phase = "开始处理".into();
        let task = task.clone();
        if self.persist(&store).is_err() {
            for (position, task) in store
                .entries
                .iter_mut()
                .enumerate()
                .filter(|(_, t)| active(&t.status))
            {
                // Workers already executing remain visible until they finish stopping.
                if position == index || task.status == "queued" {
                    task.status = "interrupted".into();
                }
                task.stop_requested = true;
            }
            store.persistence_warning = Some("无法保存任务开始状态，未执行任务".into());
            store.dispatching = store.entries.iter().any(|t| t.status == "running");
            return None;
        }
        Some(task)
    }
    pub fn pause_download(&self, id: &str) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        let task = store
            .entries
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("任务不存在")?;
        if !matches!(task.payload, TaskRequest::Download(_)) {
            return Err("只有下载任务可以暂停".into());
        }
        match task.status.as_str() {
            "queued" => {
                task.status = "paused".into();
                task.phase = "paused".into();
            }
            "running" => {
                task.pause_requested = true;
                task.stop_requested = true;
                task.phase = "正在暂停并保留断点".into();
            }
            _ => return Err("任务当前不能暂停".into()),
        }
        self.persist(&store)
    }
    pub fn resume_download(&self, id: &str) -> Result<bool, String> {
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        if store.persistence_warning.is_some() {
            return Err("任务记录无法保存，请先检查磁盘".into());
        }
        if store
            .entries
            .iter()
            .any(|t| active(&t.status) && !t.cancellable)
        {
            return Err("素材库正在维护，请等待完成".into());
        }
        let task = store
            .entries
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("任务不存在")?;
        if !matches!(task.payload, TaskRequest::Download(_))
            || !matches!(
                task.status.as_str(),
                "paused" | "failed" | "interrupted" | "stopped"
            )
        {
            return Err("此下载任务不能恢复".into());
        }
        task.payload.validate()?;
        let previous = task.clone();
        task.status = "queued".into();
        task.phase = "等待恢复".into();
        task.stop_requested = false;
        task.pause_requested = false;
        task.message.clear();
        task.updated_at = Utc::now().to_rfc3339();
        if let Err(error) = self.persist(&store) {
            if let Some(task) = store.entries.iter_mut().find(|t| t.id == id) {
                *task = previous;
            }
            store.persistence_warning = Some("无法保存恢复状态，下载未恢复；请检查磁盘".into());
            return Err(error);
        }
        let launch = !store.dispatching;
        store.dispatching = true;
        Ok(launch)
    }
    pub fn load(path: PathBuf) -> Self {
        let (mut entries, warning): (Vec<LibraryTask>, Option<String>) = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(entries) => (entries, None),
                Err(_) => (
                    Vec::new(),
                    Some(
                        "任务记录损坏，原文件已保留；请导出诊断并检查备份，未自动重新执行任务"
                            .into(),
                    ),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
            Err(_) => (
                Vec::new(),
                Some("无法读取任务记录，未自动重新执行任务".into()),
            ),
        };
        for task in &mut entries {
            if active(&task.status) {
                task.status = "interrupted".into();
                task.message = "上次运行中断，请检查结果后手动继续".into();
                task.stop_requested = false;
                task.pause_requested = false;
            }
        }
        Self {
            path,
            store: Mutex::new(TaskStore {
                persistence_warning: warning,
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
        use std::io::Write;
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(&store.entries).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(temp, &self.path).map_err(|e| e.to_string())
    }
    pub fn warning(&self) -> Option<String> {
        self.store.lock().ok()?.persistence_warning.clone()
    }

    fn persist_checked(&self, store: &mut TaskStore) {
        if self.persist(store).is_err() {
            store.persistence_warning=Some("任务记录无法保存，可能磁盘空间不足；可取消的任务将安全停止，结果请勿仅依赖任务历史".into());
            for task in store
                .entries
                .iter_mut()
                .filter(|t| t.status == "running" && t.cancellable)
            {
                task.stop_requested = true;
            }
        }
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
    pub(crate) fn ensure_library_change_allowed(
        &self,
        library_id: &str,
        migration_task: Option<&str>,
    ) -> Result<(), String> {
        let store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        if let Some(id) = migration_task {
            if !store.entries.iter().any(|t| {
                t.id == id
                    && t.library_id == library_id
                    && t.status == "running"
                    && matches!(t.payload, TaskRequest::Migration(_))
            }) {
                return Err("迁移任务已失效，未切换素材库".into());
            }
        }
        if store
            .entries
            .iter()
            .any(|t| active(&t.status) && Some(t.id.as_str()) != migration_task)
        {
            return Err("任务仍在执行，请先等待或安全停止再切换素材库".into());
        }
        Ok(())
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
                if active(&task.status)
                    || matches!(
                        task.status.as_str(),
                        "failed" | "interrupted" | "stopped" | "paused"
                    )
                {
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
                pause_requested: task.pause_requested,
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
    // Single-entry saves already attempted translation. Record the terminal result
    // without queuing it again; only an explicit retry may make another request.
    pub(crate) fn record_translation_result(
        &self,
        library_id: String,
        report: BatchTranslationReport,
    ) -> Result<(), String> {
        let timestamp = Utc::now().to_rfc3339();
        let pending = report.pending > 0;
        let task = LibraryTask {
            id: Uuid::new_v4().to_string(),
            library_id,
            label: "单条录入 · 双语补齐".into(),
            status: if pending { "failed" } else { "completed" }.into(),
            phase: if pending {
                "待补译"
            } else {
                "双语已完整"
            }
            .into(),
            current: report.rows.len(),
            total: report.rows.len(),
            current_name: report
                .rows
                .first()
                .map(|row| row.name.clone())
                .unwrap_or_default(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            cancellable: false,
            message: if pending {
                "素材已保存，双语未完成；查看结果后可手动重试。"
            } else {
                "素材与双语信息已保存。"
            }
            .into(),
            payload: TaskRequest::Translation {
                ids: report.rows.iter().map(|row| row.asset_id.clone()).collect(),
            },
            result: Some(serde_json::to_value(report).map_err(|e| e.to_string())?),
            stop_requested: false,
            pause_requested: false,
        };
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        if let Some(warning) = &store.persistence_warning {
            return Err(warning.clone());
        }
        store.entries.push(task);
        if let Err(error) = self.persist(&store) {
            store.entries.pop();
            return Err(error);
        }
        Ok(())
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
            pause_requested: false,
        };
        let mut store = self.store.lock().map_err(|_| "任务状态锁已损坏")?;
        if let TaskRequest::CourseExcerpt(request) = &task.payload {
            if let Some(existing) = store.entries.iter().find(|t| {
                t.library_id == task.library_id
                    && active(&t.status)
                    && matches!(&t.payload,TaskRequest::CourseExcerpt(saved) if saved==request)
            }) {
                return Ok((existing.clone(), false));
            }
        }
        if let TaskRequest::LearningImport(request) = &task.payload {
            if let Some(existing) = store.entries.iter().find(|t| {
                t.library_id == task.library_id
                    && active(&t.status)
                    && matches!(&t.payload,TaskRequest::LearningImport(saved) if saved==request)
            }) {
                return Ok((existing.clone(), false));
            }
        }
        if let TaskRequest::AIAnalyze(request) = &task.payload {
            if let Some(existing)=store.entries.iter().find(|t|t.library_id==task.library_id&&active(&t.status)&&matches!(&t.payload,TaskRequest::AIAnalyze(r) if r.asset_id==request.asset_id&&r.preview_id==request.preview_id&&r.force==request.force)){return Ok((existing.clone(),false));}
        }
        if let TaskRequest::CourseTranscription(request) = &task.payload {
            if let Some(existing)=store.entries.iter().find(|t|t.library_id==task.library_id&&active(&t.status)&&matches!(&t.payload,TaskRequest::CourseTranscription(r)if r.course_id==request.course_id&&r.model_id==request.model_id)){return Ok((existing.clone(),false));}
        }
        if let TaskRequest::CourseAnalysis(request) = &task.payload {
            if let Some(existing)=store.entries.iter().find(|t|t.library_id==task.library_id && active(&t.status) && matches!(&t.payload,TaskRequest::CourseAnalysis(r) if r.course_id==request.course_id && r.preview_id==request.preview_id && r.stage==request.stage)) { return Ok((existing.clone(),false)); }
        }
        if let TaskRequest::ModelThumbnail { entry_id } = &task.payload {
            if let Some(existing)=store.entries.iter().find(|value|active(&value.status)&&matches!(&value.payload,TaskRequest::ModelThumbnail{entry_id:id} if id==entry_id)){return Ok((existing.clone(),false))}
        }
        if let TaskRequest::BrowserCapture { id } = &task.payload {
            if let Some(existing) = store.entries.iter().find(|value| {
                value.library_id == task.library_id
                    && (active(&value.status) || value.status == "completed")
                    && matches!(&value.payload,TaskRequest::BrowserCapture{id:other} if other==id)
            }) {
                return Ok((existing.clone(), false));
            }
        }
        if let Some(warning) = &store.persistence_warning {
            return Err(warning.clone());
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
    #[cfg(test)]
    fn claim(&self) -> Option<LibraryTask> {
        self.claim_with_limit(1)
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
            self.persist_checked(&mut store);
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
            self.persist_checked(&mut store);
        }
    }
    pub fn should_stop(&self, id: &str) -> bool {
        self.store
            .lock()
            .map(|store| {
                store
                    .entries
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.stop_requested)
                    .unwrap_or(true)
            })
            .unwrap_or(true)
    }
    pub fn is_active_task(&self, id: &str) -> bool {
        self.store
            .lock()
            .map(|store| {
                store
                    .entries
                    .iter()
                    .any(|task| task.id == id && active(&task.status) && !task.stop_requested)
            })
            .unwrap_or(false)
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
            task.status = if matches!(task.payload, TaskRequest::Download(_)) {
                "cancelled"
            } else {
                "stopped"
            }
            .into();
            task.message = "尚未开始，已停止".into();
        } else if task.status == "running" {
            if !task.cancellable {
                return Err("此任务处于数据维护阶段，请等待安全完成".into());
            }
            task.stop_requested = true;
            task.pause_requested = false;
            task.phase = "正在安全停止，当前步骤完成后生效".into();
        } else if task.status == "paused" && matches!(task.payload, TaskRequest::Download(_)) {
            task.status = "cancelled".into();
            task.pause_requested = false;
        }
        self.persist(&store)
    }
    fn finish(&self, id: &str, result: Result<Value, String>) {
        if let Ok(mut store) = self.store.lock() {
            store.external.remove(id);
            if let Some(task) = store.entries.iter_mut().find(|t| t.id == id) {
                task.updated_at = Utc::now().to_rfc3339();
                let committed_excerpt = result.is_ok()
                    && matches!(
                        &task.payload,
                        TaskRequest::CourseExcerpt(
                            crate::learning::excerpt_runner::ExcerptTaskRequest::Commit { .. }
                        )
                    );
                let committed_import = result.is_ok()
                    && matches!(
                        &task.payload,
                        TaskRequest::LearningImport(
                            crate::learning::import_runner::ImportTaskRequest::Commit { .. }
                        )
                    );
                if committed_excerpt && task.stop_requested {
                    task.message = "摘录已提交，停止请求晚于保存；目标内容保留".into();
                }
                if committed_import && task.stop_requested {
                    task.message = "迁入已提交，停止请求晚于保存；目标内容保留".into();
                }
                task.status = if committed_excerpt || committed_import {
                    "completed"
                } else if task.stop_requested {
                    if matches!(task.payload, TaskRequest::Download(_)) {
                        if task.pause_requested {
                            "paused"
                        } else {
                            "cancelled"
                        }
                    } else {
                        "stopped"
                    }
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
            self.persist_checked(&mut store);
        }
    }
}

#[cfg(test)]
mod excerpt_lifecycle_tests {
    use super::*;
    #[test]
    fn learning_excerpt_task_late_cancel_preserves_committed_success_but_not_a_cancelled_prepare() {
        use crate::learning::{
            excerpt_publish::{CommitRequest, Destination},
            excerpt_runner::ExcerptTaskRequest,
        };
        let root = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(root.path().join("tasks.json"));
        let payload = TaskRequest::CourseExcerpt(ExcerptTaskRequest::Commit {
            request: CommitRequest {
                operation_id: Uuid::new_v4().to_string(),
                excerpt_id: Uuid::new_v4().to_string(),
                destination: Destination::New {
                    title: "已保存".into(),
                    notebook_id: None,
                },
                include_frame: false,
            },
        });
        let (task, _) = registry.submit("library".into(), payload).unwrap();
        registry.claim().unwrap();
        registry.cancel(&task.id).unwrap();
        // Lifecycle-only success fixture. Real target/receipt persistence has runner tests.
        registry.finish(&task.id,Ok(serde_json::json!({"receipt":{"noteId":Uuid::new_v4().to_string()},"targetAvailable":true})));
        assert_eq!(registry.get(&task.id).unwrap().status, "completed");
        let selection = crate::learning::excerpts::ExcerptSelection {
            ranges: vec![crate::learning::note_nodes::TimeRange {
                start_ms: 0,
                end_ms: 1,
            }],
            ..Default::default()
        };
        let (prepare, _) = registry
            .submit(
                "library".into(),
                TaskRequest::CourseExcerpt(ExcerptTaskRequest::Prepare {
                    course_id: Uuid::new_v4().to_string(),
                    selection,
                }),
            )
            .unwrap();
        registry.claim().unwrap();
        registry.cancel(&prepare.id).unwrap();
        registry.finish(
            &prepare.id,
            Ok(serde_json::json!({"previewId":Uuid::new_v4().to_string()})),
        );
        assert_eq!(registry.get(&prepare.id).unwrap().status, "stopped");
    }
}

#[cfg(test)]
mod import_lifecycle_tests {
    use super::*;
    #[test]
    fn learning_import_generic_ipc_rejects_prepare_but_allows_commit_cancel() {
        use crate::learning::import_runner::ImportTaskRequest;
        let prepare = TaskRequest::LearningImport(ImportTaskRequest::Prepare {
            profile_id: Uuid::new_v4().to_string(),
            epoch: Uuid::new_v4().to_string(),
            request_sha256: "a".repeat(64),
            vault: "vault".into(),
            relative_path: "notes/a.md".into(),
            source_sha256: "b".repeat(64),
            title: "t".into(),
            document: serde_json::json!({"type":"doc","content":[]}),
            markdown: "forged".into(),
        });
        assert!(ensure_generic_ipc_payload(&prepare)
            .unwrap_err()
            .contains("预检入口"));
        for payload in [
            TaskRequest::LearningImport(ImportTaskRequest::Commit {
                batch_id: Uuid::new_v4().to_string(),
            }),
            TaskRequest::LearningImport(ImportTaskRequest::Cancel {
                batch_id: Uuid::new_v4().to_string(),
            }),
        ] {
            ensure_generic_ipc_payload(&payload).unwrap();
        }
    }
    #[test]
    fn learning_import_task_late_cancel_preserves_committed_success() {
        use crate::learning::import_runner::ImportTaskRequest;
        let root = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(root.path().join("tasks.json"));
        let payload = TaskRequest::LearningImport(ImportTaskRequest::Commit {
            batch_id: Uuid::new_v4().to_string(),
        });
        let (task, _) = registry.submit("library".into(), payload).unwrap();
        registry.claim().unwrap();
        registry.cancel(&task.id).unwrap();
        registry.finish(
            &task.id,
            Ok(serde_json::json!({"batchId":Uuid::new_v4().to_string(),"status":"committed"})),
        );
        let finished = registry.get(&task.id).unwrap();
        assert_eq!(finished.status, "completed");
        assert!(finished.message.contains("迁入已提交"));
    }
}
#[cfg(test)]
mod download_lifecycle_tests {
    use super::*;
    #[test]
    fn download_journal_failure_keeps_running_worker_visible_and_resume_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (first, _) = registry.submit("lib".into(), request()).unwrap();
        let (second, _) = registry.submit("lib".into(), request()).unwrap();
        registry.claim_with_limit(2).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(registry.claim_with_limit(2).is_none());
        assert_eq!(registry.get(&first.id).unwrap().status, "running");
        assert!(registry.busy());
        assert!(registry.should_stop(&first.id));
        assert_eq!(registry.get(&second.id).unwrap().status, "interrupted");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (task, _) = registry.submit("lib".into(), request()).unwrap();
        registry.pause_download(&task.id).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(registry.resume_download(&task.id).is_err());
        assert_eq!(registry.get(&task.id).unwrap().status, "paused");
    }
    fn request() -> TaskRequest {
        TaskRequest::Download(crate::downloader::models::DownloadRequest {
            url: "https://example.com/video".into(),
            format: "best".into(),
            quality: "best".into(),
            audio_only: false,
            subtitles: false,
            output_mode: "library".into(),
        })
    }
    #[test]
    fn download_pause_resume_and_cancel_preserve_task_identity() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (task, _) = registry.submit("lib".into(), request()).unwrap();
        registry.pause_download(&task.id).unwrap();
        assert_eq!(registry.get(&task.id).unwrap().status, "paused");
        registry.resume_download(&task.id).unwrap();
        assert_eq!(registry.claim_with_limit(2).unwrap().id, task.id);
        registry.pause_download(&task.id).unwrap();
        assert!(registry.should_stop(&task.id));
        registry.finish(&task.id, Err("已暂停".into()));
        assert_eq!(registry.get(&task.id).unwrap().status, "paused");
        registry.resume_download(&task.id).unwrap();
        registry.claim_with_limit(2).unwrap();
        registry.cancel(&task.id).unwrap();
        registry.finish(&task.id, Err("已取消".into()));
        assert_eq!(registry.get(&task.id).unwrap().status, "cancelled");
        assert!(registry.resume_download(&task.id).is_err());
    }
    #[test]
    fn download_concurrency_does_not_parallelize_other_library_work() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        for _ in 0..3 {
            registry.submit("lib".into(), request()).unwrap();
        }
        let first = registry.claim_with_limit(2).unwrap();
        let second = registry.claim_with_limit(2).unwrap();
        assert_ne!(first.id, second.id);
        assert!(registry.claim_with_limit(2).is_none());
        registry.finish(&first.id, Ok(serde_json::json!({})));
        assert!(registry.claim_with_limit(2).is_some());
        registry.submit("lib".into(), TaskRequest::Links).unwrap();
        assert!(registry.claim_with_limit(4).is_none());
    }
}

#[tauri::command]
pub fn get_task_warning(state: tauri::State<AppState>) -> Option<String> {
    state.tasks.warning()
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
pub async fn exit_when_tasks_idle(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::learning::native_player::exit_application(&app, false)
    })
    .await
    .map_err(|e| e.to_string())?
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
    let (id, jobs) = state.with_active_mut(|active| {
        let connection = active
            .connection
            .as_ref()
            .ok_or("数据库正在维护，请稍后重试")?;
        Ok((
            active.marker.library_id.clone(),
            media_library::pending_model_thumbnails(connection)?,
        ))
    })?;
    let mut count = 0;
    for job in jobs {
        if state.tasks.has_manual_thumbnail_task(&id, &job.id) {
            continue;
        }
        start_library_task_for_library(
            app.clone(),
            state.clone(),
            TaskRequest::ModelThumbnail { entry_id: job.id },
            Some(&id),
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
    if matches!(task.payload, TaskRequest::Download(_))
        && task.library_id == state.active_library_id()?
    {
        state
            .with_library(|db, _| crate::downloader::service::record(db, &state.tasks.get(&id)?))?;
    }
    if matches!(task.payload, TaskRequest::Links) {
        state.cancel_link_check()?;
    }
    Ok(())
}

#[tauri::command]
pub fn pause_download_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    if state.tasks.get(&id)?.library_id != state.active_library_id()? {
        return Err("下载任务属于其他素材库".into());
    }
    state.tasks.pause_download(&id)?;
    state.with_library(|db, _| crate::downloader::service::record(db, &state.tasks.get(&id)?))
}
#[tauri::command]
pub fn resume_download_task(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<LibraryTask, String> {
    if state.tasks.get(&id)?.library_id != state.active_library_id()? {
        return Err("下载任务属于其他素材库".into());
    }
    let launch = state.tasks.resume_download(&id)?;
    let recorded =
        state.with_library(|db, _| crate::downloader::service::record(db, &state.tasks.get(&id)?));
    if launch {
        tauri::async_runtime::spawn(dispatch(app));
    }
    recorded?;
    state.tasks.get(&id)
}
#[tauri::command]
pub fn start_library_task(
    app: AppHandle,
    state: tauri::State<AppState>,
    payload: TaskRequest,
    expected_library_id: Option<String>,
) -> Result<LibraryTask, String> {
    ensure_generic_ipc_payload(&payload)?;
    start_library_task_for_library(app, state, payload, expected_library_id.as_deref())
}
/// Generic IPC must not enqueue learning-import Prepare: its markdown/digest would be
/// caller-asserted. Prepare is only built by `import_access` after a user-bound source preflight.
pub(crate) fn ensure_generic_ipc_payload(payload: &TaskRequest) -> Result<(), String> {
    if matches!(
        payload,
        TaskRequest::LearningImport(
            crate::learning::import_runner::ImportTaskRequest::Prepare { .. }
        )
    ) {
        return Err("学习资料迁入准备只能经来源绑定与预检入口排队".into());
    }
    Ok(())
}
pub fn start_library_task_for_library(
    app: AppHandle,
    state: tauri::State<AppState>,
    payload: TaskRequest,
    expected_library: Option<&str>,
) -> Result<LibraryTask, String> {
    if matches!(
        payload,
        TaskRequest::AssetScan { .. }
            | TaskRequest::AssetImport(_)
            | TaskRequest::AssetProcess { .. }
            | TaskRequest::AIAnalyze(_)
            | TaskRequest::CourseAnalysis(_)
            | TaskRequest::CourseTranscription(_)
            | TaskRequest::LearningImport(_)
    ) && expected_library.is_none()
    {
        return Err("资产任务必须指定目标素材库".into());
    }
    if let TaskRequest::AssetImport(request) = &payload {
        if let Some(id) = &request.scan_task_id {
            let scan = state.tasks.get(id)?;
            if Some(scan.library_id.as_str()) != expected_library
                || !matches!(scan.payload, TaskRequest::AssetScan { .. })
                || !matches!(scan.status.as_str(), "completed" | "stopped")
            {
                return Err("扫描任务未完成或不属于当前素材库".into());
            }
        }
    }
    if matches!(
        payload,
        TaskRequest::Restore { .. } | TaskRequest::Migration(_)
    ) && state.reference_window_board_id()?.is_some()
    {
        return Err("请先收回悬浮参考板，再维护素材库".into());
    }
    let (task, launch) = state.with_active_mut(|active| {
        if expected_library.is_some_and(|id| id != active.marker.library_id) {
            return Err("任务目标素材库已切换，未提交".into());
        }
        state
            .tasks
            .submit(active.marker.library_id.clone(), payload)
    })?;
    let recorded = state.with_library(|db, _| crate::downloader::service::record(db, &task));
    if launch {
        tauri::async_runtime::spawn(dispatch(app));
    }
    recorded?;
    Ok(task)
}
#[tauri::command]
pub fn retry_library_task(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<LibraryTask, String> {
    let previous = state.tasks.get(&id)?;
    previous.payload.ensure_retryable()?;
    if previous.library_id != state.active_library_id()? {
        return Err("请先打开任务所属的素材库".into());
    }
    if active(&previous.status) {
        return Err("任务仍在执行".into());
    }
    if matches!(previous.payload, TaskRequest::Download(_))
        && previous.status != "completed"
        && previous.status != "cancelled"
    {
        if state.tasks.resume_download(&id)? {
            tauri::async_runtime::spawn(dispatch(app));
        }
        state
            .with_library(|db, _| crate::downloader::service::record(db, &state.tasks.get(&id)?))?;
        return state.tasks.get(&id);
    }
    if matches!(previous.payload, TaskRequest::AssetMediaImport { .. }) {
        return Err(
            "请先在素材详情检查已导入附件，再仅导入缺少的文件；为避免重复附件，不重放中断批次"
                .into(),
        );
    }
    let mut payload = previous.payload;
    if let TaskRequest::AssetImport(request) = &mut payload {
        retain_incomplete_core_import(request, previous.result.as_ref());
    }
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
    start_library_task_for_library(app, state, payload, Some(&previous.library_id))
}
fn retain_incomplete_core_import(
    request: &mut crate::asset::models::CoreImportRequest,
    result: Option<&Value>,
) {
    let Some(rows) = result.and_then(|r| r.get("rows")).and_then(Value::as_array) else {
        return;
    };
    let successful = |field: &str, id: &str| {
        rows.iter().any(|row| {
            row[field].as_str() == Some(id)
                && matches!(row["status"].as_str(), Some("imported" | "duplicate"))
        })
    };
    request.paths.retain(|path| !successful("path", path));
    request
        .scan_row_ids
        .retain(|id| !successful("scanRowId", id));
    if request.scan_task_id.is_some() && request.scan_row_ids.is_empty() {
        request.scan_task_id = None;
    }
}
async fn dispatch(app: AppHandle) {
    loop {
        let limit = app
            .state::<AppState>()
            .download_settings()
            .map(|s| s.concurrency)
            .unwrap_or(1);
        let Some(task) = app.state::<AppState>().tasks.claim_with_limit(limit) else {
            if !app.state::<AppState>().tasks.busy() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        };
        let _ = app.emit("library-task-updated", &task);
        let worker = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = execute(worker.clone(), task.clone()).await;
            worker.state::<AppState>().tasks.finish(&task.id, result);
            if let Ok(updated) = worker.state::<AppState>().tasks.get(&task.id) {
                if matches!(updated.payload, TaskRequest::Download(_)) {
                    let _ = worker
                        .state::<AppState>()
                        .with_library(|db, _| crate::downloader::service::record(db, &updated));
                }
                let _ = worker.emit("library-task-updated", updated);
            }
        });
    }
}

pub async fn await_task_result(app: &AppHandle, id: &str) -> Result<Value, String> {
    loop {
        let task = app.state::<AppState>().tasks.get(id)?;
        match task.status.as_str() {
            "completed" => return task.result.ok_or("任务缺少结果".into()),
            "failed" | "stopped" | "interrupted" | "paused" | "cancelled" => {
                return Err(task.message)
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
}
pub(crate) fn open_media_worker_connection(path: &Path) -> Result<rusqlite::Connection, String> {
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
        TaskRequest::CourseExcerpt(request) => {
            crate::learning::excerpt_runner::run(app, task, request).await
        }
        TaskRequest::LearningImport(request) => {
            crate::learning::import_runner::run(app, task, request).await
        }
        TaskRequest::CourseTranscription(request) => {
            crate::learning::transcription_runner::run(app, task, request).await
        }
        TaskRequest::CourseAnalysis(request) => {
            crate::learning::analysis_runner::run(app, task, request).await
        }
        TaskRequest::AIAnalyze(request) => crate::asset::analysis::run(app, task, request).await,
        TaskRequest::AssetProcess { asset_id, force } => {
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                let (base, path) = state.with_active_mut(|active| {
                    if active.marker.library_id != task.library_id {
                        return Err("处理所属素材库已切换".into());
                    }
                    Ok((active.base_dir.clone(), active.db_path.clone()))
                })?;
                let db = open_media_worker_connection(&path)?;
                let resource = app.path().resource_dir().ok();
                state
                    .tasks
                    .update(&task.id, 0, 1, String::new(), "processing".into(), None);
                let report = crate::asset::metadata::process_asset(
                    &db,
                    &base,
                    resource.as_deref(),
                    &asset_id,
                    force,
                    &|| state.tasks.should_stop(&task.id),
                )?;
                state.tasks.update(
                    &task.id,
                    1,
                    1,
                    String::new(),
                    "processing".into(),
                    Some(report.clone()),
                );
                if report["rows"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|r| r["status"] == "error"))
                {
                    Err("部分文件处理失败，原件和已有预览保留，请查看逐文件报告".into())
                } else {
                    Ok(report)
                }
            })
            .await
            .map_err(|e| e.to_string())?
        }
        TaskRequest::AssetScan { root } => tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let (base, db_path) = state.with_active_mut(|active| {
                if active.marker.library_id != task.library_id {
                    return Err("扫描所属素材库已切换".into());
                }
                Ok((active.base_dir.clone(), active.db_path.clone()))
            })?;
            let manifest = crate::asset::scanner::manifest_path(&base, &task.id)?;
            let last = std::cell::Cell::new(std::time::Instant::now());
            let summary = crate::asset::scanner::scan(
                Path::new(&root),
                &base,
                &manifest,
                &|current| {
                    if last.get().elapsed() > std::time::Duration::from_millis(300) {
                        state.tasks.update(
                            &task.id,
                            current,
                            0,
                            String::new(),
                            "scanning".into(),
                            None,
                        );
                        last.set(std::time::Instant::now());
                    }
                },
                &|| state.tasks.should_stop(&task.id),
            )?;
            let db = open_media_worker_connection(&db_path)?;
            let summary = crate::asset::service::annotate_scan(&db, &manifest, summary, &|| {
                state.tasks.should_stop(&task.id)
            })?;
            let value = serde_json::to_value(summary).map_err(|e| e.to_string())?;
            state.tasks.update(
                &task.id,
                value["total"].as_u64().unwrap_or(0) as usize,
                0,
                String::new(),
                "scanning".into(),
                Some(value.clone()),
            );
            Ok(value)
        })
        .await
        .map_err(|e| e.to_string())?,
        TaskRequest::AssetImport(request) => tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let (base, path) = state.with_active_mut(|active| {
                if active.marker.library_id != task.library_id {
                    return Err("导入所属素材库已切换".into());
                }
                Ok((active.base_dir.clone(), active.db_path.clone()))
            })?;
            let mut db = open_media_worker_connection(&path)?;
            let resource = app.path().resource_dir().ok();
            let control_app = app.clone();
            let control_id = task.id.clone();
            let _control = crate::media::ProcessControl::new(move || {
                control_app
                    .state::<AppState>()
                    .tasks
                    .should_stop(&control_id)
            });
            let report = crate::asset::service::import_files(
                &mut db,
                &base,
                resource.as_deref(),
                &request,
                &|current, total| {
                    state.tasks.update(
                        &task.id,
                        current,
                        total,
                        String::new(),
                        "importing".into(),
                        None,
                    );
                },
                &|| state.tasks.should_stop(&task.id),
            )?;
            let value = serde_json::to_value(&report).map_err(|e| e.to_string())?;
            state.tasks.update(
                &task.id,
                report.rows.len(),
                report.rows.len(),
                String::new(),
                "importing".into(),
                Some(value.clone()),
            );
            if report.failed > 0 {
                Err(format!(
                    "已导入 {} 项，跳过 {} 项，失败 {} 项；成功内容已保留",
                    report.imported, report.skipped, report.failed
                ))
            } else {
                Ok(value)
            }
        })
        .await
        .map_err(|e| e.to_string())?,
        TaskRequest::Download(request) => tauri::async_runtime::spawn_blocking(move || {
            crate::downloader::service::execute(&app, &task, request)
        })
        .await
        .map_err(|_| "下载任务异常结束")?,
        TaskRequest::UeAssets(_) => Err(UE_TASK_HISTORY_ONLY.into()),
        TaskRequest::BrowserCapture { id } => tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let (base, path) = state.with_active_mut(|active| {
                if active.marker.library_id != task.library_id {
                    return Err("采集所属库已切换".into());
                }
                Ok((active.base_dir.clone(), active.db_path.clone()))
            })?;
            let mut connection = open_media_worker_connection(&path)?;
            let root = crate::browser_capture::mailbox_root()?;
            let resource = app.path().resource_dir().ok();
            let stop_app = app.clone();
            let stop_id = task.id.clone();
            let _control = crate::media::ProcessControl::new(move || {
                stop_app.state::<AppState>().tasks.should_stop(&stop_id)
            });
            state.tasks.update(
                &task.id,
                0,
                1,
                String::new(),
                "校验 / 复制 / 生成预览".into(),
                None,
            );
            let result = crate::browser_capture::import(
                &mut connection,
                &base,
                resource.as_deref(),
                &root,
                &id,
                &task.library_id,
                &|| state.tasks.should_stop(&task.id),
            );
            let response = match &result {
                Ok(value) => serde_json::json!({"status":"completed","result":value}),
                Err(error) => serde_json::json!({"status":"failed","message":error}),
            };
            crate::browser_capture::write_result(&id, &response)?;
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                "采集处理结束".into(),
                result.as_ref().ok().cloned(),
            );
            result
        })
        .await
        .map_err(|e| e.to_string())?,
        payload @ (TaskRequest::AssetMediaImport { .. } | TaskRequest::AssetMediaRetry { .. }) => {
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                let (base, path) = state.with_active_mut(|active| {
                    Ok((active.base_dir.clone(), active.db_path.clone()))
                })?;
                let mut connection = open_media_worker_connection(&path)?;
                let resource = app.path().resource_dir().ok();
                let stop_app = app.clone();
                let stop_id = task.id.clone();
                let _control = crate::media::ProcessControl::new(move || {
                    stop_app.state::<AppState>().tasks.should_stop(&stop_id)
                });
                match payload {
                    TaskRequest::AssetMediaImport { asset_id, paths } => {
                        let mut imported = Vec::new();
                        let total = paths.len();
                        for (index, path) in paths.into_iter().enumerate() {
                            if state.tasks.should_stop(&task.id) {
                                break;
                            }
                            imported.extend(crate::media::import(
                                &mut connection,
                                &base,
                                resource.as_deref(),
                                &asset_id,
                                vec![path],
                            )?);
                            state.tasks.update(
                                &task.id,
                                index + 1,
                                total,
                                String::new(),
                                "复制 / 生成预览".into(),
                                serde_json::to_value(&imported).ok(),
                            );
                        }
                        serde_json::to_value(imported).map_err(|e| e.to_string())
                    }
                    TaskRequest::AssetMediaRetry { id } => serde_json::to_value(
                        crate::media::retry(&mut connection, &base, resource.as_deref(), &id)?,
                    )
                    .map_err(|e| e.to_string()),
                    _ => unreachable!(),
                }
            })
            .await
            .map_err(|e| e.to_string())?
        }
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
                        let _playback =
                            crate::learning::native_player::quiesce_for_library_change(&state)?;
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
                    TaskRequest::Migration(_) => {
                        serde_json::to_value(state.change_library_for_migration(&task.id)?)
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
    #[test]
    fn learning_transcription_task_is_typed_and_deduplicated() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let tasks = TaskRegistry::load(dir.path().join("tasks.json"));
        let request = TaskRequest::CourseTranscription(
            crate::learning::transcription::CourseTranscriptionRequest {
                course_id: Uuid::new_v4().to_string(),
                model_id: "base".into(),
            },
        );
        let (first, _) = tasks.submit("library".into(), request.clone()).unwrap();
        let (second, launch) = tasks.submit("library".into(), request.clone()).unwrap();
        assert_eq!(first.id, second.id);
        assert!(!launch);
        let mut bad = request;
        if let TaskRequest::CourseTranscription(r) = &mut bad {
            r.model_id = "../model".into();
        }
        assert!(tasks.submit("library".into(), bad).is_err());
    }
    use super::*;
    #[test]
    fn core_import_retry_keeps_only_incomplete_paths_and_scan_rows() {
        let mut request: crate::asset::models::CoreImportRequest =
            serde_json::from_value(serde_json::json!({"paths":["one","two"]})).unwrap();
        let result = serde_json::json!({"rows":[{"path":"one","status":"imported"},{"path":"two","status":"error"}]});
        retain_incomplete_core_import(&mut request, Some(&result));
        assert_eq!(request.paths, vec!["two"]);
        let first = Uuid::new_v4().to_string();
        let second = Uuid::new_v4().to_string();
        request=serde_json::from_value(serde_json::json!({"scanTaskId":Uuid::new_v4().to_string(),"scanRowIds":[first,second]})).unwrap();
        let result = serde_json::json!({"rows":[{"scanRowId":first,"status":"duplicate"}]});
        retain_incomplete_core_import(&mut request, Some(&result));
        assert_eq!(request.scan_row_ids, vec![second]);
    }
    #[test]
    fn core_scan_uses_existing_queue_cancellation_and_blocks_library_changes() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (scan, _) = registry
            .submit(
                "library".into(),
                TaskRequest::AssetScan {
                    root: "C:/source".into(),
                },
            )
            .unwrap();
        assert!(registry
            .ensure_library_change_allowed("other", None)
            .is_err());
        registry.cancel(&scan.id).unwrap();
        assert_eq!(registry.get(&scan.id).unwrap().status, "stopped");
        assert!(registry
            .ensure_library_change_allowed("other", None)
            .is_ok());
        let restored = TaskRegistry::load(dir.path().join("tasks.json"));
        assert!(matches!(
            restored.get(&scan.id).unwrap().payload,
            TaskRequest::AssetScan { .. }
        ));
        let malformed = TaskRequest::AssetImport(
            serde_json::from_value(
                serde_json::json!({"paths":[],"scanTaskId":"../secret","scanRowIds":["bad"]}),
            )
            .unwrap(),
        );
        assert!(registry.submit("library".into(), malformed).is_err());
    }
    #[test]
    fn library_change_only_exempts_the_exact_running_migration() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        assert!(registry.ensure_library_change_allowed("lib", None).is_ok());
        let (capture, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::BrowserCapture {
                    id: "capture".into(),
                },
            )
            .unwrap();
        assert!(registry.ensure_library_change_allowed("lib", None).is_err());
        registry.claim().unwrap();
        assert!(registry
            .ensure_library_change_allowed("lib", Some(&capture.id))
            .is_err());
        registry.finish(&capture.id, Ok(serde_json::json!({})));
        let (migration, _) = registry
            .submit(
                "lib".into(),
                TaskRequest::Migration(StorageChangeRequest {
                    mode: "migrate".into(),
                    path: "target".into(),
                    name: None,
                }),
            )
            .unwrap();
        assert!(registry
            .ensure_library_change_allowed("lib", Some(&migration.id))
            .is_err());
        registry.claim().unwrap();
        assert!(registry.ensure_library_change_allowed("lib", None).is_err());
        assert!(registry
            .ensure_library_change_allowed("other", Some(&migration.id))
            .is_err());
        assert!(registry
            .ensure_library_change_allowed("lib", Some("wrong-task"))
            .is_err());
        assert!(registry
            .ensure_library_change_allowed("lib", Some(&migration.id))
            .is_ok());
        registry.finish(&migration.id, Ok(serde_json::json!({})));
        assert!(registry
            .ensure_library_change_allowed("lib", Some(&migration.id))
            .is_err());
        assert!(registry.ensure_library_change_allowed("lib", None).is_ok());
    }

    #[test]
    fn capture_submission_deduplicates_pending_running_and_completed_within_its_library() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let payload = TaskRequest::BrowserCapture {
            id: "capture".into(),
        };
        let (first, _) = registry.submit("lib".into(), payload.clone()).unwrap();
        let (queued, launch) = registry.submit("lib".into(), payload.clone()).unwrap();
        assert_eq!(queued.id, first.id);
        assert!(!launch);
        registry.claim().unwrap();
        let (running, launch) = registry.submit("lib".into(), payload.clone()).unwrap();
        assert_eq!(running.id, first.id);
        assert!(!launch);
        registry.finish(&first.id, Ok(serde_json::json!({})));
        let (completed, launch) = registry.submit("lib".into(), payload.clone()).unwrap();
        assert_eq!(completed.id, first.id);
        assert!(!launch);
        let (other, _) = registry.submit("other".into(), payload).unwrap();
        assert_ne!(other.id, first.id);
        assert_eq!(registry.list("lib").unwrap().len(), 1);
        assert_eq!(registry.list("other").unwrap().len(), 1);
    }
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
    fn course_task_queue_deduplicates_approval_and_refuses_generic_retry() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let request =
            TaskRequest::CourseAnalysis(crate::learning::analysis_models::CourseAnalysisRequest {
                course_id: Uuid::new_v4().to_string(),
                preview_id: Uuid::new_v4().to_string(),
                stage: crate::learning::analysis_models::CourseAnalysisPhase::Segments,
            });
        let (first, _) = registry.submit("library".into(), request.clone()).unwrap();
        let (second, launch) = registry.submit("library".into(), request.clone()).unwrap();
        assert_eq!(
            first.id, second.id,
            "double click must not queue the same approval twice"
        );
        assert!(!launch);
        assert!(request.ensure_retryable().unwrap_err().contains("重新预览"));
        assert!(TaskRequest::Links.ensure_retryable().is_ok());
        let mut invalid = request.clone();
        if let TaskRequest::CourseAnalysis(r) = &mut invalid {
            r.course_id = "bad".into();
        }
        assert!(registry.submit("library".into(), invalid).is_err());
    }
    #[test]
    fn task_history_keeps_old_action_items_but_bounds_completed_history() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TaskRegistry::load(dir.path().join("tasks.json"));
        let (failed, _) = registry.submit("lib".into(), TaskRequest::Links).unwrap();
        registry.finish(&failed.id, Err("需要处理".into()));
        for _ in 0..205 {
            let (task, _) = registry.submit("lib".into(), TaskRequest::Links).unwrap();
            registry.finish(&task.id, Ok(serde_json::json!({})));
        }
        let visible = registry.list("lib").unwrap();
        assert!(
            visible.iter().any(|t| t.id == failed.id),
            "old failed task cannot disappear behind completed history"
        );
        assert_eq!(
            visible.iter().filter(|t| t.status == "completed").count(),
            200
        );
        assert!(
            visible.iter().all(|t| t.result.is_none()),
            "summary does not include full results"
        );
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
    fn v21_ue_history_preserves_payload_and_allows_translation_without_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (translation, _) = registry
            .submit(
                "library".into(),
                TaskRequest::Translation {
                    ids: vec!["a".into()],
                },
            )
            .unwrap();
        let mut translation = serde_json::to_value(translation).unwrap();
        translation["status"] = serde_json::json!("running");
        translation["result"] = serde_json::json!({"rows":[{"assetId":"a","status":"complete"}]});
        let mut ue = translation.clone();
        ue["id"] = serde_json::json!("ue-history");
        ue["label"] = serde_json::json!("UE 原生资产");
        ue["payload"] = serde_json::json!({"kind":"ueAssets","request":{
            "operation":"index","sourceId":"source","engineRoot":"fixture-engine",
            "assetIds":["asset"],"planId":null,"confirmed":false,
            "futureField":{"nested":[1,true,null]}
        }});
        let expected_payload = ue["payload"].clone();
        fs::write(
            &path,
            serde_json::to_vec(&vec![translation.clone(), ue]).unwrap(),
        )
        .unwrap();

        let restored = TaskRegistry::load(path.clone());
        let (new_task, launch) = restored
            .submit(
                "library".into(),
                TaskRequest::Translation {
                    ids: vec!["b".into()],
                },
            )
            .unwrap_or_else(|error| panic!("混合历史应允许补译提交：{error}"));
        assert!(launch);
        assert!(restored.warning().is_none());
        assert_eq!(restored.list("library").unwrap().len(), 3);
        let old_translation = restored.get(translation["id"].as_str().unwrap()).unwrap();
        assert_eq!(old_translation.status, "interrupted");
        assert_eq!(old_translation.result.unwrap(), translation["result"]);
        let old_ue = restored.get("ue-history").unwrap();
        assert_eq!(old_ue.status, "interrupted");
        assert!(old_ue.payload.validate().is_err());
        assert!(restored.submit("library".into(), old_ue.payload).is_err());
        assert_eq!(restored.claim().unwrap().id, new_task.id);
        assert!(restored.claim().is_none());
        let saved: Vec<Value> = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            saved
                .iter()
                .find(|task| task["id"] == "ue-history")
                .unwrap()["payload"],
            expected_payload
        );
    }
    #[test]
    fn corrupt_journal_is_preserved_and_blocks_new_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        fs::write(&path, b"broken journal").unwrap();
        let registry = TaskRegistry::load(path.clone());
        assert!(registry.warning().is_some());
        assert!(registry
            .submit("library".into(), TaskRequest::Links)
            .is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken journal");
    }
    #[test]
    fn unknown_kinds_and_invalid_known_payloads_still_preserve_and_block_journals() {
        for payload in [
            serde_json::json!({"kind":"futureTask","request":{"value":1}}),
            serde_json::json!({"kind":"translation","request":{"ids":42}}),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("tasks.json");
            let registry = TaskRegistry::load(path.clone());
            let (task, _) = registry
                .submit("library".into(), TaskRequest::Links)
                .unwrap();
            let mut task = serde_json::to_value(task).unwrap();
            task["payload"] = payload;
            let bytes = serde_json::to_vec(&vec![task]).unwrap();
            fs::write(&path, &bytes).unwrap();
            let restored = TaskRegistry::load(path.clone());
            assert!(restored.warning().is_some());
            assert!(restored
                .submit("library".into(), TaskRequest::Links)
                .is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
    #[test]
    fn failed_checkpoint_stops_cancellable_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let registry = TaskRegistry::load(path.clone());
        let (task, _) = registry
            .submit("library".into(), TaskRequest::Links)
            .unwrap();
        registry.claim().unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        registry.update(&task.id, 1, 2, "".into(), "检查".into(), None);
        assert!(registry.warning().is_some());
        assert!(registry.should_stop(&task.id));
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

/// Enqueue a verified learning import only while its original library session is active.
pub(crate) fn submit_scoped_learning_import(
    state: &crate::state::AppState,
    payload: TaskRequest,
    scope: &crate::learning::source_profiles::LibraryScope,
) -> Result<(LibraryTask, bool), String> {
    if !matches!(payload, TaskRequest::LearningImport(_)) {
        return Err("迁入任务类型无效".into());
    }
    state.with_active_mut(|active| {
        let current = crate::learning::analysis_store::library_identity(&active.base_dir)?;
        if current
            != (
                scope.library_id().to_string(),
                scope.root_hash().to_string(),
            )
            || *state
                .source_scope_nonce
                .lock()
                .map_err(|_| "来源会话状态不可用")?
                != scope.session_nonce()
        {
            return Err("素材库会话已变化，未提交迁入任务".into());
        }
        state
            .tasks
            .submit(active.marker.library_id.clone(), payload)
    })
}

pub(crate) fn start_scoped_learning_import(
    app: AppHandle,
    state: tauri::State<AppState>,
    payload: TaskRequest,
    scope: &crate::learning::source_profiles::LibraryScope,
) -> Result<LibraryTask, String> {
    let (task, launch) = submit_scoped_learning_import(&state, payload, scope)?;
    if launch {
        tauri::async_runtime::spawn(dispatch(app));
    }
    Ok(task)
}
