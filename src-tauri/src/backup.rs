use crate::{db, state::ActiveLibrary};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    for managed_dir in ["images", "reference-boards", "media-library"] {
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
    for (source, name) in &sources {
        let data = fs::read(source).map_err(|e| e.to_string())?;
        entries.push(ManifestEntry {
            path: name.clone(),
            size: data.len() as u64,
            sha256: hex_digest(&data),
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
    for (source, name) in &sources {
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
    if destination.exists() {
        fs::remove_file(destination).map_err(|e| e.to_string())?;
    }
    fs::rename(temp_output, destination).map_err(|e| e.to_string())
}

pub fn restore_library(active: &mut ActiveLibrary, source: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err("备份文件不存在".into());
    }
    let stage = tempdir().map_err(|e| e.to_string())?;
    extract_and_verify(source, stage.path())?;
    let staged_db = stage.path().join("library.db");
    if !staged_db.is_file() {
        return Err("备份中缺少数据库".into());
    }
    let check = Connection::open(&staged_db).map_err(|e| format!("备份数据库无效：{e}"))?;
    let integrity: String = check
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        return Err(format!("备份数据库完整性检查失败：{integrity}"));
    }
    drop(check);
    let current = active.connection.as_ref().ok_or("数据库正在维护")?;
    let safety = active.base_dir.join("safety-backups").join(format!(
        "恢复前-{}.dccassetlib",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    export_library(current, &active.base_dir, &safety)?;
    let old_connection = active.connection.take();
    drop(old_connection);
    let rollback_db = active.base_dir.join("library.pre-restore.db");
    let rollback_images = active.base_dir.join("images.pre-restore");
    let rollback_boards = active.base_dir.join("reference-boards.pre-restore");
    let rollback_media = active.base_dir.join("media-library.pre-restore");
    remove_file_if_exists(&rollback_db)?;
    remove_dir_if_exists(&rollback_images)?;
    remove_dir_if_exists(&rollback_boards)?;
    remove_dir_if_exists(&rollback_media)?;
    for suffix in ["-wal", "-shm"] {
        remove_file_if_exists(Path::new(&format!(
            "{}{}",
            active.db_path.display(),
            suffix
        )))?;
    }
    if active.db_path.exists() {
        fs::rename(&active.db_path, &rollback_db)
            .map_err(|e| format!("准备恢复数据库失败：{e}"))?;
    }
    let current_images = active.base_dir.join("images");
    if current_images.exists() {
        fs::rename(&current_images, &rollback_images)
            .map_err(|e| format!("准备恢复图片失败：{e}"))?;
    }
    let current_boards = active.base_dir.join("reference-boards");
    if current_boards.exists() {
        fs::rename(&current_boards, &rollback_boards)
            .map_err(|e| format!("准备恢复参考板图片失败：{e}"))?;
    }
    let current_media = active.base_dir.join("media-library");
    if current_media.exists() {
        fs::rename(&current_media, &rollback_media)
            .map_err(|e| format!("准备恢复媒体库失败：{e}"))?;
    }
    let install = (|| -> Result<(), String> {
        fs::copy(&staged_db, &active.db_path).map_err(|e| format!("恢复数据库失败：{e}"))?;
        let staged_images = stage.path().join("images");
        if staged_images.exists() {
            copy_tree(&staged_images, &current_images)?
        } else {
            fs::create_dir_all(current_images.join("originals")).map_err(|e| e.to_string())?;
            fs::create_dir_all(current_images.join("thumbnails")).map_err(|e| e.to_string())?;
        }
        let staged_boards = stage.path().join("reference-boards");
        if staged_boards.exists() {
            copy_tree(&staged_boards, &current_boards)?;
        } else {
            fs::create_dir_all(&current_boards).map_err(|e| e.to_string())?;
        }
        let staged_media = stage.path().join("media-library");
        if staged_media.exists() {
            copy_tree(&staged_media, &current_media)?;
        } else {
            fs::create_dir_all(&current_media).map_err(|e| e.to_string())?;
        }
        let reopened = db::open_database(&active.db_path)?;
        active.connection = Some(reopened);
        Ok(())
    })();
    if let Err(error) = install {
        remove_file_if_exists(&active.db_path)?;
        remove_dir_if_exists(&current_images)?;
        remove_dir_if_exists(&current_boards)?;
        remove_dir_if_exists(&current_media)?;
        if rollback_db.exists() {
            fs::rename(&rollback_db, &active.db_path).map_err(|e| e.to_string())?;
        }
        if rollback_images.exists() {
            fs::rename(&rollback_images, &current_images).map_err(|e| e.to_string())?;
        }
        if rollback_boards.exists() {
            fs::rename(&rollback_boards, &current_boards).map_err(|e| e.to_string())?;
        }
        if rollback_media.exists() {
            fs::rename(&rollback_media, &current_media).map_err(|e| e.to_string())?;
        }
        active.connection = Some(db::open_database(&active.db_path)?);
        return Err(error);
    }
    remove_file_if_exists(&rollback_db)?;
    remove_dir_if_exists(&rollback_images)?;
    remove_dir_if_exists(&rollback_boards)?;
    remove_dir_if_exists(&rollback_media)?;
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
    for expected in manifest.entries {
        let data = fs::read(destination.join(&expected.path))
            .map_err(|_| format!("备份文件缺失：{}", expected.path))?;
        if data.len() as u64 != expected.size || hex_digest(&data) != expected.sha256 {
            return Err(format!("备份校验失败：{}", expected.path));
        }
    }
    Ok(())
}

fn hex_digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
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
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn digest_is_stable() {
        assert_eq!(
            hex_digest(b"asset"),
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
        let connection = db::open_database(&base.join("library.db")).unwrap();
        let output = directory.path().join("backup.dccassetlib");
        export_library(&connection, &base, &output).unwrap();
        let file = fs::File::open(output).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive
            .by_name("media-library/image/demo/original.png")
            .is_ok());
    }
}
