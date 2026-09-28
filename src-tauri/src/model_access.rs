use crate::media;
use rusqlite::{Connection, OptionalExtension};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn contained_file(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let base = base
        .canonicalize()
        .map_err(|e| format!("素材库目录不可用：{e}"))?;
    let file = media::safe_relative(&base, relative)?
        .canonicalize()
        .map_err(|_| "模型或依赖文件缺失，请检查媒体库健康状态".to_string())?;
    if !file.starts_with(&base) || !file.is_file() {
        return Err("模型文件不在当前素材库内".into());
    }
    Ok(file)
}
pub fn model_location(connection: &Connection, base: &Path, id: &str) -> Result<PathBuf, String> {
    let relative: String = connection.query_row("SELECT f.rel_path FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id AND f.entry_id=e.id WHERE e.id=?1 AND e.kind='model' AND e.deleted_at IS NULL AND f.role='main'",[id],|r|r.get(0)).optional().map_err(|e|e.to_string())?.ok_or("三维模型不存在或已在回收站")?;
    contained_file(base, &relative)
}
pub fn reveal_model(connection: &Connection, base: &Path, id: &str) -> Result<(), String> {
    let file = model_location(connection, base, id)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let display = file
            .to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap_or(&file.to_string_lossy())
            .to_string();
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .ok_or("无法定位 Windows 系统目录")?;
        std::process::Command::new(windows.join("explorer.exe"))
            .arg(format!("/select,{display}"))
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("打开资源管理器失败：{e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        open::that(file.parent().ok_or("模型目录无效")?).map_err(|e| e.to_string())
    }
}
pub fn export_model(
    connection: &Connection,
    base: &Path,
    id: &str,
    destination: &Path,
) -> Result<PathBuf, String> {
    model_location(connection, base, id)?;
    let parent = destination
        .canonicalize()
        .map_err(|_| "导出目标目录不存在或无法访问")?;
    let library = base.canonicalize().map_err(|e| e.to_string())?;
    if !parent.is_dir() || parent.starts_with(&library) {
        return Err("请选择素材库之外的工作目录导出模型".into());
    }
    let name: String = connection
        .query_row("SELECT name FROM media_entries WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let label: String = name
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(64)
        .collect();
    let label = label.trim_matches([' ', '.']);
    let suffix = uuid::Uuid::parse_str(id)
        .map_err(|_| "模型 ID 无效")?
        .simple()
        .to_string();
    let folder = format!(
        "{}-{}",
        if label.is_empty() { "模型" } else { label },
        &suffix[..8]
    );
    let mut statement=connection.prepare("SELECT logical_path,rel_path FROM media_files WHERE entry_id=?1 AND role IN ('main','dependency') ORDER BY sort_order,id").map_err(|e|e.to_string())?;
    let records = statement
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".dcc-model-export-")
        .tempdir_in(&parent)
        .map_err(|e| format!("创建导出目录失败：{e}"))?;
    for (logical, relative) in records {
        let logical = logical.replace('\\', "/");
        if logical.is_empty() || logical.contains(':') {
            return Err("模型包依赖路径不安全".into());
        }
        let source = contained_file(base, &relative)?;
        let target = media::safe_relative(stage.path(), &logical)?;
        if target.exists() {
            return Err("模型包中存在重复文件路径".into());
        }
        fs::create_dir_all(target.parent().ok_or("模型文件名无效")?).map_err(|e| e.to_string())?;
        fs::copy(source, &target).map_err(|e| format!("复制模型或贴图失败：{e}"))?;
    }
    // Always publish a new folder; never merge or overwrite an existing project.
    for number in 1..10000 {
        let target = parent.join(if number == 1 {
            folder.clone()
        } else {
            format!("{folder} ({number})")
        });
        if target.exists() {
            continue;
        }
        fs::rename(stage.path(), &target).map_err(|e| format!("完成模型导出失败：{e}"))?;
        return Ok(target);
    }
    Err("导出目录名称冲突过多，请选择其他目录".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> (Connection, String) {
        let connection = crate::db::open_database(&root.join("library.db")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let main = uuid::Uuid::new_v4().to_string();
        let dep = uuid::Uuid::new_v4().to_string();
        let folder = format!("media-library/model/{id}/files");
        fs::create_dir_all(root.join(&folder).join("textures")).unwrap();
        fs::write(root.join(&folder).join("scene.gltf"), b"gltf").unwrap();
        fs::write(root.join(&folder).join("textures/a.png"), b"texture").unwrap();
        connection.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'model','石桥',?2,'now','now')",rusqlite::params![id,main]).unwrap();
        for (fid, role, logical) in [
            (&main, "main", "scene.gltf"),
            (&dep, "dependency", "textures/a.png"),
        ] {
            connection.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,?3,?4,?4,?5,'now')",rusqlite::params![fid,id,role,logical,format!("{folder}/{logical}")]).unwrap();
        }
        (connection, id)
    }
    #[test]
    fn exports_dependencies_without_overwriting_or_moving_originals() {
        let library = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let (c, id) = fixture(library.path());
        c.execute(
            "UPDATE media_files SET logical_path=?1 WHERE entry_id=?2 AND role='dependency'",
            rusqlite::params![r"textures\a.png", id],
        )
        .unwrap();
        let first = export_model(&c, library.path(), &id, output.path()).unwrap();
        let second = export_model(&c, library.path(), &id, output.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first.join("textures/a.png")).unwrap(), b"texture");
        assert!(model_location(&c, library.path(), &id).unwrap().is_file());
        assert!(export_model(&c, library.path(), &id, library.path()).is_err());
    }
    #[test]
    fn missing_dependencies_abort_and_trash_cannot_be_opened() {
        let library = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let (c, id) = fixture(library.path());
        c.execute("UPDATE media_files SET rel_path='../outside.png' WHERE entry_id=?1 AND role='dependency'",[&id]).unwrap();
        assert!(export_model(&c, library.path(), &id, output.path()).is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
        c.execute(
            "UPDATE media_entries SET deleted_at='now' WHERE id=?1",
            [&id],
        )
        .unwrap();
        assert!(model_location(&c, library.path(), &id).is_err());
    }
}
