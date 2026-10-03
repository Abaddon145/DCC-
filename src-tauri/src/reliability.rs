//! Recovery copies are separate from notes; only an explicit user action restores content.
use crate::{
    knowledge::{self, NoteInput},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tauri::State;

pub fn migrate_workflows(db: &rusqlite::Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS workflow_templates(id TEXT PRIMARY KEY,name TEXT NOT NULL,titles_json TEXT NOT NULL,updated_at TEXT NOT NULL);").map_err(|e| e.to_string())?;
    let has_usage = tx
        .prepare("PRAGMA table_info(ai_sessions)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|name| name == "actual_tokens");
    if !has_usage {
        tx.execute_batch("ALTER TABLE ai_sessions ADD COLUMN actual_tokens INTEGER;")
            .map_err(|e| e.to_string())?;
    }
    tx.execute("UPDATE schema_meta SET value='19' WHERE key='schema_version' AND CAST(value AS INTEGER)<19",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTemplate {
    pub id: Option<String>,
    pub name: String,
    pub titles: Vec<String>,
}

fn save_workflow(
    db: &rusqlite::Connection,
    input: WorkflowTemplate,
) -> Result<WorkflowTemplate, String> {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 100
        || input.titles.is_empty()
        || input.titles.len() > 30
        || input
            .titles
            .iter()
            .any(|t| t.trim().is_empty() || t.chars().count() > 300)
    {
        return Err("请填写模板名称及 1-30 个有效任务标题".into());
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    uuid::Uuid::parse_str(&id).map_err(|_| "模板标识无效")?;
    let result = WorkflowTemplate {
        id: Some(id.clone()),
        name: input.name.trim().into(),
        titles: input.titles.iter().map(|t| t.trim().into()).collect(),
    };
    let titles = serde_json::to_string(&result.titles).map_err(|e| e.to_string())?;
    if input.id.is_some() {
        if db
            .execute(
                "UPDATE workflow_templates SET name=?1,titles_json=?2,updated_at=?3 WHERE id=?4",
                rusqlite::params![result.name, titles, chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("模板已被删除，请另存为新模板".into());
        }
    } else {
        db.execute(
            "INSERT INTO workflow_templates(id,name,titles_json,updated_at) VALUES(?1,?2,?3,?4)",
            rusqlite::params![id, result.name, titles, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(result)
}
#[tauri::command]
pub fn list_workflow_templates(state: State<AppState>) -> Result<serde_json::Value, String> {
    let library = state.active_library_id()?;
    state.with_library_if_id(&library,|db,_|{
        let mut statement=db.prepare("SELECT id,name,titles_json FROM workflow_templates ORDER BY updated_at DESC,id").map_err(|e|e.to_string())?;
        let rows=statement.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let templates=rows.into_iter().map(|(id,name,titles)|Ok(WorkflowTemplate{id:Some(id),name,titles:serde_json::from_str(&titles).map_err(|_|"模板内容损坏")?})).collect::<Result<Vec<_>,String>>()?;
        Ok(serde_json::json!({"libraryId":library,"templates":templates}))
    })?.ok_or("素材库已切换".into())
}
#[tauri::command]
pub fn save_workflow_template(
    state: State<AppState>,
    library_id: String,
    input: WorkflowTemplate,
) -> Result<WorkflowTemplate, String> {
    state
        .with_library_if_id(&library_id, |db, _| save_workflow(db, input))?
        .ok_or("素材库已切换，模板未保存".into())
}
#[tauri::command]
pub fn delete_workflow_template(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<(), String> {
    state
        .with_library_if_id(&library_id, |db, _| {
            db.execute("DELETE FROM workflow_templates WHERE id=?", [id])
                .map_err(|e| e.to_string())?;
            Ok(())
        })?
        .ok_or("素材库已切换".into())
}
#[tauri::command]
pub fn get_interrupted_restore(state: State<AppState>) -> Result<bool, String> {
    state.with_active_mut(|active| Ok(active.base_dir.join(crate::backup::RESTORE_MARKER).exists()))
}
#[tauri::command]
pub fn recover_interrupted_restore(state: State<AppState>) -> Result<(), String> {
    if state.tasks.busy() {
        return Err("请先等待当前任务结束".into());
    }
    state.with_active_mut(crate::backup::recover_interrupted_restore)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDraft {
    pub input: NoteInput,
    pub saved_at: String,
}

fn draft_path(root: &Path, library: &str, id: &str) -> Result<PathBuf, String> {
    let library = uuid::Uuid::parse_str(library).map_err(|_| "素材库 ID 无效")?;
    let id = uuid::Uuid::parse_str(id).map_err(|_| "笔记 ID 无效")?;
    Ok(root
        .join("recovery-drafts")
        .join(library.to_string())
        .join(format!("{id}.json")))
}
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("恢复路径无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建恢复目录，请检查磁盘空间和权限")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| "无法创建恢复副本")?;
    file.write_all(&serde_json::to_vec(value).map_err(|_| "恢复数据无效")?)
        .map_err(|_| "恢复副本写入失败")?;
    file.as_file().sync_all().map_err(|_| "恢复副本同步失败")?;
    file.persist(path).map_err(|_| "恢复副本提交失败")?;
    Ok(())
}
#[tauri::command]
pub fn stash_note_recovery(
    state: State<AppState>,
    library_id: String,
    input: NoteInput,
) -> Result<(), String> {
    let id = input.id.as_deref().ok_or("请先创建笔记")?;
    knowledge::inspect_document(&input.document)?;
    if input.title.chars().count() > 300 || input.tags.len() > 100 {
        return Err("恢复数据过大".into());
    }
    let path = draft_path(&state.config_directory(), &library_id, id)?;
    state
        .with_library_if_id(&library_id, |db, _| {
            knowledge::get(db, id)?;
            if path.exists() {
                serde_json::from_slice::<RecoveryDraft>(
                    &fs::read(&path).map_err(|_| "无法读取原恢复副本")?,
                )
                .map_err(|_| "原恢复副本损坏，已保留；请先处理恢复副本")?;
            }
            write_json(
                &path,
                &RecoveryDraft {
                    input: input.clone(),
                    saved_at: chrono::Utc::now().to_rfc3339(),
                },
            )
        })?
        .ok_or_else(|| "素材库已改变".to_string())
}
#[tauri::command]
pub fn get_note_recovery(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<Option<RecoveryDraft>, String> {
    let path = draft_path(&state.config_directory(), &library_id, &id)?;
    state
        .with_library_if_id(&library_id, |_, _| match fs::read(&path) {
            Ok(bytes) => {
                let value: RecoveryDraft =
                    serde_json::from_slice(&bytes).map_err(|_| "恢复副本损坏，原文件已保留")?;
                knowledge::inspect_document(&value.input.document)?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("无法读取恢复副本".into()),
        })?
        .ok_or_else(|| "素材库已改变".to_string())
}
#[tauri::command]
pub fn clear_note_recovery(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<(), String> {
    let path = draft_path(&state.config_directory(), &library_id, &id)?;
    state
        .with_library_if_id(&library_id, |_, _| match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清理已保存恢复副本".into()),
        })?
        .ok_or_else(|| "素材库已改变".to_string())
}

#[tauri::command]
pub fn restore_note_recovery_copy(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<knowledge::Note, String> {
    let path = draft_path(&state.config_directory(), &library_id, &id)?;
    state
        .with_library_mut_if_id(&library_id, |db, base| {
            let mut draft: RecoveryDraft =
                serde_json::from_slice(&fs::read(&path).map_err(|_| "恢复副本不可用")?)
                    .map_err(|_| "恢复副本损坏")?;
            if draft.input.id.as_deref() != Some(&id) {
                return Err("恢复副本来源不一致".into());
            }
            draft.input.title = format!(
                "{}（恢复副本）",
                draft.input.title.chars().take(290).collect::<String>()
            );
            let temp = tempfile::tempdir().map_err(|_| "无法创建恢复工作目录")?;
            let package = temp.path().join("draft.dccnotes");
            crate::note_package::export_draft(db, base, draft.input, &package)?;
            let result = crate::note_package::import(db, base, &package)?;
            Ok(result.note)
        })?
        .ok_or_else(|| "素材库已改变".to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewNote {
    id: String,
    title: String,
    due_date: String,
    status: String,
}
#[tauri::command]
pub fn get_note_review(
    state: State<AppState>,
    library_id: String,
    id: String,
) -> Result<Option<ReviewNote>, String> {
    use rusqlite::OptionalExtension;
    state.with_library_if_id(&library_id,|db,_|db.query_row("SELECT n.id,n.title,r.due_date,r.status FROM knowledge_reviews r JOIN knowledge_notes n ON n.id=r.note_id WHERE n.id=?1 AND n.deleted_at IS NULL",[id],|row|Ok(ReviewNote{id:row.get(0)?,title:row.get(1)?,due_date:row.get(2)?,status:row.get(3)?})).optional().map_err(|e|e.to_string()))?.ok_or_else(||"素材库已改变".to_string())
}
#[tauri::command]
pub fn list_note_reviews(
    state: State<AppState>,
    library_id: String,
    today_only: bool,
) -> Result<Vec<ReviewNote>, String> {
    state.with_library_if_id(&library_id,|db,_|{
        let mut query=db.prepare("SELECT n.id,n.title,r.due_date,r.status FROM knowledge_reviews r JOIN knowledge_notes n ON n.id=r.note_id WHERE n.deleted_at IS NULL AND r.status='waiting' AND (?1=0 OR r.due_date<=?2) ORDER BY r.due_date,n.title LIMIT 200").map_err(|e|e.to_string())?;
        let values=query.query_map(rusqlite::params![today_only,chrono::Local::now().date_naive().to_string()],|row|Ok(ReviewNote{id:row.get(0)?,title:row.get(1)?,due_date:row.get(2)?,status:row.get(3)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(values)
    })?.ok_or_else(||"素材库已改变".to_string())
}
#[tauri::command]
pub fn set_note_review(
    state: State<AppState>,
    library_id: String,
    id: String,
    due_date: String,
    status: String,
) -> Result<(), String> {
    if !["waiting", "mastered"].contains(&status.as_str())
        || chrono::NaiveDate::parse_from_str(&due_date, "%Y-%m-%d").is_err()
    {
        return Err("复习日期或状态无效".into());
    }
    state.with_library_if_id(&library_id,|db,_|{
        let note=knowledge::get(db,&id)?;if note.deleted_at.is_some(){return Err("请先恢复笔记".into())}
        db.execute("INSERT INTO knowledge_reviews(note_id,due_date,status) VALUES(?1,?2,?3) ON CONFLICT(note_id) DO UPDATE SET due_date=excluded.due_date,status=excluded.status",rusqlite::params![id,due_date,status]).map_err(|e|e.to_string())?;
        Ok(())
    })?.ok_or_else(||"素材库已改变".to_string())
}

#[tauri::command]
pub fn list_recovery_notes(
    state: State<AppState>,
    library_id: String,
) -> Result<Vec<String>, String> {
    let folder = draft_path(
        &state.config_directory(),
        &library_id,
        &uuid::Uuid::nil().to_string(),
    )?
    .parent()
    .unwrap()
    .to_path_buf();
    state
        .with_library_if_id(&library_id, |_, _| {
            if !folder.exists() {
                return Ok(vec![]);
            }
            let mut ids = Vec::new();
            for file in fs::read_dir(folder)
                .map_err(|_| "无法读取恢复列表")?
                .take(1000)
            {
                let file = file.map_err(|_| "无法读取恢复列表")?;
                if !file.file_type().map_err(|_| "恢复文件无效")?.is_file() {
                    continue;
                }
                if let Some(id) = file.path().file_stem().and_then(|v| v.to_str()) {
                    if uuid::Uuid::parse_str(id).is_ok() {
                        ids.push(id.to_owned())
                    }
                }
            }
            Ok(ids)
        })?
        .ok_or_else(|| "素材库已改变".to_string())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEvent {
    time: String,
    code: String,
}

fn add_checklist(
    db: &mut rusqlite::Connection,
    project: &str,
    titles: Vec<String>,
) -> Result<usize, String> {
    if titles.len() > 30
        || titles
            .iter()
            .any(|v| v.trim().is_empty() || v.chars().count() > 300)
    {
        return Err("清单条目无效".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND archived_at IS NULL)",
            [project],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("项目不存在或已归档".into());
    }
    let mut order:i64=tx.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM project_tasks WHERE project_id=?1 AND status='todo'",[project],|r|r.get(0)).map_err(|e|e.to_string())?;
    let mut count = 0;
    let time = chrono::Utc::now().to_rfc3339();
    for title in titles {
        let added=tx.execute("INSERT INTO project_tasks(id,project_id,title,status,priority,sort_order,created_at,updated_at) SELECT ?1,?2,?3,'todo','normal',?4,?5,?5 WHERE NOT EXISTS(SELECT 1 FROM project_tasks WHERE project_id=?2 AND title=?3)",rusqlite::params![uuid::Uuid::new_v4().to_string(),project,title.trim(),order,time]).map_err(|e|e.to_string())?;
        count += added;
        order += added as i64;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(count)
}
#[tauri::command]
pub fn add_project_checklist(
    state: State<AppState>,
    project_id: String,
    titles: Vec<String>,
    library_id: Option<String>,
) -> Result<usize, String> {
    if let Some(id) = library_id {
        return state
            .with_library_mut_if_id(&id, |db, _| add_checklist(db, &project_id, titles))?
            .ok_or("素材库已切换，任务未添加".into());
    }
    state.with_library_mut(|db, _| add_checklist(db, &project_id, titles))
}
#[tauri::command]
pub fn export_diagnostics(path: String, events: Vec<DiagnosticEvent>) -> Result<(), String> {
    if events.len() > 100
        || events.iter().any(|e| {
            chrono::DateTime::parse_from_rfc3339(&e.time).is_err()
                || ![
                    "render-failure",
                    "read-failure",
                    "save-failure",
                    "unhandled-error",
                ]
                .contains(&e.code.as_str())
        })
    {
        return Err("诊断事件无效".into());
    }
    write_json(
        Path::new(&path),
        &serde_json::json!({"format":"dcc-diagnostics-v1","version":env!("CARGO_PKG_VERSION"),"events":events}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_migration_is_idempotent_scoped_and_included_in_backup() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("library.db");
        let db = crate::db::open_database(&path).unwrap();
        db.execute_batch("ALTER TABLE ai_sessions DROP COLUMN actual_tokens; UPDATE schema_meta SET value='18' WHERE key='schema_version';").unwrap();
        migrate_workflows(&db).unwrap();
        migrate_workflows(&db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "19"
        );
        let saved = save_workflow(
            &db,
            WorkflowTemplate {
                id: None,
                name: " UE5 制作 ".into(),
                titles: vec![" 白盒 ".into(), "材质".into()],
            },
        )
        .unwrap();
        assert_eq!(saved.titles, vec!["白盒", "材质"]);
        assert!(save_workflow(
            &db,
            WorkflowTemplate {
                id: saved.id.clone(),
                name: "保留".into(),
                titles: vec![]
            }
        )
        .is_err());
        assert_eq!(
            db.query_row("SELECT name FROM workflow_templates", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "UE5 制作"
        );
        let updated = save_workflow(
            &db,
            WorkflowTemplate {
                id: saved.id.clone(),
                name: "我的流程".into(),
                titles: vec!["材质".into(), "白盒".into()],
            },
        )
        .unwrap();
        assert_eq!(saved.id, updated.id);
        let second = tempfile::tempdir().unwrap();
        let other = crate::db::open_database(&second.path().join("library.db")).unwrap();
        assert_eq!(
            other
                .query_row("SELECT count(*) FROM workflow_templates", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
        assert!(save_workflow(&other, updated).is_err());
        let backup = root.path().join("workflow.dccassetlib");
        crate::backup::export_library(&db, root.path(), &backup).unwrap();
        db.execute("DELETE FROM workflow_templates", []).unwrap();
        let mut active = crate::state::ActiveLibrary {
            base_dir: root.path().into(),
            db_path: path,
            marker: crate::state::LibraryMarker {
                library_id: uuid::Uuid::new_v4().to_string(),
                name: "test".into(),
                format_version: 1,
                created_at: "t".into(),
            },
            connection: Some(db),
        };
        crate::backup::restore_library(&mut active, &backup).unwrap();
        let db = active.connection.as_ref().unwrap();
        assert_eq!(
            db.query_row("SELECT name FROM workflow_templates", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "我的流程"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM project_tasks", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn project_checklist_is_atomic_and_skips_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&root.path().join("library.db")).unwrap();
        db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES('project','test','still','active','now','now')",[]).unwrap();
        assert_eq!(
            add_checklist(
                &mut db,
                "project",
                vec!["UV 检查".into(), "UV 检查".into(), "输出检查".into()]
            )
            .unwrap(),
            2
        );
        assert_eq!(
            add_checklist(&mut db, "project", vec!["UV 检查".into()]).unwrap(),
            0
        );
        assert!(add_checklist(&mut db, "project", vec!["".into()]).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM project_tasks", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn diagnostics_rejects_raw_content_and_unknown_fields() {
        let root = tempfile::tempdir().unwrap();
        assert!(serde_json::from_str::<DiagnosticEvent>(
            r#"{"time":"2026-10-01T00:00:00Z","code":"read-failure","secret":"private"}"#
        )
        .is_err());
        assert!(export_diagnostics(
            root.path().join("log.json").to_string_lossy().into(),
            vec![DiagnosticEvent {
                time: "2026-10-01T00:00:00Z".into(),
                code: "secret text".into()
            }]
        )
        .is_err());
        assert!(!root.path().join("log.json").exists());
    }
    #[test]
    fn recovery_is_atomic_and_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let library = uuid::Uuid::new_v4().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let path = draft_path(dir.path(), &library, &id).unwrap();
        write_json(&path, &serde_json::json!({"version":1})).unwrap();
        write_json(&path, &serde_json::json!({"version":2})).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()
                ["version"],
            2
        );
        assert!(draft_path(dir.path(), "../unsafe", &id).is_err());
        assert!(draft_path(dir.path(), &library, "../unsafe").is_err());
    }
}
