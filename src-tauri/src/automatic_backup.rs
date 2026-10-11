use crate::{reliability::write_json, state::AppState};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPolicy {
    pub enabled: bool,
    pub directory: String,
    #[serde(default = "hours")]
    pub interval_hours: u32,
    #[serde(default = "keep")]
    pub keep: u32,
    #[serde(default)]
    last_completed: i64,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    pending: String,
    #[serde(default)]
    owned: Vec<String>,
}
fn hours() -> u32 {
    24
}
fn keep() -> u32 {
    3
}
fn policy_path(state: &AppState, id: &str) -> PathBuf {
    state
        .config_directory()
        .join("backup-policies")
        .join(format!("{id}.json"))
}
fn read_policy(state: &AppState, id: &str) -> Result<BackupPolicy, String> {
    let path = policy_path(state, id);
    match fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "自动备份设置损坏，未启动备份".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BackupPolicy {
            interval_hours: hours(),
            keep: keep(),
            ..Default::default()
        }),
        Err(_) => Err("无法读取自动备份设置".into()),
    }
}
fn validate(policy: &BackupPolicy, base: &Path) -> Result<(), String> {
    if !(1..=168).contains(&policy.interval_hours) || !(1..=20).contains(&policy.keep) {
        return Err("备份间隔或保留数量无效".into());
    }
    if policy.enabled {
        let directory = fs::canonicalize(&policy.directory).map_err(|_| "自动备份目录不可用")?;
        if !directory.is_dir()
            || directory.starts_with(fs::canonicalize(base).map_err(|_| "素材库目录不可用")?)
        {
            return Err("请选择当前素材库之外的备份目录".into());
        }
    }
    Ok(())
}
fn fingerprint(base: &Path) -> String {
    ["library.db", "library.db-wal"]
        .iter()
        .map(|name| {
            fs::metadata(base.join(name))
                .ok()
                .map(|m| {
                    format!(
                        "{}-{}",
                        m.len(),
                        m.modified()
                            .ok()
                            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|v| v.as_nanos())
                            .unwrap_or(0)
                    )
                })
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(":")
}
#[tauri::command]
pub fn get_automatic_backup_policy(
    state: State<AppState>,
    library_id: String,
) -> Result<BackupPolicy, String> {
    if state.active_library_id()? != library_id {
        return Err("素材库已改变，请重新打开设置".into());
    }
    read_policy(&state, &library_id)
}
#[tauri::command]
pub fn save_automatic_backup_policy(
    state: State<AppState>,
    library_id: String,
    enabled: bool,
    directory: String,
    interval_hours: u32,
    keep: u32,
) -> Result<BackupPolicy, String> {
    let id = state.active_library_id()?;
    if id != library_id {
        return Err("素材库已改变，未保存备份设置".into());
    }
    let mut policy = read_policy(&state, &id)?;
    policy.enabled = enabled;
    policy.directory = directory;
    policy.interval_hours = interval_hours;
    policy.keep = keep;
    state.with_library(|_, base| validate(&policy, base))?;
    write_json(&policy_path(&state, &id), &policy)?;
    Ok(policy)
}
#[tauri::command]
pub fn get_automatic_backup_due(state: State<AppState>) -> Result<Option<String>, String> {
    if state.tasks.busy() || state.reference_window_board_id()?.is_some() {
        return Ok(None);
    }
    let id = state.active_library_id()?;
    let mut policy = read_policy(&state, &id)?;
    if !policy.enabled
        || chrono::Utc::now().timestamp() - policy.last_completed
            < i64::from(policy.interval_hours) * 3600
    {
        return Ok(None);
    }
    let due = state.with_library(|_, base| {
        validate(&policy, base)?;
        if fingerprint(base) == policy.fingerprint {
            return Ok(None);
        }
        Ok(Some(
            Path::new(&policy.directory)
                .join(format!(
                    "栈藏-自动-{id}-{}-{}.dccassetlib",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S"),
                    uuid::Uuid::new_v4()
                ))
                .to_string_lossy()
                .into_owned(),
        ))
    })?;
    if let Some(path) = &due {
        policy.pending = path.clone();
        write_json(&policy_path(&state, &id), &policy)?;
    }
    Ok(due)
}
#[tauri::command]
pub fn finish_automatic_backup(state: State<AppState>, path: String) -> Result<(), String> {
    let id = state.active_library_id()?;
    let mut policy = read_policy(&state, &id)?;
    let folder = fs::canonicalize(&policy.directory).map_err(|_| "备份目录不可用")?;
    let path = fs::canonicalize(path).map_err(|_| "备份文件不可用")?;
    let prefix = format!("栈藏-自动-{id}-");
    if fs::canonicalize(&policy.pending).ok().as_ref() != Some(&path)
        || path.parent() != Some(folder.as_path())
        || !path.is_file()
        || !path
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with(&prefix) && v.ends_with(".dccassetlib"))
    {
        return Err("不是当前库的自动备份".into());
    }
    policy.pending.clear();
    policy.owned.push(path.to_string_lossy().into_owned());
    policy.last_completed = chrono::Utc::now().timestamp();
    policy.fingerprint = state.with_library(|_, base| Ok(fingerprint(base)))?;
    write_json(&policy_path(&state, &id), &policy)?;
    let remove = policy.owned.len().saturating_sub(policy.keep as usize);
    for file in policy.owned.iter().take(remove) {
        let file = Path::new(file);
        if !file.exists() {
            continue;
        }
        let safe = fs::canonicalize(file).map_err(|_| "旧备份不可用")?;
        if safe.parent() == Some(folder.as_path())
            && safe
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.starts_with(&prefix) && v.ends_with(".dccassetlib"))
        {
            fs::remove_file(safe).map_err(|_| "备份已完成，但旧自动备份清理失败")?
        }
    }
    policy.owned.drain(..remove);
    write_json(&policy_path(&state, &id), &policy)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_is_opt_in_and_rejects_library_destinations() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        fs::create_dir(&library).unwrap();
        let child = library.join("nested");
        fs::create_dir(&child).unwrap();
        let outside = root.path().join("backups");
        fs::create_dir(&outside).unwrap();
        let mut policy: BackupPolicy =
            serde_json::from_value(serde_json::json!({"enabled":false,"directory":""})).unwrap();
        assert!(!policy.enabled);
        assert_eq!(policy.keep, 3);
        assert_eq!(policy.interval_hours, 24);
        assert!(validate(&policy, &library).is_ok());
        policy.enabled = true;
        policy.directory = child.to_string_lossy().into();
        assert!(validate(&policy, &library).is_err());
        policy.directory = outside.to_string_lossy().into();
        assert!(validate(&policy, &library).is_ok());
        policy.keep = 0;
        assert!(validate(&policy, &library).is_err());
    }
}
