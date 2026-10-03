//! Browser ingress is a bounded native-messaging mailbox, never an HTTP server.
use crate::{
    models::{MediaImportRequest, MediaKind},
    state::AppState,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{Manager, State};
use uuid::Uuid;

const HOST: &str = "com.stackdcc.capture";
const FRAME_LIMIT: usize = 512 * 1024;
const IMAGE_LIMIT: u64 = 25 * 1024 * 1024;
const VIDEO_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub page_url: String,
    pub media_url: String,
    pub kind: String,
    pub extension: Option<String>,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub project_id: Option<String>,
    pub created_at: String,
}
impl Capture {
    fn validate(&self) -> Result<(), String> {
        for id in [&self.id, &self.library_id] {
            Uuid::parse_str(id).map_err(|_| "采集标识无效")?;
        }
        if !["image", "video", "link"].contains(&self.kind.as_str())
            || self.title.trim().is_empty()
            || self.title.len() > 1024
            || self.tags.len() > 50
            || self.tags.iter().any(|t| t.len() > 100)
            || chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err()
        {
            return Err("采集内容超出限制".into());
        }
        if !self.media_url.is_empty() {
            return Err("资源下载地址只能留在浏览器中".into());
        }
        let source = crate::note_remote_images::validate_url(&self.page_url)?;
        if self.page_url.contains('#') || source.query_pairs().any(|(key, _)| sensitive_query(&key))
        {
            return Err("来源网址含登录或签名参数，请先移除".into());
        }
        if self.page_url.is_empty() {
            return Err("采集缺少来源网页".into());
        }
        let valid = match self.kind.as_str() {
            "image" => matches!(
                self.extension.as_deref(),
                Some("png" | "jpg" | "jpeg" | "webp" | "gif")
            ),
            "video" => matches!(self.extension.as_deref(), Some("mp4" | "webm")),
            _ => matches!(
                self.extension.as_deref(),
                None | Some("png" | "jpg" | "jpeg" | "webp" | "gif")
            ),
        };
        if !valid {
            return Err("采集文件格式无效".into());
        }
        for id in [&self.folder_id, &self.project_id].into_iter().flatten() {
            Uuid::parse_str(id).map_err(|_| "目标标识无效")?;
        }
        Ok(())
    }
}
fn sensitive_query(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let normalized = key.replace(['-', '_'], "");
    key.starts_with("x-amz-")
        || key.starts_with("x-goog-")
        || [
            "token",
            "accesstoken",
            "refreshtoken",
            "idtoken",
            "authtoken",
            "authorization",
            "auth",
            "password",
            "passwd",
            "pwd",
            "cookie",
            "session",
            "sessionid",
            "sid",
            "jwt",
            "apikey",
            "key",
            "signature",
            "sig",
            "credential",
            "credentials",
            "secret",
            "clientsecret",
            "code",
        ]
        .contains(&normalized.as_str())
}
fn bound_choices(choices: &mut Vec<Value>) -> bool {
    let mut bytes = 0;
    let before = choices.len();
    choices.retain(|value| {
        bytes += serde_json::to_vec(value)
            .map(|v| v.len() + 1)
            .unwrap_or(FRAME_LIMIT);
        bytes <= FRAME_LIMIT / 3
    });
    choices.len() != before
}
fn mailbox() -> Result<PathBuf, String> {
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法定位用户数据目录")?)
        .join("DCCAssetLibrary/browser-capture");
    for dir in ["pending", "results"] {
        fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
    }
    Ok(root)
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("暂存目录无效")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, value).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn migrate(db: &Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS browser_captures(id TEXT PRIMARY KEY,title TEXT NOT NULL,page_url TEXT NOT NULL,media_url TEXT NOT NULL,kind TEXT NOT NULL,entry_id TEXT,project_id TEXT,status TEXT NOT NULL,message TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL,organized INTEGER NOT NULL DEFAULT 0); CREATE INDEX IF NOT EXISTS idx_browser_captures_created ON browser_captures(created_at DESC); UPDATE schema_meta SET value='18' WHERE key='schema_version' AND CAST(value AS INTEGER)<18;").map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn record(
    db: &Connection,
    c: &Capture,
    entry: Option<&str>,
    status: &str,
    message: &str,
) -> Result<(), String> {
    db.execute("INSERT INTO browser_captures(id,title,page_url,media_url,kind,entry_id,project_id,status,message,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET entry_id=excluded.entry_id,status=excluded.status,message=excluded.message",params![c.id,c.title,c.page_url,c.media_url,c.kind,entry,c.project_id,status,message,c.created_at]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn apply_source(db: &Connection, c: &Capture, id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE media_entries SET name=?2,source_url=?3 WHERE id=?1",
        params![id, c.title, c.page_url],
    )
    .map_err(|e| e.to_string())?;
    if let Some(project) = &c.project_id {
        crate::media_library::link_projects(db, vec![id.into()], vec![project.clone()], true)?;
    }
    record(
        db,
        c,
        Some(id),
        if c.kind == "link" { "link" } else { "saved" },
        if c.kind == "link" {
            "仅保存来源链接与封面，未下载视频文件"
        } else {
            "文件已保存"
        },
    )
}
fn pending_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    Uuid::parse_str(id).map_err(|_| "采集标识无效")?;
    Ok(root.join("pending").join(id))
}
pub fn import(
    db: &mut Connection,
    base: &Path,
    resource: Option<&Path>,
    root: &Path,
    id: &str,
    library: &str,
    stop: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let path = pending_path(root, id)?;
    let c: Capture =
        serde_json::from_slice(&fs::read(path.join("capture.json")).map_err(|_| "采集暂存不存在")?)
            .map_err(|_| "采集暂存损坏")?;
    c.validate()?;
    if c.id != id || c.library_id != library {
        return Err("采集所属素材库已改变，未执行".into());
    }
    let existing: Option<String> = db
        .query_row(
            "SELECT status FROM browser_captures WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if existing
        .as_deref()
        .is_some_and(|s| matches!(s, "saved" | "duplicate" | "link"))
    {
        return Ok(json!({"id":id,"status":existing}));
    }
    if stop() {
        return Err("采集已取消，暂存仍保留".into());
    }
    if let Some(project) = &c.project_id {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status<>'archived')",
                [project],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("目标项目不存在或已归档".into());
        }
    }
    if c.kind == "link" && c.extension.is_none() {
        record(db, &c, None, "link", "仅保存来源链接，未下载视频文件")?;
        return Ok(json!({"id":id,"status":"link"}));
    }
    let source = path.join(format!(
        "file.{}",
        c.extension.as_deref().ok_or("缺少文件格式")?
    ));
    if !source.is_file()
        || fs::symlink_metadata(&source)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("采集文件不存在或路径不安全".into());
    }
    let report = crate::media_library::import_controlled_with_capture(
        db,
        base,
        resource,
        MediaImportRequest {
            kind: if c.kind == "video" {
                MediaKind::Video
            } else {
                MediaKind::Image
            },
            paths: vec![source.to_string_lossy().into()],
            folder_id: c.folder_id.clone(),
            tags: c.tags.clone(),
            favorite: false,
        },
        &|_, _, _, _| {},
        stop,
        Some(&c),
    )?;
    let row = report.rows.first().ok_or("采集已停止，暂存仍保留")?;
    if let Some(existing) = &row.duplicate_entry_id {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        if let Some(project) = &c.project_id {
            crate::media_library::link_projects(
                &tx,
                vec![existing.clone()],
                vec![project.clone()],
                true,
            )?;
        }
        record(
            &tx,
            &c,
            Some(existing),
            "duplicate",
            "内容相同，未覆盖或移动已有条目",
        )?;
        tx.commit().map_err(|e| e.to_string())?;
    } else if row.status == "error" {
        record(db, &c, None, "failed", &row.message)?;
        return Err(row.message.clone());
    }
    let status: String = db
        .query_row(
            "SELECT status FROM browser_captures WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({"id":id,"status":status,"report":report}))
}

#[tauri::command]
pub fn list_browser_captures(
    state: State<AppState>,
    library_id: String,
    offset: i64,
) -> Result<Value, String> {
    state.with_library_if_id(&library_id,|db,_|{
        let mut statement=db.prepare("SELECT c.id,c.title,c.page_url,c.kind,c.entry_id,c.status,c.message,c.created_at,c.organized,(SELECT f.id FROM media_files f WHERE f.entry_id=e.id AND f.role='thumbnail' LIMIT 1),(c.entry_id IS NOT NULL AND (e.id IS NULL OR e.deleted_at IS NOT NULL)) FROM browser_captures c LEFT JOIN media_entries e ON e.id=c.entry_id ORDER BY c.created_at DESC,c.id LIMIT 100 OFFSET ?1").map_err(|e|e.to_string())?;
        let rows=statement.query_map([offset.max(0)],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"pageUrl":r.get::<_,String>(2)?,"kind":r.get::<_,String>(3)?,"entryId":r.get::<_,Option<String>>(4)?,"status":r.get::<_,String>(5)?,"message":r.get::<_,String>(6)?,"createdAt":r.get::<_,String>(7)?,"organized":r.get::<_,bool>(8)?,"thumbnailId":r.get::<_,Option<String>>(9)?,"unavailable":r.get::<_,bool>(10)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let total:i64=db.query_row("SELECT count(*) FROM browser_captures",[],|r|r.get(0)).map_err(|e|e.to_string())?;
        Ok(json!({"items":rows,"total":total,"offset":offset.max(0),"limit":100}))
    })?.ok_or("素材库已切换，请重新读取收件箱".into())
}
#[tauri::command]
pub fn organize_browser_capture(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<(), String> {
    state
        .with_library_if_id(&library_id, |db, _| {
            db.execute("UPDATE browser_captures SET organized=1 WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
            Ok(())
        })?
        .ok_or("素材库已切换，未修改".into())
}

#[tauri::command]
pub fn create_browser_capture_note(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<String, String> {
    state.with_active_mut(|active|{
        if active.marker.library_id!=library_id {return Err("素材库已切换，未创建笔记".into());}
        let db=active.connection.as_mut().ok_or("素材库正在维护")?;
        let (title,url,entry):(String,String,Option<String>)=db.query_row("SELECT title,page_url,entry_id FROM browser_captures WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_| "采集记录不存在")?;
        let mut content=vec![json!({"type":"paragraph","content":[{"type":"text","text":"来源："},{"type":"text","text":url,"marks":[{"type":"link","attrs":{"href":url}}]}]})];
        if let Some(entry)=entry {content.push(json!({"type":"reference","attrs":{"kind":"media","targetId":entry,"label":title}}));}
        content.push(json!({"type":"paragraph"}));
        let note=crate::knowledge::save(db,crate::knowledge::NoteInput{id:None,expected_revision:None,notebook_id:None,title,document:json!({"type":"doc","content":content}),tags:vec![],favorite:false,journal_date:None})?;
        Ok(note.id)
    })
}

#[tauri::command]
pub fn pump_browser_captures(
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<Value, String> {
    let connection = get_browser_capture_connection(state.clone())?;
    let root = mailbox()?;
    let library = connection["libraryId"]
        .as_str()
        .ok_or("采集素材库标识无效")?
        .to_owned();
    let mut submitted = 0;
    if state.tasks.maintenance().is_none() && connection["enabled"] == true {
        let tasks = state.tasks.list(&library)?;
        for id in connection["pending"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .take(100)
        {
            if root.join("results").join(format!("{id}.json")).exists() || tasks.iter().any(|t|matches!(&t.payload,crate::tasks::TaskRequest::BrowserCapture{id:task_id} if task_id==id)) {continue;}
            crate::tasks::start_library_task_for_library(
                app.clone(),
                state.clone(),
                crate::tasks::TaskRequest::BrowserCapture { id: id.into() },
                Some(&library),
            )?;
            submitted += 1;
        }
    }
    let request = root.join("open.json");
    let open = fs::read(&request)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["time"].as_i64())
        .is_some_and(|t| (Utc::now().timestamp() - t).abs() < 30);
    if open {
        fs::remove_file(request).map_err(|e| e.to_string())?;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(
        json!({"libraryId":connection["libraryId"],"name":connection["name"],"submitted":submitted,"otherLibraries":connection["otherLibraries"],"open":open}),
    )
}

/// Main window polling publishes only identifiers/names, never paths or credentials.
#[tauri::command]
pub fn get_browser_capture_connection(state: State<AppState>) -> Result<Value, String> {
    let root = mailbox()?;
    let heartbeat=state.with_active_mut(|active|{
        let db=active.connection.as_ref().ok_or("素材库正在维护")?;
        // No library counts/task summaries in the heartbeat: keep UI/database lock time bounded.
        let mut folders_query=db.prepare("SELECT id,name,kind,parent_id FROM media_folders WHERE deleted_at IS NULL AND kind IN ('image','video') ORDER BY kind,sort_order,id LIMIT 1000").map_err(|e|e.to_string())?;
        let mut folders=folders_query.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?.chars().take(160).collect::<String>(),"kind":r.get::<_,String>(2)?,"parentId":r.get::<_,Option<String>>(3)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let mut projects_query=db.prepare("SELECT id,name FROM projects WHERE status<>'archived' ORDER BY updated_at DESC,id LIMIT 500").map_err(|e|e.to_string())?;
        let mut projects=projects_query.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?.chars().take(160).collect::<String>()}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let folder_cap=bound_choices(&mut folders);let project_cap=bound_choices(&mut projects);
        let choices_truncated=folder_cap||project_cap||folders.len()==1000||projects.len()==500;
        let preferences=crate::organization::load_library_preferences(db)?;
        let enabled=state.tasks.maintenance().is_none() && !active.base_dir.join(crate::backup::RESTORE_MARKER).exists() && !preferences.disabled_modules.iter().any(|id|id=="browserCapture");
        Ok(json!({"libraryId":active.marker.library_id,"name":active.marker.name.chars().take(160).collect::<String>(),"folders":folders,"projects":projects,"choicesTruncated":choices_truncated,"time":Utc::now().timestamp(),"pid":std::process::id(),"enabled":enabled}))
    })?;
    atomic_json(&root.join("connection.json"), &heartbeat)?;
    let mut pending = Vec::new();
    let mut other = 0;
    for file in fs::read_dir(root.join("pending"))
        .map_err(|e| e.to_string())?
        .take(501)
    {
        let file = file.map_err(|e| e.to_string())?;
        if !file.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let name = file.file_name().to_string_lossy().into_owned();
        if Uuid::parse_str(&name).is_err() {
            continue;
        }
        if let Ok(bytes) = fs::read(file.path().join("capture.json")) {
            if let Ok(c) = serde_json::from_slice::<Capture>(&bytes) {
                if c.id == name && c.validate().is_ok() {
                    if c.library_id == heartbeat["libraryId"] {
                        pending.push(c.id);
                    } else {
                        other += 1;
                    }
                }
            }
        }
    }
    Ok(
        json!({"libraryId":heartbeat["libraryId"],"name":heartbeat["name"],"pending":pending,"otherLibraries":other,"enabled":heartbeat["enabled"]}),
    )
}
fn write_host_configuration(
    root: &Path,
    exe: &Path,
    extension_id: &str,
) -> Result<PathBuf, String> {
    if extension_id.len() != 32 || !extension_id.chars().all(|c| ('a'..='p').contains(&c)) {
        return Err("请输入扩展页面中的 32 位扩展 ID".into());
    }
    let text = exe.to_str().ok_or("程序路径无法编码")?;
    if text.contains(['\r', '\n', '%', '"']) {
        return Err("安装路径不适合浏览器通信，请重新安装到普通目录".into());
    }
    let launcher = root.join("host.cmd");
    let manifest = root.join("host.json");
    let mut origins = fs::read(&manifest)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["allowed_origins"].as_array().cloned())
        .unwrap_or_default();
    let origin = json!(format!("chrome-extension://{extension_id}/"));
    if !origins.contains(&origin) {
        origins.push(origin);
    }
    if origins.len() > 8 {
        return Err("已连接扩展过多，请清理旧连接配置后重试".into());
    }
    fs::write(&launcher,format!("@echo off\r\nchcp 65001 >nul\r\nsetlocal DisableDelayedExpansion\r\n\"{text}\" --browser-capture-host %*\r\n")).map_err(|e|e.to_string())?;
    atomic_json(
        &manifest,
        &json!({"name":HOST,"description":"栈藏浏览器采集","path":launcher,"type":"stdio","allowed_origins":origins}),
    )?;
    Ok(manifest)
}

#[tauri::command]
pub fn register_browser_capture_host(extension_id: String) -> Result<String, String> {
    let root = mailbox()?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let manifest = write_host_configuration(&root, &exe, &extension_id)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for browser in ["Google\\Chrome", "Microsoft\\Edge"] {
            let result = std::process::Command::new("reg.exe")
                .creation_flags(0x08000000)
                .args([
                    "ADD",
                    &format!("HKCU\\Software\\{browser}\\NativeMessagingHosts\\{HOST}"),
                    "/ve",
                    "/t",
                    "REG_SZ",
                    "/d",
                    manifest.to_str().ok_or("注册路径无效")?,
                    "/f",
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !result.status.success() {
                return Err("浏览器连接注册失败，请检查当前用户权限".into());
            }
        }
    }
    Ok("已为当前用户连接 Edge 和 Chrome；程序升级或更换扩展 ID 后请重新连接".into())
}

pub fn refresh_registered_host() -> Result<(), String> {
    let manifest = mailbox()?.join("host.json");
    if !manifest.exists() {
        return Ok(());
    }
    let value: Value = serde_json::from_slice(&fs::read(manifest).map_err(|e| e.to_string())?)
        .map_err(|_| "浏览器连接配置损坏，请在采集收件箱重新连接")?;
    if let Some(id) = value["allowed_origins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|origin| {
            origin
                .strip_prefix("chrome-extension://")?
                .strip_suffix('/')
        })
        .find(|id| id.len() == 32 && id.chars().all(|c| ('a'..='p').contains(&c)))
    {
        // Keep explicitly authorized IDs, but point them at the newly installed binary.
        register_browser_capture_host(id.to_owned())?;
    }
    Ok(())
}
fn read_frame(input: &mut impl Read) -> Result<Option<Value>, String> {
    let mut size = [0; 4];
    match input.read(&mut size[..1]).map_err(|e| e.to_string())? {
        0 => return Ok(None),
        _ => input
            .read_exact(&mut size[1..])
            .map_err(|_| "消息头不完整")?,
    }
    let len = u32::from_le_bytes(size) as usize;
    if len == 0 || len > FRAME_LIMIT {
        return Err("浏览器消息过大".into());
    }
    let mut bytes = vec![0; len];
    input.read_exact(&mut bytes).map_err(|_| "消息不完整")?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "浏览器消息无效".into())
}
fn send_frame(output: &mut impl Write, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > FRAME_LIMIT {
        return Err("响应过大".into());
    }
    output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|_| output.write_all(&bytes))
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
fn connection(root: &Path) -> Option<Value> {
    // OS file lock releases even after a crash; stale heartbeat never means a live app.
    let instance = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("application.lock"))
        .ok()?;
    if instance.try_lock().is_ok() {
        return None;
    }
    let bytes = fs::read(root.join("connection.json")).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    ((Utc::now().timestamp() - v["time"].as_i64()?).abs() < 20).then_some(v)
}
pub fn application_lock() -> Result<Option<fs::File>, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(mailbox()?.join("application.lock"))
        .map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn request_open() -> Result<(), String> {
    atomic_json(
        &mailbox()?.join("open.json"),
        &json!({"time":Utc::now().timestamp()}),
    )
}
fn launch_app() -> Result<(), String> {
    let mut command =
        std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.arg("--browser-capture-background");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "无法启动栈藏，请手动打开后重试")?;
    Ok(())
}
pub fn native_host() -> Result<(), String> {
    let root = mailbox()?;
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("host.json")).map_err(|_| "未注册浏览器连接")?)
            .map_err(|_| "连接配置损坏")?;
    let origin = std::env::args().nth(2).ok_or("缺少浏览器来源")?;
    if !manifest["allowed_origins"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&origin)))
    {
        return Err("浏览器来源未授权".into());
    }
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut upload: Option<(Capture, tempfile::TempDir, fs::File, u64)> = None;
    while let Some(message) = read_frame(&mut input)? {
        let result = (|| -> Result<Value, String> {
            match message["action"].as_str().unwrap_or("") {
                "hello" => {
                    let mut started = false;
                    if connection(&root).is_none() {
                        launch_app()?;
                        started = true;
                        for _ in 0..60 {
                            if connection(&root).is_some() {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(250));
                        }
                    }
                    let info = connection(&root).ok_or("栈藏连接超时，请手动打开后重试")?;
                    Ok(json!({"connection":info,"started":started}))
                }
                "open" => {
                    atomic_json(
                        &root.join("open.json"),
                        &json!({"time":Utc::now().timestamp()}),
                    )?;
                    Ok(json!({}))
                }
                "begin" => {
                    if upload.is_some() {
                        return Err("已有采集传输尚未结束".into());
                    }
                    if fs::read_dir(root.join("pending"))
                        .map_err(|e| e.to_string())?
                        .count()
                        >= 500
                    {
                        return Err("采集队列已满，请先在栈藏处理".into());
                    }
                    let c: Capture = serde_json::from_value(message["capture"].clone())
                        .map_err(|_| "采集参数无效")?;
                    c.validate()?;
                    let info = connection(&root).ok_or("软件连接已失效，请重试")?;
                    if info["libraryId"] != c.library_id || info["enabled"] != true {
                        return Err("素材库已切换或正在维护，请重新确认目标库".into());
                    }
                    if pending_path(&root, &c.id)?.exists() {
                        return Err("采集标识已存在".into());
                    }
                    let dir =
                        tempfile::tempdir_in(root.join("pending")).map_err(|e| e.to_string())?;
                    let file = fs::File::create(
                        dir.path()
                            .join(format!("file.{}", c.extension.as_deref().unwrap_or("none"))),
                    )
                    .map_err(|e| e.to_string())?;
                    upload = Some((c, dir, file, 0));
                    Ok(json!({}))
                }
                "chunk" => {
                    let (c, _, file, total) = upload.as_mut().ok_or("采集尚未开始")?;
                    let bytes = STANDARD
                        .decode(message["data"].as_str().ok_or("文件块无效")?)
                        .map_err(|_| "文件块编码无效")?;
                    let limit = if c.kind == "video" {
                        VIDEO_LIMIT
                    } else {
                        IMAGE_LIMIT
                    };
                    if (c.kind == "link" && c.extension.is_none())
                        || bytes.len() > 128 * 1024
                        || *total + bytes.len() as u64 > limit
                    {
                        return Err("采集文件超出限制".into());
                    }
                    file.write_all(&bytes)
                        .map_err(|_| "暂存失败，请检查磁盘空间")?;
                    *total += bytes.len() as u64;
                    Ok(json!({"bytes":total}))
                }
                "finish" => {
                    let (c, dir, file, total) = upload.take().ok_or("采集尚未开始")?;
                    if c.extension.is_some()
                        && (total == 0 || message["bytes"].as_u64() != Some(total))
                    {
                        return Err("文件传输不完整，未添加".into());
                    }
                    file.sync_all().map_err(|_| "无法保存采集文件")?;
                    drop(file);
                    if c.kind != "video" && c.extension.is_some() {
                        let path = dir
                            .path()
                            .join(format!("file.{}", c.extension.as_deref().unwrap()));
                        let dims = image::ImageReader::open(path)
                            .map_err(|_| "图片无效")?
                            .with_guessed_format()
                            .map_err(|_| "图片无效")?
                            .into_dimensions()
                            .map_err(|_| "图片无法解析")?;
                        if u64::from(dims.0) * u64::from(dims.1) > 25_000_000 {
                            return Err("图片超过 2500 万像素，请改用区域截图".into());
                        }
                    }
                    atomic_json(&dir.path().join("capture.json"), &c)?;
                    fs::rename(dir.path(), pending_path(&root, &c.id)?)
                        .map_err(|e| e.to_string())?;
                    let _ = dir.keep();
                    Ok(json!({"id":c.id,"status":"queued","message":"已暂存，等待栈藏处理"}))
                }
                "cancel" => {
                    upload = None;
                    Ok(json!({}))
                }
                "result" => {
                    let id = message["id"].as_str().ok_or("缺少采集 ID")?;
                    Uuid::parse_str(id).map_err(|_| "采集标识无效")?;
                    let path = root.join("results").join(format!("{id}.json"));
                    match fs::read(path) {
                        Ok(bytes) => {
                            let value: Value =
                                serde_json::from_slice(&bytes).map_err(|_| "结果无效")?;
                            // The browser needs status, not internal filesystem paths from import reports.
                            Ok(
                                json!({"status":value["status"],"message":value["message"],"result":{"status":value["result"]["status"]}}),
                            )
                        }
                        Err(_) => Ok(json!({"status":"queued"})),
                    }
                }
                _ => Err("不支持的浏览器操作".into()),
            }
        })();
        if result.is_err() {
            upload = None;
        }
        let response = match result {
            Ok(v) => json!({"requestId":message["requestId"],"ok":true,"value":v}),
            Err(e) => json!({"requestId":message["requestId"],"ok":false,"error":e}),
        };
        send_frame(&mut output, &response)?;
    }
    Ok(())
}
pub fn write_result(id: &str, value: &Value) -> Result<(), String> {
    let root = mailbox()?;
    let path = pending_path(&root, id)?;
    atomic_json(&root.join("results").join(format!("{id}.json")), value)?;
    if value["status"] == "completed" {
        // Only exact files in this validated capture directory; never delete a library tree.
        let c: Capture = serde_json::from_slice(
            &fs::read(path.join("capture.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        c.validate()?;
        if c.id != id {
            return Err("暂存标识不一致".into());
        }
        fs::remove_file(path.join(format!("file.{}", c.extension.as_deref().unwrap_or("none"))))
            .map_err(|e| e.to_string())?;
        fs::remove_file(path.join("capture.json")).map_err(|e| e.to_string())?;
        fs::remove_dir(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn mailbox_root() -> Result<PathBuf, String> {
    mailbox()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_are_bounded_and_partial_headers_fail() {
        assert!(read_frame(&mut std::io::Cursor::new(
            ((FRAME_LIMIT + 1) as u32).to_le_bytes()
        ))
        .is_err());
        assert!(read_frame(&mut std::io::Cursor::new(vec![1, 0])).is_err());
        let mut bytes = vec![];
        send_frame(&mut bytes, &json!({"hello":"世界"})).unwrap();
        assert_eq!(
            read_frame(&mut std::io::Cursor::new(bytes)).unwrap(),
            Some(json!({"hello":"世界"}))
        );
    }
    #[test]
    fn heartbeat_requires_live_os_lock_and_fresh_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.path().join("application.lock"))
            .unwrap();
        file.lock().unwrap();
        atomic_json(
            &dir.path().join("connection.json"),
            &json!({"time":Utc::now().timestamp(),"libraryId":Uuid::new_v4().to_string()}),
        )
        .unwrap();
        assert!(connection(dir.path()).is_some());
        atomic_json(
            &dir.path().join("connection.json"),
            &json!({"time":Utc::now().timestamp()-30}),
        )
        .unwrap();
        assert!(connection(dir.path()).is_none());
        atomic_json(
            &dir.path().join("connection.json"),
            &json!({"time":Utc::now().timestamp()}),
        )
        .unwrap();
        drop(file);
        assert!(connection(dir.path()).is_none());
    }
    #[test]
    fn registration_repoints_unicode_launcher_without_losing_authorized_browser_ids() {
        let dir = tempfile::tempdir().unwrap();
        let first = "a".repeat(32);
        let second = "b".repeat(32);
        write_host_configuration(dir.path(), Path::new("C:/旧安装/栈藏.exe"), &first).unwrap();
        write_host_configuration(dir.path(), Path::new("C:/新安装/栈藏.exe"), &second).unwrap();
        let manifest =
            write_host_configuration(dir.path(), Path::new("C:/新安装/栈藏.exe"), &first).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
        assert_eq!(
            value["allowed_origins"],
            json!([
                format!("chrome-extension://{first}/"),
                format!("chrome-extension://{second}/")
            ])
        );
        let launcher = fs::read_to_string(dir.path().join("host.cmd")).unwrap();
        assert!(launcher.contains("\"C:/新安装/栈藏.exe\" --browser-capture-host %*"));
        assert!(!launcher.contains("旧安装"));
        for id in ["*".into(), "z".repeat(32), "a".repeat(31)] {
            assert!(write_host_configuration(dir.path(), Path::new("C:/new.exe"), &id).is_err());
        }
        assert!(
            write_host_configuration(dir.path(), Path::new("C:/%unsafe%/app.exe"), &first).is_err()
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("host.cmd")).unwrap(),
            launcher
        );
    }

    #[test]
    fn capture_rejects_paths_credentials_and_bad_ids() {
        let mut c = Capture {
            id: Uuid::new_v4().to_string(),
            library_id: Uuid::new_v4().to_string(),
            title: "参考".into(),
            page_url: "https://example.com/page".into(),
            media_url: String::new(),
            kind: "image".into(),
            extension: Some("png".into()),
            folder_id: None,
            tags: vec![],
            project_id: None,
            created_at: Utc::now().to_rfc3339(),
        };
        assert!(c.validate().is_ok());
        c.extension = Some("../exe".into());
        assert!(c.validate().is_err());
        c.extension = Some("png".into());
        c.page_url = "https://user:secret@example.com".into();
        assert!(c.validate().is_err());
        assert!(pending_path(Path::new("x"), "../y").is_err());
    }
    #[test]
    fn source_metadata_never_keeps_download_urls_or_login_query_parameters() {
        let mut c = sample_capture();
        for query in [
            "access_token=secret",
            "Signature=secret",
            "x-amz-credential=secret",
            "x-goog-signature=secret",
            "session_id=secret",
            "API_KEY=secret",
            "auth=secret",
            "code=secret",
        ] {
            c.page_url = format!("https://example.com/page?{query}&id=123");
            assert!(c.validate().is_err());
        }
        c.page_url = "https://example.com/page#access_token=secret".into();
        assert!(c.validate().is_err());
        c.page_url = "https://example.com/page?lang=zh&id=123".into();
        assert!(c.validate().is_ok());
        c.media_url = "https://example.com/image.png".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn large_unicode_target_lists_remain_below_native_response_limit() {
        let item = json!({"id":Uuid::new_v4().to_string(),"name":"长名称\"\\\n".repeat(160),"parentId":Uuid::new_v4().to_string()});
        let mut folders = vec![item.clone(); 1000];
        let mut projects = vec![item; 500];
        assert!(bound_choices(&mut folders));
        assert!(bound_choices(&mut projects));
        let response = json!({"ok":true,"value":{"connection":{"folders":folders,"projects":projects,"name":"测试素材库","choicesTruncated":true}}});
        assert!(serde_json::to_vec(&response).unwrap().len() < FRAME_LIMIT);
        send_frame(&mut Vec::new(), &response).unwrap();
    }

    fn sample_capture() -> Capture {
        Capture {
            id: Uuid::new_v4().to_string(),
            library_id: Uuid::new_v4().to_string(),
            title: "浏览器采集概念图".into(),
            page_url: "https://example.com/references/page".into(),
            media_url: String::new(),
            kind: "image".into(),
            extension: Some("png".into()),
            folder_id: None,
            tags: vec!["概念图".into()],
            project_id: None,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    fn stage_capture(root: &Path, c: &Capture) -> PathBuf {
        let path = pending_path(root, &c.id).unwrap();
        fs::create_dir_all(&path).unwrap();
        atomic_json(&path.join("capture.json"), c).unwrap();
        if c.extension.as_deref() == Some("png") {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
                .save(path.join("file.png"))
                .unwrap();
        }
        path
    }

    fn count(db: &Connection, table: &str) -> i64 {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn image_capture_commits_metadata_files_tags_and_project_together() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let mut c = sample_capture();
        let project = Uuid::new_v4().to_string();
        db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES(?1,'UE 场景','scene','active','now','now')", [&project]).unwrap();
        c.project_id = Some(project.clone());
        let folder =
            crate::media_library::upsert_folder(&db, MediaKind::Image, None, None, "参考图".into())
                .unwrap();
        c.folder_id = Some(folder.id.clone());
        stage_capture(dir.path(), &c);
        let result = import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        assert_eq!(result["status"], "saved");
        let entry_id: String = db
            .query_row(
                "SELECT entry_id FROM browser_captures WHERE id=?1",
                [&c.id],
                |r| r.get(0),
            )
            .unwrap();
        let detail = crate::media_library::get(&db, &entry_id).unwrap();
        assert_eq!(detail.entry.name, c.title);
        assert_eq!(detail.entry.source_url, c.page_url);
        assert_eq!(detail.entry.folder_id, Some(folder.id));
        assert_eq!(detail.entry.tags, c.tags);
        assert_eq!(detail.project_ids, vec![project]);
        assert_eq!(detail.files.len(), 2);
        let paths = db
            .prepare("SELECT rel_path FROM media_files WHERE entry_id=?1")
            .unwrap()
            .query_map([&entry_id], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(paths.iter().all(|p| dir.path().join(p).is_file()));
        assert_eq!(count(&db, "media_search"), 1);
        fs::remove_file(pending_path(dir.path(), &c.id).unwrap().join("file.png")).unwrap();
        assert!(paths.iter().all(|p| dir.path().join(p).is_file()));
    }

    #[test]
    fn duplicate_capture_keeps_existing_metadata_folder_tags_and_favorite() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let c = sample_capture();
        stage_capture(dir.path(), &c);
        import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        let entry_id: String = db
            .query_row(
                "SELECT entry_id FROM browser_captures WHERE id=?1",
                [&c.id],
                |r| r.get(0),
            )
            .unwrap();
        let folder = crate::media_library::upsert_folder(
            &db,
            MediaKind::Image,
            None,
            None,
            "手动整理".into(),
        )
        .unwrap();
        db.execute(
            "UPDATE media_entries SET folder_id=?2,favorite=1 WHERE id=?1",
            params![entry_id, folder.id],
        )
        .unwrap();
        crate::media_library::update(
            &db,
            &entry_id,
            "手动命名".into(),
            "自己的说明".into(),
            "作者".into(),
            "https://example.com/original".into(),
            "许可".into(),
            vec!["用户标签".into()],
        )
        .unwrap();
        let before = crate::media_library::get(&db, &entry_id).unwrap();
        let mut duplicate = c.clone();
        duplicate.id = Uuid::new_v4().to_string();
        duplicate.title = "不应覆盖".into();
        duplicate.tags = vec!["不应加入".into()];
        duplicate.folder_id = None;
        duplicate.page_url = "https://example.com/another".into();
        stage_capture(dir.path(), &duplicate);
        let result = import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &duplicate.id,
            &duplicate.library_id,
            &|| false,
        )
        .unwrap();
        assert_eq!(result["status"], "duplicate");
        assert_eq!(result["report"]["rows"][0]["duplicateEntryId"], entry_id);
        let after = crate::media_library::get(&db, &entry_id).unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
        assert_eq!(count(&db, "media_entries"), 1);
        assert_eq!(count(&db, "media_files"), 2);
        assert_eq!(count(&db, "browser_captures"), 2);
    }

    #[test]
    fn wrong_library_and_cancel_keep_staging_while_retry_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let c = sample_capture();
        let staged = stage_capture(dir.path(), &c);
        let wrong_library = Uuid::new_v4().to_string();
        assert!(import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &wrong_library,
            &|| false
        )
        .unwrap_err()
        .contains("素材库已改变"));
        assert!(import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| true
        )
        .unwrap_err()
        .contains("已取消"));
        assert_eq!(count(&db, "media_entries"), 0);
        assert_eq!(count(&db, "browser_captures"), 0);
        assert!(staged.join("capture.json").is_file());
        assert!(staged.join("file.png").is_file());
        assert!(!dir.path().join("media-library").exists());
        import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        let retry = import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        assert_eq!(retry["status"], "saved");
        assert_eq!(count(&db, "browser_captures"), 1);
        assert_eq!(count(&db, "media_entries"), 1);
        assert_eq!(count(&db, "media_files"), 2);
    }

    #[test]
    fn corrupt_image_and_rejected_capture_record_leave_no_partial_media() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let c = sample_capture();
        let staged = stage_capture(dir.path(), &c);
        fs::write(staged.join("file.png"), b"not an image").unwrap();
        assert!(import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false
        )
        .is_err());
        assert_eq!(count(&db, "media_entries"), 0);
        assert_eq!(count(&db, "media_files"), 0);
        assert_eq!(count(&db, "media_entry_tags"), 0);
        let status: String = db
            .query_row(
                "SELECT status FROM browser_captures WHERE id=?1",
                [&c.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "failed");
        stage_capture(dir.path(), &c);
        db.execute_batch("CREATE TRIGGER reject_saved_capture BEFORE INSERT ON browser_captures WHEN NEW.status='saved' BEGIN SELECT RAISE(ABORT,'test capture record failure'); END;").unwrap();
        assert!(import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false
        )
        .is_err());
        for table in [
            "media_entries",
            "media_files",
            "media_entry_tags",
            "localized_tags",
            "media_search",
        ] {
            assert_eq!(count(&db, table), 0, "{table} should roll back");
        }
        assert!(fs::read_dir(dir.path().join("media-library/image"))
            .unwrap()
            .next()
            .is_none());
        assert!(staged.join("file.png").is_file());
    }

    #[test]
    fn source_only_link_is_explicit_and_never_creates_media() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let mut c = sample_capture();
        c.kind = "link".into();
        c.extension = None;
        stage_capture(dir.path(), &c);
        let result = import(
            &mut db,
            dir.path(),
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        assert_eq!(result["status"], "link");
        let (entry, title, page_url, message): (Option<String>, String, String, String) = db
            .query_row(
                "SELECT entry_id,title,page_url,message FROM browser_captures WHERE id=?1",
                [&c.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert!(entry.is_none());
        assert_eq!(title, c.title);
        assert_eq!(page_url, c.page_url);
        assert!(message.contains("未下载视频文件"));
        assert_eq!(count(&db, "media_entries"), 0);
        assert_eq!(count(&db, "media_files"), 0);
    }

    #[test]
    fn migration_is_idempotent_atomic_and_preserves_newer_versions() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE schema_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO schema_meta VALUES('schema_version','17');").unwrap();
        migrate(&db).unwrap();
        let c = sample_capture();
        record(&db, &c, None, "link", "来源链接").unwrap();
        migrate(&db).unwrap();
        assert_eq!(count(&db, "browser_captures"), 1);
        assert_eq!(
            db.query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "18"
        );
        db.execute("UPDATE schema_meta SET value='99'", []).unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM schema_meta", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "99"
        );
        let invalid = Connection::open_in_memory().unwrap();
        assert!(migrate(&invalid).is_err());
        let tables: i64 = invalid
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='browser_captures'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
    }

    #[test]
    fn backup_restore_keeps_capture_history_and_managed_image() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("library");
        fs::create_dir(&base).unwrap();
        let db_path = base.join("library.db");
        let mut db = crate::db::open_database(&db_path).unwrap();
        let c = sample_capture();
        stage_capture(dir.path(), &c);
        import(
            &mut db,
            &base,
            None,
            dir.path(),
            &c.id,
            &c.library_id,
            &|| false,
        )
        .unwrap();
        let rel: String = db
            .query_row(
                "SELECT rel_path FROM media_files WHERE role='main'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let image_before = fs::read(base.join(&rel)).unwrap();
        let output = dir.path().join("capture.dccassetlib");
        crate::backup::export_library(&db, &base, &output).unwrap();
        db.execute("DELETE FROM browser_captures", []).unwrap();
        fs::write(base.join(&rel), b"changed").unwrap();
        let mut active = crate::state::ActiveLibrary {
            base_dir: base.clone(),
            db_path,
            marker: crate::state::LibraryMarker {
                format_version: 1,
                library_id: c.library_id,
                name: "测试采集库".into(),
                created_at: c.created_at,
            },
            connection: Some(db),
        };
        crate::backup::restore_library(&mut active, &output).unwrap();
        assert_eq!(fs::read(base.join(&rel)).unwrap(), image_before);
        let restored = active.connection.as_ref().unwrap();
        assert_eq!(count(restored, "browser_captures"), 1);
        assert_eq!(count(restored, "media_entries"), 1);
        assert_eq!(
            restored
                .query_row("SELECT page_url FROM browser_captures", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            c.page_url
        );
    }
}
