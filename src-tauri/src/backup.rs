use crate::{db, state::ActiveLibrary};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::tempdir;
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    schema_version: u32,
    created_at: String,
    entries: Vec<ManifestEntry>,
}

#[derive(Serialize, Deserialize)]
struct ManifestEntry {
    path: String,
    size: u64,
    sha256: String,
}

pub fn export_library(
    connection: &Connection,
    base_dir: &Path,
    destination: &Path,
) -> Result<(), String> {
    export_library_with_progress(connection, base_dir, destination, &|_, _, _, _| {})
}

pub fn export_library_with_progress(
    connection: &Connection,
    base_dir: &Path,
    destination: &Path,
    progress: &dyn Fn(usize, usize, String, String),
) -> Result<(), String> {
    if destination
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.eq_ignore_ascii_case("dccassetlib"))
        != Some(true)
    {
        return Err("备份输出必须为 .dccassetlib 文件".into());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let stage = tempdir().map_err(|e| e.to_string())?;
    let snapshot = stage.path().join("library.db");
    let snapshot_text = snapshot.to_string_lossy().to_string();
    connection
        .execute("VACUUM INTO ?1", [snapshot_text])
        .map_err(|e| format!("创建数据库快照失败：{e}"))?;
    let mut sources: Vec<(PathBuf, String)> = vec![(snapshot, "library.db".into())];
    for managed_dir in [
        "images",
        "reference-boards",
        "media-library",
        "media",
        "note-attachments",
        ".dcc/cache/thumbnails",
    ] {
        let directory = base_dir.join(managed_dir);
        if !directory.exists() {
            continue;
        }
        for entry in WalkDir::new(&directory)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let relative = entry
                .path()
                .strip_prefix(base_dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            sources.push((entry.path().to_path_buf(), relative));
        }
    }
    let mut entries = Vec::new();
    for (index, (source, name)) in sources.iter().enumerate() {
        progress(index, sources.len(), name.clone(), "生成校验清单".into());
        let (size, sha256) = crate::file_digest::digest_file(source)?;
        entries.push(ManifestEntry {
            path: name.clone(),
            size,
            sha256,
        });
    }
    let manifest = Manifest {
        format: "dcc-asset-library".into(),
        schema_version: 1,
        created_at: Utc::now().to_rfc3339(),
        entries,
    };
    let temp_output = destination.with_extension("dccassetlib.tmp");
    let file = fs::File::create(&temp_output).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (index, (source, name)) in sources.iter().enumerate() {
        progress(index, sources.len(), name.clone(), "压缩备份".into());
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut zip).map_err(|e| e.to_string())?;
    }
    zip.start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    fs::rename(temp_output, destination).map_err(|e| e.to_string())?;
    progress(
        sources.len(),
        sources.len(),
        String::new(),
        "备份已完成".into(),
    );
    Ok(())
}

pub const RESTORE_MARKER: &str = ".restore-pending.json";
const RESTORE_TARGETS: [&str; 7] = [
    "library.db",
    "images",
    "reference-boards",
    "media-library",
    "media",
    "note-attachments",
    ".dcc/cache/thumbnails",
];
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreCheckpoint {
    rollback: String,
    originals: Vec<String>,
}

pub fn recover_interrupted_restore(
    active: &mut ActiveLibrary,
    before_migration: impl FnOnce(&Path),
) -> Result<(), String> {
    let marker = active.base_dir.join(RESTORE_MARKER);
    let value: RestoreCheckpoint =
        serde_json::from_slice(&fs::read(&marker).map_err(|_| "没有可恢复的中断记录")?)
            .map_err(|_| "恢复记录损坏，未修改文件；请使用恢复前安全备份")?;
    if !value.rollback.starts_with(".restore-old-")
        || Path::new(&value.rollback).components().count() != 1
        || !value.originals.iter().any(|name| name == "library.db")
        || value
            .originals
            .iter()
            .any(|n| !RESTORE_TARGETS.contains(&n.as_str()))
    {
        return Err("恢复记录路径无效，未修改文件".into());
    }
    let old = active.base_dir.join(&value.rollback);
    if !old.is_dir()
        || fs::symlink_metadata(&old)
            .map_err(|_| "原库副本不可用")?
            .file_type()
            .is_symlink()
    {
        return Err("原库副本不可用，请使用恢复前安全备份".into());
    }
    if value
        .originals
        .iter()
        .any(|name| !old.join(name).exists() && !active.base_dir.join(name).exists())
    {
        return Err("原库文件缺失，未继续恢复；请使用恢复前安全备份".into());
    }
    drop(active.connection.take());
    if old.join("library.db").exists() {
        remove_file_if_exists(&active.base_dir.join("library.db-wal"))?;
        remove_file_if_exists(&active.base_dir.join("library.db-shm"))?;
    }
    for name in RESTORE_TARGETS {
        let target = active.base_dir.join(name);
        let original = old.join(name);
        if original.exists() || !value.originals.iter().any(|v| v == name) {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if target.is_dir() {
                remove_dir_if_exists(&target)?
            } else {
                remove_file_if_exists(&target)?
            }
            if original.exists() {
                fs::rename(original, target).map_err(|_| "恢复原库文件失败，已保留中断记录")?
            }
        }
    }
    if !active.db_path.is_file() {
        return Err("原库数据库缺失，已保留中断记录".into());
    }
    before_migration(&active.base_dir);
    active.connection = Some(db::open_database(&active.db_path)?);
    fs::remove_file(marker).map_err(|_| "原库已恢复，但中断标记未能清除")?;
    Ok(())
}

pub fn restore_library(active: &mut ActiveLibrary, source: &Path) -> Result<(), String> {
    if active.base_dir.join(RESTORE_MARKER).exists() {
        return Err("上次恢复中断，请先恢复原库".into());
    }
    if !source.is_file() {
        return Err("备份文件不存在".into());
    }
    let stage = tempdir().map_err(|e| e.to_string())?;
    extract_and_verify(source, stage.path())?;
    let prepared = tempfile::Builder::new()
        .prefix(".restore-new-")
        .tempdir_in(&active.base_dir)
        .map_err(|e| e.to_string())?;
    let prepared_db = prepared.path().join("library.db");
    fs::copy(stage.path().join("library.db"), &prepared_db)
        .map_err(|e| format!("备份数据库缺失：{e}"))?;
    let check = db::open_database(&prepared_db)?;
    let integrity: String = check
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("备份数据库完整性检查失败：{integrity}"));
    }
    check
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| e.to_string())?;
    drop(check);
    let directories = [
        "images",
        "reference-boards",
        "media-library",
        "media",
        "note-attachments",
        ".dcc/cache/thumbnails",
    ];
    for name in directories {
        if stage.path().join(name).exists() {
            copy_tree(&stage.path().join(name), &prepared.path().join(name))?;
        } else {
            fs::create_dir_all(prepared.path().join(name)).map_err(|e| e.to_string())?;
        }
    }
    let safety = active.base_dir.join("safety-backups").join(format!(
        "恢复前-{}-{}.dccassetlib",
        Utc::now().format("%Y%m%d-%H%M%S"),
        uuid::Uuid::new_v4()
    ));
    let current = active.connection.as_ref().ok_or("数据库正在维护")?;
    export_library(current, &active.base_dir, &safety)?;
    current
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|e| e.to_string())?;
    let rollback = tempfile::Builder::new()
        .prefix(".restore-old-")
        .tempdir_in(&active.base_dir)
        .map_err(|e| e.to_string())?;
    let checkpoint = RestoreCheckpoint {
        rollback: rollback
            .path()
            .file_name()
            .ok_or("恢复路径无效")?
            .to_string_lossy()
            .into_owned(),
        originals: RESTORE_TARGETS
            .iter()
            .filter(|name| active.base_dir.join(name).exists())
            .map(|name| name.to_string())
            .collect(),
    };
    crate::reliability::write_json(&active.base_dir.join(RESTORE_MARKER), &checkpoint)?;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut installed: Vec<PathBuf> = Vec::new();
    drop(active.connection.take());
    let targets = std::iter::once(("library.db", active.db_path.clone()))
        .chain(
            directories
                .into_iter()
                .map(|name| (name, active.base_dir.join(name))),
        )
        .collect::<Vec<_>>();
    let install = (|| -> Result<(), String> {
        for (name, target) in &targets {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if target.exists() {
                let old = rollback.path().join(name);
                if let Some(parent) = old.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::rename(target, &old).map_err(|e| format!("准备恢复 {name} 失败：{e}"))?;
                moved.push((target.clone(), old));
            }
            fs::rename(prepared.path().join(name), target)
                .map_err(|e| format!("安装恢复 {name} 失败：{e}"))?;
            installed.push(target.clone());
        }
        active.connection = Some(db::open_database(&active.db_path)?);
        Ok(())
    })();
    if let Err(error) = install {
        drop(active.connection.take());
        let restore_old = (|| -> Result<(), String> {
            for path in installed.iter().rev() {
                if path.is_dir() {
                    remove_dir_if_exists(path)?
                } else {
                    remove_file_if_exists(path)?
                }
            }
            for (target, old) in moved.iter().rev() {
                fs::rename(old, target).map_err(|e| e.to_string())?;
            }
            active.connection = Some(db::open_database(&active.db_path)?);
            Ok(())
        })();
        if let Err(rollback_error) = restore_old {
            let preserved = rollback.keep();
            return Err(format!(
                "{error}；回滚未完成：{rollback_error}。原库副本保留于 {}；完整安全备份：{}",
                preserved.display(),
                safety.display()
            ));
        }
        if fs::remove_file(active.base_dir.join(RESTORE_MARKER)).is_err() {
            let _ = rollback.keep();
            return Err("原库已还原，但恢复中断标记未清除；原库副本仍保留".into());
        }
        return Err(format!(
            "{error}；已还原原库，安全备份：{}",
            safety.display()
        ));
    }
    if fs::remove_file(active.base_dir.join(RESTORE_MARKER)).is_err() {
        let _ = rollback.keep();
        return Err("恢复完成，但中断标记未清除；原库副本仍保留".into());
    }
    Ok(())
}

fn extract_and_verify(source: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取备份包：{e}"))?;
    if archive.len() > 100_000 {
        return Err("备份包包含过多文件".into());
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let enclosed = entry
            .enclosed_name()
            .ok_or("备份包包含非法路径")?
            .to_path_buf();
        if !allowed_backup_path(&enclosed, entry.is_dir()) {
            return Err("备份包包含不允许恢复的运行时或配置路径".into());
        }
        if entry.size() > 8 * 1024 * 1024 * 1024 {
            return Err("备份包中的单个文件过大".into());
        }
        let target = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut output = fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        }
    }
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(destination.join("manifest.json")).map_err(|_| "备份包缺少清单")?,
    )
    .map_err(|e| format!("备份清单无效：{e}"))?;
    if manifest.format != "dcc-asset-library" || manifest.schema_version != 1 {
        return Err("不支持的备份格式或版本".into());
    }
    let mut listed = std::collections::HashSet::new();
    for expected in manifest.entries {
        let path = Path::new(&expected.path);
        if expected.path.contains("\\")
            || !allowed_backup_path(path, false)
            || path
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || !listed.insert(expected.path.clone())
        {
            return Err("备份清单包含非法或重复路径".into());
        }
        let (size, digest) = crate::file_digest::digest_file(&destination.join(&expected.path))
            .map_err(|_| format!("备份文件缺失：{}", expected.path))?;
        if size != expected.size || digest != expected.sha256 {
            return Err(format!("备份校验失败：{}", expected.path));
        }
    }
    for entry in WalkDir::new(destination) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() {
            let name = entry
                .path()
                .strip_prefix(destination)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if name != "manifest.json" && !listed.contains(&name) {
                return Err(format!("备份文件不在校验清单中：{name}"));
            }
        }
    }
    Ok(())
}

fn allowed_backup_path(path: &Path, directory: bool) -> bool {
    let parts = path
        .components()
        .map(|c| match c {
            std::path::Component::Normal(n) => n.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    let Some(parts) = parts else {
        return false;
    };
    let Some(first) = parts.first() else {
        return false;
    };
    if matches!(*first, "library.db" | "manifest.json") {
        return !directory && parts.len() == 1;
    }
    if matches!(
        *first,
        "images" | "reference-boards" | "media-library" | "media" | "note-attachments"
    ) {
        return true;
    }
    let prefix = [".dcc", "cache", "thumbnails"];
    if directory && parts.len() <= 3 {
        return prefix.starts_with(&parts);
    }
    parts.len() >= 5 && parts[..3] == prefix && uuid::Uuid::parse_str(parts[3]).is_ok()
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in WalkDir::new(source) {
        let entry = entry.map_err(|e| e.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| e.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn learning_baseline_two_libraries_preserve_scoped_identities_and_media_hash() {
        use crate::{asset, knowledge, knowledge_files, media_library, projects, reference_boards};
        use serde_json::json;

        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let source_id = uuid::Uuid::new_v4().to_string();
        let source_marker = serde_json::to_vec(&crate::state::LibraryMarker {
            format_version: 1,
            library_id: source_id.clone(),
            name: "来源测试库".into(),
            created_at: "now".into(),
        })
        .unwrap();
        std::fs::write(source.path().join("library.json"), &source_marker).unwrap();
        let mut db = crate::db::open_database(&source.path().join("library.db")).unwrap();
        let image = source.path().join("source.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([1, 2, 3, 255]))
            .save(&image)
            .unwrap();
        let source_hash = crate::file_digest::digest_file(&image).unwrap();
        eprintln!(
            "learning baseline source fixture: PNG, bytes={}, sha256={}",
            source_hash.0, source_hash.1
        );
        let imported = media_library::import(
            &mut db,
            source.path(),
            None,
            crate::models::MediaImportRequest {
                kind: crate::models::MediaKind::Image,
                paths: vec![image.to_string_lossy().into_owned()],
                folder_id: None,
                tags: vec!["学习".into()],
                favorite: true,
            },
        )
        .unwrap();
        let media_id = imported.rows[0].entry_id.clone().unwrap();
        let core_id = asset::repository::register_origin(
            &db,
            &asset::models::CoreOrigin {
                kind: "media".into(),
                id: media_id.clone(),
            },
        )
        .unwrap();
        let project = projects::upsert_project(
            &mut db,
            serde_json::from_value(
                json!({"name":"同名学习项目","projectType":"scene","status":"planning"}),
            )
            .unwrap(),
        )
        .unwrap();
        media_library::link_projects(
            &db,
            vec![media_id.clone()],
            vec![project.summary.id.clone()],
            true,
        )
        .unwrap();
        let note = knowledge::save(&mut db, knowledge::NoteInput {
            id: None, expected_revision: None, notebook_id: None,
            title: "学习笔记".into(),
            document: json!({"type":"doc","content":[
                {"type":"reference","attrs":{"kind":"media","targetId":media_id,"label":"来源图片"}},
                {"type":"reference","attrs":{"kind":"project","targetId":project.summary.id,"label":"同名学习项目"}}
            ]}),
            tags: vec![], favorite: false, journal_date: None,
        }).unwrap();
        let attachment = knowledge_files::import(&db, source.path(), &note.id, &image).unwrap();
        let board = reference_boards::create(&db, "学习参考").unwrap();
        reference_boards::import_paths(
            &mut db,
            source.path(),
            &board.id,
            &[image.to_string_lossy().into_owned()],
            &crate::models::ReferencePlacement { x: 0.0, y: 0.0 },
        )
        .unwrap();
        let original = asset::repository::get(&db, source.path(), &core_id, "zh-CN")
            .unwrap()
            .files
            .into_iter()
            .find(|f| f.role == "original")
            .unwrap();
        let archive = source.path().join("learning.dccassetlib");
        super::export_library(&db, source.path(), &archive).unwrap();

        let target_id = uuid::Uuid::new_v4().to_string();
        assert_ne!(source_id, target_id);
        let marker = crate::state::LibraryMarker {
            format_version: 1,
            library_id: target_id.clone(),
            name: "恢复测试库".into(),
            created_at: "now".into(),
        };
        let marker_bytes = serde_json::to_vec(&marker).unwrap();
        std::fs::write(target.path().join("library.json"), &marker_bytes).unwrap();
        let mut active = crate::state::ActiveLibrary {
            base_dir: target.path().to_owned(),
            db_path: target.path().join("library.db"),
            marker,
            connection: Some(crate::db::open_database(&target.path().join("library.db")).unwrap()),
        };
        assert!(media_library::get(active.connection.as_ref().unwrap(), &media_id).is_err());
        super::restore_library(&mut active, &archive).unwrap();
        assert_eq!(active.marker.library_id, target_id);
        assert_eq!(
            std::fs::read(target.path().join("library.json")).unwrap(),
            marker_bytes
        );
        let restored = active.connection.as_ref().unwrap();
        let media = media_library::get(restored, &media_id).unwrap();
        assert_eq!(media.project_ids, vec![project.summary.id.clone()]);
        assert_eq!(
            asset::repository::origin(restored, &core_id).unwrap().id,
            media_id
        );
        let restored_asset =
            asset::repository::get(restored, target.path(), &core_id, "zh-CN").unwrap();
        let restored_file = restored_asset
            .files
            .iter()
            .find(|f| f.id == original.id)
            .unwrap();
        let restored_path =
            asset::repository::file_path(restored, target.path(), &restored_file.source_ref)
                .unwrap();
        assert_eq!(
            crate::file_digest::digest_file(&restored_path).unwrap(),
            source_hash
        );
        assert_eq!(
            knowledge::get(restored, &note.id).unwrap().document,
            note.document
        );
        assert!(knowledge::references(restored, &note.id)
            .unwrap()
            .iter()
            .all(|r| r.available));
        let (attachment_path, _) =
            knowledge_files::path(restored, target.path(), &attachment.id).unwrap();
        assert_eq!(
            crate::file_digest::digest_file(&attachment_path).unwrap(),
            source_hash
        );
        assert_eq!(
            reference_boards::get(restored, &board.id)
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            restored
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "22"
        );
        assert_eq!(
            restored
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        // Restoring into another library keeps its own library UUID; entity UUIDs alone are not a cross-app identity.
        assert_eq!(
            crate::file_digest::digest_file(&image).unwrap(),
            source_hash
        );
        assert_eq!(
            knowledge::get(&db, &note.id).unwrap().document,
            note.document
        );
        assert_eq!(
            std::fs::read(source.path().join("library.json")).unwrap(),
            source_marker
        );
    }

    #[test]
    fn asset_core_cache_versions_survive_restore_without_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("library");
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("library.db");
        let mut connection = crate::db::open_database(&path).unwrap();
        let source = dir.path().join("a.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let imported = crate::media_library::import(
            &mut connection,
            &base,
            None,
            crate::models::MediaImportRequest {
                kind: crate::models::MediaKind::Image,
                paths: vec![source.to_string_lossy().into_owned()],
                folder_id: None,
                tags: vec![],
                favorite: false,
            },
        )
        .unwrap();
        let entry = imported.rows[0].entry_id.as_ref().unwrap();
        let (core,file):(String,String)=connection.query_row("SELECT c.id,f.id FROM asset_core c JOIN asset_files f ON f.asset_id=c.id WHERE c.media_entry_id=?1 AND f.role='original'",[entry],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        connection.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,created_at) VALUES('version',?1,?2,'v001','now')",rusqlite::params![core,file]).unwrap();
        let cache = format!(".dcc/cache/thumbnails/{file}/preview.png");
        std::fs::create_dir_all(base.join(&cache).parent().unwrap()).unwrap();
        std::fs::write(base.join(&cache), b"thumbnail").unwrap();
        std::fs::create_dir_all(base.join(".dcc/runtime/scans")).unwrap();
        std::fs::write(base.join(".dcc/runtime/scans/private.jsonl"), b"temporary").unwrap();
        let output = dir.path().join("backup.dccassetlib");
        super::export_library(&connection, &base, &output).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
        assert!(zip.by_name(&cache).is_ok());
        assert!(zip.by_name(".dcc/runtime/scans/private.jsonl").is_err());
        drop(zip);
        std::fs::write(base.join(&cache), b"changed").unwrap();
        let mut active = crate::state::ActiveLibrary {
            base_dir: base.clone(),
            db_path: path,
            marker: crate::state::LibraryMarker {
                format_version: 1,
                library_id: "test".into(),
                name: "test".into(),
                created_at: "now".into(),
            },
            connection: Some(connection),
        };
        super::restore_library(&mut active, &output).unwrap();
        assert_eq!(std::fs::read(base.join(&cache)).unwrap(), b"thumbnail");
        assert_eq!(
            active
                .connection
                .as_ref()
                .unwrap()
                .query_row("SELECT file_id FROM asset_versions", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            file
        );
        assert!(base.join(".dcc/runtime/scans/private.jsonl").is_file());
    }
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn interrupted_restore_requires_explicit_recovery_and_keeps_original_files() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let db_path = base.join("library.db");
        drop(db::open_database(&db_path).unwrap());
        fs::create_dir(base.join("images")).unwrap();
        fs::write(base.join("images/original.txt"), b"original").unwrap();
        let old = base.join(".restore-old-test");
        fs::create_dir(&old).unwrap();
        let checkpoint = RestoreCheckpoint {
            rollback: ".restore-old-test".into(),
            originals: vec!["library.db".into(), "images".into()],
        };
        crate::reliability::write_json(&base.join(RESTORE_MARKER), &checkpoint).unwrap();
        fs::rename(base.join("images"), old.join("images")).unwrap();
        fs::create_dir(base.join("images")).unwrap();
        fs::write(base.join("images/new.txt"), b"partial").unwrap();
        let mut active = ActiveLibrary {
            base_dir: base.to_path_buf(),
            db_path,
            marker: crate::state::LibraryMarker {
                format_version: 1,
                library_id: "test".into(),
                name: "test".into(),
                created_at: "now".into(),
            },
            connection: None,
        };
        assert!(base.join("images/new.txt").exists());
        recover_interrupted_restore(&mut active, |_| {}).unwrap();
        assert_eq!(
            fs::read(base.join("images/original.txt")).unwrap(),
            b"original"
        );
        assert!(!base.join("images/new.txt").exists());
        assert!(!base.join(RESTORE_MARKER).exists());
        assert!(active.connection.is_some());
    }
    #[test]
    fn restore_keeps_all_preview_directories_and_safety_backup() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("library");
        fs::create_dir_all(base.join("media/asset/originals")).unwrap();
        fs::write(base.join("media/asset/originals/model.glb"), b"model").unwrap();
        fs::create_dir_all(base.join("note-attachments/note")).unwrap();
        fs::write(
            base.join("note-attachments/note/learning.png"),
            b"note image",
        )
        .unwrap();
        let db_path = base.join("library.db");
        let connection = db::open_database(&db_path).unwrap();
        connection.execute("INSERT INTO categories(id,name,sort_order,created_at) VALUES('test','环境',0,'now')",[]).unwrap();
        let output = dir.path().join("backup.dccassetlib");
        export_library(&connection, &base, &output).unwrap();
        fs::write(base.join("media/asset/originals/model.glb"), b"changed").unwrap();
        fs::write(base.join("note-attachments/note/learning.png"), b"changed").unwrap();
        connection.execute("DELETE FROM categories", []).unwrap();
        let marker = crate::state::LibraryMarker {
            format_version: 1,
            library_id: "lib".into(),
            name: "测试".into(),
            created_at: "now".into(),
        };
        let mut active = ActiveLibrary {
            base_dir: base.clone(),
            db_path,
            marker,
            connection: Some(connection),
        };
        restore_library(&mut active, &output).unwrap();
        assert_eq!(
            fs::read(base.join("note-attachments/note/learning.png")).unwrap(),
            b"note image"
        );
        assert_eq!(
            fs::read(base.join("media/asset/originals/model.glb")).unwrap(),
            b"model"
        );
        let count: i64 = active
            .connection
            .as_ref()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            fs::read_dir(base.join("safety-backups")).unwrap().count(),
            1
        );
    }
    #[test]
    fn damaged_backup_does_not_replace_current_library() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("library");
        fs::create_dir_all(&base).unwrap();
        let db_path = base.join("library.db");
        let connection = db::open_database(&db_path).unwrap();
        let marker = crate::state::LibraryMarker {
            format_version: 1,
            library_id: "lib".into(),
            name: "测试".into(),
            created_at: "now".into(),
        };
        let mut active = ActiveLibrary {
            base_dir: base.clone(),
            db_path,
            marker,
            connection: Some(connection),
        };
        let damaged = dir.path().join("bad.dccassetlib");
        fs::write(&damaged, b"bad zip").unwrap();
        assert!(restore_library(&mut active, &damaged).is_err());
        assert_eq!(
            active
                .connection
                .as_ref()
                .unwrap()
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    #[test]
    fn digest_is_stable() {
        assert_eq!(
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(b"asset")),
            "d59386e0ae435e292fbe0ebcdb954b75ed5fb3922091277cb19f798fc5d50718"
        );
    }

    #[test]
    fn export_includes_independent_media_library() {
        let directory = tempdir().unwrap();
        let base = directory.path().join("library");
        fs::create_dir_all(base.join("media-library/image/demo")).unwrap();
        fs::write(
            base.join("media-library/image/demo/original.png"),
            b"preview",
        )
        .unwrap();
        fs::create_dir_all(base.join("media/asset/originals")).unwrap();
        fs::write(
            base.join("media/asset/originals/model.glb"),
            b"asset attachment",
        )
        .unwrap();
        let connection = db::open_database(&base.join("library.db")).unwrap();
        let output = directory.path().join("backup.dccassetlib");
        export_library(&connection, &base, &output).unwrap();
        let file = fs::File::open(output).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive
            .by_name("media-library/image/demo/original.png")
            .is_ok());
        assert!(archive.by_name("media/asset/originals/model.glb").is_ok());
    }
}
