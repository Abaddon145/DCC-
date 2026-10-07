use crate::{fab, state::AppState};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{collections::HashSet, fs::File, io::Read, path::Path};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const MAX_PROJECT_BYTES: u64 = 1024 * 1024;
const LIBRARY_CHANGED: &str = "素材库已切换，请重新打开素材的 Epic 添加入口";

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
    pub opened: bool,
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
    format!("com.epicgames.launcher://ue/Fab/product/{listing_id}")
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
) -> Result<EpicOpenResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        in_library(&app.state::<AppState>(), &expected_library_id, |connection, _| {
        let listing_id = registered_listing(connection, &asset_id)?;
        check_epic_protocol()?;
        let url = epic_listing_url(listing_id);
        open::that(&url).map_err(|error| format!("无法打开 Epic 商品入口：{error}"))?;
        Ok(EpicOpenResult {
            listing_id: listing_id.to_string(),
            url,
            opened: true,
            message: "已打开 Epic 商品入口，请在 Epic 中核对素材版本、选择目标工程并点击 Add to Project。尚未发起下载或确认安装。".into(),
        })
        })
    })
    .await
    .map_err(|error| format!("Epic 商品入口任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

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
            format!("com.epicgames.launcher://ue/Fab/product/{id}")
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
