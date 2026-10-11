use super::{domain, models::*, parser, ytdlp};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use tauri::Manager;
// ponytail: serialize postflight only; per-library locks if simultaneous libraries become supported.
static IMPORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn migrate(db: &Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS download_tasks(id TEXT PRIMARY KEY,task_id TEXT NOT NULL UNIQUE,url TEXT NOT NULL,title TEXT NOT NULL DEFAULT '',platform TEXT NOT NULL DEFAULT '',status TEXT NOT NULL,created_at TEXT NOT NULL,finished_at TEXT); CREATE TABLE IF NOT EXISTS media_sources(id TEXT PRIMARY KEY,media_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,platform TEXT NOT NULL,url TEXT NOT NULL,title TEXT NOT NULL,author TEXT NOT NULL,download_task_id TEXT NOT NULL REFERENCES download_tasks(id),created_at TEXT NOT NULL,UNIQUE(media_id,download_task_id)); CREATE INDEX IF NOT EXISTS idx_media_sources_media ON media_sources(media_id); UPDATE schema_meta SET value='20' WHERE key='schema_version' AND CAST(value AS INTEGER)<20;").map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn import_downloaded(
    db: &mut Connection,
    base: &Path,
    resource: Option<&Path>,
    file: &Path,
    info: &DownloadInfo,
    task_id: &str,
    audio: bool,
    stop: &dyn Fn() -> bool,
) -> Result<serde_json::Value, String> {
    let _guard = loop {
        match IMPORT_LOCK.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("媒体入库状态异常，请重启后重试；文件保留".into())
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if stop() {
                    return Err("入库已停止，文件保留".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    };
    if stop() {
        return Err("入库已停止，文件保留".into());
    }
    let probe = crate::runtime::tools::require(resource, "ffprobe")?;
    let media_info = crate::media_engine::probe::analyze(&probe, file, stop)?;
    let size = std::fs::metadata(file)
        .map_err(|_| "下载文件无法读取")?
        .len();
    crate::runtime::tools::ensure_space(
        base,
        size.saturating_mul(3).saturating_add(64 * 1024 * 1024),
    )?;
    let report = crate::media_library::import_controlled(
        db,
        base,
        resource,
        crate::models::MediaImportRequest {
            kind: if audio {
                crate::models::MediaKind::Audio
            } else {
                crate::models::MediaKind::Video
            },
            paths: vec![file.to_string_lossy().into()],
            folder_id: None,
            tags: vec![],
            favorite: false,
        },
        &|_, _, _, _| {},
        stop,
    )?;
    let row = report.rows.first().ok_or("媒体未入库，文件已保留")?;
    let id = row
        .entry_id
        .as_ref()
        .or(row.duplicate_entry_id.as_ref())
        .ok_or_else(|| format!("入库失败：{}；下载文件已保留，可重试", row.message))?;
    crate::media_library::get(db, id)
        .map_err(|_| "相同媒体位于回收站，请先恢复或永久删除后重试")?;
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT OR IGNORE INTO media_sources(id,media_id,platform,url,title,author,download_task_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![uuid::Uuid::new_v4().to_string(),id,info.platform,info.url,info.title,info.author,task_id,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE media_entries SET source_url=CASE WHEN source_url='' THEN ?2 ELSE source_url END,author=CASE WHEN author='' THEN ?3 ELSE author END WHERE id=?1",params![id,info.url,info.author]).map_err(|e|e.to_string())?;
    crate::media_library::reindex(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    retain_subtitles(db, base, file, id, stop)?;
    if stop() {
        return Err("核心资产登记已停止，已入库文件保留，可继续任务".into());
    }
    // The managed media already owns the bytes; core registration is references only.
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let asset_id = crate::asset::repository::register_origin(
        &tx,
        &crate::asset::models::CoreOrigin {
            kind: "media".into(),
            id: id.clone(),
        },
    )?;
    crate::asset::repository::register_files(&tx, &asset_id)?;
    let file_id:String=tx.query_row("SELECT f.id FROM asset_files f JOIN media_entries m ON m.primary_file_id=f.source_id WHERE f.asset_id=?1 AND f.source_kind='mediaFile' AND f.role='original'",[&asset_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    crate::asset::metadata::save(
        &tx,
        &file_id,
        &crate::asset::models::AssetMetadataValues {
            width: if audio { None } else { media_info.width },
            height: if audio { None } else { media_info.height },
            duration: media_info.duration,
            codec: Some(media_info.codec.clone()),
            fps: if audio { None } else { media_info.fps },
        },
    )
    .map_err(|e| format!("核心资产登记失败：{e}；已托管文件和下载原件保留，可重试"))?;
    if stop() {
        return Err("核心资产登记已停止，文件保留".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({"entryId":id,"assetId":asset_id,"name":info.title,"status":row.status,"mediaInfo":media_info}),
    )
}
fn retain_subtitles(
    db: &Connection,
    base: &Path,
    file: &Path,
    id: &str,
    stop: &dyn Fn() -> bool,
) -> Result<(), String> {
    use rusqlite::OptionalExtension;
    let folder = file.parent().ok_or("下载目录缺失")?;
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("文件名无效")?;
    let kind: String = db
        .query_row("SELECT kind FROM media_entries WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let managed = base.join(format!("media-library/{kind}/{id}/subtitles"));
    for item in std::fs::read_dir(folder).map_err(|_| "无法读取字幕目录")? {
        if stop() {
            return Err("字幕入库已停止，文件保留".into());
        }
        let path = item.map_err(|_| "无法读取字幕文件")?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !name.starts_with(&format!("{stem}."))
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("vtt" | "srt" | "ass" | "ssa" | "ttml" | "srv1" | "srv2" | "srv3" | "json3")
            )
        {
            continue;
        }
        checked_file(folder, &path)?;
        let logical = format!("subtitles/{name}");
        if db
            .query_row(
                "SELECT id FROM media_files WHERE entry_id=?1 AND logical_path=?2",
                params![id, logical],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let stored = copy_no_overwrite(&path, &managed, stop)?;
        let rel = stored
            .strip_prefix(base)
            .map_err(|_| "字幕路径越界")?
            .to_string_lossy()
            .replace('\\', "/");
        let result=db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,file_size,created_at) VALUES(?1,?2,'dependency',?3,?4,?5,?6,?7)",params![uuid::Uuid::new_v4().to_string(),id,logical,name,rel,std::fs::metadata(&stored).map_err(|_|"字幕文件无法读取")?.len() as i64,chrono::Utc::now().to_rfc3339()]);
        if let Err(error) = result {
            let _ = std::fs::remove_file(&stored);
            return Err(error.to_string());
        }
    }
    Ok(())
}
pub fn workspace(base: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "下载任务标识无效")?;
    let mut path = base.to_path_buf();
    for part in [".dcc", "runtime", "downloads", id] {
        path.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("下载目录不能是链接或重解析点".into());
                }
            }
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err("下载目录不安全".into());
            }
        }
    }
    std::fs::create_dir_all(&path).map_err(|e| domain::classify_error(&e.to_string()))?;
    Ok(path)
}
pub fn copy_no_overwrite(
    source: &Path,
    folder: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| domain::classify_error(&e.to_string()))?;
    crate::runtime::tools::ensure_space(
        folder,
        std::fs::metadata(source)
            .map_err(|e| domain::classify_error(&e.to_string()))?
            .len()
            .saturating_add(16 * 1024 * 1024),
    )?;
    let mut temp = tempfile::NamedTempFile::new_in(folder)
        .map_err(|e| domain::classify_error(&e.to_string()))?;
    let mut file =
        std::fs::File::open(source).map_err(|e| domain::classify_error(&e.to_string()))?;
    use std::io::{Read, Write};
    let mut buffer = [0u8; 65536];
    loop {
        if stop() {
            return Err("文件复制已停止，原件保留".into());
        }
        let count = file
            .read(&mut buffer)
            .map_err(|e| domain::classify_error(&e.to_string()))?;
        if count == 0 {
            break;
        }
        temp.write_all(&buffer[..count])
            .map_err(|e| domain::classify_error(&e.to_string()))?;
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("文件名无效")?;
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    for number in 0..10_000 {
        let name = if number == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}({number}).{ext}")
        };
        let target = folder.join(name);
        match temp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temp = error.file
            }
            Err(error) => return Err(domain::classify_error(&error.error.to_string())),
        }
    }
    Err("同名文件过多，请更换下载目录".into())
}

pub fn record(db: &Connection, task: &crate::tasks::LibraryTask) -> Result<(), String> {
    if let crate::tasks::TaskRequest::Download(request) = &task.payload {
        let status = if task.status == "running"
            && matches!(
                task.phase.as_str(),
                "resolving" | "downloading" | "processing"
            ) {
            &task.phase
        } else {
            &task.status
        };
        let finished = matches!(task.status.as_str(), "completed" | "failed" | "cancelled")
            .then_some(&task.updated_at);
        db.execute("INSERT INTO download_tasks(id,task_id,url,platform,status,title,created_at,finished_at) VALUES(?1,?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET status=excluded.status,title=CASE WHEN excluded.title='' THEN download_tasks.title ELSE excluded.title END,finished_at=excluded.finished_at",params![task.id,request.url,domain::platform(&request.url),status,task.current_name,task.created_at,finished]).map_err(|e|e.to_string())?;
    }
    Ok(())
}
fn context(
    resource: Option<&Path>,
    settings: &DownloadSettings,
) -> Result<(Vec<String>, Option<super::auth::CookieFile>), String> {
    settings.validate()?;
    let ffmpeg = crate::runtime::tools::require(resource, "ffmpeg")?;
    let deno = crate::runtime::tools::require(resource, "deno")?;
    let mut args = vec![
        "--ffmpeg-location".into(),
        ffmpeg
            .parent()
            .ok_or("媒体工具目录无效")?
            .to_string_lossy()
            .into(),
        "--js-runtimes".into(),
        format!("deno:{}", deno.display()),
        "--proxy".into(),
        settings.proxy.clone(),
    ];
    let mut cookie = None;
    if settings.cookie_source == "file" {
        let file = super::auth::temporary()?;
        args.extend(["--cookies".into(), file.path().to_string_lossy().into()]);
        cookie = Some(file);
    } else if matches!(settings.cookie_source.as_str(), "edge" | "chrome") {
        args.extend([
            "--cookies-from-browser".into(),
            settings.cookie_source.clone(),
        ]);
    }
    Ok((args, cookie))
}
fn resolve(
    resource: Option<&Path>,
    settings: &DownloadSettings,
    url: &str,
    stop: &dyn Fn() -> bool,
) -> Result<DownloadInfo, String> {
    let url = domain::validate_url(url)?;
    let tool = crate::runtime::tools::require(resource, "yt-dlp")?;
    let (mut args, _cookie) = context(resource, settings)?;
    args.extend(ytdlp::base_args());
    args.extend(
        [
            "--flat-playlist",
            "--playlist-end",
            "500",
            "--dump-single-json",
            "--skip-download",
            "--",
        ]
        .map(str::to_string),
    );
    args.push(url.clone());
    let output = crate::runtime::process::capture(
        &tool,
        &args,
        std::time::Duration::from_secs(90),
        stop,
        &mut |_| {},
    )?;
    parser::info(
        &url,
        &serde_json::from_str(output.trim_start_matches('\u{feff}'))
            .map_err(|_| "源站信息无法解析")?,
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Receipt {
    info: DownloadInfo,
    files: Vec<PathBuf>,
    #[serde(default)]
    exports: Vec<PathBuf>,
}
fn save_receipt(folder: &Path, receipt: &Receipt) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(folder).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, receipt).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(folder.join("receipt.json"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn checked_file(folder: &Path, path: &Path) -> Result<(), String> {
    let root = folder.canonicalize().map_err(|_| "下载目录缺失")?;
    let actual = path.canonicalize().map_err(|_| "下载文件缺失")?;
    if !actual.starts_with(&root) || !path.is_file() {
        return Err("下载结果路径越界".into());
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "下载文件无法读取")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("下载文件不能是链接".into());
        }
    }
    if metadata.is_symlink() || metadata.len() == 0 {
        return Err("下载文件为空或不安全".into());
    }
    Ok(())
}

pub fn execute(
    app: &tauri::AppHandle,
    task: &crate::tasks::LibraryTask,
    request: DownloadRequest,
) -> Result<serde_json::Value, String> {
    domain::validate_request(&request)?;
    let state = app.state::<crate::state::AppState>();
    let settings = state.download_settings()?;
    let resource = app.path().resource_dir().ok();
    let (base, dbpath) = state.with_active_mut(|active| {
        if active.marker.library_id != task.library_id {
            return Err("下载所属素材库已切换".into());
        }
        Ok((active.base_dir.clone(), active.db_path.clone()))
    })?;
    let folder = workspace(&base, &task.id)?;
    crate::runtime::tools::ensure_space(&folder, 256 * 1024 * 1024)?;
    let stop = || state.tasks.should_stop(&task.id);
    let mut db = crate::tasks::open_media_worker_connection(&dbpath)?;
    record(&db, task)?;
    let mut receipt = if folder.join("receipt.json").is_file() {
        serde_json::from_slice::<Receipt>(
            &std::fs::read(folder.join("receipt.json")).map_err(|_| "无法读取下载收据")?,
        )
        .map_err(|_| "下载收据损坏，已保留文件")?
    } else {
        state
            .tasks
            .update(&task.id, 0, 0, String::new(), "resolving".into(), None);
        let info = resolve(resource.as_deref(), &settings, &request.url, &stop)?;
        if !info.entries.is_empty() {
            return Err("请在解析结果中将播放列表条目加入队列".into());
        }
        let tool = crate::runtime::tools::require(resource.as_deref(), "yt-dlp")?;
        let (mut args, _cookie) = context(resource.as_deref(), &settings)?;
        args.extend(ytdlp::download_args(&request, &folder)?);
        state.tasks.update(
            &task.id,
            0,
            0,
            info.title.clone(),
            "downloading".into(),
            None,
        );
        let mut files = Vec::new();
        let mut last = std::time::Instant::now();
        let output = crate::runtime::process::capture(
            &tool,
            &args,
            std::time::Duration::from_secs(24 * 60 * 60),
            &stop,
            &mut |line| {
                if let Some(raw) = line.strip_prefix("DCC_FILE") {
                    if let Ok(path) = serde_json::from_str::<String>(raw) {
                        files.push(PathBuf::from(path));
                    }
                }
                if let Some(raw) = line.strip_prefix("DCC_PROGRESS") {
                    if last.elapsed() > std::time::Duration::from_millis(500) {
                        if let Ok(json) = serde_json::from_str(raw) {
                            let progress = parser::progress(&json);
                            state.tasks.update(
                                &task.id,
                                progress.downloaded as usize,
                                progress.total.unwrap_or_default() as usize,
                                info.title.clone(),
                                "downloading".into(),
                                Some(serde_json::json!({"progress":progress})),
                            );
                            last = std::time::Instant::now();
                        }
                    }
                }
                if line.starts_with("DCC_PROCESS") {
                    state.tasks.update(
                        &task.id,
                        0,
                        0,
                        info.title.clone(),
                        "processing".into(),
                        None,
                    );
                }
            },
        );
        // A failed postprocessor must run again on resume, even if its main file exists.
        output?;
        files.retain(|p| checked_file(&folder, p).is_ok());
        files.sort();
        files.dedup();
        if !files.is_empty() {
            let receipt = Receipt {
                info,
                files,
                exports: vec![],
            };
            save_receipt(&folder, &receipt)?;
            receipt
        } else {
            return Err("下载进程未生成有效媒体文件，已保留临时目录".into());
        }
    };
    if stop() {
        return Err("下载已停止，文件保留".into());
    }
    state.tasks.update(
        &task.id,
        0,
        receipt.files.len(),
        receipt.info.title.clone(),
        "processing".into(),
        None,
    );
    let mut rows = Vec::new();
    for (index, file) in receipt.files.iter().enumerate() {
        if stop() {
            return Err("入库已停止，文件保留".into());
        }
        checked_file(&folder, file)?;
        let stop_app = app.clone();
        let stop_id = task.id.clone();
        let _control = crate::media::ProcessControl::new(move || {
            stop_app
                .state::<crate::state::AppState>()
                .tasks
                .should_stop(&stop_id)
        });
        rows.push(import_downloaded(
            &mut db,
            &base,
            resource.as_deref(),
            file,
            &receipt.info,
            &task.id,
            request.audio_only,
            &stop,
        )?);
        state.tasks.update(
            &task.id,
            index + 1,
            receipt.files.len(),
            receipt.info.title.clone(),
            "processing".into(),
            Some(serde_json::json!({"rows":rows})),
        );
    }
    if request.output_mode == "libraryAndFolder" {
        if settings.directory.is_empty() {
            return Err("素材已入库，但下载目录未设置；设置后可重试导出".into());
        }
        if receipt.exports.len() < receipt.files.len() {
            for file in &receipt.files[receipt.exports.len()..] {
                receipt.exports.push(copy_no_overwrite(
                    file,
                    Path::new(&settings.directory),
                    &stop,
                )?);
                save_receipt(&folder, &receipt)?;
            }
        }
    }
    db.execute(
        "UPDATE download_tasks SET title=?2,status='completed',finished_at=?3 WHERE id=?1",
        params![task.id, receipt.info.title, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"rows":rows,"outputPath":folder,"exports":receipt.exports}))
}

#[tauri::command]
pub async fn resolve_download(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    url: String,
) -> Result<DownloadInfo, String> {
    let settings = state.download_settings()?;
    let resource = app.path().resource_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        resolve(resource.as_deref(), &settings, &url, &|| false)
    })
    .await
    .map_err(|_| "解析任务异常结束")?
}
#[tauri::command]
pub async fn get_download_tools(
    app: tauri::AppHandle,
) -> Result<Vec<crate::runtime::tools::ToolStatus>, String> {
    let resource = app.path().resource_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        crate::runtime::tools::inspect(resource.as_deref())
    })
    .await
    .map_err(|_| "检测下载工具失败".into())
}
#[tauri::command]
pub async fn get_download_thumbnail(url: String) -> Result<String, String> {
    let directory = tempfile::tempdir().map_err(|_| "无法建立封面暂存目录")?;
    let path = crate::note_remote_images::fetch(&url, directory.path()).await?;
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let image = image::open(path)
            .map_err(|_| "封面无法解码")?
            .thumbnail(480, 270);
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::WebP)
            .map_err(|_| "封面生成失败")?;
        drop(directory);
        Ok(format!(
            "data:image/webp;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(output.into_inner())
        ))
    })
    .await
    .map_err(|_| "封面读取异常")?
}
#[tauri::command]
pub fn get_download_settings(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<DownloadSettings, String> {
    state.download_settings()
}
#[tauri::command]
pub fn save_download_settings(
    state: tauri::State<'_, crate::state::AppState>,
    settings: DownloadSettings,
) -> Result<(), String> {
    state.save_download_settings(settings)
}
#[tauri::command]
pub fn get_download_cookie_status() -> bool {
    super::auth::configured()
}
#[tauri::command]
pub fn import_download_cookies(path: String) -> Result<(), String> {
    super::auth::import(Path::new(&path))
}
#[tauri::command]
pub fn delete_download_cookies() -> Result<(), String> {
    super::auth::delete()
}
#[tauri::command]
pub fn list_download_tasks(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<crate::tasks::LibraryTask>, String> {
    state
        .tasks
        .list(&state.active_library_id()?)?
        .into_iter()
        .filter(|t| matches!(t.payload, crate::tasks::TaskRequest::Download(_)))
        .map(|t| state.tasks.get(&t.id))
        .collect()
}
#[tauri::command]
pub fn open_download_output(
    state: tauri::State<'_, crate::state::AppState>,
    id: String,
) -> Result<(), String> {
    let task = state.tasks.get(&id)?;
    if !matches!(task.payload, crate::tasks::TaskRequest::Download(_))
        || task.library_id != state.active_library_id()?
    {
        return Err("任务不属于当前下载中心".into());
    }
    let base = state.with_active_mut(|a| Ok(a.base_dir.clone()))?;
    let folder = workspace(&base, &task.id)?;
    open::that_detached(folder).map_err(|_| "无法打开下载目录".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_local_fixture_runs_ytdlp_and_imports() {
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let resource = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let wav = base.join("fixture.wav");
        let ffmpeg = crate::runtime::tools::require(Some(&resource), "ffmpeg").unwrap();
        let args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.2",
            "-c:a",
            "pcm_s16le",
        ]
        .map(str::to_string)
        .into_iter()
        .chain(Some(wav.to_string_lossy().into()))
        .collect::<Vec<_>>();
        crate::runtime::process::capture(
            &ffmpeg,
            &args,
            std::time::Duration::from_secs(10),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        let bytes = std::fs::read(&wav).unwrap();
        let video = base.join("fixture.mp4");
        let args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x48:d=0.2",
            "-c:v",
            "mpeg4",
        ]
        .map(str::to_string)
        .into_iter()
        .chain(Some(video.to_string_lossy().into()))
        .collect::<Vec<_>>();
        crate::runtime::process::capture(
            &ffmpeg,
            &args,
            std::time::Duration::from_secs(10),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        let video_bytes = std::fs::read(video).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let end = stopped.clone();
        let server = std::thread::spawn(move || {
            while !end.load(Ordering::Relaxed) {
                if let Ok((mut client, _)) = listener.accept() {
                    // Windows inherits the listener's nonblocking flag on accepted sockets.
                    client.set_nonblocking(false).unwrap();
                    let _ = client.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                    let mut request = [0u8; 8192];
                    let mut count = 0;
                    while count < request.len() {
                        let read = client.read(&mut request[count..]).unwrap_or_default();
                        if read == 0 {
                            break;
                        }
                        count += read;
                        if request[..count].windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8_lossy(&request[..count]);
                    let head = request.starts_with("HEAD ");
                    let (bytes, kind) = if request.contains("fixture.mp4") {
                        (&video_bytes, "video/mp4")
                    } else {
                        (&bytes, "audio/wav")
                    };
                    let _=write!(client,"HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len());
                    if !head {
                        let _ = client.write_all(&bytes);
                    }
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        });
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        for (audio, filename, extension) in
            [(true, "fixture.wav", "mp3"), (false, "fixture.mp4", "mp4")]
        {
            let url = format!("http://{address}/{filename}");
            let request = DownloadRequest {
                url: format!("https://example.com/{filename}"),
                format: "best".into(),
                quality: "best".into(),
                audio_only: audio,
                subtitles: false,
                output_mode: "library".into(),
            };
            let folder = workspace(base, &uuid::Uuid::new_v4().to_string()).unwrap();
            let (mut args, _) = context(Some(&resource), &DownloadSettings::default()).unwrap();
            args.extend(ytdlp::download_args(&request, &folder).unwrap());
            // Only this in-process fixture bypasses public URL validation; production never does.
            *args.last_mut().unwrap() = url;
            let tool = crate::runtime::tools::require(Some(&resource), "yt-dlp").unwrap();
            let mut files = Vec::new();
            let output = crate::runtime::process::capture(
                &tool,
                &args,
                std::time::Duration::from_secs(30),
                &|| false,
                &mut |line| {
                    if let Some(raw) = line.strip_prefix("DCC_FILE") {
                        if let Ok(path) = serde_json::from_str::<String>(raw) {
                            files.push(PathBuf::from(path));
                        }
                    }
                },
            );
            output.unwrap_or_else(|error| panic!("{filename}: {error}"));
            assert_eq!(files.len(), 1);
            checked_file(&folder, &files[0]).unwrap();
            assert_eq!(files[0].extension().unwrap(), extension);
            db.execute("INSERT INTO download_tasks(id,task_id,url,status,created_at) VALUES(?1,?1,?2,'processing','now')",params![filename,request.url]).unwrap();
            let info = parser::info(&request.url, &serde_json::json!({"title":"Fixture"})).unwrap();
            let result = import_downloaded(
                &mut db,
                base,
                Some(&resource),
                &files[0],
                &info,
                filename,
                audio,
                &|| false,
            )
            .unwrap();
            let detail =
                crate::media_library::get(&db, result["entryId"].as_str().unwrap()).unwrap();
            assert!(detail.entry.thumbnail_file_id.is_some());
            let core = crate::asset::repository::get(
                &db,
                base,
                result["assetId"]
                    .as_str()
                    .expect("download returns core identity"),
                "zh-CN",
            )
            .unwrap();
            assert_eq!(core.asset.origin.id, detail.entry.id);
            assert_eq!(core.metadata.len(), 1);
            assert_eq!(core.metadata[0].width, if audio { None } else { Some(64) });
            assert_eq!(
                core.metadata[0].codec.as_deref(),
                result["mediaInfo"]["codec"].as_str()
            );
            if !audio {
                assert!(core.metadata[0].fps.unwrap() > 0.0);
            }
            assert_eq!(
                result["mediaInfo"]["width"].as_u64(),
                if audio { None } else { Some(64) }
            );
        }
        stopped.store(true, Ordering::Relaxed);
        server.join().unwrap();
    }
    #[test]
    fn download_postflight_checks_file_imports_and_links_source() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let resource = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let tool = crate::runtime::tools::require(Some(&resource), "ffmpeg").unwrap();
        let file = base.join("audio.wav");
        let args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.2",
            "-c:a",
            "pcm_s16le",
        ]
        .map(str::to_string)
        .into_iter()
        .chain(Some(file.to_string_lossy().into()))
        .collect::<Vec<_>>();
        crate::runtime::process::capture(
            &tool,
            &args,
            std::time::Duration::from_secs(10),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        let info = parser::info(
            "https://example.com/source",
            &serde_json::json!({"title":"Sound","uploader":"作者"}),
        )
        .unwrap();
        db.execute("INSERT INTO download_tasks(id,task_id,url,status,created_at) VALUES('task','task',?1,'processing','now')",[&info.url]).unwrap();
        std::fs::write(
            base.join("audio.en.vtt"),
            b"WEBVTT\n\n00:00.000 --> 00:00.200\nSound",
        )
        .unwrap();
        let row = import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &file,
            &info,
            "task",
            true,
            &|| false,
        )
        .unwrap();
        let id = row["entryId"].as_str().unwrap();
        let asset_id = row["assetId"].as_str().expect("stable core id");
        let core = crate::asset::repository::get(&db, base, asset_id, "zh-CN").unwrap();
        assert_eq!(
            core.files.iter().filter(|f| f.role == "original").count(),
            1
        );
        assert_eq!(core.metadata[0].codec.as_deref(), Some("pcm_s16le"));
        assert_eq!(core.sources[0].url, info.url);
        let detail = crate::media_library::get(&db, id).unwrap();
        assert_eq!(detail.entry.source_url, info.url);
        assert!(detail.entry.thumbnail_file_id.is_some());
        assert!(detail.files.iter().any(|f| f.role == "waveform"));
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM media_sources WHERE media_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let subtitle = detail
            .files
            .iter()
            .find(|f| f.logical_path.starts_with("subtitles/"))
            .expect("subtitle retained in managed library");
        let rel: String = db
            .query_row(
                "SELECT rel_path FROM media_files WHERE id=?1",
                [&subtitle.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(base.join(rel).is_file());
        import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &file,
            &info,
            "task",
            true,
            &|| false,
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            crate::media_library::get(&db, id)
                .unwrap()
                .files
                .iter()
                .filter(|f| f.logical_path.starts_with("subtitles/"))
                .count(),
            1
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers = ["parallel-one", "parallel-two"].map(|task| {
            db.execute("INSERT INTO download_tasks(id,task_id,url,status,created_at) VALUES(?1,?1,?2,'processing','now')",params![task,info.url]).unwrap();
            let mut connection = crate::db::open_database(&base.join("library.db")).unwrap();
            let (base, resource, file, info, barrier) = (base.to_path_buf(), resource.clone(), file.clone(), info.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                import_downloaded(&mut connection, &base, Some(&resource), &file, &info, task, true, &||false).unwrap()
            })
        });
        for worker in workers {
            let result = worker.join().unwrap();
            assert_eq!(result["entryId"], id);
            assert_eq!(result["assetId"], asset_id);
        }
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM media_sources", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        let backup = base.join("download.dccassetlib");
        crate::backup::export_library(&db, base, &backup).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(backup).unwrap()).unwrap();
        let subtitle_path: String = db
            .query_row(
                "SELECT rel_path FROM media_files WHERE id=?1",
                [&subtitle.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(archive.by_name(&subtitle_path.replace('\\', "/")).is_ok());
        let snapshot = base.join("snapshot.db");
        std::io::copy(
            &mut archive.by_name("library.db").unwrap(),
            &mut std::fs::File::create(&snapshot).unwrap(),
        )
        .unwrap();
        assert_eq!(
            Connection::open(snapshot)
                .unwrap()
                .query_row("SELECT COUNT(*) FROM media_sources", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        std::fs::write(base.join("bad.wav"), b"damaged").unwrap();
        assert!(import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &base.join("bad.wav"),
            &info,
            "task",
            true,
            &|| false
        )
        .is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn download_migration_is_additive_and_repeatable() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM download_tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(db.execute("INSERT INTO media_sources(id,media_id,platform,url,title,author,download_task_id,created_at) VALUES('a','missing','','','','','none','now')",[]).is_err());
    }

    #[test]
    fn core_registration_failure_keeps_managed_download_and_retry_reuses_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
        let resource = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let file = base.join("sound.wav");
        let tool = crate::runtime::tools::require(Some(&resource), "ffmpeg").unwrap();
        let args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=480:duration=0.2",
            "-c:a",
            "pcm_s16le",
        ]
        .map(str::to_string)
        .into_iter()
        .chain(Some(file.to_string_lossy().into()))
        .collect::<Vec<_>>();
        crate::runtime::process::capture(
            &tool,
            &args,
            std::time::Duration::from_secs(10),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        std::fs::write(
            base.join("sound.zh.vtt"),
            "WEBVTT\n\n00:00.000 --> 00:00.200\n声音",
        )
        .unwrap();
        let info = parser::info(
            "https://youtube.com/watch?v=owned",
            &serde_json::json!({"title":"声音","uploader":"作者"}),
        )
        .unwrap();
        db.execute("INSERT INTO download_tasks(id,task_id,url,status,created_at) VALUES('task','task',?1,'processing','now')",[&info.url]).unwrap();
        db.execute_batch("CREATE TRIGGER reject_core_metadata BEFORE INSERT ON asset_metadata BEGIN SELECT RAISE(ABORT,'injected postflight failure'); END;").unwrap();
        assert!(import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &file,
            &info,
            "task",
            true,
            &|| false
        )
        .is_err());
        let entry_id: String = db
            .query_row("SELECT id FROM media_entries", [], |r| r.get(0))
            .unwrap();
        let files_before: Vec<(String, String)> = db
            .prepare("SELECT id,rel_path FROM media_files ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(files_before
            .iter()
            .all(|(_, path)| base.join(path).is_file()));
        assert_eq!(
            db.query_row("SELECT count(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.execute_batch("DROP TRIGGER reject_core_metadata;")
            .unwrap();
        let row = import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &file,
            &info,
            "task",
            true,
            &|| false,
        )
        .unwrap();
        assert_eq!(row["entryId"], entry_id);
        let files_after: Vec<(String, String)> = db
            .prepare("SELECT id,rel_path FROM media_files ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(files_before, files_after);
        let detail =
            crate::asset::repository::get(&db, base, row["assetId"].as_str().unwrap(), "zh-CN")
                .unwrap();
        assert_eq!(detail.metadata.len(), 1);
        assert!(detail.files.iter().any(|f| f.role == "dependency"));
        assert_eq!(detail.sources.len(), 1);
        assert_eq!(detail.sources[0].source_type, "youtube");
        assert!(import_downloaded(
            &mut db,
            base,
            Some(&resource),
            &file,
            &info,
            "task",
            true,
            &|| true
        )
        .is_err());
        assert_eq!(
            db.query_row("SELECT count(*) FROM media_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn download_paths_and_duplicate_outputs_do_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        assert!(workspace(dir.path(), "../../secret").is_err());
        let id = uuid::Uuid::new_v4().to_string();
        assert!(workspace(dir.path(), &id)
            .unwrap()
            .starts_with(dir.path().join(".dcc/runtime/downloads")));
        let source = dir.path().join("video.mp4");
        std::fs::write(&source, b"video").unwrap();
        let output = dir.path().join("output");
        assert!(copy_no_overwrite(&source, &output, &|| true).is_err());
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
        let first = copy_no_overwrite(&source, &output, &|| false).unwrap();
        let second = copy_no_overwrite(&source, &output, &|| false).unwrap();
        assert_eq!(first.file_name().unwrap(), "video.mp4");
        assert_eq!(second.file_name().unwrap(), "video(1).mp4");
        assert_eq!(std::fs::read(first).unwrap(), b"video");
    }
}
