use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct UpgradeBackupFile {
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct UpgradeBackupRecord {
    pub last_success_app_version: String,
    pub last_attempt_at: Option<String>,
    pub result: Option<String>,
    pub message: Option<String>,
    pub snapshots: Vec<UpgradeBackupFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeBackupStatus {
    pub keep: i64,
    pub last_attempt_at: Option<String>,
    pub result: Option<String>,
    pub message: Option<String>,
    pub backup_directory: String,
}

#[tauri::command]
pub fn get_upgrade_backup_status(
    state: tauri::State<crate::state::AppState>,
    library_id: String,
) -> Result<UpgradeBackupStatus, String> {
    state.upgrade_backup_status(&library_id)
}

#[tauri::command]
pub fn save_upgrade_backup_keep(
    state: tauri::State<crate::state::AppState>,
    library_id: String,
    keep: i64,
) -> Result<UpgradeBackupStatus, String> {
    state.save_upgrade_backup_keep(&library_id, keep)
}

pub(crate) fn run_upgrade_backup(
    base_dir: &Path,
    version: &str,
    keep: usize,
    record: &mut UpgradeBackupRecord,
) -> Result<UpgradeBackupStatus, String> {
    run_upgrade_backup_checked(base_dir, version, keep, record, verify_snapshot)
}

pub(crate) fn library_key(base_dir: &Path) -> Result<String, String> {
    let path = fs::canonicalize(base_dir).map_err(|e| format!("素材库目录不可用：{e}"))?;
    let text = path.to_string_lossy().to_string();
    #[cfg(windows)]
    let text = text.to_lowercase();
    Ok(format!("{:x}", Sha256::digest(text.as_bytes())))
}

fn backup_directory(base_dir: &Path) -> Result<PathBuf, String> {
    let base = fs::canonicalize(base_dir).map_err(|e| e.to_string())?;
    Ok(base
        .parent()
        .ok_or("素材库同级目录不可用")?
        .join("升级前自动备份")
        .join(library_key(&base)?))
}

pub(crate) fn status(
    base_dir: &Path,
    keep: i64,
    record: &UpgradeBackupRecord,
) -> Result<UpgradeBackupStatus, String> {
    Ok(UpgradeBackupStatus {
        keep: keep.clamp(1, 20),
        last_attempt_at: record.last_attempt_at.clone(),
        result: record.result.clone(),
        message: record.message.clone(),
        backup_directory: backup_directory(base_dir)?.to_string_lossy().into_owned(),
    })
}

fn reject_links(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(metadata) => {
                #[cfg(windows)]
                let reparse = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let reparse = false;
                if metadata.file_type().is_symlink() || reparse {
                    return Err("升级快照路径包含链接或重解析点，未写入或清理".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("无法确认升级快照路径：{e}")),
        }
    }
    Ok(())
}

fn verify_snapshot(path: &Path) -> Result<(), String> {
    reject_links(path)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    let result: String = connection
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("升级快照完整性校验失败：{result}"))
    }
}

fn owned_snapshot_path(directory: &Path, item: &UpgradeBackupFile) -> Result<PathBuf, String> {
    let expression = regex::Regex::new(r"^栈藏-元数据库-[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?-([0-9]{8}T[0-9]{9}Z)-([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.db$").unwrap();
    let captures = expression
        .captures(&item.name)
        .ok_or("快照名称无效，未清理")?;
    chrono::NaiveDateTime::parse_from_str(&captures[1], "%Y%m%dT%H%M%S%3fZ")
        .map_err(|_| "快照时间无效，未清理")?;
    Uuid::parse_str(&captures[2]).map_err(|_| "快照标识无效，未清理")?;
    let path = directory.join(&item.name);
    reject_links(&path)?;
    if !fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("快照不是普通文件，未清理".into());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if canonical.parent() != Some(directory)
        || canonical.file_name() != Some(std::ffi::OsStr::new(&item.name))
    {
        return Err("快照不在所属素材库的备份目录，未清理".into());
    }
    Ok(path)
}

fn clean_registered_snapshots(
    directory: &Path,
    keep: usize,
    record: &mut UpgradeBackupRecord,
) -> Result<(), String> {
    reject_links(directory)?;
    let directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    let mut candidates = Vec::new();
    for (index, item) in record.snapshots.iter().enumerate() {
        match chrono::DateTime::parse_from_rfc3339(&item.created_at)
            .map_err(|_| "快照登记时间无效，未清理".to_string())
            .and_then(|date| owned_snapshot_path(&directory, item).map(|path| (date, path)))
        {
            Ok((date, path)) => candidates.push((date, index, path)),
            Err(error) => warnings.push(error),
        }
    }
    candidates.sort_by_key(|(date, _, _)| *date);
    let count = candidates.len().saturating_sub(keep.clamp(1, 20));
    let mut removed = Vec::new();
    for (_, index, path) in candidates.into_iter().take(count) {
        // Recheck immediately before removal; never follow a substituted link.
        match owned_snapshot_path(&directory, &record.snapshots[index]).and_then(|checked| {
            if checked != path {
                return Err("快照路径已改变，未清理".into());
            }
            fs::remove_file(&checked).map_err(|e| format!("旧快照清理失败：{e}"))
        }) {
            Ok(()) => removed.push(index),
            Err(error) => warnings.push(error),
        }
    }
    removed.sort_unstable();
    for index in removed.into_iter().rev() {
        record.snapshots.remove(index);
    }
    if warnings.is_empty() {
        Ok(())
    } else {
        Err(format!("部分旧快照未清理：{}", warnings.join("；")))
    }
}

fn run_upgrade_backup_checked(
    base_dir: &Path,
    version: &str,
    keep: usize,
    record: &mut UpgradeBackupRecord,
    verify: fn(&Path) -> Result<(), String>,
) -> Result<UpgradeBackupStatus, String> {
    let keep = keep.clamp(1, 20);
    if record.last_success_app_version == version {
        return status(base_dir, keep as i64, record);
    }
    record.last_attempt_at = Some(Utc::now().to_rfc3339());
    let result = (|| -> Result<(), String> {
        if !regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?$")
            .unwrap()
            .is_match(version)
        {
            return Err("应用版本无效，未生成快照".into());
        }
        let base = fs::canonicalize(base_dir).map_err(|e| e.to_string())?;
        let source = base.join("library.db");
        reject_links(&source)?;
        let directory = backup_directory(&base)?;
        reject_links(&directory)?;
        fs::create_dir_all(&directory).map_err(|e| format!("创建升级快照目录失败：{e}"))?;
        reject_links(&directory)?;
        let name = format!(
            "栈藏-元数据库-{version}-{}-{}.db",
            Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
            Uuid::new_v4()
        );
        let path = directory.join(&name);
        if path.exists() {
            return Err("升级快照目标已存在，未覆盖".into());
        }
        let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        connection
            .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
            .map_err(|e| format!("生成升级快照失败：{e}"))?;
        verify(&path)?;
        record.snapshots.push(UpgradeBackupFile {
            name,
            created_at: Utc::now().to_rfc3339(),
        });
        record.last_success_app_version = version.into();
        Ok(())
    })();
    match result {
        Ok(()) => {
            record.result = Some("success".into());
            record.message = Some("升级前数据库快照已校验；只含数据库，不含媒体".into());
            if let Err(error) = backup_directory(base_dir)
                .and_then(|directory| clean_registered_snapshots(&directory, keep, record))
            {
                record.message = Some(format!(
                    "升级前数据库快照已校验；只含数据库，不含媒体。{error}"
                ));
            }
            status(base_dir, keep as i64, record)
        }
        Err(error) => {
            record.result = Some("failed".into());
            record.message = Some(error.clone());
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use tempfile::tempdir;
    use walkdir::WalkDir;

    fn library(parent: &Path) -> std::path::PathBuf {
        let path = parent.join("library");
        fs::create_dir(&path).unwrap();
        Connection::open(path.join("library.db"))
            .unwrap()
            .execute_batch("CREATE TABLE example(value); INSERT INTO example VALUES(1);")
            .unwrap();
        path
    }

    #[test]
    fn upgrade_backup_same_version_skips() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord::default();
        let first = run_upgrade_backup(&path, "0.20.6", 5, &mut record).unwrap();
        assert_eq!(first.result.as_deref(), Some("success"));
        let attempt = record.last_attempt_at.clone();
        run_upgrade_backup(&path, "0.20.6", 5, &mut record).unwrap();
        assert_eq!(record.snapshots.len(), 1);
        assert_eq!(record.last_attempt_at, attempt);
        let snapshot =
            Connection::open(Path::new(&first.backup_directory).join(&record.snapshots[0].name))
                .unwrap();
        assert_eq!(
            snapshot
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            snapshot
                .query_row("SELECT value FROM example", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn upgrade_backup_new_version_once() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord::default();
        run_upgrade_backup(&path, "0.20.5", 5, &mut record).unwrap();
        run_upgrade_backup(&path, "0.20.6", 5, &mut record).unwrap();
        run_upgrade_backup(&path, "0.20.6", 5, &mut record).unwrap();
        assert_eq!(record.snapshots.len(), 2);
        assert_eq!(record.last_success_app_version, "0.20.6");
    }

    #[test]
    fn upgrade_backup_integrity_failure_keeps_file_and_version() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord {
            last_success_app_version: "0.20.5".into(),
            ..Default::default()
        };
        assert!(
            run_upgrade_backup_checked(&path, "0.20.6", 5, &mut record, |_| Err(
                "模拟校验失败".into()
            ))
            .is_err()
        );
        assert_eq!(record.last_success_app_version, "0.20.5");
        assert!(record.snapshots.is_empty());
        assert_eq!(record.result.as_deref(), Some("failed"));
        let files = WalkDir::new(dir.path().join("升级前自动备份"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|s| s == "db"))
            .count();
        assert_eq!(files, 1, "failed verification output must be kept");
    }

    #[test]
    fn upgrade_backup_keep_only_removes_registered_oldest() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord::default();
        let first = run_upgrade_backup(&path, "0.20.1", 2, &mut record).unwrap();
        let folder = Path::new(&first.backup_directory);
        let oldest = folder.join(&record.snapshots[0].name);
        let manual = folder.join(format!(
            "栈藏-元数据库-0.20.0-20200101T000000000Z-{}.db",
            Uuid::new_v4()
        ));
        fs::write(&manual, "manual").unwrap();
        run_upgrade_backup(&path, "0.20.2", 2, &mut record).unwrap();
        run_upgrade_backup(&path, "0.20.3", 2, &mut record).unwrap();
        assert!(!oldest.exists());
        assert_eq!(record.snapshots.len(), 2);
        assert!(record
            .snapshots
            .iter()
            .all(|s| folder.join(&s.name).is_file()));
        assert_eq!(fs::read(&manual).unwrap(), b"manual");
    }

    #[test]
    fn upgrade_backup_retention_uses_registered_time_not_vector_order() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord::default();
        for version in ["0.20.1", "0.20.2", "0.20.3", "0.20.4"] {
            run_upgrade_backup(&path, version, 5, &mut record).unwrap();
        }
        for (item, year) in record.snapshots.iter_mut().zip([2021, 2022, 2020, 2023]) {
            item.created_at = format!("{year}-01-01T00:00:00Z");
        }
        let expected = vec![
            record.snapshots[1].name.clone(),
            record.snapshots[3].name.clone(),
        ];
        clean_registered_snapshots(&backup_directory(&path).unwrap(), 2, &mut record).unwrap();
        assert_eq!(
            record
                .snapshots
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn upgrade_backup_outside_and_unregistered_candidates_are_untouched() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let mut record = UpgradeBackupRecord::default();
        let first = run_upgrade_backup(&path, "0.20.1", 1, &mut record).unwrap();
        let folder = Path::new(&first.backup_directory);
        let oldest = folder.join(&record.snapshots[0].name);
        let outside = dir.path().join("outside.db");
        fs::write(&outside, "outside").unwrap();
        let manual = folder.join("栈藏-元数据库-0.20.0-20200101T000000000Z-not-a-uuid.db");
        fs::write(&manual, "manual").unwrap();
        for name in [
            outside.to_string_lossy().into_owned(),
            "../../outside.db".into(),
            manual.file_name().unwrap().to_string_lossy().into_owned(),
        ] {
            record.snapshots.push(UpgradeBackupFile {
                name,
                created_at: "2020-01-01T00:00:00Z".into(),
            });
        }
        let other_library = dir.path().join("other-library");
        fs::create_dir(&other_library).unwrap();
        fs::copy(path.join("library.db"), other_library.join("library.db")).unwrap();
        let mut other_record = UpgradeBackupRecord::default();
        let other = run_upgrade_backup(&other_library, "0.20.1", 1, &mut other_record).unwrap();
        let other_file = Path::new(&other.backup_directory).join(&other_record.snapshots[0].name);
        run_upgrade_backup(&path, "0.20.2", 1, &mut record).unwrap();
        assert!(
            !oldest.exists(),
            "only a validated owned old snapshot may be removed"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert_eq!(fs::read(&manual).unwrap(), b"manual");
        assert!(other_file.is_file());
    }

    #[test]
    fn upgrade_backup_rejects_linked_backup_directory() {
        let dir = tempdir().unwrap();
        let path = library(dir.path());
        let target = dir.path().join("external");
        fs::create_dir(&target).unwrap();
        let link = dir.path().join("升级前自动备份");
        #[cfg(windows)]
        assert!(std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap()
            .status
            .success());
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut record = UpgradeBackupRecord::default();
        assert!(run_upgrade_backup(&path, "0.20.6", 1, &mut record)
            .unwrap_err()
            .contains("链接"));
        assert!(record.last_success_app_version.is_empty());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }
}
