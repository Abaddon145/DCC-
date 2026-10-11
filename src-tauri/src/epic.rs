use crate::{fab, state::AppState};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const MAX_PROJECT_BYTES: u64 = 1024 * 1024;
const LIBRARY_CHANGED: &str = "素材库已切换，请重新打开素材的 Epic 添加入口";
static EPIC_NAVIGATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
const EPIC_ROUTE_REJECTED: &str = "Epic 本次商品导航被拒绝";

// Only inspect router lines appended after dispatch; never persist or expose Launcher logs.
fn epic_route_rejected(log: &str, listing_id: Uuid) -> bool {
    let target = format!("/plugins/egl/listings/{listing_id}'");
    let mut matching = false;
    let mut stopped = false;
    for line in log.lines().filter(|line| line.contains("[MainRouter]")) {
        if line.contains("GoToUrl('") {
            matching = line.contains(&target);
            stopped = false;
        } else if matching && line.contains("Navigation Stopped. Already at location.") {
            stopped = true;
        } else if matching && stopped && line.contains("Navigation Canceled.") {
            return true;
        }
    }
    false
}

fn epic_log_cursor(path: &Path) -> Option<File> {
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::End(0)).ok()?;
    Some(file)
}

fn wait_for_epic_rejection(file: Option<File>, listing_id: Uuid) -> bool {
    let Some(mut file) = file else {
        return false;
    };
    let mut bytes = Vec::new();
    // ponytail: bounded best-effort observation, not a documented Epic readiness API.
    // Missing/rotated/delayed logs leave manual recovery available; no unconditional page reset.
    for _ in 0..15 {
        if (&mut file)
            .take(65536_u64.saturating_sub(bytes.len() as u64))
            .read_to_end(&mut bytes)
            .is_err()
        {
            return false;
        }
        if epic_route_rejected(&String::from_utf8_lossy(&bytes), listing_id) {
            return true;
        }
        if bytes.len() >= 65536 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Known,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabUeOptions {
    pub asset_id: String,
    pub listing_id: String,
    pub canonical_url: String,
    pub epic_url: String,
    pub supported_versions: Vec<String>,
    pub version_status: VersionStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UeProjectTarget {
    pub project_id: Option<String>,
    pub path_id: Option<String>,
    pub project_name: Option<String>,
    pub name: String,
    pub path: String,
    pub engine_association: Option<String>,
    pub engine_version: Option<String>,
    pub available: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicOpenResult {
    pub listing_id: String,
    pub url: String,
    pub requested: bool,
    pub message: String,
}

fn in_library<T>(
    state: &AppState,
    expected_library_id: &str,
    action: impl FnOnce(&Connection, &Path) -> Result<T, String>,
) -> Result<T, String> {
    state
        .with_library_if_id(expected_library_id, action)?
        .ok_or_else(|| LIBRARY_CHANGED.into())
}

fn registered_listing(connection: &Connection, asset_id: &str) -> Result<Uuid, String> {
    let (source_url, stored_id): (String, String) = connection
        .query_row(
            "SELECT source_url,fab_listing_id FROM assets WHERE id=?1 AND deleted_at IS NULL",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or("素材不存在或已删除")?;
    let source = url::Url::parse(source_url.trim()).map_err(|_| "素材的 Fab 来源网址无效")?;
    if !source.username().is_empty() || source.password().is_some() || source.port().is_some() {
        return Err("素材的 Fab 来源网址包含不支持的登录信息或端口".into());
    }
    let segments: Vec<_> = source
        .path()
        .trim_end_matches('/')
        .split('/')
        .skip(1)
        .collect();
    let listing_path = match segments.as_slice() {
        ["listings", _] => true,
        [locale, "listings", _] => {
            let pieces: Vec<_> = locale.split('-').collect();
            (1..=2).contains(&pieces.len())
                && pieces.iter().all(|piece| {
                    (2..=3).contains(&piece.len())
                        && piece
                            .bytes()
                            .all(|character| character.is_ascii_alphabetic())
                })
        }
        _ => false,
    };
    if !listing_path {
        return Err("素材的 Fab 来源必须是商品详情网址".into());
    }
    let source_id = fab::listing_id_from_url(source_url.trim())?;
    let stored_id = Uuid::parse_str(stored_id.trim())
        .map_err(|_| "素材没有有效的 Fab 商品 ID，请重新读取 Fab 来源")?;
    if stored_id != source_id {
        return Err("Fab 来源网址与保存的商品 ID 不一致，请先重新编辑该素材".into());
    }
    Ok(stored_id)
}

fn epic_listing_url(listing_id: Uuid) -> String {
    format!("com.epicgames.launcher://fab/plugins/egl/listings/{listing_id}")
}

fn launcher_running_from_tasklist(output: &str) -> bool {
    output
        .to_ascii_lowercase()
        .contains("epicgameslauncher.exe")
}

#[cfg(windows)]
fn is_launcher_running() -> Result<bool, String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("tasklist.exe")
        .args([
            "/FI",
            "IMAGENAME eq EpicGamesLauncher.exe",
            "/FO",
            "CSV",
            "/NH",
        ])
        .creation_flags(0x08000000)
        .output()
        .map_err(|error| format!("无法检查 Epic Launcher 运行状态：{error}"))?;
    if !output.status.success() {
        return Err("Epic Launcher 运行状态检查失败".into());
    }
    Ok(launcher_running_from_tasklist(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn request_epic_navigation_when_running(
    listing_id: Uuid,
    recover: bool,
    running: Result<bool, String>,
    dispatch: impl FnMut(&str) -> Result<(), String>,
    handoff: impl FnMut() -> Result<(), String>,
) -> Result<String, String> {
    if matches!(running, Ok(false)) {
        return Err("Epic Games Launcher 未运行，请先启动 Epic 并登录后再定位商品".into());
    }
    request_epic_navigation(listing_id, recover, dispatch, handoff)
}

fn request_epic_navigation(
    listing_id: Uuid,
    recover: bool,
    mut dispatch: impl FnMut(&str) -> Result<(), String>,
    mut handoff: impl FnMut() -> Result<(), String>,
) -> Result<String, String> {
    if recover {
        dispatch("com.epicgames.launcher://ue/library")?;
        handoff()?;
    }
    let url = epic_listing_url(listing_id);
    match dispatch(&url) {
        Err(error) if !recover && error == EPIC_ROUTE_REJECTED => {
            dispatch("com.epicgames.launcher://ue/library")?;
            handoff()?;
            dispatch(&url).map_err(|error| {
                if error == EPIC_ROUTE_REJECTED {
                    "Epic 恢复后仍拒绝定位，请在 Epic 搜索商品或打开 Fab 官方页面".into()
                } else {
                    error
                }
            })?;
        }
        Err(error) if error == EPIC_ROUTE_REJECTED => {
            return Err("Epic 恢复后仍拒绝定位，请在 Epic 搜索商品或打开 Fab 官方页面".into())
        }
        result => result?,
    }
    Ok(url)
}

#[tauri::command]
pub async fn get_fab_ue_options(
    state: State<'_, AppState>,
    asset_id: String,
    expected_library_id: String,
) -> Result<FabUeOptions, String> {
    let listing_id = in_library(&state, &expected_library_id, |connection, _| {
        registered_listing(connection, &asset_id)
    })?;
    let (supported_versions, warnings) = match fab::fetch_ue_versions(listing_id).await {
        Ok(result) => result,
        Err(error) => (
            Vec::new(),
            vec![format!(
                "Fab UE 支持信息读取失败：{error}。仍可进入 Epic 核对版本。"
            )],
        ),
    };
    in_library(&state, &expected_library_id, |connection, _| {
        if registered_listing(connection, &asset_id)? != listing_id {
            return Err("素材的 Fab 来源已改变，请重新打开 Epic 添加入口".into());
        }
        Ok(FabUeOptions {
            asset_id,
            listing_id: listing_id.to_string(),
            canonical_url: format!("https://www.fab.com/listings/{listing_id}"),
            epic_url: epic_listing_url(listing_id),
            version_status: if supported_versions.is_empty() {
                VersionStatus::Unknown
            } else if warnings.is_empty() {
                VersionStatus::Known
            } else {
                VersionStatus::Partial
            },
            supported_versions,
            warnings,
        })
    })
}

fn inspect_project(path: &str) -> Result<UeProjectTarget, String> {
    let path = Path::new(path.trim());
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("uproject"))
    {
        return Err("请选择绝对路径的 .uproject 工程文件".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("工程文件无法读取：{error}"))?;
    if !canonical
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("uproject"))
    {
        return Err("工程文件链接的实际目标不是 .uproject".into());
    }
    let file = File::open(&canonical).map_err(|error| format!("工程文件无法读取：{error}"))?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_PROJECT_BYTES {
        return Err("工程文件不是普通文件或超过 1 MB，未读取".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("工程文件读取失败：{error}"))?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err("工程文件超过 1 MB，未读取".into());
    }
    let data: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "工程文件不是有效的 JSON，未修改文件")?;
    if !data.is_object()
        || !data
            .get("FileVersion")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version > 0)
    {
        return Err("文件缺少有效的 UE 工程 FileVersion，未修改文件".into());
    }
    let engine_association = match data.get("EngineAssociation") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) if value.len() <= 256 => {
            (!value.trim().is_empty()).then(|| value.trim().to_string())
        }
        _ => return Err("工程的 EngineAssociation 格式无效，未修改文件".into()),
    };
    let engine_version = engine_association
        .as_deref()
        .and_then(fab::normalize_ue_version);
    let warnings = if engine_version.is_none() {
        vec!["工程未声明可识别的 UE 数字版本，可能使用自定义引擎；请在 Epic 中核对。".into()]
    } else {
        Vec::new()
    };
    Ok(UeProjectTarget {
        project_id: None,
        path_id: None,
        project_name: None,
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("UE 工程")
            .into(),
        path: path.to_string_lossy().into_owned(),
        engine_association,
        engine_version,
        available: true,
        warnings,
    })
}

#[tauri::command]
pub async fn inspect_ue_project(
    app: AppHandle,
    path: String,
    expected_library_id: String,
) -> Result<UeProjectTarget, String> {
    tauri::async_runtime::spawn_blocking(move || {
        in_library(&app.state::<AppState>(), &expected_library_id, |_, _| {
            inspect_project(&path)
        })
    })
    .await
    .map_err(|error| format!("工程读取任务失败：{error}"))?
}

fn project_path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.trim().replace('\\', "/").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.trim().to_string()
    }
}

fn registered_project_targets(connection: &Connection) -> Result<Vec<UeProjectTarget>, String> {
    let mut query = connection.prepare("SELECT p.id,pp.id,p.name,pp.label,pp.path FROM project_paths pp JOIN projects p ON p.id=pp.project_id WHERE pp.path_type='file' AND lower(pp.path) LIKE '%.uproject' ORDER BY p.name COLLATE NOCASE,pp.sort_order,pp.id").map_err(|error| error.to_string())?;
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let (project_id, path_id, project_name, label, path) =
            row.map_err(|error| error.to_string())?;
        if !seen.insert(project_path_key(&path)) {
            continue;
        }
        let mut target = inspect_project(&path).unwrap_or_else(|error| UeProjectTarget {
            project_id: None,
            path_id: None,
            project_name: None,
            name: label.clone(),
            path,
            engine_association: None,
            engine_version: None,
            available: false,
            warnings: vec![error],
        });
        target.project_id = Some(project_id);
        target.path_id = Some(path_id);
        target.project_name = Some(project_name);
        if !label.trim().is_empty() {
            target.name = label;
        }
        targets.push(target);
    }
    Ok(targets)
}

#[tauri::command]
pub async fn list_ue_project_targets(
    app: AppHandle,
    expected_library_id: String,
) -> Result<Vec<UeProjectTarget>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        in_library(
            &app.state::<AppState>(),
            &expected_library_id,
            |connection, _| registered_project_targets(connection),
        )
    })
    .await
    .map_err(|error| format!("工程列表读取任务失败：{error}"))?
}

#[cfg(windows)]
fn check_epic_protocol() -> Result<(), String> {
    use std::{ffi::c_void, ptr};
    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            key: *mut c_void,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            value_type: *mut u32,
            data: *mut c_void,
            size: *mut u32,
        ) -> i32;
    }
    // Read only the public protocol handler, never Epic account or login data.
    let subkey: Vec<u16> = "com.epicgames.launcher\\shell\\open\\command"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut buffer = [0u16; 4096];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            (0x8000_0000u32 as i32 as isize) as *mut c_void,
            subkey.as_ptr(),
            ptr::null(),
            0x0000_0006,
            ptr::null_mut(),
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if status != 0 {
        return Err("未找到可用的 Epic Games Launcher 协议，请先安装或修复 Epic 桌面端".into());
    }
    let command = String::from_utf16(
        &buffer[..buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len())],
    )
    .map_err(|_| "Epic 协议注册信息无法识别")?;
    let executable = if let Some(quoted) = command.strip_prefix('"') {
        quoted.split_once('"').map(|(path, _)| path)
    } else {
        command.split_whitespace().next()
    }
    .ok_or("Epic 协议没有有效的启动程序")?;
    let executable = Path::new(executable);
    if !command.contains("%1")
        || !executable.is_absolute()
        || !executable.is_file()
        || !executable
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("EpicGamesLauncher.exe"))
    {
        return Err("Epic 协议未指向可用的 EpicGamesLauncher.exe，请修复 Epic 桌面端注册".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn check_epic_protocol() -> Result<(), String> {
    Err("此入口目前仅支持 Windows 的 Epic Games Launcher".into())
}

#[tauri::command]
pub async fn open_fab_in_epic(
    app: AppHandle,
    asset_id: String,
    expected_library_id: String,
    recover: Option<bool>,
) -> Result<EpicOpenResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _navigation = EPIC_NAVIGATION
            .try_lock()
            .map_err(|_| "Epic 正在切换商品，请等待当前定位请求完成后重试")?;
        let state = app.state::<AppState>();
        let listing_id = in_library(&state, &expected_library_id, |connection, _| {
            registered_listing(connection, &asset_id)
        })?;
        check_epic_protocol()?;
        #[cfg(windows)]
        let running = is_launcher_running();
        #[cfg(not(windows))]
        let running = Ok(true); // The protocol check rejects unsupported platforms first.
        let recovered = std::cell::Cell::new(false);
        let url = request_epic_navigation_when_running(
            listing_id,
            recover.unwrap_or(false),
            running,
            |url| {
                let product = url == epic_listing_url(listing_id);
                let log = if product {
                    std::env::var_os("LOCALAPPDATA").and_then(|root| epic_log_cursor(
                        &std::path::PathBuf::from(root).join("EpicGamesLauncher/Saved/Logs/EpicGamesLauncher.log")
                    ))
                } else { None };
                in_library(&state, &expected_library_id, |connection, _| {
                if registered_listing(connection, &asset_id)? != listing_id {
                    return Err("素材的 Fab 来源已改变，请重新打开 Epic 添加入口".into());
                }
                open::that(url).map_err(|error| format!("无法打开 Epic 商品入口：{error}"))
                })?;
                if !product { recovered.set(true); }
                if product && wait_for_epic_rejection(log, listing_id) {
                    return Err(EPIC_ROUTE_REJECTED.into());
                }
                Ok(())
            },
            || {
                // ponytail: two-second handoff verified in this warm Launcher session;
                // no documented ready callback exists. Slow startup still needs manual retry.
                // Wait outside the library lock; never restart Epic or touch its cache.
                std::thread::sleep(std::time::Duration::from_secs(2));
                Ok(())
            },
        )?;
        Ok(EpicOpenResult {
            listing_id: listing_id.to_string(),
            url,
            requested: true,
            message: if recovered.get() {
                "已执行一次恢复定位并重新发送商品请求。请在 Epic 核对商品名称；尚未下载或安装。"
            } else {
                "已向 Epic 直接发送商品定位请求。请核对商品名称，再在 Epic 选择工程和版本；若仍停在原页面，可手动恢复定位。"
            }.into(),
        })
    })
    .await
    .map_err(|error| format!("Epic 商品入口任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_cursor_ignores_old_rejections_and_reads_only_new_bytes() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("launcher.log");
        let id = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        let rejected = "[MainRouter] GoToUrl('/fab/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e')\n[MainRouter] Navigation Stopped. Already at location.\n[MainRouter] Navigation Canceled.\n";
        std::fs::write(&path, rejected).unwrap();
        let mut cursor = epic_log_cursor(&path).unwrap();
        let mut fresh = String::new();
        cursor.read_to_string(&mut fresh).unwrap();
        assert!(fresh.is_empty());
        let mut writer = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writer.write_all(rejected.as_bytes()).unwrap();
        assert!(wait_for_epic_rejection(Some(cursor), id));
        assert!(!wait_for_epic_rejection(None, id));
    }

    #[test]
    fn router_rejection_must_follow_this_product_request_not_an_unrelated_route() {
        let id = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        let request = "[MainRouter] GoToUrl('https://www.fab.com/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e')\n";
        let rejected = "[MainRouter] Navigation Stopped. Already at location.\n[MainRouter] Navigation Canceled.\n";
        assert!(epic_route_rejected(&format!("{request}{rejected}"), id));
        assert!(!epic_route_rejected(rejected, id));
        assert!(!epic_route_rejected(&format!("{rejected}{request}"), id));
        assert!(!epic_route_rejected(
            &format!("{request}[MainRouter] GoToUrl('/ue/library')\n{rejected}"),
            id
        ));
        assert!(!epic_route_rejected(
            &format!("{request}{rejected}"),
            Uuid::new_v4()
        ));
        assert!(!epic_route_rejected(
            &format!("{request}[MainRouter] Navigation Canceled.\n"),
            id
        ));
    }

    #[test]
    fn rejected_direct_navigation_recovers_once_without_restarting_epic() {
        let trace = std::cell::RefCell::new(Vec::new());
        let attempts = std::cell::Cell::new(0);
        let id = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        let result = request_epic_navigation(
            id,
            false,
            |url| {
                trace.borrow_mut().push(url.to_string());
                if url.contains("/listings/") {
                    attempts.set(attempts.get() + 1);
                    if attempts.get() == 1 {
                        return Err("Epic 本次商品导航被拒绝".into());
                    }
                }
                Ok(())
            },
            || {
                trace.borrow_mut().push("handoff".into());
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(*trace.borrow(), [
            "com.epicgames.launcher://fab/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e",
            "com.epicgames.launcher://ue/library", "handoff",
            "com.epicgames.launcher://fab/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e",
        ]);
    }

    #[test]
    fn recovery_rejection_is_not_retried_forever_or_reported_as_success() {
        let attempts = std::cell::Cell::new(0);
        let result = request_epic_navigation(
            Uuid::new_v4(),
            false,
            |url| {
                if url.contains("/listings/") {
                    attempts.set(attempts.get() + 1);
                    return Err("Epic 本次商品导航被拒绝".into());
                }
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(attempts.get(), 2);
    }

    #[test]
    fn launcher_csv_matches_target() {
        assert!(launcher_running_from_tasklist(
            "\"EpicGamesLauncher.exe\",\"1456\",\"Console\",\"1\",\"125,000 K\""
        ));
    }

    #[test]
    fn launcher_empty_output_is_not_running() {
        assert!(!launcher_running_from_tasklist(""));
    }

    #[test]
    fn launcher_localized_no_tasks_is_not_running() {
        assert!(!launcher_running_from_tasklist(
            "信息: 没有运行的任务匹配指定标准。"
        ));
    }

    #[test]
    fn launcher_process_name_is_case_insensitive() {
        assert!(launcher_running_from_tasklist(
            "\"EPICGAMESLAUNCHER.EXE\",\"123\""
        ));
    }

    #[test]
    fn launcher_web_helper_is_not_target() {
        assert!(!launcher_running_from_tasklist(
            "\"EpicWebHelper.exe\",\"123\""
        ));
    }

    #[test]
    fn launcher_not_running_prevents_both_routes() {
        for recover in [false, true] {
            let mut dispatched = Vec::new();
            let error = request_epic_navigation_when_running(
                Uuid::new_v4(),
                recover,
                Ok(false),
                |url| {
                    dispatched.push(url.to_string());
                    Ok(())
                },
                || panic!("must not hand off when Epic is closed"),
            )
            .unwrap_err();
            assert_eq!(
                error,
                "Epic Games Launcher 未运行，请先启动 Epic 并登录后再定位商品"
            );
            assert!(dispatched.is_empty());
        }
    }

    #[test]
    fn launcher_detection_failure_preserves_navigation() {
        let listing_id = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        for recover in [false, true] {
            let trace = std::cell::RefCell::new(Vec::new());
            request_epic_navigation_when_running(
                listing_id,
                recover,
                Err("tasklist blocked".into()),
                |url| {
                    trace.borrow_mut().push(url.to_string());
                    Ok(())
                },
                || {
                    trace.borrow_mut().push("handoff".into());
                    Ok(())
                },
            )
            .unwrap();
            let expected = if recover {
                vec!["com.epicgames.launcher://ue/library", "handoff", "com.epicgames.launcher://fab/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e"]
            } else {
                vec!["com.epicgames.launcher://fab/plugins/egl/listings/778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e"]
            };
            assert_eq!(*trace.borrow(), expected);
        }
    }

    #[test]
    fn repeated_epic_requests_leave_fab_before_each_product_and_wait_for_handoff() {
        let trace = std::cell::RefCell::new(Vec::new());
        let first = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        let second = Uuid::parse_str("5d9d932b-9e6e-42d0-8afc-45249e7535a5").unwrap();
        // The same listing must also navigate away: Epic can reject it as "Already at location".
        for listing in [first, second, second] {
            let url = request_epic_navigation(
                listing,
                true,
                |url| {
                    trace.borrow_mut().push(url.to_string());
                    Ok(())
                },
                || {
                    trace.borrow_mut().push("handoff".into());
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(url, epic_listing_url(listing));
        }
        assert_eq!(
            trace.into_inner(),
            [first, second, second]
                .into_iter()
                .flat_map(|id| [
                    "com.epicgames.launcher://ue/library".to_string(),
                    "handoff".to_string(),
                    epic_listing_url(id),
                ])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn failed_epic_handoff_or_changed_listing_does_not_dispatch_the_product() {
        let listing = Uuid::parse_str("778cd12e-d8bf-49aa-8ac8-e66d2f41ad6e").unwrap();
        let trace = std::cell::RefCell::new(Vec::new());
        let error = request_epic_navigation(
            listing,
            true,
            |url| {
                trace.borrow_mut().push(url.to_string());
                Ok(())
            },
            || Err(LIBRARY_CHANGED.into()),
        )
        .unwrap_err();
        assert_eq!(error, LIBRARY_CHANGED);
        assert_eq!(*trace.borrow(), ["com.epicgames.launcher://ue/library"]);

        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE assets(id TEXT,source_url TEXT,fab_listing_id TEXT,deleted_at TEXT);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO assets VALUES('asset',?1,?2,NULL)",
                [
                    format!("https://www.fab.com/listings/{listing}"),
                    listing.to_string(),
                ],
            )
            .unwrap();
        trace.borrow_mut().clear();
        let error = request_epic_navigation(
            listing,
            true,
            |url| {
                registered_listing(&connection, "asset")?;
                trace.borrow_mut().push(url.to_string());
                Ok(())
            },
            || {
                connection
                    .execute("UPDATE assets SET deleted_at='now'", [])
                    .unwrap();
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.contains("已删除"));
        assert_eq!(*trace.borrow(), ["com.epicgames.launcher://ue/library"]);

        let handoff_called = std::cell::Cell::new(false);
        assert!(request_epic_navigation(
            listing,
            true,
            |_| Err("Epic 协议调用失败".into()),
            || {
                handoff_called.set(true);
                Ok(())
            },
        )
        .is_err());
        assert!(!handoff_called.get());
    }

    #[test]
    fn normal_epic_navigation_never_resets_the_launcher_or_waits_between_products() {
        let mut urls = Vec::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        for id in [a, b, b] {
            request_epic_navigation(
                id,
                false,
                |url| {
                    urls.push(url.to_string());
                    Ok(())
                },
                || panic!("normal navigation must not hand off through the library"),
            )
            .unwrap();
        }
        assert_eq!(urls, [a, b, b].map(epic_listing_url));
    }

    #[test]
    fn epic_url_only_contains_a_validated_database_listing_uuid() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE assets(id TEXT,source_url TEXT,fab_listing_id TEXT,deleted_at TEXT);",
            )
            .unwrap();
        let id = "06003f78-9a59-4fb8-abbc-14dc276f0b4a";
        connection
            .execute(
                "INSERT INTO assets VALUES('asset',?1,?2,NULL)",
                [
                    format!("https://www.fab.com/zh-cn/listings/{id}?lang=zh-CN"),
                    id.into(),
                ],
            )
            .unwrap();
        assert_eq!(
            epic_listing_url(registered_listing(&connection, "asset").unwrap()),
            format!("com.epicgames.launcher://fab/plugins/egl/listings/{id}")
        );
        for source in [
            format!("https://evil.example/listings/{id}"),
            format!("https://user:secret@fab.com/listings/{id}"),
            format!("https://fab.com:8443/listings/{id}"),
            format!("https://fab.com/listings/{id}/install"),
            format!("https://fab.com/anything/listings/{id}"),
            format!("https://fab.com/listings/{}", Uuid::new_v4()),
        ] {
            connection
                .execute("UPDATE assets SET source_url=?1", [source])
                .unwrap();
            assert!(registered_listing(&connection, "asset").is_err());
        }
        connection
            .execute(
                "UPDATE assets SET source_url=?1,fab_listing_id='x?install=yes'",
                [format!("https://fab.com/listings/{id}")],
            )
            .unwrap();
        assert!(registered_listing(&connection, "asset").is_err());
        assert!(registered_listing(&connection, "missing").is_err());
        connection
            .execute(
                "UPDATE assets SET source_url=?1,fab_listing_id=?2,deleted_at='now'",
                [format!("https://fab.com/listings/{id}"), id.into()],
            )
            .unwrap();
        assert!(registered_listing(&connection, "asset").is_err());
    }

    #[test]
    fn reads_the_real_project_engine_without_writing_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Scene.uproject");
        let original = br#"{"FileVersion":3,"EngineAssociation":"5.4.2"}"#;
        std::fs::write(&path, original).unwrap();
        let target = inspect_project(path.to_str().unwrap()).unwrap();
        assert_eq!(target.engine_association.as_deref(), Some("5.4.2"));
        assert_eq!(target.engine_version.as_deref(), Some("5.4"));
        assert!(target.available);
        assert!(target.project_id.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::write(
            &path,
            br#"{"FileVersion":3,"EngineAssociation":"{custom-engine}"}"#,
        )
        .unwrap();
        let target = inspect_project(path.to_str().unwrap()).unwrap();
        assert!(target.engine_version.is_none());
        assert!(!target.warnings.is_empty());
    }

    #[test]
    fn rejects_non_projects_invalid_json_and_oversized_files() {
        let directory = tempfile::tempdir().unwrap();
        let other = directory.path().join("Settings.json");
        std::fs::write(&other, br#"{"FileVersion":3,"EngineAssociation":"5.4"}"#).unwrap();
        assert!(inspect_project(other.to_str().unwrap()).is_err());
        assert!(inspect_project("relative.uproject").is_err());
        let path = directory.path().join("Broken.uproject");
        for content in [b"not json".as_slice(), b"{}", b"[]"] {
            std::fs::write(&path, content).unwrap();
            assert!(inspect_project(path.to_str().unwrap()).is_err());
        }
        std::fs::write(&path, vec![b' '; MAX_PROJECT_BYTES as usize + 1]).unwrap();
        assert!(inspect_project(path.to_str().unwrap())
            .unwrap_err()
            .contains("1 MB"));
    }

    #[test]
    fn registered_projects_are_deduplicated_and_missing_targets_remain_visible() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("Scene.uproject");
        std::fs::write(&project, br#"{"FileVersion":3,"EngineAssociation":"5.4"}"#).unwrap();
        let project = project.to_string_lossy().into_owned();
        let missing = directory
            .path()
            .join("Missing.uproject")
            .to_string_lossy()
            .into_owned();
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE projects(id TEXT,name TEXT); CREATE TABLE project_paths(id TEXT,project_id TEXT,label TEXT,path TEXT,path_type TEXT,sort_order INTEGER); INSERT INTO projects VALUES('p1','Alpha'),('p2','Beta');").unwrap();
        for (id, project_id, label, path, path_type) in [
            ("first", "p1", "Primary shortcut", project.as_str(), "file"),
            (
                "duplicate",
                "p2",
                "Repeated shortcut",
                project.as_str(),
                "file",
            ),
            (
                "missing",
                "p2",
                "Disconnected project",
                missing.as_str(),
                "file",
            ),
            (
                "directory",
                "p1",
                "Not a project file",
                "directory.uproject",
                "directory",
            ),
            ("other", "p1", "Other file", "Settings.json", "file"),
        ] {
            connection
                .execute(
                    "INSERT INTO project_paths VALUES(?1,?2,?3,?4,?5,0)",
                    [id, project_id, label, path, path_type],
                )
                .unwrap();
        }
        let targets = registered_project_targets(&connection).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].project_id.as_deref(), Some("p1"));
        assert_eq!(targets[0].path_id.as_deref(), Some("first"));
        assert_eq!(targets[0].project_name.as_deref(), Some("Alpha"));
        assert_eq!(targets[0].name, "Primary shortcut");
        assert!(targets[0].available);
        assert_eq!(targets[0].engine_version.as_deref(), Some("5.4"));
        assert!(!targets[1].available);
        assert!(!targets[1].warnings.is_empty());
    }

    #[test]
    fn path_identity_does_not_duplicate_the_same_windows_shortcut() {
        assert_eq!(
            project_path_key(" /tmp/Scene.uproject "),
            project_path_key("/tmp/Scene.uproject")
        );
        #[cfg(windows)]
        assert_eq!(
            project_path_key(r"C:\Game\Scene.uproject"),
            project_path_key("c:/game/scene.UPROJECT")
        );
    }
}
