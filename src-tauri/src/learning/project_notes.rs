//! Project-to-note links do not edit note content or own notes.
use crate::{models::Page, state::AppState};
use rusqlite::{params, Connection};
use serde::Serialize;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNote {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub course_count: i64,
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id) {
        Ok(())
    } else {
        Err("项目或笔记ID无效".into())
    }
}
fn project(db: &Connection, id: &str, for_link: bool) -> Result<(), String> {
    let status: String = db
        .query_row("SELECT status FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(|_| "项目不存在")?;
    if for_link && status == "archived" {
        return Err("项目已归档，请先恢复项目".into());
    }
    Ok(())
}
pub fn list_in_library(
    state: &AppState,
    library: &str,
    project_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Page<ProjectNote>, String> {
    uuid(project_id)?;
    if offset < 0 || !(1..=50).contains(&limit) {
        return Err("学习资料分页无效，每页最多50条".into());
    }
    super::excerpt_access::scoped(state, library, |db, _, _| {
        project(db, project_id, false)?;
        let total=db.query_row("SELECT count(*) FROM project_learning_notes p JOIN knowledge_notes n ON n.id=p.note_id WHERE p.project_id=?1 AND n.deleted_at IS NULL",[project_id],|r|r.get(0)).map_err(error)?;
        let mut stmt=db.prepare("SELECT n.id,n.title,n.updated_at,(SELECT count(*) FROM learning_course_notes c WHERE c.note_id=n.id) FROM project_learning_notes p JOIN knowledge_notes n ON n.id=p.note_id WHERE p.project_id=?1 AND n.deleted_at IS NULL ORDER BY n.updated_at DESC,n.id LIMIT ?2 OFFSET ?3").map_err(error)?;
        let items = stmt
            .query_map(params![project_id, limit, offset], |r| {
                Ok(ProjectNote {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    updated_at: r.get(2)?,
                    course_count: r.get(3)?,
                })
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        Ok(Page {
            items,
            total,
            offset,
            limit,
        })
    })
}
pub fn link_in_library(
    state: &AppState,
    library: &str,
    project_id: &str,
    note_id: &str,
) -> Result<(), String> {
    uuid(project_id)?;
    uuid(note_id)?;
    super::excerpt_access::scoped(state, library, |db, _, _| {
        let tx = db.unchecked_transaction().map_err(error)?;
        project(&tx, project_id, true)?;
        let available: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL)",
                [note_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if !available {
            return Err("笔记不存在或位于回收站".into());
        }
        tx.execute("INSERT OR IGNORE INTO project_learning_notes(project_id,note_id,created_at) VALUES(?1,?2,datetime('now'))",params![project_id,note_id]).map_err(error)?;
        tx.commit().map_err(error)
    })
}
pub fn unlink_in_library(
    state: &AppState,
    library: &str,
    project_id: &str,
    note_id: &str,
) -> Result<(), String> {
    uuid(project_id)?;
    uuid(note_id)?;
    super::excerpt_access::scoped(state, library, |db, _, _| {
        let tx = db.unchecked_transaction().map_err(error)?;
        project(&tx, project_id, false)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1)",
                [note_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if !exists {
            return Err("笔记不存在".into());
        }
        tx.execute(
            "DELETE FROM project_learning_notes WHERE project_id=?1 AND note_id=?2",
            params![project_id, note_id],
        )
        .map_err(error)?;
        tx.commit().map_err(error)
    })
}
#[tauri::command]
pub async fn list_project_learning_notes(
    app: tauri::AppHandle,
    expected_library_id: String,
    project_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<ProjectNote>, String> {
    super::excerpt_access::job(app, expected_library_id, move |state, library| {
        list_in_library(state, library, &project_id, offset, limit)
    })
    .await
}
#[tauri::command]
pub async fn link_project_learning_note(
    app: tauri::AppHandle,
    expected_library_id: String,
    project_id: String,
    note_id: String,
) -> Result<(), String> {
    super::excerpt_access::job(app, expected_library_id, move |state, library| {
        link_in_library(state, library, &project_id, &note_id)
    })
    .await
}
#[tauri::command]
pub async fn unlink_project_learning_note(
    app: tauri::AppHandle,
    expected_library_id: String,
    project_id: String,
    note_id: String,
) -> Result<(), String> {
    super::excerpt_access::job(app, expected_library_id, move |state, library| {
        unlink_in_library(state, library, &project_id, &note_id)
    })
    .await
}
