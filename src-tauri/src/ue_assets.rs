//! UE metadata is read by a matched editor, never inferred from a .uasset filename.
use crate::{models::Page, state::AppState};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeJob {
    pub operation: String,
    pub source_id: Option<String>,
    pub engine_root: String,
    #[serde(default)]
    pub asset_ids: Vec<String>,
    pub plan_id: Option<String>,
    pub confirmed: bool,
}
impl NativeJob {
    pub fn validate(&self) -> Result<(), String> {
        if !self.confirmed {
            return Err("请先确认引擎启动、工具安装或工程写入".into());
        }
        if ![
            "prepareTool",
            "index",
            "render",
            "preview",
            "collect",
            "import",
            "installMenu",
        ]
        .contains(&self.operation.as_str())
            || self.asset_ids.len() > 5000
        {
            return Err("UE 操作或选择数量无效".into());
        }
        engine(Path::new(&self.engine_root))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Engine {
    pub id: String,
    pub root: String,
    pub version: String,
    pub build_id: String,
    pub tool_ready: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub original_path: String,
    pub content_root: String,
    pub kind: String,
    pub engine_version: String,
    pub storage_mode: String,
    pub last_indexed: Option<String>,
    pub asset_count: i64,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAsset {
    pub id: String,
    pub source_id: String,
    pub package_name: String,
    pub object_path: String,
    pub name: String,
    pub alias: String,
    pub class_name: String,
    pub relative_path: String,
    pub status: String,
    pub message: String,
    pub favorite: bool,
    pub folder_id: Option<String>,
    pub thumbnail: Option<String>,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Search {
    #[serde(default)]
    pub query: String,
    pub source_id: Option<String>,
    pub folder_id: Option<String>,
    pub class_name: Option<String>,
    #[serde(default)]
    pub favorite_only: bool,
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "page_limit")]
    pub limit: i64,
    #[serde(default)]
    pub object_paths: Vec<String>,
}
fn page_limit() -> i64 {
    80
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn read_json(path: &Path) -> Result<Value, String> {
    let meta = fs::metadata(path).map_err(err)?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err("UE 清单超过安全大小限制".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
}
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let temporary = path.with_extension("partial");
    fs::write(&temporary, serde_json::to_vec(value).map_err(err)?).map_err(err)?;
    fs::rename(temporary, path).map_err(err)
}
fn config_root() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法定位本机设置目录")?)
            .join("DCCAssetLibrary"),
    )
}
fn cache(engine: &Engine) -> Result<PathBuf, String> {
    let mut checksum = Sha256::new();
    checksum.update(include_bytes!(
        "../../ue-tools/StackAssetBridge/Source/StackAssetBridge/Private/StackAssetBridge.cpp"
    ));
    checksum.update(include_bytes!(
        "../../ue-tools/StackAssetBridge/Source/StackAssetBridge/Private/StackThumbnailLibrary.cpp"
    ));
    checksum.update(include_bytes!(
        "../../ue-tools/StackAssetBridge/Source/StackAssetBridge/Public/StackThumbnailLibrary.h"
    ));
    checksum.update(include_bytes!(
        "../../ue-tools/StackAssetBridge/Source/StackAssetBridge/StackAssetBridge.Build.cs"
    ));
    // Keep UBT Intermediate filenames below the Windows 260-character build limit.
    Ok(config_root()?
        .join("ue-bridge")
        .join(&engine.id[..24])
        .join(format!(
            "v0.21-{:.16}",
            format!("{:x}", checksum.finalize())
        )))
}
fn native_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(&value).to_string()
    }
}
pub fn engine(root: &Path) -> Result<Engine, String> {
    let root = root.canonicalize().map_err(|_| "UE 安装目录不存在")?;
    let version = read_json(&root.join("Engine/Build/Build.version"))?;
    let major = version["MajorVersion"].as_u64().ok_or("UE 版本无效")?;
    let minor = version["MinorVersion"].as_u64().ok_or("UE 版本无效")?;
    let patch = version["PatchVersion"].as_u64().ok_or("UE 版本无效")?;
    if major != 5 || minor < 6 {
        return Err("首批工具仅面向 UE 5.6 及以上；不自动降级资产".into());
    }
    if !root
        .join("Engine/Binaries/Win64/UnrealEditor.exe")
        .is_file()
        || !root.join("Engine/Build/BatchFiles/Build.bat").is_file()
    {
        return Err("这不是完整的 Windows UE 编辑器安装".into());
    }
    let modules = read_json(&root.join("Engine/Binaries/Win64/UnrealEditor.modules"))?;
    let build_id = modules["BuildId"]
        .as_str()
        .ok_or("引擎模块版本清单缺失")?
        .to_string();
    let id = format!(
        "{:x}",
        Sha256::digest(format!(
            "{}:{build_id}",
            root.to_string_lossy().to_lowercase()
        ))
    );
    let mut result = Engine {
        id,
        root: native_path(&root),
        version: format!("{major}.{minor}.{patch}"),
        build_id,
        tool_ready: false,
    };
    let binaries = cache(&result)?.join("Plugins/StackAssetBridge/Binaries/Win64");
    result.tool_ready = binaries.join("UnrealEditor-StackAssetBridge.dll").is_file()
        && read_json(&binaries.join("UnrealEditor.modules"))
            .ok()
            .is_some_and(|v| v["BuildId"].as_str() == Some(&result.build_id));
    Ok(result)
}
#[tauri::command]
pub fn list_ue_engines(extra_root: Option<String>) -> Result<Vec<Engine>, String> {
    let mut roots = BTreeSet::new();
    if let Some(root) = extra_root {
        roots.insert(PathBuf::from(root));
    }
    if let Some(program) = std::env::var_os("PROGRAMDATA") {
        if let Ok(installed) = read_json(
            &PathBuf::from(program).join("Epic/UnrealEngineLauncher/LauncherInstalled.dat"),
        ) {
            if let Some(items) = installed["InstallationList"].as_array() {
                for item in items {
                    if let Some(path) = item["InstallLocation"].as_str() {
                        roots.insert(PathBuf::from(path));
                    }
                }
            }
        }
    }
    if let Some(program) = std::env::var_os("ProgramFiles") {
        if let Ok(entries) = fs::read_dir(PathBuf::from(program).join("Epic Games")) {
            for entry in entries.flatten().take(100) {
                if entry.file_name().to_string_lossy().starts_with("UE_") {
                    roots.insert(entry.path());
                }
            }
        }
    }
    Ok(roots
        .into_iter()
        .filter_map(|root| engine(&root).ok())
        .collect())
}
pub fn migrate(db: &Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(err)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS ue_sources(
        id TEXT PRIMARY KEY,name TEXT NOT NULL,original_path TEXT NOT NULL UNIQUE,content_root TEXT NOT NULL,
        kind TEXT NOT NULL,engine_version TEXT NOT NULL,storage_mode TEXT NOT NULL DEFAULT 'reference',
        last_indexed TEXT,warnings_json TEXT NOT NULL DEFAULT '[]');
        CREATE TABLE IF NOT EXISTS ue_folders(id TEXT PRIMARY KEY,parent_id TEXT REFERENCES ue_folders(id),name TEXT NOT NULL,sort_order INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS ue_assets(id TEXT PRIMARY KEY,source_id TEXT NOT NULL REFERENCES ue_sources(id) ON DELETE CASCADE,
        package_name TEXT NOT NULL,object_path TEXT NOT NULL,name TEXT NOT NULL,alias TEXT NOT NULL DEFAULT '',class_name TEXT NOT NULL,
        relative_path TEXT NOT NULL,status TEXT NOT NULL,message TEXT NOT NULL DEFAULT '',favorite INTEGER NOT NULL DEFAULT 0,
        folder_id TEXT REFERENCES ue_folders(id) ON DELETE SET NULL,thumbnail TEXT,dependencies_json TEXT NOT NULL DEFAULT '[]',
        UNIQUE(source_id,object_path));
        CREATE INDEX IF NOT EXISTS idx_ue_source ON ue_assets(source_id,status);
        CREATE TABLE IF NOT EXISTS ue_asset_tags(asset_id TEXT NOT NULL REFERENCES ue_assets(id) ON DELETE CASCADE,tag_id TEXT NOT NULL REFERENCES localized_tags(id) ON DELETE CASCADE,PRIMARY KEY(asset_id,tag_id));
        CREATE TABLE IF NOT EXISTS ue_links(asset_id TEXT NOT NULL REFERENCES ue_assets(id) ON DELETE CASCADE,kind TEXT NOT NULL,entity_id TEXT NOT NULL,PRIMARY KEY(asset_id,kind,entity_id));
        CREATE TABLE IF NOT EXISTS ue_import_plans(id TEXT PRIMARY KEY,expires_at TEXT NOT NULL,plan_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS ue_capture_installations(project_path TEXT PRIMARY KEY,token TEXT NOT NULL,engine_version TEXT NOT NULL);
        UPDATE schema_meta SET value='20' WHERE key='schema_version' AND CAST(value AS INTEGER)<20;") .map_err(err)?;
    tx.commit().map_err(err)
}
fn sources(db: &Connection) -> Result<Vec<Source>, String> {
    let mut stmt=db.prepare("SELECT id,name,original_path,content_root,kind,engine_version,storage_mode,last_indexed,(SELECT count(*) FROM ue_assets a WHERE a.source_id=s.id),warnings_json FROM ue_sources s ORDER BY name COLLATE NOCASE").map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Source {
                id: r.get(0)?,
                name: r.get(1)?,
                original_path: r.get(2)?,
                content_root: r.get(3)?,
                kind: r.get(4)?,
                engine_version: r.get(5)?,
                storage_mode: r.get(6)?,
                last_indexed: r.get(7)?,
                asset_count: r.get(8)?,
                warnings: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
fn source(db: &Connection, id: &str) -> Result<Source, String> {
    sources(db)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("UE 来源不存在".into())
}
fn register(db: &Connection, path: &Path, engine_version: &str) -> Result<Source, String> {
    let path = path.canonicalize().map_err(|_| "工程或资产目录不存在")?;
    let mut warnings: Vec<String> = Vec::new();
    let (kind, content, version) = if path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("uproject"))
    {
        let project = read_json(&path)?;
        let association = project["EngineAssociation"].as_str().unwrap_or("");
        if project["Modules"].as_array().is_some_and(|a| !a.is_empty()) {
            warnings.push(
                "来源包含工程代码；辅助工程不会自动加载这些代码，相关类型可能无法解析".into(),
            );
        }
        if project["Plugins"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v["Enabled"].as_bool() == Some(true)))
        {
            warnings.push("来源启用了插件；插件依赖需要逐项检查，首版不自动执行来源插件".into());
        }
        let version = if association.starts_with("5.") {
            association.to_string()
        } else {
            warnings.push("工程未提供可确认的版本，使用您明确指定的版本".into());
            engine_version.to_string()
        };
        (
            "project",
            path.parent().ok_or("工程目录无效")?.join("Content"),
            version,
        )
    } else if path.is_dir() {
        ("pack", path.clone(), engine_version.to_string())
    } else {
        return Err(
            "请选 .uproject 或保留目录结构的 Content 目录；孤立 .uasset 无法确认包路径与依赖"
                .into(),
        );
    };
    if version_pair(&version)?.0 != 5 {
        return Err("请明确指定来源 UE 版本".into());
    }
    let content = content
        .canonicalize()
        .map_err(|_| "工程 Content 目录不存在")?;
    if content.parent().is_none() {
        return Err("不能将整个磁盘登记为 Content 资产包".into());
    }
    let original = path.to_string_lossy().to_string();
    let existing = db
        .query_row(
            "SELECT id FROM ue_sources WHERE original_path=?1 COLLATE NOCASE",
            [&original],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(err)?;
    let id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
    let name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("UE 资产包");
    db.execute("INSERT INTO ue_sources(id,name,original_path,content_root,kind,engine_version,warnings_json) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET original_path=excluded.original_path",params![id,name,original,content.to_string_lossy(),kind,version,serde_json::to_string(&warnings).map_err(err)?]).map_err(err)?;
    source(db, &id)
}
#[tauri::command]
pub fn list_ue_sources(state: State<AppState>) -> Result<Vec<Source>, String> {
    state.with_library(|db, _| sources(db))
}
#[tauri::command]
pub fn register_ue_source(
    state: State<AppState>,
    path: String,
    engine_version: String,
) -> Result<Source, String> {
    state.with_library(|db, _| register(db, Path::new(&path), &engine_version))
}
fn asset_row(r: &rusqlite::Row) -> rusqlite::Result<NativeAsset> {
    Ok(NativeAsset {
        id: r.get(0)?,
        source_id: r.get(1)?,
        package_name: r.get(2)?,
        object_path: r.get(3)?,
        name: r.get(4)?,
        alias: r.get(5)?,
        class_name: r.get(6)?,
        relative_path: r.get(7)?,
        status: r.get(8)?,
        message: r.get(9)?,
        favorite: r.get(10)?,
        folder_id: r.get(11)?,
        thumbnail: r.get(12)?,
        dependencies: serde_json::from_str(&r.get::<_, String>(13)?).unwrap_or_default(),
        tags: vec![],
    })
}
const ASSET_COLUMNS:&str="id,source_id,package_name,object_path,name,alias,class_name,relative_path,status,message,favorite,folder_id,thumbnail,dependencies_json";
const ASSET_CONDITION:&str="(?1 IS NULL OR source_id=?1) AND (?2 IS NULL OR folder_id IN (WITH RECURSIVE tree(id) AS (SELECT ?2 UNION ALL SELECT f.id FROM ue_folders f JOIN tree t ON f.parent_id=t.id) SELECT id FROM tree)) AND (?3 IS NULL OR class_name=?3) AND (?4=0 OR favorite=1) AND (name LIKE ?5 ESCAPE '\\' OR alias LIKE ?5 ESCAPE '\\' OR package_name LIKE ?5 ESCAPE '\\' OR EXISTS(SELECT 1 FROM ue_asset_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=ue_assets.id AND t.name LIKE ?5 ESCAPE '\\')) AND (?6='[]' OR object_path IN (SELECT value FROM json_each(?6)))";
fn search_values(r: &Search) -> Result<(String, String), String> {
    if r.query.len() > 4000 || r.object_paths.len() > 5000 {
        return Err("搜索词或收录选择超过限制".into());
    }
    Ok((
        format!(
            "%{}%",
            r.query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        ),
        serde_json::to_string(&r.object_paths).map_err(err)?,
    ))
}
fn get_asset(db: &Connection, id: &str) -> Result<NativeAsset, String> {
    let mut item = db
        .query_row(
            &format!("SELECT {ASSET_COLUMNS} FROM ue_assets WHERE id=?1"),
            [id],
            asset_row,
        )
        .optional()
        .map_err(err)?
        .ok_or("UE 资产不存在")?;
    let mut stmt=db.prepare("SELECT t.name FROM localized_tags t JOIN ue_asset_tags a ON a.tag_id=t.id WHERE a.asset_id=?1 ORDER BY t.name").map_err(err)?;
    item.tags = stmt
        .query_map([id], |r| r.get(0))
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(item)
}
#[tauri::command]
pub fn get_ue_asset(state: State<AppState>, id: String) -> Result<NativeAsset, String> {
    state.with_library(|db, _| get_asset(db, &id))
}
fn search(db: &Connection, r: &Search) -> Result<Page<NativeAsset>, String> {
    // ponytail: SQLite LIKE is adequate for the first index; add FTS only after measured slow searches.
    let (pattern, objects) = search_values(r)?;
    let total = db
        .query_row(
            &format!("SELECT count(*) FROM ue_assets WHERE {ASSET_CONDITION}"),
            params![
                r.source_id,
                r.folder_id,
                r.class_name,
                r.favorite_only,
                pattern,
                objects
            ],
            |row| row.get(0),
        )
        .map_err(err)?;
    let mut stmt=db.prepare(&format!("SELECT id FROM ue_assets WHERE {ASSET_CONDITION} ORDER BY COALESCE(NULLIF(alias,''),name) COLLATE NOCASE,id LIMIT ?7 OFFSET ?8")).map_err(err)?;
    let ids = stmt
        .query_map(
            params![
                r.source_id,
                r.folder_id,
                r.class_name,
                r.favorite_only,
                pattern,
                objects,
                r.limit.clamp(1, 500),
                r.offset.max(0)
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(Page {
        items: ids
            .iter()
            .map(|id| get_asset(db, id))
            .collect::<Result<_, _>>()?,
        total,
        offset: r.offset.max(0),
        limit: r.limit.clamp(1, 500),
    })
}
#[tauri::command]
pub fn search_ue_assets(
    state: State<AppState>,
    request: Search,
) -> Result<Page<NativeAsset>, String> {
    state.with_library(|db, _| search(db, &request))
}
#[tauri::command]
pub fn select_ue_asset_ids(
    state: State<AppState>,
    library_id: String,
    request: Search,
) -> Result<Vec<String>, String> {
    state
        .with_library_if_id(&library_id, |db, _| select_ids(db, &request))?
        .ok_or("素材库已切换".into())
}
fn select_ids(db: &Connection, r: &Search) -> Result<Vec<String>, String> {
    let (pattern, objects) = search_values(r)?;
    let mut stmt = db
        .prepare(&format!(
            "SELECT id FROM ue_assets WHERE {ASSET_CONDITION} ORDER BY id LIMIT 5001"
        ))
        .map_err(err)?;
    let ids = stmt
        .query_map(
            params![
                r.source_id,
                r.folder_id,
                r.class_name,
                r.favorite_only,
                pattern,
                objects
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    if ids.len() > 5000 {
        return Err("单次最多选择 5000 个 UE 资产，请缩小筛选范围".into());
    }
    Ok(ids)
}
#[tauri::command]
pub fn update_ue_asset(
    state: State<AppState>,
    id: String,
    alias: String,
    favorite: bool,
    folder_id: Option<String>,
    tags: Vec<String>,
) -> Result<NativeAsset, String> {
    if alias.chars().count() > 200 || tags.len() > 100 {
        return Err("别名或标签数量超过限制".into());
    }
    state.with_library_mut(|db,_|{
        get_asset(db,&id)?;
        if let Some(folder)=&folder_id {if !db.query_row("SELECT EXISTS(SELECT 1 FROM ue_folders WHERE id=?1)",[folder],|r|r.get::<_,bool>(0)).map_err(err)?{return Err("文件夹不存在".into());}}
        let tx=db.transaction().map_err(err)?;
        tx.execute("UPDATE ue_assets SET alias=?1,favorite=?2,folder_id=?3 WHERE id=?4",params![alias.trim(),favorite,folder_id,id]).map_err(err)?;
        tx.execute("DELETE FROM ue_asset_tags WHERE asset_id=?1",[&id]).map_err(err)?;
        for tag in tags.iter().map(|v|v.trim()).filter(|v|!v.is_empty()) {
            if tag.chars().count()>100 {return Err("标签过长".into());}
            let tag_id=Uuid::new_v4().to_string();
            tx.execute("INSERT OR IGNORE INTO localized_tags(id,locale,name,normalized_name) VALUES(?1,'zh-CN',?2,?3)",params![tag_id,tag,tag.to_lowercase()]).map_err(err)?;
            tx.execute("INSERT OR IGNORE INTO ue_asset_tags(asset_id,tag_id) SELECT ?1,id FROM localized_tags WHERE locale='zh-CN' AND normalized_name=?2",params![id,tag.to_lowercase()]).map_err(err)?;
        }
        tx.commit().map_err(err)?;get_asset(db,&id)
    })
}

fn contained(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(err)?;
    let candidate = crate::media::safe_relative(&root, relative)?
        .canonicalize()
        .map_err(err)?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
        return Err("UE 文件超出已确认目录或不存在".into());
    }
    Ok(candidate)
}
fn content_path(source: &Source, base: &Path) -> Result<PathBuf, String> {
    if source.storage_mode == "collected" {
        Ok(base
            .join("ue-library/sources")
            .join(&source.id)
            .join("Content"))
    } else {
        Ok(PathBuf::from(&source.content_root))
    }
}
#[tauri::command]
pub async fn reveal_ue_asset(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app.state::<AppState>().with_library(|db, base| {
            let asset = get_asset(db, &id)?;
            contained(
                &content_path(&source(db, &asset.source_id)?, base)?,
                &asset.relative_path,
            )
        })?;
        Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(err)?;
        Ok(())
    })
    .await
    .map_err(err)?
}
pub fn thumbnail_response(
    db: &Connection,
    base: &Path,
    id: &str,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let asset = get_asset(db, id)?;
    let path = contained(
        &base.join("ue-library"),
        asset.thumbnail.as_deref().ok_or("尚无原生缩略图")?,
    )?;
    let bytes = fs::read(path).map_err(err)?;
    Ok(tauri::http::Response::builder()
        .header("Content-Type", "image/png")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .map_err(err)?)
}

fn tools_root(resources: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ue-tools")];
    if let Some(root) = resources {
        candidates.insert(0, root.join("ue-tools"));
        candidates.push(root.join("_up_/ue-tools"));
    }
    candidates
        .into_iter()
        .find(|p| {
            p.join("bridge.py").is_file()
                && p.join("StackAssetBridge/StackAssetBridge.uplugin")
                    .is_file()
        })
        .ok_or("UE 辅助工具资源缺失，请重新安装测试版".into())
}
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let root = from.canonicalize().map_err(err)?;
    for item in walkdir::WalkDir::new(&root).follow_links(false) {
        let item = item.map_err(err)?;
        let relative = item.path().strip_prefix(&root).map_err(err)?;
        if relative.components().any(|c| {
            ["Intermediate", "Saved", "Binaries"]
                .contains(&c.as_os_str().to_string_lossy().as_ref())
        }) {
            continue;
        }
        if item.file_type().is_symlink() {
            return Err("工具目录含符号链接，未复制".into());
        }
        let target = to.join(relative);
        if item.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(err)?;
        } else {
            crate::media::copy_atomic(item.path(), &target)?;
        }
    }
    Ok(())
}
fn project_manifest() -> Value {
    json!({"FileVersion":3,"DisableEnginePluginsByDefault":true,"Plugins":[
    {"Name":"PythonScriptPlugin","Enabled":true},{"Name":"EditorScriptingUtilities","Enabled":true},
    {"Name":"StackAssetBridge","Enabled":true},{"Name":"ConnecterUEPlugin","Enabled":false}]})
}
fn prepare_tool(
    engine: &Engine,
    tools: &Path,
    progress: &dyn Fn(usize, usize, String, String),
) -> Result<Value, String> {
    if engine.tool_ready {
        return Ok(json!({"message":"匹配当前引擎的辅助工具已就绪"}));
    }
    let root = cache(engine)?;
    if !root.to_string_lossy().is_ascii() {
        return Err(
            "UE 编译工具当前需要英文缓存路径；请使用英文 Windows 用户目录，未修改工程或引擎".into(),
        );
    }
    fs::create_dir_all(root.join("Content")).map_err(err)?;
    copy_tree(
        &tools.join("StackAssetBridge"),
        &root.join("Plugins/StackAssetBridge"),
    )?;
    write_json(&root.join("StackHost.uproject"), &project_manifest())?;
    progress(
        0,
        1,
        engine.version.clone(),
        "编译本机 Editor 辅助工具；不修改 UE 安装".into(),
    );
    let mut build = Command::new("cmd.exe");
    build
        .args(["/d", "/c"])
        .arg(Path::new(&engine.root).join("Engine/Build/BatchFiles/Build.bat"))
        .args(["UnrealEditor", "Win64", "Development"])
        .arg(format!(
            "-Project={}",
            root.join("StackHost.uproject").display()
        ))
        .arg(format!(
            "-plugin={}",
            root.join("Plugins/StackAssetBridge/StackAssetBridge.uplugin")
                .display()
        ))
        .args([
            "-DisablePlugin=ConnecterUEPlugin",
            "-nohotreload",
            "-MaxParallelActions=2",
        ]);
    let output = crate::media::process_output_with_progress(
        &mut build,
        std::time::Duration::from_secs(1800),
        &|| {},
        true,
    )?;
    fs::write(root.join("build.log"), &output.stdout).map_err(err)?;
    if !output.status.success() || !crate::ue_assets::engine(Path::new(&engine.root))?.tool_ready {
        let message = String::from_utf8_lossy(&output.stdout)
            .lines()
            .rev()
            .find(|s| s.contains("error") || s.contains("Error") || s.contains("SDK"))
            .unwrap_or("请检查 VS C++/Windows/.NET Framework SDK 安装")
            .to_string();
        return Err(format!(
            "UE 辅助工具编译失败：{message}。日志：{}",
            root.join("build.log").display()
        ));
    }
    progress(1, 1, engine.version.clone(), "辅助工具就绪".into());
    Ok(json!({"message":"辅助工具已编译","engine":engine.version}))
}
fn matching_engine(source: &Source, engine: &Engine) -> Result<(), String> {
    if source.engine_version.split('.').take(2).collect::<Vec<_>>()
        != engine.version.split('.').take(2).collect::<Vec<_>>()
    {
        return Err(format!(
            "来源版本 {} 与引擎 {} 不匹配；不自动升级或降级来源",
            source.engine_version, engine.version
        ));
    }
    Ok(())
}
fn native_process(
    engine: &Engine,
    project: &Path,
    tools: &Path,
    job: &Value,
    progress: &dyn Fn(usize, usize, String, String),
    preview: bool,
    stop: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let root = project.parent().ok_or("预览工程路径无效")?;
    let manifest = root.join("native-job.json");
    let report = root.join("native-result.json");
    let progress_file = root.join("native-progress.json");
    let mut job = job.clone();
    job["report"] = json!(report);
    job["progress"] = json!(progress_file);
    write_json(&manifest, &job)?;
    let mut command = Command::new(Path::new(&engine.root).join(if preview {
        "Engine/Binaries/Win64/UnrealEditor.exe"
    } else {
        "Engine/Binaries/Win64/UnrealEditor-Cmd.exe"
    }));
    command
        .arg(project)
        .arg(format!(
            "-ExecutePythonScript={}",
            tools.join("bridge.py").display()
        ))
        .arg(format!("-StackJob={}", manifest.display()))
        .args([
            "-nop4",
            "-nosplash",
            "-NoLiveCoding",
            "-stdout",
            "-LogCmds=Global Warning,LogPython Log",
        ])
        .arg(format!("-abslog={}", root.join("native.log").display()));
    if !preview {
        command.args(["-unattended", "-nosound", "-RenderOffscreen"]);
    }
    progress(
        0,
        job["assets"].as_array().map_or(0, |v| v.len()),
        String::new(),
        "启动匹配的 UE；首次着色器编译可能较久".into(),
    );
    if preview {
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = command.spawn().map_err(err)?;
        let started = std::time::Instant::now();
        loop {
            if report.is_file() {
                let report_value = read_json(&report)?;
                if report_value["ok"].as_bool() == Some(true) {
                    return Ok(json!({"opened":true,"pid":child.id(),"project":project}));
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(report_value["error"]
                    .as_str()
                    .unwrap_or("原生预览失败")
                    .into());
            }
            if stop() || started.elapsed() > std::time::Duration::from_secs(900) {
                let mut kill = Command::new("taskkill.exe");
                let _ = kill
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .output();
                let _ = child.kill();
                let _ = child.wait();
                return Err("原生预览已停止或启动超时；缓存未删除".into());
            }
            if child.try_wait().map_err(err)?.is_some() {
                return Err(format!(
                    "UE 未完成原生预览，请检查 {}",
                    root.join("native.log").display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    let current = std::cell::Cell::new(usize::MAX);
    let output = crate::media::process_output_with_progress(
        &mut command,
        std::time::Duration::from_secs(1800),
        &|| {
            if let Ok(value) = read_json(&progress_file) {
                let next = value["current"].as_u64().unwrap_or(0) as usize;
                if next != current.get() {
                    current.set(next);
                    progress(
                        next,
                        value["total"].as_u64().unwrap_or(0) as usize,
                        value["name"].as_str().unwrap_or("").into(),
                        "生成 UE 原生缩略图".into(),
                    );
                }
            }
        },
        true,
    )?;
    let report = read_json(&report).map_err(|_| {
        format!(
            "UE 未生成有效结果（退出码 {:?}）；上次缓存已保留。日志：{}",
            output.status.code(),
            root.join("native.log").display()
        )
    })?;
    if !output.status.success() || report["ok"].as_bool() != Some(true) {
        return Err(format!(
            "{}；日志：{}",
            report["error"].as_str().unwrap_or("UE 原生处理失败"),
            root.join("native.log").display()
        ));
    }
    Ok(report["result"].clone())
}
fn stage_project(engine: &Engine, destination: &Path, tools: &Path) -> Result<PathBuf, String> {
    let binaries = cache(engine)?.join("Plugins/StackAssetBridge/Binaries/Win64");
    let plugin = destination.join("Plugins/StackAssetBridge");
    fs::create_dir_all(plugin.join("Binaries/Win64")).map_err(err)?;
    fs::create_dir_all(destination.join("Content")).map_err(err)?;
    for name in ["UnrealEditor-StackAssetBridge.dll", "UnrealEditor.modules"] {
        crate::media::copy_atomic(
            &binaries.join(name),
            &plugin.join("Binaries/Win64").join(name),
        )?;
    }
    crate::media::copy_atomic(
        &tools.join("StackAssetBridge/StackAssetBridge.uplugin"),
        &plugin.join("StackAssetBridge.uplugin"),
    )?;
    let project = destination.join("StackPreview.uproject");
    write_json(&project, &project_manifest())?;
    Ok(project)
}
fn package_files(
    root: &Path,
    assets: &[NativeAsset],
    selected: &[NativeAsset],
    stop: &dyn Fn() -> bool,
) -> Result<BTreeSet<PathBuf>, String> {
    let mut files = BTreeSet::new();
    let mut queue = selected
        .iter()
        .map(|a| a.package_name.clone())
        .collect::<Vec<_>>();
    let mut visited = HashSet::new();
    let records = assets
        .iter()
        .map(|a| (a.package_name.as_str(), a))
        .collect::<std::collections::HashMap<_, _>>();
    while let Some(package) = queue.pop() {
        if stop() {
            return Err("依赖整理已停止".into());
        }
        if !visited.insert(package.clone()) {
            continue;
        }
        if package.starts_with("/Engine/") || package.starts_with("/Script/") {
            continue;
        }
        let relative = package
            .strip_prefix("/Game/")
            .ok_or_else(|| format!("依赖 {package} 来自插件或未映射挂载点，暂不能迁移"))?;
        let stem = crate::media::safe_relative(root, relative)?;
        let main = if stem.with_extension("uasset").is_file() {
            stem.with_extension("uasset")
        } else {
            stem.with_extension("umap")
        };
        let file = contained(
            root,
            &main.strip_prefix(root).map_err(err)?.to_string_lossy(),
        )?;
        files.insert(file.clone());
        for ext in ["uexp", "ubulk", "uptnl"] {
            let path = file.with_extension(ext);
            if path.exists() {
                files.insert(contained(
                    root,
                    &path.strip_prefix(root).map_err(err)?.to_string_lossy(),
                )?);
            }
        }
        if let Some(asset) = records.get(package.as_str()) {
            queue.extend(asset.dependencies.iter().cloned());
        } else {
            return Err(format!("依赖 {package} 未登记；请先重新读取完整来源"));
        }
        if visited.len() > 100000 {
            return Err("依赖数量超过安全限制".into());
        }
    }
    Ok(files)
}
fn source_assets(db: &Connection, id: &str) -> Result<Vec<NativeAsset>, String> {
    let mut stmt = db
        .prepare("SELECT id FROM ue_assets WHERE source_id=?1")
        .map_err(err)?;
    let ids = stmt
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    ids.iter().map(|id| get_asset(db, id)).collect()
}
fn commit_index(
    db: &mut Connection,
    source: &Source,
    root: &Path,
    report: &Value,
) -> Result<usize, String> {
    let rows = report["assets"].as_array().ok_or("原生索引结果无效")?;
    if rows.len() > 100000 {
        return Err("单来源最多登记 100000 个原生资产".into());
    }
    let mut validated = Vec::new();
    let packages = rows
        .iter()
        .filter_map(|row| row["packageName"].as_str())
        .collect::<HashSet<_>>();
    for row in rows {
        let path = row["relativePath"].as_str().ok_or("原生路径缺失")?;
        contained(root, path)?;
        let package = row["packageName"]
            .as_str()
            .filter(|p| p.starts_with("/Game/") && !p.contains("..") && !p.contains('\\'))
            .ok_or("UE 包名无效")?;
        let object = row["objectPath"]
            .as_str()
            .filter(|p| p.starts_with(&format!("{package}.")))
            .ok_or("UE 对象路径无效")?;
        let class = row["className"].as_str().ok_or("UE 原生类型缺失")?;
        let deps: Vec<String> = serde_json::from_value(row["dependencies"].clone()).map_err(err)?;
        let supported = [
            "StaticMesh",
            "Texture2D",
            "TextureCube",
            "Material",
            "MaterialInstanceConstant",
        ]
        .contains(&class);
        let unavailable = deps.iter().find(|dependency| {
            !dependency.starts_with("/Script/")
                && !dependency.starts_with("/Engine/")
                && (!dependency.starts_with("/Game/") || !packages.contains(dependency.as_str()))
        });
        let message = unavailable
            .map(|dependency| format!("依赖 {dependency} 未登记或来自插件；未标为可用原生预览"))
            .unwrap_or_default();
        let status = if unavailable.is_some() {
            "unresolved"
        } else if supported {
            "ready"
        } else {
            "unsupported"
        };
        validated.push((
            path.to_string(),
            package.to_string(),
            object.to_string(),
            class.to_string(),
            row["assetName"].as_str().unwrap_or("").to_string(),
            deps,
            status,
            message,
        ));
    }
    if let Some(files) = report["unresolvedFiles"].as_array() {
        for file in files {
            let relative = file["relativePath"].as_str().ok_or("未解析包路径缺失")?;
            contained(root, relative)?;
            if !Path::new(relative)
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("uasset") || v.eq_ignore_ascii_case("umap"))
            {
                return Err("未解析文件不是 UE 包".into());
            }
            // A filename is only a display fallback, not a claimed UE class or object path.
            validated.push((
                relative.into(),
                String::new(),
                format!("file:{relative}"),
                "Unknown".into(),
                Path::new(relative)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                vec![],
                "unresolved",
                file["message"]
                    .as_str()
                    .unwrap_or("匹配引擎无法解析此包")
                    .into(),
            ));
        }
    }
    if validated.len() > 100000 {
        return Err("原生条目数量超过安全限制".into());
    }
    let tx = db.transaction().map_err(err)?;
    tx.execute("UPDATE ue_assets SET status='missing',message='来源中已找不到该对象；别名、标签和缓存仍保留' WHERE source_id=?1",[&source.id]).map_err(err)?;
    for (path, package, object, class, name, deps, status, message) in &validated {
        tx.execute("INSERT INTO ue_assets(id,source_id,package_name,object_path,name,class_name,relative_path,status,dependencies_json,message) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(source_id,object_path) DO UPDATE SET name=excluded.name,class_name=excluded.class_name,relative_path=excluded.relative_path,status=excluded.status,message=excluded.message,dependencies_json=excluded.dependencies_json",params![Uuid::new_v4().to_string(),source.id,package,object,name,class,path,status,serde_json::to_string(deps).map_err(err)?,message]).map_err(err)?;
    }
    tx.execute(
        "UPDATE ue_sources SET last_indexed=?1 WHERE id=?2",
        params![Utc::now().to_rfc3339(), source.id],
    )
    .map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(validated.len())
}
pub fn execute(
    db: &mut Connection,
    base: &Path,
    resources: Option<&Path>,
    request: &NativeJob,
    progress: &dyn Fn(usize, usize, String, String),
    stop: &dyn Fn() -> bool,
) -> Result<Value, String> {
    request.validate()?;
    let engine = engine(Path::new(&request.engine_root))?;
    let tools = tools_root(resources)?;
    if request.operation == "prepareTool" {
        return prepare_tool(&engine, &tools, progress);
    }
    if !engine.tool_ready {
        return Err("请先确认并准备匹配引擎的辅助工具".into());
    }
    let source = source(db, request.source_id.as_deref().ok_or("请选择 UE 来源")?)?;
    if request.operation != "import" {
        matching_engine(&source, &engine)?;
    }
    if request.operation == "installMenu" {
        let library = read_json(&base.join("library.json"))?["libraryId"]
            .as_str()
            .ok_or("素材库标识缺失")?
            .to_string();
        return install_menu(db, &source, &engine, &tools, &library);
    }
    let root = content_path(&source, base)?
        .canonicalize()
        .map_err(|_| "来源路径已失效，请重新定位")?;
    if request.operation == "collect" {
        return collect_source(db, base, &source, &root, progress, stop);
    }
    let job_dir = config_root()?
        .join("ue-jobs")
        .join(Uuid::new_v4().to_string());
    let project = stage_project(&engine, &job_dir, &tools)?;
    if request.operation == "index" {
        let report = native_process(
            &engine,
            &project,
            &tools,
            &json!({"operation":"index","contentRoot":native_path(&root)}),
            progress,
            false,
            stop,
        )?;
        if stop() {
            return Err("索引已停止，上次索引与缓存保留".into());
        }
        let count = commit_index(db, &source, &root, &report)?;
        return Ok(json!({"count":count,"message":format!("已读取 {count} 个 UE 原生资产")}));
    }
    let all = source_assets(db, &source.id)?;
    let selected = all
        .iter()
        .filter(|a| request.asset_ids.contains(&a.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty()
        || selected.len() != request.asset_ids.iter().collect::<HashSet<_>>().len()
    {
        return Err("选择已失效或不属于当前来源".into());
    }
    if selected.iter().any(|a| a.status != "ready") {
        return Err("选择包含未支持、失效或尚未解析的原生资产".into());
    }
    let files = package_files(&root, &all, &selected, stop)?;
    let closure_packages = files
        .iter()
        .filter(|p| p.extension().is_some_and(|v| v == "uasset" || v == "umap"))
        .map(|p| {
            format!(
                "/Game/{}",
                p.strip_prefix(&root)
                    .unwrap()
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/")
            )
        })
        .collect::<HashSet<_>>();
    for asset in all
        .iter()
        .filter(|a| closure_packages.contains(&a.package_name))
    {
        for dependency in &asset.dependencies {
            if let Some(module) = dependency.strip_prefix("/Script/") {
                if module.contains(['/', '\\', '.'])
                    || !Path::new(&engine.root)
                        .join("Engine/Binaries/Win64")
                        .join(format!("UnrealEditor-{module}.dll"))
                        .is_file()
                {
                    return Err(format!(
                        "依赖模块 {module} 需要工程代码或插件；未自动执行这些代码"
                    ));
                }
            }
        }
    }
    // Copy only the selected dependency closure, not the entire source project or its executable plugins.
    let files = files.into_iter().collect::<Vec<_>>();
    let mut hashes = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if stop() {
            return Err("准备预览副本已停止".into());
        }
        let relative = file.strip_prefix(&root).map_err(err)?;
        let target = job_dir.join("Content").join(relative);
        let before = crate::file_digest::digest_file(file)?.1;
        crate::media::copy_atomic(file, &target)?;
        if crate::file_digest::digest_file(&target)?.1 != before {
            return Err("复制校验不一致，未启动 UE".into());
        }
        hashes.push((file.clone(), before));
        progress(
            index + 1,
            files.len(),
            relative.to_string_lossy().into(),
            "准备原生副本与依赖；不加载源工程代码".into(),
        );
    }
    let manifest_assets=selected.iter().map(|a|json!({"id":a.id,"packageName":a.package_name,"objectPath":a.object_path,"output":job_dir.join("thumbnails").join(format!("{}.png",a.id))})).collect::<Vec<_>>();
    let mut native_job = json!({"operation":request.operation,"contentRoot":job_dir.join("Content"),"assets":manifest_assets});
    let import_plan = if request.operation == "import" {
        let id = request.plan_id.as_deref().ok_or("请先生成并确认导入计划")?;
        let plan: Value = serde_json::from_str(
            &db.query_row(
                "SELECT plan_json FROM ue_import_plans WHERE id=?1 AND expires_at>?2",
                params![id, Utc::now().to_rfc3339()],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(err)?
            .ok_or("导入计划已失效，请重新确认")?,
        )
        .map_err(err)?;
        if plan["engineId"].as_str() != Some(&engine.id)
            || plan["sourceId"].as_str() != Some(&source.id)
            || plan["ids"] != json!(request.asset_ids)
        {
            return Err("引擎、来源或选择已变化，请重新确认导入计划".into());
        }
        if plan["sourceRoot"].as_str() != Some(native_path(&root).as_str())
            || hashes.len() != plan["files"].as_array().ok_or("导入清单无效")?.len()
            || hashes.iter().any(|(path, checksum)| {
                !plan["files"].as_array().unwrap().iter().any(|file| {
                    file["source"].as_str() == Some(native_path(path).as_str())
                        && file["sha256"].as_str() == Some(checksum)
                })
            })
        {
            return Err("来源目录、依赖或文件变化，请重新确认导入计划".into());
        }
        revalidate_plan(&plan)?;
        let destination = job_dir.join("Migrated/Content");
        fs::create_dir_all(&destination).map_err(err)?;
        write_json(
            &job_dir.join("Migrated/StackTarget.uproject"),
            &json!({"FileVersion":3}),
        )?;
        native_job["operation"] = json!("migrate");
        native_job["destination"] = json!(destination);
        Some(plan)
    } else {
        None
    };
    let result = native_process(
        &engine,
        &project,
        &tools,
        &native_job,
        progress,
        request.operation == "preview",
        stop,
    )?;
    for (path, before) in hashes {
        if crate::file_digest::digest_file(&path)?.1 != before {
            return Err("源包在处理期间被修改，结果未写入素材库，请重新索引".into());
        }
    }
    if let Some(plan) = import_plan {
        if stop() {
            return Err("导入已停止，未发布到目标工程".into());
        }
        let current_root = content_path(&crate::ue_assets::source(db, &source.id)?, base)?
            .canonicalize()
            .map_err(err)?;
        if native_path(&current_root) != plan["sourceRoot"].as_str().unwrap_or("") {
            return Err("来源在处理期间被重新定位，未写入目标工程".into());
        }
        revalidate_plan(&plan)?;
        let count = publish_import(&plan, &job_dir.join("Migrated/Content"))?;
        db.execute(
            "DELETE FROM ue_import_plans WHERE id=?1",
            [request.plan_id.as_deref().unwrap()],
        )
        .map_err(err)?;
        return Ok(
            json!({"message":format!("已导入 {count} 个原生文件及依赖；未覆盖现有文件"),"files":count}),
        );
    }
    if request.operation == "render" {
        let mut succeeded = 0usize;
        let mut failed = 0usize;
        for result in result["results"].as_array().ok_or("缩略图结果无效")? {
            let id = result["id"].as_str().ok_or("缩略图对象无效")?;
            if !selected.iter().any(|a| a.id == id) {
                return Err("缩略图结果不属于选择".into());
            }
            if result["succeeded"].as_bool() != Some(true) {
                failed += 1;
                continue;
            }
            let path = job_dir.join("thumbnails").join(format!("{id}.png"));
            let image = image::open(&path).map_err(err)?;
            if image.width() == 0
                || image.height() == 0
                || image.width() > 4096
                || image.height() > 4096
            {
                return Err("原生缩略图尺寸无效".into());
            }
            let relative = format!("thumbnails/{id}/{}.png", Uuid::new_v4());
            let destination = base.join("ue-library").join(&relative);
            crate::media::copy_atomic(&path, &destination)?;
            db.execute(
                "UPDATE ue_assets SET thumbnail=?1 WHERE id=?2",
                params![relative, id],
            )
            .map_err(err)?;
            succeeded += 1;
        }
        return Ok(
            json!({"succeeded":succeeded,"failed":failed,"message":format!("原生缩略图：成功 {succeeded}，失败 {failed}；未覆盖失败项缓存")}),
        );
    }
    Ok(result)
}

fn collect_source(
    db: &Connection,
    base: &Path,
    source: &Source,
    root: &Path,
    progress: &dyn Fn(usize, usize, String, String),
    stop: &dyn Fn() -> bool,
) -> Result<Value, String> {
    if source.storage_mode == "collected" {
        return Err("此来源已经备份收录，无需重复复制".into());
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(err)?;
        if entry.file_type().is_symlink() {
            return Err("来源含符号链接，请先整理为完整本地目录".into());
        }
        if entry.file_type().is_file()
            && entry.path().extension().is_some_and(|v| {
                ["uasset", "umap", "uexp", "ubulk", "uptnl"]
                    .contains(&v.to_string_lossy().to_ascii_lowercase().as_str())
            })
        {
            files.push(entry.into_path());
        }
        if files.len() > 100000 {
            return Err("来源文件数量超过安全限制".into());
        }
    }
    let parent = base.join("ue-library/sources");
    fs::create_dir_all(&parent).map_err(err)?;
    let destination = parent.join(&source.id);
    if destination.exists() {
        return Err("已有托管目录，未覆盖，请先检查数据".into());
    }
    let staging = tempfile::tempdir_in(&parent).map_err(err)?;
    fs::create_dir(staging.path().join("Content")).map_err(err)?;
    for (index, file) in files.iter().enumerate() {
        if stop() {
            return Err("备份收录已停止，源文件与库内引用未改变".into());
        }
        let relative = file.strip_prefix(root).map_err(err)?;
        let file = contained(root, &relative.to_string_lossy())?;
        let target = staging.path().join("Content").join(relative);
        let checksum = crate::file_digest::digest_file(&file)?.1;
        crate::media::copy_atomic(&file, &target)?;
        if crate::file_digest::digest_file(&file)?.1 != checksum
            || crate::file_digest::digest_file(&target)?.1 != checksum
        {
            return Err("文件在复制期间变化，未收录".into());
        }
        progress(
            index + 1,
            files.len(),
            relative.to_string_lossy().into(),
            "校验并复制全部来源原生文件；外部插件不包含在内".into(),
        );
    }
    fs::rename(staging.path(), &destination).map_err(err)?;
    db.execute(
        "UPDATE ue_sources SET storage_mode='collected' WHERE id=?1",
        [&source.id],
    )
    .map_err(err)?;
    Ok(
        json!({"files":files.len(),"message":format!("已备份收录 {} 个原生文件；未修改外部来源",files.len())}),
    )
}
fn version_pair(value: &str) -> Result<(u32, u32), String> {
    let mut parts = value.split('.');
    Ok((
        parts.next().ok_or("版本缺失")?.parse().map_err(err)?,
        parts.next().ok_or("版本缺失")?.parse().map_err(err)?,
    ))
}
fn install_menu(
    db: &Connection,
    source: &Source,
    engine: &Engine,
    tools: &Path,
    library: &str,
) -> Result<Value, String> {
    Uuid::parse_str(library).map_err(|_| "素材库标识无效")?;
    if source.kind != "project" {
        return Err("右键入口仅安装到已登记的 .uproject，不写入裸 Content 包".into());
    }
    matching_engine(source, engine)?;
    let project = Path::new(&source.original_path)
        .canonicalize()
        .map_err(err)?;
    let project_root = project.parent().ok_or("工程目录无效")?;
    let original = fs::read(&project).map_err(err)?;
    let mut manifest: Value = serde_json::from_slice(&original).map_err(err)?;
    let plugins = project_root.join("Plugins");
    fs::create_dir_all(&plugins).map_err(err)?;
    if !plugins
        .canonicalize()
        .map_err(err)?
        .starts_with(project_root)
    {
        return Err("工程 Plugins 指向外部目录，未安装".into());
    }
    let destination = plugins.join("StackAssetBridge");
    if destination.exists() {
        if !destination
            .canonicalize()
            .map_err(err)?
            .starts_with(project_root)
            || read_json(&destination.join("Resources/stack-library.json"))
                .ok()
                .is_none_or(|v| v["bridge"].as_str() != Some("StackAssetBridge"))
        {
            return Err("已有同名插件不是栈藏安装的，未覆盖".into());
        }
    }
    let staging = tempfile::tempdir_in(&plugins).map_err(err)?;
    fs::create_dir_all(staging.path().join("Binaries/Win64")).map_err(err)?;
    let binaries = cache(engine)?.join("Plugins/StackAssetBridge/Binaries/Win64");
    for name in ["UnrealEditor-StackAssetBridge.dll", "UnrealEditor.modules"] {
        crate::media::copy_atomic(
            &binaries.join(name),
            &staging.path().join("Binaries/Win64").join(name),
        )?;
    }
    crate::media::copy_atomic(
        &tools.join("StackAssetBridge/StackAssetBridge.uplugin"),
        &staging.path().join("StackAssetBridge.uplugin"),
    )?;
    fs::create_dir(staging.path().join("Resources")).map_err(err)?;
    let token = Uuid::new_v4().to_string();
    write_json(
        &staging.path().join("Resources/stack-library.json"),
        &json!({"bridge":"StackAssetBridge","libraryId":library,"token":token}),
    )?;
    if manifest["Plugins"].is_null() {
        manifest["Plugins"] = json!([]);
    }
    let entries = manifest["Plugins"]
        .as_array_mut()
        .ok_or("工程 Plugins 配置无效")?;
    if let Some(entry) = entries
        .iter_mut()
        .find(|p| p["Name"].as_str() == Some("StackAssetBridge"))
    {
        entry["Enabled"] = json!(true);
    } else {
        entries.push(json!({"Name":"StackAssetBridge","Enabled":true}));
    }
    if fs::read(&project).map_err(err)? != original {
        return Err("工程配置在准备期间变化，未安装".into());
    }
    let saved = project_root.join("Saved");
    if saved.exists() && !saved.canonicalize().map_err(err)?.starts_with(project_root) {
        return Err("工程 Saved 指向外部目录，未安装".into());
    }
    let backups = saved.join("StackBridgeBackups");
    fs::create_dir_all(&backups).map_err(err)?;
    if !backups
        .canonicalize()
        .map_err(err)?
        .starts_with(project_root)
    {
        return Err("工程备份目录指向外部位置，未安装".into());
    }
    let backup = backups.join(Uuid::new_v4().to_string());
    fs::create_dir(&backup).map_err(err)?;
    fs::write(
        backup.join(project.file_name().ok_or("工程文件名无效")?),
        &original,
    )
    .map_err(err)?;
    let had_plugin = destination.exists();
    if had_plugin {
        fs::rename(&destination, backup.join("StackAssetBridge")).map_err(err)?;
    }
    if let Err(error) = fs::rename(staging.path(), &destination) {
        if had_plugin {
            let _ = fs::rename(backup.join("StackAssetBridge"), &destination);
        }
        return Err(error.to_string());
    }
    let patched = serde_json::to_vec_pretty(&manifest).map_err(err)?;
    let mut project_written = false;
    let result: Result<(), String> = (|| {
        let mut temporary = tempfile::NamedTempFile::new_in(project_root).map_err(err)?;
        std::io::Write::write_all(temporary.as_file_mut(), &patched).map_err(err)?;
        temporary.as_file().sync_all().map_err(err)?;
        if fs::read(&project).map_err(err)? != original {
            return Err("工程配置在安装期间变化，未写入配置".into());
        }
        temporary.persist(&project).map_err(err)?;
        project_written = true;
        db.execute("INSERT INTO ue_capture_installations(project_path,token,engine_version) VALUES(?1,?2,?3) ON CONFLICT(project_path) DO UPDATE SET token=excluded.token,engine_version=excluded.engine_version",params![native_path(&project),token,source.engine_version]).map_err(err)?;
        Ok(())
    })();
    if let Err(error) = result {
        // Keep the attempted helper in the recovery folder; no recursive deletion of a project plugin.
        let _ = fs::rename(&destination, backup.join("AttemptedStackAssetBridge"));
        if had_plugin {
            let _ = fs::rename(backup.join("StackAssetBridge"), &destination);
        }
        let restore = if !project_written {
            Ok(())
        } else if fs::read(&project).map_err(err)? == patched {
            let mut temporary = tempfile::NamedTempFile::new_in(project_root).map_err(err)?;
            std::io::Write::write_all(temporary.as_file_mut(), &original).map_err(err)?;
            temporary.as_file().sync_all().map_err(err)?;
            temporary.persist(&project).map(|_| ()).map_err(err)
        } else {
            Err("工程配置又被外部修改，未覆盖新的修改".into())
        };
        return Err(format!(
            "安装未完成：{error}。配置恢复：{}。安全副本：{}",
            if restore.is_ok() {
                "成功"
            } else {
                "请手动从安全副本恢复"
            },
            native_path(&backup)
        ));
    }
    Ok(
        json!({"message":format!("已安装右键入口。请重新打开来源工程；原配置与旧辅助工具保存在 {}",native_path(&backup))}),
    )
}
fn capture_root(library: &str) -> Result<PathBuf, String> {
    Uuid::parse_str(library).map_err(|_| "素材库标识无效")?;
    Ok(config_root()?.join("ue-capture").join(library))
}
fn verified_capture(db: &Connection, library: &str, id: &str) -> Result<Value, String> {
    Uuid::parse_str(id).map_err(|_| "收录请求标识无效")?;
    let value = read_json(&capture_root(library)?.join(format!("{id}.json")))?;
    if value["id"].as_str() != Some(id) || value["libraryId"].as_str() != Some(library) {
        return Err("收录请求不属于当前库".into());
    }
    let project = Path::new(value["project"].as_str().ok_or("收录工程无效")?)
        .canonicalize()
        .map_err(err)?;
    let binding:Option<(String,String)>=db.query_row("SELECT token,engine_version FROM ue_capture_installations WHERE project_path=?1 COLLATE NOCASE",[native_path(&project)],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
    let (token, version) = binding.ok_or("收录入口未授权或已重新连接，未登记此请求")?;
    if value["token"].as_str() != Some(&token)
        || version_pair(value["engineVersion"].as_str().ok_or("收录版本无效")?)?
            != version_pair(&version)?
    {
        return Err("收录入口授权或引擎版本已变化，未登记此请求".into());
    }
    let paths = value["objectPaths"].as_array().ok_or("收录选择无效")?;
    if paths.is_empty()
        || paths.len() > 5000
        || paths.iter().any(|p| {
            p.as_str().is_none_or(|p| {
                !p.starts_with("/Game/") || p.contains("..") || p.contains('\\') || p.len() > 2000
            })
        })
    {
        return Err("收录选择无效".into());
    }
    Ok(value)
}
#[tauri::command]
pub fn list_ue_captures(state: State<AppState>, library_id: String) -> Result<Vec<Value>, String> {
    state
        .with_library_if_id(&library_id, |db, _| {
            let root = capture_root(&library_id)?;
            if !root.exists() {
                return Ok(vec![]);
            }
            let mut values = Vec::new();
            for entry in fs::read_dir(root).map_err(err)?.flatten().take(1000) {
                let path = entry.path();
                if path.extension().is_some_and(|v| v == "json") {
                    if let Some(id) = path.file_stem().and_then(|v| v.to_str()) {
                        if let Ok(mut value) = verified_capture(db, &library_id, id) {
                            value.as_object_mut().unwrap().remove("token");
                            values.push(value);
                            if values.len() >= 100 {
                                break;
                            }
                        }
                    }
                }
            }
            Ok(values)
        })?
        .ok_or("素材库已切换".into())
}
#[tauri::command]
pub fn accept_ue_capture(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<Value, String> {
    state
        .with_library_if_id(&library_id, |db, _| {
            let value = verified_capture(db, &library_id, &id)?;
            let source = register(
                db,
                Path::new(value["project"].as_str().unwrap()),
                value["engineVersion"].as_str().ok_or("来源版本缺失")?,
            )?;
            let root = capture_root(&library_id)?;
            fs::create_dir_all(root.join("accepted")).map_err(err)?;
            fs::rename(
                root.join(format!("{id}.json")),
                root.join("accepted").join(format!("{id}.json")),
            )
            .map_err(err)?;
            Ok(json!({"source":source,"objectPaths":value["objectPaths"]}))
        })?
        .ok_or("素材库已切换".into())
}
fn make_import_plan(
    db: &Connection,
    base: &Path,
    ids: &[String],
    target: &Path,
    engine: &Engine,
) -> Result<Value, String> {
    if ids.is_empty() || ids.len() > 5000 {
        return Err("请选择同一来源的 1–5000 个 UE 原生资产".into());
    }
    let selected = ids
        .iter()
        .map(|id| get_asset(db, id))
        .collect::<Result<Vec<_>, _>>()?;
    let source = source(db, &selected[0].source_id)?;
    if selected
        .iter()
        .any(|a| a.source_id != source.id || a.status != "ready")
    {
        return Err("导入要求同一来源且原生类型已确认".into());
    }
    let target = target.canonicalize().map_err(|_| "目标工程不存在")?;
    if !target
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("uproject"))
    {
        return Err("请选择目标 .uproject".into());
    }
    let project = read_json(&target)?;
    let version = project["EngineAssociation"]
        .as_str()
        .ok_or("目标工程版本未确认；请先在 UE 中确认版本")?;
    if version_pair(version)? != version_pair(&engine.version)? {
        return Err("目标工程与选定引擎版本不匹配".into());
    }
    if version_pair(version)? < version_pair(&source.engine_version)? {
        return Err("禁止向较低版本 UE 导入原生资产".into());
    }
    let root = content_path(&source, base)?.canonicalize().map_err(err)?;
    let content = target
        .parent()
        .ok_or("目标路径无效")?
        .join("Content")
        .canonicalize()
        .map_err(|_| "目标 Content 不存在，请先用 UE 创建工程")?;
    if content == root || content.starts_with(base.canonicalize().map_err(err)?) {
        return Err("目标不能是来源或素材库托管目录".into());
    }
    let all = source_assets(db, &source.id)?;
    let files = package_files(&root, &all, &selected, &|| false)?;
    let mut records = Vec::new();
    let mut bytes = 0u64;
    for file in files {
        let relative = file
            .strip_prefix(&root)
            .map_err(err)?
            .to_string_lossy()
            .replace('\\', "/");
        let (size, checksum) = crate::file_digest::digest_file(&file)?;
        bytes += size;
        let target_file = crate::media::safe_relative(&content, &relative)?;
        let conflict = target_file.exists();
        let target_checksum = if conflict {
            Some(crate::file_digest::digest_file(&contained(&content, &relative)?)?.1)
        } else {
            None
        };
        records.push(json!({"source":native_path(&file),"relative":relative,"size":size,"sha256":checksum,"conflict":conflict,"targetChecksum":target_checksum}));
    }
    let conflicts = records.iter().filter(|v| v["conflict"] == true).count();
    Ok(
        json!({"id":Uuid::new_v4().to_string(),"sourceId":source.id,"sourceRoot":native_path(&root),"ids":ids,"engineId":engine.id,
        "targetProject":native_path(&target),"targetChecksum":crate::file_digest::digest_file(&target)?.1,
        "contentRoot":native_path(&content),"files":records,"bytes":bytes,"conflicts":conflicts,
        "upgrade":version_pair(version)?>version_pair(&source.engine_version)?,"sourceVersion":source.engine_version,"targetVersion":version}),
    )
}
#[tauri::command]
pub async fn prepare_ue_import(
    app: AppHandle,
    ids: Vec<String>,
    target_project: String,
    engine_root: String,
) -> Result<Value, String> {
    let (base, path, library) = app.state::<AppState>().with_active_mut(|active| {
        Ok((
            active.base_dir.clone(),
            active.db_path.clone(),
            active.marker.library_id.clone(),
        ))
    })?;
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let db = Connection::open(path).map_err(err)?;
        let engine = engine(Path::new(&engine_root))?;
        make_import_plan(&db, &base, &ids, Path::new(&target_project), &engine)
    })
    .await
    .map_err(err)??;
    app.state::<AppState>()
        .with_library_if_id(&library, |db, _| {
            db.execute(
                "DELETE FROM ue_import_plans WHERE expires_at<=?1",
                [Utc::now().to_rfc3339()],
            )
            .map_err(err)?;
            db.execute(
                "INSERT INTO ue_import_plans(id,expires_at,plan_json) VALUES(?1,?2,?3)",
                params![
                    plan["id"].as_str(),
                    (Utc::now() + chrono::Duration::minutes(30)).to_rfc3339(),
                    serde_json::to_string(&plan).map_err(err)?
                ],
            )
            .map_err(err)?;
            Ok(())
        })?
        .ok_or("素材库已切换，导入计划未保存")?;
    Ok(plan)
}
fn revalidate_plan(plan: &Value) -> Result<(), String> {
    let target = Path::new(plan["targetProject"].as_str().ok_or("目标路径缺失")?);
    if crate::file_digest::digest_file(target)?.1 != plan["targetChecksum"].as_str().unwrap_or("") {
        return Err("目标工程配置变化，请重新确认导入计划".into());
    }
    let root = Path::new(plan["contentRoot"].as_str().ok_or("目标 Content 无效")?)
        .canonicalize()
        .map_err(err)?;
    if native_path(&root) != plan["contentRoot"].as_str().unwrap_or("") {
        return Err("目标目录变化，未导入".into());
    }
    for file in plan["files"].as_array().ok_or("导入文件清单无效")? {
        let source = Path::new(file["source"].as_str().ok_or("来源文件无效")?);
        if crate::file_digest::digest_file(source)?.1 != file["sha256"].as_str().unwrap_or("") {
            return Err("来源文件已变化，请重新索引并确认导入计划".into());
        }
        let target = crate::media::safe_relative(
            &root,
            file["relative"].as_str().ok_or("目标相对路径无效")?,
        )?;
        if target.exists() != file["conflict"].as_bool().unwrap_or(false) {
            return Err("目标文件冲突情况变化，请重新确认".into());
        }
        if target.exists()
            && crate::file_digest::digest_file(&contained(
                &root,
                file["relative"].as_str().unwrap(),
            )?)?
            .1 != file["targetChecksum"]
                .as_str()
                .ok_or("导入计划未记录冲突文件校验值，请重新确认")?
        {
            return Err("已存在的目标文件变化，请重新确认；未覆盖任何文件".into());
        }
        let mut parent = target.parent().ok_or("目标路径无效")?;
        while !parent.exists() {
            parent = parent.parent().ok_or("目标路径无效")?;
        }
        if !parent.canonicalize().map_err(err)?.starts_with(&root) {
            return Err("目标路径包含目录链接，未写入".into());
        }
    }
    Ok(())
}
fn publish_import(plan: &Value, migrated: &Path) -> Result<usize, String> {
    let root = Path::new(plan["contentRoot"].as_str().ok_or("目标无效")?);
    let mut installed = Vec::new();
    let result = (|| {
        for file in plan["files"].as_array().ok_or("文件清单无效")? {
            if file["conflict"].as_bool() == Some(true) {
                continue;
            }
            let relative = file["relative"].as_str().ok_or("文件路径无效")?;
            let from = contained(migrated, relative)?;
            let target = crate::media::safe_relative(root, relative)?;
            fs::create_dir_all(target.parent().ok_or("目标路径无效")?).map_err(err)?;
            if !target
                .parent()
                .unwrap()
                .canonicalize()
                .map_err(err)?
                .starts_with(root.canonicalize().map_err(err)?)
            {
                return Err("目标目录链接不安全".into());
            }
            let temporary =
                tempfile::NamedTempFile::new_in(target.parent().unwrap()).map_err(err)?;
            fs::copy(&from, temporary.path()).map_err(err)?;
            let checksum = crate::file_digest::digest_file(temporary.path())?.1;
            temporary.persist_noclobber(&target).map_err(err)?;
            installed.push((target, checksum));
        }
        Ok(installed.len())
    })();
    if result.is_err() {
        for (path, checksum) in &installed {
            if crate::file_digest::digest_file(path)
                .ok()
                .is_some_and(|v| v.1 == *checksum)
            {
                let _ = fs::remove_file(path);
            }
        }
    }
    result
}

#[tauri::command]
pub fn list_ue_folders(state: State<AppState>) -> Result<Vec<crate::models::Category>, String> {
    state.with_library(|db, _| folders(db))
}
fn folders(db: &Connection) -> Result<Vec<crate::models::Category>, String> {
    let mut statement=db.prepare("SELECT id,name,parent_id,sort_order,(SELECT count(*) FROM ue_assets WHERE folder_id=f.id) FROM ue_folders f ORDER BY parent_id,sort_order,name COLLATE NOCASE").map_err(err)?;
    let rows = statement
        .query_map([], |r| {
            Ok(crate::models::Category {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                sort_order: r.get(3)?,
                asset_count: r.get(4)?,
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
fn validate_parent(db: &Connection, id: &str, parent: Option<&str>) -> Result<(), String> {
    if parent == Some(id) {
        return Err("文件夹不能放入自身".into());
    }
    if let Some(parent) = parent {
        if !db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ue_folders WHERE id=?1)",
                [parent],
                |r| r.get::<_, bool>(0),
            )
            .map_err(err)?
        {
            return Err("目标文件夹不存在".into());
        }
        if db.query_row("WITH RECURSIVE tree(id) AS (SELECT id FROM ue_folders WHERE parent_id=?1 UNION ALL SELECT f.id FROM ue_folders f JOIN tree t ON f.parent_id=t.id) SELECT EXISTS(SELECT 1 FROM tree WHERE id=?2)",params![id,parent],|r|r.get::<_,bool>(0)).map_err(err)?{return Err("文件夹不能放入自己的后代".into());}
    }
    Ok(())
}
#[tauri::command]
pub fn upsert_ue_folder(
    state: State<AppState>,
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
) -> Result<String, String> {
    if name.trim().is_empty() || name.chars().count() > 100 {
        return Err("文件夹名称需要 1–100 个字符".into());
    }
    state.with_library(|db,_|{
        if let Some(id)=&id {if !db.query_row("SELECT EXISTS(SELECT 1 FROM ue_folders WHERE id=?1)",[id],|r|r.get::<_,bool>(0)).map_err(err)?{return Err("文件夹不存在".into());}}
        let id=id.unwrap_or_else(||Uuid::new_v4().to_string());validate_parent(db,&id,parent_id.as_deref())?;
        db.execute("INSERT INTO ue_folders(id,parent_id,name,sort_order) VALUES(?1,?2,?3,(SELECT COALESCE(MAX(sort_order),-1)+1 FROM ue_folders WHERE parent_id IS ?2)) ON CONFLICT(id) DO UPDATE SET name=excluded.name",params![id,parent_id,name.trim()]).map_err(err)?;Ok(id)
    })
}
#[tauri::command]
pub fn move_ue_folder(
    state: State<AppState>,
    request: crate::models::MoveCategoryRequest,
) -> Result<(), String> {
    state.with_library_mut(|db, _| {
        move_folder(
            db,
            &request.id,
            request.target_parent_id.as_deref(),
            request.target_index,
        )
    })
}
fn move_folder(
    db: &mut Connection,
    id: &str,
    parent: Option<&str>,
    index: usize,
) -> Result<(), String> {
    validate_parent(db, id, parent)?;
    let old: Option<String> = db
        .query_row("SELECT parent_id FROM ue_folders WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(err)?
        .ok_or("文件夹不存在")?;
    let tx = db.transaction().map_err(err)?;
    tx.execute(
        "UPDATE ue_folders SET parent_id=?1 WHERE id=?2",
        params![parent, id],
    )
    .map_err(err)?;
    for group in [old.as_deref(), parent] {
        let mut statement=tx.prepare("SELECT id FROM ue_folders WHERE parent_id IS ?1 AND id<>?2 ORDER BY sort_order,name COLLATE NOCASE").map_err(err)?;
        let mut ids = statement
            .query_map(params![group, id], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        drop(statement);
        if group == parent {
            ids.insert(index.min(ids.len()), id.to_string());
        }
        for (position, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE ue_folders SET sort_order=?1 WHERE id=?2",
                params![position as i64, id],
            )
            .map_err(err)?;
        }
    }
    tx.commit().map_err(err)
}
#[tauri::command]
pub fn delete_ue_folder(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|db,_|{
    db.execute("WITH RECURSIVE tree(id) AS (SELECT ?1 UNION ALL SELECT f.id FROM ue_folders f JOIN tree t ON f.parent_id=t.id) DELETE FROM ue_folders WHERE id IN (SELECT id FROM tree)",[id]).map_err(err)?;Ok(())
})
}
#[tauri::command]
pub fn batch_move_ue_assets(
    state: State<AppState>,
    ids: Vec<String>,
    folder_id: Option<String>,
) -> Result<(), String> {
    if ids.len() > 5000 {
        return Err("单次最多移动 5000 个资产".into());
    }
    state.with_library_mut(|db, _| {
        if let Some(folder) = &folder_id {
            if !db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM ue_folders WHERE id=?1)",
                    [folder],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(err)?
            {
                return Err("目标文件夹不存在".into());
            }
        }
        let tx = db.transaction().map_err(err)?;
        for id in ids {
            if tx
                .execute(
                    "UPDATE ue_assets SET folder_id=?1 WHERE id=?2",
                    params![folder_id, id],
                )
                .map_err(err)?
                != 1
            {
                return Err("资产选择已失效，未移动任何资产".into());
            }
        }
        tx.commit().map_err(err)
    })
}
#[tauri::command]
pub fn relocate_ue_source(state: State<AppState>, id: String, path: String) -> Result<(), String> {
    state.with_library(|db, _| {
        let existing = source(db, &id)?;
        if existing.storage_mode == "collected" {
            return Err("托管来源无需重新定位".into());
        }
        let path = Path::new(&path).canonicalize().map_err(|_| "路径不存在")?;
        let content = if existing.kind == "project" {
            let project = read_json(&path)?;
            let association = project["EngineAssociation"].as_str().unwrap_or("");
            if version_pair(association)? != version_pair(&existing.engine_version)? {
                return Err("重新定位的工程版本不同，请登记为新来源".into());
            }
            path.parent().ok_or("工程目录无效")?.join("Content")
        } else {
            path.clone()
        }
        .canonicalize()
        .map_err(|_| "Content 目录不存在")?;
        db.execute(
            "UPDATE ue_sources SET original_path=?1,content_root=?2 WHERE id=?3",
            params![native_path(&path), native_path(&content), id],
        )
        .map_err(err)?;
        Ok(())
    })
}
#[tauri::command]
pub fn get_ue_backup_summary(state: State<AppState>) -> Result<Value, String> {
    state.with_library(|db,_|{
    let values=sources(db)?;Ok(json!({"referenced":values.iter().filter(|s|s.storage_mode=="reference").count(),"collected":values.iter().filter(|s|s.storage_mode=="collected").count()}))
})
}
#[tauri::command]
pub fn snapshot_ue_to_board(
    state: State<AppState>,
    library_id: String,
    ids: Vec<String>,
    board_id: String,
) -> Result<crate::models::ReferenceImageAddReport, String> {
    if ids.is_empty() || ids.len() > 5000 {
        return Err("请选 1–5000 个已有原生缩略图的资产".into());
    }
    if state.reference_window_board_id()?.as_deref() == Some(&board_id) {
        return Err("请先收回悬浮参考板，避免覆盖其编辑状态".into());
    }
    state
        .with_library_mut_if_id(&library_id, |db, base| {
            let paths = ids
                .iter()
                .map(|id| {
                    let asset = get_asset(db, id)?;
                    Ok(native_path(&contained(
                        &base.join("ue-library"),
                        asset
                            .thumbnail
                            .as_deref()
                            .ok_or("选择包含未生成原生缩略图的资产")?,
                    )?))
                })
                .collect::<Result<Vec<_>, String>>()?;
            crate::reference_boards::import_paths(
                db,
                base,
                &board_id,
                &paths,
                &crate::models::ReferencePlacement { x: 0.0, y: 0.0 },
            )
        })?
        .ok_or("素材库已切换".into())
}
#[tauri::command]
pub fn link_ue_entity(
    state: State<AppState>,
    library_id: String,
    asset_ids: Vec<String>,
    kind: String,
    entity_id: String,
    remove: bool,
) -> Result<(), String> {
    if asset_ids.len() > 5000 {
        return Err("单次关联最多 5000 项".into());
    }
    state.with_library_mut_if_id(&library_id,|db,_|{
        let sql=match kind.as_str(){"project"=>"SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)","note"=>"SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL)","board"=>"SELECT EXISTS(SELECT 1 FROM reference_boards WHERE id=?1)","asset"=>"SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",_=>return Err("关联类型无效".into())};
        if !remove && !db.query_row(sql,[&entity_id],|r|r.get::<_,bool>(0)).map_err(err)?{return Err("关联目标不存在或不在当前库".into());}
        let tx=db.transaction().map_err(err)?;
        for id in asset_ids {get_asset(&tx,&id)?;if remove{tx.execute("DELETE FROM ue_links WHERE asset_id=?1 AND kind=?2 AND entity_id=?3",params![id,kind,entity_id]).map_err(err)?;}else{tx.execute("INSERT OR IGNORE INTO ue_links(asset_id,kind,entity_id) VALUES(?1,?2,?3)",params![id,kind,entity_id]).map_err(err)?;}}
        tx.commit().map_err(err)
    })?.ok_or("素材库已切换".into())
}
#[tauri::command]
pub fn get_ue_links(
    state: State<AppState>,
    library_id: String,
    asset_id: Option<String>,
    kind: Option<String>,
    entity_id: Option<String>,
) -> Result<Vec<Value>, String> {
    state.with_library_if_id(&library_id,|db,_|{
    let mut statement=db.prepare("SELECT l.asset_id,l.kind,l.entity_id,COALESCE(NULLIF(a.alias,''),a.name) FROM ue_links l JOIN ue_assets a ON a.id=l.asset_id WHERE (?1 IS NULL OR l.asset_id=?1) AND (?2 IS NULL OR l.kind=?2) AND (?3 IS NULL OR l.entity_id=?3)").map_err(err)?;
    let rows=statement.query_map(params![asset_id,kind,entity_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(err)?.collect::<Result<Vec<_>,_>>().map_err(err)?;
    rows.into_iter().map(|(asset,kind,id,name)|{
        let sql=match kind.as_str(){"project"=>"SELECT name FROM projects WHERE id=?1","note"=>"SELECT title FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL","board"=>"SELECT name FROM reference_boards WHERE id=?1","asset"=>"SELECT name FROM assets WHERE id=?1 AND deleted_at IS NULL",_=>return Err("关联类型无效".into())};
        let label=db.query_row(sql,[&id],|r|r.get::<_,String>(0)).optional().map_err(err)?;
        Ok(json!({"assetId":asset,"assetName":name,"kind":kind,"entityId":id,"label":label.clone().unwrap_or("来源暂不可用".into()),"available":label.is_some()}))
    }).collect()
})?.ok_or("素材库已切换".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_library_endpoints_keep_data_and_reject_unsafe_paths() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        fs::create_dir(dir.path().join("Content")).unwrap();
        fs::write(
            dir.path().join("Test.uproject"),
            r#"{"EngineAssociation":"5.6"}"#,
        )
        .unwrap();
        let saved = register(&db, &dir.path().join("Test.uproject"), "5.6").unwrap();
        assert_eq!(
            register(&db, &dir.path().join("Test.uproject"), "5.6")
                .unwrap()
                .id,
            saved.id
        );
        assert_eq!(sources(&db).unwrap().len(), 1);
        assert!(register(&db, &dir.path().join("isolated.uasset"), "5.6").is_err());
        assert!(contained(dir.path(), "../outside.uasset").is_err());
        assert!(search(&db, &Search::default()).unwrap().items.is_empty());
        assert_eq!(
            db.query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "20"
        );
    }
    #[test]
    fn index_commit_is_atomic_and_preserves_alias_and_cache() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Content");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Mesh.uasset"), b"fixture").unwrap();
        let mut db = crate::db::open_database(&directory.path().join("library.db")).unwrap();
        let source = register(&db, &root, "5.6").unwrap();
        let record = json!({"packageName":"/Game/Mesh","objectPath":"/Game/Mesh.Mesh","assetName":"Mesh","className":"StaticMesh","relativePath":"Mesh.uasset","dependencies":[]});
        assert_eq!(
            commit_index(&mut db, &source, &root, &json!({"assets":[record.clone()]})).unwrap(),
            1
        );
        db.execute(
            "UPDATE ue_assets SET alias='中文别名',thumbnail='last.png'",
            [],
        )
        .unwrap();
        let mut bad = record.clone();
        bad["relativePath"] = json!("../outside.uasset");
        assert!(commit_index(
            &mut db,
            &source,
            &root,
            &json!({"assets":[record.clone(),bad]})
        )
        .is_err());
        let items = search(&db, &Search::default()).unwrap().items;
        assert_eq!(items[0].status, "ready");
        assert_eq!(items[0].alias, "中文别名");
        assert_eq!(items[0].thumbnail.as_deref(), Some("last.png"));
        commit_index(&mut db, &source, &root, &json!({"assets":[record]})).unwrap();
        assert_eq!(
            search(&db, &Search::default()).unwrap().items[0].id,
            items[0].id
        );
        commit_index(&mut db, &source, &root, &json!({"assets":[]})).unwrap();
        assert_eq!(get_asset(&db, &items[0].id).unwrap().status, "missing");
    }
    #[test]
    fn select_all_uses_current_filters_not_the_loaded_page() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Content");
        fs::create_dir(&root).unwrap();
        let mut db = crate::db::open_database(&directory.path().join("library.db")).unwrap();
        let source = register(&db, &root, "5.6").unwrap();
        db.execute_batch("INSERT INTO ue_folders(id,name) VALUES('parent','Parent'); INSERT INTO ue_folders(id,name,parent_id) VALUES('child','Child','parent');").unwrap();
        let tx = db.transaction().unwrap();
        for index in 0..150 {
            let name = format!("Match%{index}");
            tx.execute("INSERT INTO ue_assets(id,source_id,package_name,object_path,name,class_name,relative_path,status,favorite,folder_id) VALUES(?1,?2,?3,?4,?5,'StaticMesh',?6,'ready',?7,'child')", params![format!("asset-{index}"),source.id,format!("/Game/{name}"),format!("/Game/{name}.{name}"),name,format!("{name}.uasset"),index % 2 == 0]).unwrap();
        }
        tx.commit().unwrap();
        let request = Search {
            source_id: Some(source.id),
            folder_id: Some("parent".into()),
            query: "Match%".into(),
            limit: 80,
            ..Default::default()
        };
        let page = search(&db, &request).unwrap();
        assert_eq!(page.total, 150);
        assert_eq!(page.items.len(), 80);
        assert_eq!(select_ids(&db, &request).unwrap().len(), 150);
        let favorites = Search {
            favorite_only: true,
            ..request.clone()
        };
        assert_eq!(select_ids(&db, &favorites).unwrap().len(), 75);
        let receipt = Search {
            object_paths: vec![
                "/Game/Match%100.Match%100".into(),
                "/Game/Match%101.Match%101".into(),
            ],
            ..request.clone()
        };
        assert_eq!(select_ids(&db, &receipt).unwrap().len(), 2);
        assert!(select_ids(
            &db,
            &Search {
                query: "No match".into(),
                ..request
            }
        )
        .unwrap()
        .is_empty());
    }
    #[test]
    fn dependency_closure_does_not_include_unselected_broken_assets() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("Mesh.uasset"), b"mesh").unwrap();
        fs::write(root.join("Material.uasset"), b"material").unwrap();
        let fixture = |name: &str, deps: Vec<String>| NativeAsset {
            id: name.into(),
            source_id: "source".into(),
            package_name: format!("/Game/{name}"),
            object_path: format!("/Game/{name}.{name}"),
            name: name.into(),
            alias: String::new(),
            class_name: "StaticMesh".into(),
            relative_path: format!("{name}.uasset"),
            status: "ready".into(),
            message: String::new(),
            favorite: false,
            folder_id: None,
            thumbnail: None,
            dependencies: deps,
            tags: vec![],
        };
        let mesh = fixture(
            "Mesh",
            vec!["/Game/Material".into(), "/Script/Engine".into()],
        );
        let material = fixture("Material", vec![]);
        let broken = fixture("Missing", vec![]);
        assert_eq!(
            package_files(
                root,
                &[mesh.clone(), material, broken],
                &[mesh.clone()],
                &|| false
            )
            .unwrap()
            .len(),
            2
        );
        let plugin = fixture("Mesh", vec!["/CustomPlugin/Texture".into()]);
        assert!(package_files(root, &[plugin.clone()], &[plugin], &|| false).is_err());
        assert!(package_files(root, &[mesh.clone()], &[mesh], &|| true).is_err());
    }
    #[test]
    fn publish_import_never_overwrites_and_rolls_back_new_files() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let migrated = directory.path().join("migrated");
        fs::create_dir(&target).unwrap();
        fs::create_dir(&migrated).unwrap();
        fs::write(target.join("Existing.uasset"), b"user-owned").unwrap();
        fs::write(migrated.join("New.uasset"), b"new").unwrap();
        fs::write(migrated.join("Existing.uasset"), b"different").unwrap();
        let plan = json!({"contentRoot":target,"files":[{"relative":"Existing.uasset","conflict":true},{"relative":"New.uasset","conflict":false}]});
        assert_eq!(publish_import(&plan, &migrated).unwrap(), 1);
        assert_eq!(
            fs::read(target.join("Existing.uasset")).unwrap(),
            b"user-owned"
        );
        assert!(publish_import(&plan, &migrated).is_err());
        assert_eq!(fs::read(target.join("New.uasset")).unwrap(), b"new");
        fs::write(migrated.join("First.uasset"), b"first").unwrap();
        let invalid = json!({"contentRoot":target,"files":[{"relative":"First.uasset","conflict":false},{"relative":"Missing.uasset","conflict":false}]});
        assert!(publish_import(&invalid, &migrated).is_err());
        assert!(!target.join("First.uasset").exists());
    }
    #[test]
    fn import_plan_detects_modified_existing_target_files() {
        let directory = tempfile::tempdir().unwrap();
        let content = directory.path().join("Content");
        fs::create_dir(&content).unwrap();
        let project = directory.path().join("Target.uproject");
        fs::write(&project, b"{}").unwrap();
        let source = directory.path().join("source.uasset");
        fs::write(&source, b"source").unwrap();
        let target = content.join("Mesh.uasset");
        fs::write(&target, b"existing").unwrap();
        let plan = json!({"targetProject":project,"targetChecksum":crate::file_digest::digest_file(&project).unwrap().1,"contentRoot":native_path(&content.canonicalize().unwrap()),"files":[{"source":source,"sha256":crate::file_digest::digest_file(&source).unwrap().1,"relative":"Mesh.uasset","conflict":true,"targetChecksum":crate::file_digest::digest_file(&target).unwrap().1}]});
        revalidate_plan(&plan).unwrap();
        fs::write(&target, b"changed-existing").unwrap();
        assert!(revalidate_plan(&plan).is_err());
        assert_eq!(fs::read(target).unwrap(), b"changed-existing");
    }
    #[test]
    fn missing_dependencies_and_unparsed_packages_are_not_ready() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Content");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Mesh.uasset"), b"mesh").unwrap();
        fs::write(root.join("Broken.uasset"), b"broken").unwrap();
        let mut db = crate::db::open_database(&directory.path().join("library.db")).unwrap();
        let source = register(&db, &root, "5.6").unwrap();
        let report = json!({"assets":[{"packageName":"/Game/Mesh","objectPath":"/Game/Mesh.Mesh","assetName":"Mesh","className":"StaticMesh","relativePath":"Mesh.uasset","dependencies":["/Game/Missing"]}],"unresolvedFiles":[{"relativePath":"Broken.uasset"}]});
        commit_index(&mut db, &source, &root, &report).unwrap();
        let values = source_assets(&db, &source.id).unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(|a| a.status == "unresolved"));
        assert!(values
            .iter()
            .any(|a| a.class_name == "Unknown" && a.package_name.is_empty()));
    }
    #[test]
    fn folder_move_rejects_cycles_and_orders_siblings() {
        let directory = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&directory.path().join("library.db")).unwrap();
        db.execute_batch("INSERT INTO ue_folders(id,name,sort_order) VALUES('root','Root',0),('other','Other',1);INSERT INTO ue_folders(id,parent_id,name) VALUES('child','root','Child');").unwrap();
        assert!(move_folder(&mut db, "root", Some("child"), 0).is_err());
        move_folder(&mut db, "child", None, 0).unwrap();
        assert_eq!(folders(&db).unwrap()[0].id, "child");
    }
    #[test]
    #[ignore = "Requires explicit approval, installed UE 5.6 and the disposable native fixture; no online services"]
    fn real_native_editor_pipeline() {
        let engine_root = std::env::var("STACK_UE_TEST_ENGINE")
            .expect("Set STACK_UE_TEST_ENGINE after confirming UE launch");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tools/ue-native-smoke/Smoke.uproject");
        let directory = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&directory.path().join("library.db")).unwrap();
        let request =
            |operation: &str, source_id: Option<String>, asset_ids: Vec<String>| NativeJob {
                operation: operation.into(),
                source_id,
                engine_root: engine_root.clone(),
                asset_ids,
                plan_id: None,
                confirmed: true,
            };
        execute(
            &mut db,
            directory.path(),
            None,
            &request("prepareTool", None, vec![]),
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        let source = register(&db, &fixture, "5.6").unwrap();
        let indexed = execute(
            &mut db,
            directory.path(),
            None,
            &request("index", Some(source.id.clone()), vec![]),
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(indexed["count"], 4);
        let assets = source_assets(&db, &source.id).unwrap();
        let ids = assets.iter().map(|a| a.id.clone()).collect();
        let thumbnails = execute(
            &mut db,
            directory.path(),
            None,
            &request("render", Some(source.id.clone()), ids),
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(thumbnails["succeeded"], 4);
        for asset in &assets {
            assert!(
                thumbnail_response(&db, directory.path(), &asset.id)
                    .unwrap()
                    .body()
                    .len()
                    > 2000
            );
        }
        let target = tempfile::tempdir().unwrap();
        fs::create_dir(target.path().join("Content")).unwrap();
        let target_project = target.path().join("Target.uproject");
        write_json(
            &target_project,
            &json!({"FileVersion":3,"EngineAssociation":"5.6"}),
        )
        .unwrap();
        let installed = engine(Path::new(&engine_root)).unwrap();
        let ids = assets
            .iter()
            .filter(|a| a.class_name == "StaticMesh" || a.class_name == "Texture2D")
            .map(|a| a.id.clone())
            .collect::<Vec<_>>();
        let plan =
            make_import_plan(&db, directory.path(), &ids, &target_project, &installed).unwrap();
        assert_eq!(plan["files"].as_array().unwrap().len(), 4);
        db.execute(
            "INSERT INTO ue_import_plans(id,expires_at,plan_json) VALUES(?1,?2,?3)",
            params![
                plan["id"].as_str(),
                (Utc::now() + chrono::Duration::minutes(30)).to_rfc3339(),
                plan.to_string()
            ],
        )
        .unwrap();
        let mut job = request("import", Some(source.id), ids);
        job.plan_id = Some(plan["id"].as_str().unwrap().into());
        let imported = execute(
            &mut db,
            directory.path(),
            None,
            &job,
            &|_, _, _, _| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(imported["files"], 4);
        let target_source = register(&db, &target_project, "5.6").unwrap();
        assert_eq!(
            execute(
                &mut db,
                directory.path(),
                None,
                &request("index", Some(target_source.id.clone()), vec![]),
                &|_, _, _, _| {},
                &|| false
            )
            .unwrap()["count"],
            4
        );
        let target_ids = source_assets(&db, &target_source.id)
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(
            execute(
                &mut db,
                directory.path(),
                None,
                &request("render", Some(target_source.id), target_ids),
                &|_, _, _, _| {},
                &|| false
            )
            .unwrap()["succeeded"],
            4
        );
        install_menu(
            &db,
            &register(&db, &target_project, "5.6").unwrap(),
            &installed,
            &tools_root(None).unwrap(),
            &Uuid::new_v4().to_string(),
        )
        .unwrap();
        assert!(target
            .path()
            .join("Plugins/StackAssetBridge/Binaries/Win64/UnrealEditor-StackAssetBridge.dll")
            .is_file());
        assert_eq!(
            read_json(&target_project).unwrap()["Plugins"][0]["Enabled"],
            true
        );
        assert_eq!(
            fs::read_dir(target.path().join("Saved/StackBridgeBackups"))
                .unwrap()
                .count(),
            1
        );
    }
}
