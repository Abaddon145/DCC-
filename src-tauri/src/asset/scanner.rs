use super::models::*;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn kind(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" => Some("image"),
        "mp4" | "mov" | "mkv" | "webm" => Some("video"),
        "mp3" | "wav" | "flac" => Some("audio"),
        _ => None,
    }
}
pub fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn check_ancestors(path: &Path) -> Result<(), String> {
    if path.ancestors().any(is_link) {
        return Err("不接受符号链接、junction 或重解析目录".into());
    }
    Ok(())
}
fn belongs_to_library(path: &Path) -> bool {
    path.ancestors()
        .any(|parent| parent.join("library.json").is_file() && parent.join("library.db").is_file())
}
pub fn validate_source(path: &Path, library: &Path) -> Result<PathBuf, String> {
    check_ancestors(path)?;
    let path = path.canonicalize().map_err(|_| "源文件不存在或无法访问")?;
    let library = library.canonicalize().map_err(|e| e.to_string())?;
    if !path.is_file() || path.starts_with(&library) || belongs_to_library(&path) {
        return Err("请选择素材库托管目录之外的普通文件".into());
    }
    if path
        .components()
        .any(|c| matches!(c,Component::Normal(n) if n==".git"||n==".dcc"))
    {
        return Err("不导入版本库或运行时目录".into());
    }
    Ok(path)
}
pub fn manifest_path(base: &Path, task_id: &str) -> Result<PathBuf, String> {
    let id = Uuid::parse_str(task_id)
        .map_err(|_| "扫描任务 ID 无效")?
        .to_string();
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = base
        .join(".dcc/runtime/scans")
        .join(id)
        .join("manifest.jsonl");
    check_ancestors(&path)?;
    Ok(path)
}
pub fn scan(
    root: &Path,
    library_base: &Path,
    manifest: &Path,
    progress: &dyn Fn(usize),
    stop: &dyn Fn() -> bool,
) -> Result<ScanSummary, String> {
    check_ancestors(root)?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("扫描目录不可访问：{e}"))?;
    let library = library_base.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() || root.starts_with(&library) || belongs_to_library(&root) {
        return Err("请选择素材库之外的扫描目录".into());
    }
    let parent = manifest.parent().ok_or("扫描清单目录无效")?;
    check_ancestors(parent)?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stage = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(stage.as_file());
    serde_json::to_writer(
        &mut writer,
        &serde_json::json!({"root":root.to_string_lossy()}),
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer).map_err(|e| e.to_string())?;
    let mut summary = ScanSummary::default();
    let mut hashes = HashSet::new();
    let walker = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if is_link(entry.path()) {
                return false;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git" | ".dcc" | ".tmp" | "node_modules" | "target"
            ) || entry.path().starts_with(&library)
            {
                return false;
            }
            !(entry.file_type().is_dir()
                && entry.path().join("library.json").is_file()
                && entry.path().join("library.db").is_file())
        });
    for entry in walker {
        if stop() {
            summary.cancelled = true;
            break;
        }
        let mut row = match entry {
            Ok(entry) if entry.file_type().is_file() => {
                let path = entry.path();
                ScanRow {
                    id: Uuid::new_v4().to_string(),
                    path: path.to_string_lossy().into_owned(),
                    relative_path: path
                        .strip_prefix(&root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned(),
                    kind: kind(path).map(str::to_string),
                    size: 0,
                    hash: String::new(),
                    status: "ready".into(),
                    message: String::new(),
                    duplicate_asset_id: None,
                }
            }
            Ok(_) => continue,
            Err(error) => ScanRow {
                id: Uuid::new_v4().to_string(),
                path: error.path().unwrap_or(&root).to_string_lossy().into_owned(),
                relative_path: String::new(),
                kind: None,
                size: 0,
                hash: String::new(),
                status: "error".into(),
                message: format!("无法读取：{error}"),
                duplicate_asset_id: None,
            },
        };
        if row.status == "error" {
            summary.failed += 1;
        } else if row.kind.is_none() {
            row.status = "unsupported".into();
            row.message = if matches!(
                Path::new(&row.path).extension().and_then(|e| e.to_str()),
                Some("uasset" | "umap")
            ) {
                "UE 格式为预留类型，本阶段不解析或导入"
            } else {
                "不在本阶段扫描格式范围内"
            }
            .into();
            summary.unsupported += 1;
        } else {
            match crate::file_digest::digest_file_controlled(Path::new(&row.path), stop) {
                Ok((size, hash)) => {
                    row.size = size;
                    row.hash = hash;
                    if hashes.insert(row.hash.clone()) {
                        summary.importable += 1;
                    } else {
                        row.status = "duplicate".into();
                        row.message = "扫描目录中存在相同内容".into();
                        summary.duplicate += 1;
                    }
                }
                Err(error) => {
                    if stop() {
                        summary.cancelled = true;
                        break;
                    }
                    row.status = "error".into();
                    row.message = error;
                    summary.failed += 1;
                }
            }
        }
        serde_json::to_writer(&mut writer, &row).map_err(|e| e.to_string())?;
        writeln!(writer).map_err(|e| e.to_string())?;
        summary.total += 1;
        progress(summary.total);
    }
    writer.flush().map_err(|e| e.to_string())?;
    drop(writer);
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage.persist(manifest).map_err(|e| e.to_string())?;
    Ok(summary)
}
pub fn visit_manifest(
    manifest: &Path,
    visitor: &mut dyn FnMut(&Path, ScanRow) -> Result<(), String>,
) -> Result<usize, String> {
    check_ancestors(manifest)?;
    let mut reader =
        BufReader::new(File::open(manifest).map_err(|e| format!("扫描清单不存在：{e}"))?);
    let mut line = String::new();
    Read::by_ref(&mut reader)
        .take(128 * 1024 + 1)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if line.len() > 128 * 1024 {
        return Err("扫描清单头超出大小限制".into());
    }
    let header: serde_json::Value = serde_json::from_str(&line).map_err(|_| "扫描清单头损坏")?;
    let root = PathBuf::from(header["root"].as_str().ok_or("扫描目录记录缺失")?);
    if !root.is_absolute() {
        return Err("扫描目录记录无效".into());
    }
    let mut count = 0;
    loop {
        let mut line = String::new();
        if Read::by_ref(&mut reader)
            .take(128 * 1024 + 1)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        if line.len() > 128 * 1024 {
            return Err("扫描记录超出大小限制".into());
        }
        let row: ScanRow = serde_json::from_str(&line).map_err(|_| "扫描清单损坏")?;
        Uuid::parse_str(&row.id).map_err(|_| "扫描条目 ID 损坏")?;
        let relative = Path::new(&row.relative_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            if row.status != "error" {
                return Err("扫描条目路径不安全".into());
            }
        }
        if row.status != "error" && root.join(relative) != Path::new(&row.path) {
            return Err("扫描条目越过原目录".into());
        }
        visitor(&root, row)?;
        count += 1;
    }
    Ok(count)
}
pub fn read_page(manifest: &Path, offset: usize, limit: usize) -> Result<Page<ScanRow>, String> {
    let limit = limit.clamp(1, 200);
    let mut items = Vec::with_capacity(limit);
    let mut index = 0;
    // ponytail: sequential manifest pages avoid retaining the whole tree; add an offsets index only if huge scans make page reads slow.
    let total = visit_manifest(manifest, &mut |_, row| {
        if index >= offset && items.len() < limit {
            items.push(row);
        }
        index += 1;
        Ok(())
    })?;
    Ok(Page {
        items,
        total,
        offset,
        limit,
    })
}
