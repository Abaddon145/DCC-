use crate::models::*;
use chrono::Utc;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension, Row};
use std::{collections::HashSet, path::Path};
use uuid::Uuid;

const PROJECT_TYPES: [&str; 3] = ["still", "scene", "animation"];
const PROJECT_STATUSES: [&str; 5] = ["planning", "active", "paused", "completed", "archived"];
const ASSET_STATUSES: [&str; 4] = ["candidate", "selected", "used", "rejected"];
const TASK_STATUSES: [&str; 4] = ["todo", "in_progress", "review", "done"];
const TASK_PRIORITIES: [&str; 4] = ["low", "normal", "high", "urgent"];
const PATH_KINDS: [&str; 4] = ["root", "project_file", "output", "custom"];
const FILE_EXTENSIONS: [&str; 15] = [
    "uproject", "blend", "hip", "hiplc", "hipnc", "ma", "mb", "max", "c4d", "spp", "sbs", "sbsar",
    "ztl", "psd", "psb",
];

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn json_vec(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}
fn json_text(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".into())
}
fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn valid_project_input(input: &ProjectInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("项目名称不能为空".into());
    }
    if !PROJECT_TYPES.contains(&input.project_type.as_str()) {
        return Err("项目类型无效".into());
    }
    if !PROJECT_STATUSES.contains(&input.status.as_str()) {
        return Err("项目状态无效".into());
    }
    if input.resolution_width.is_some_and(|v| v <= 0)
        || input.resolution_height.is_some_and(|v| v <= 0)
    {
        return Err("项目分辨率必须大于 0".into());
    }
    if input
        .frame_rate
        .is_some_and(|v| !(0.1..=1000.0).contains(&v))
    {
        return Err("项目帧率必须在 0.1–1000 之间".into());
    }
    Ok(())
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectSummary> {
    let task_count: i64 = row.get(14)?;
    let completed: i64 = row.get(15)?;
    Ok(ProjectSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        project_type: row.get(3)?,
        status: row.get(4)?,
        target_tools: json_vec(row.get(5)?),
        versions: json_vec(row.get(6)?),
        resolution_width: row.get(7)?,
        resolution_height: row.get(8)?,
        frame_rate: row.get(9)?,
        cover_asset_id: row.get(10)?,
        cover_image_id: row.get(11)?,
        asset_count: row.get(12)?,
        unavailable_asset_count: row.get(13)?,
        task_count,
        completed_task_count: completed,
        review_task_count: row.get(16)?,
        progress: if task_count == 0 {
            0
        } else {
            completed * 100 / task_count
        },
        main_board_id: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
        last_opened_at: row.get(20)?,
        archived_at: row.get(21)?,
    })
}

const SUMMARY_SELECT: &str = "SELECT p.id,p.name,p.description,p.project_type,p.status,p.target_tools_json,p.versions_json,p.resolution_width,p.resolution_height,p.frame_rate,p.cover_asset_id,
 (SELECT i.id FROM images i WHERE i.asset_id=p.cover_asset_id ORDER BY i.is_cover DESC,i.sort_order LIMIT 1),
 (SELECT COUNT(*) FROM project_assets pa WHERE pa.project_id=p.id),
 (SELECT COUNT(*) FROM project_assets pa JOIN assets a ON a.id=pa.asset_id WHERE pa.project_id=p.id AND a.deleted_at IS NOT NULL),
 (SELECT COUNT(*) FROM project_tasks pt WHERE pt.project_id=p.id),
 (SELECT COUNT(*) FROM project_tasks pt WHERE pt.project_id=p.id AND pt.status='done'),
 (SELECT COUNT(*) FROM project_tasks pt WHERE pt.project_id=p.id AND pt.status='review'),
 (SELECT board_id FROM project_reference_boards prb WHERE prb.project_id=p.id AND prb.is_main=1 LIMIT 1),
 p.created_at,p.updated_at,p.last_opened_at,p.archived_at FROM projects p";

pub fn list_projects(
    connection: &Connection,
    include_archived: bool,
) -> Result<Vec<ProjectSummary>, String> {
    let sql = format!("{SUMMARY_SELECT} {} ORDER BY CASE p.status WHEN 'active' THEN 0 WHEN 'planning' THEN 1 WHEN 'paused' THEN 2 WHEN 'completed' THEN 3 ELSE 4 END,p.updated_at DESC", if include_archived { "" } else { "WHERE p.status<>'archived'" });
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let values = statement
        .query_map([], summary_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn get_summary(connection: &Connection, id: &str) -> Result<ProjectSummary, String> {
    connection
        .query_row(
            &format!("{SUMMARY_SELECT} WHERE p.id=?1"),
            [id],
            summary_from_row,
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "项目不存在".into())
}

fn unit_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectUnit> {
    Ok(ProjectUnit {
        id: row.get(0)?,
        project_id: row.get(1)?,
        parent_id: row.get(2)?,
        kind: row.get(3)?,
        name: row.get(4)?,
        description: row.get(5)?,
        start_frame: row.get(6)?,
        end_frame: row.get(7)?,
        resolution_width: row.get(8)?,
        resolution_height: row.get(9)?,
        frame_rate: row.get(10)?,
        sort_order: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn list_units(connection: &Connection, project_id: &str) -> Result<Vec<ProjectUnit>, String> {
    let mut statement = connection.prepare("SELECT id,project_id,parent_id,kind,name,description,start_frame,end_frame,resolution_width,resolution_height,frame_rate,sort_order,created_at,updated_at FROM project_units WHERE project_id=?1 ORDER BY CASE kind WHEN 'scene' THEN 0 ELSE 1 END,sort_order,name").map_err(|e| e.to_string())?;
    let values = statement
        .query_map([project_id], unit_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn task_from_row(connection: &Connection, row: &Row<'_>) -> rusqlite::Result<ProjectTask> {
    let id: String = row.get(0)?;
    let asset_ids = {
        let mut statement = connection.prepare(
            "SELECT asset_id FROM project_task_assets WHERE task_id=?1 ORDER BY asset_id",
        )?;
        let values = statement
            .query_map([&id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        values
    };
    Ok(ProjectTask {
        id,
        project_id: row.get(1)?,
        unit_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        due_date: row.get(7)?,
        sort_order: row.get(8)?,
        asset_ids,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn list_tasks(connection: &Connection, project_id: &str) -> Result<Vec<ProjectTask>, String> {
    let mut statement = connection.prepare("SELECT id,project_id,unit_id,title,description,status,priority,due_date,sort_order,created_at,updated_at FROM project_tasks WHERE project_id=?1 ORDER BY status,sort_order,created_at").map_err(|e| e.to_string())?;
    let values = statement
        .query_map([project_id], |row| task_from_row(connection, row))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn list_assets(
    connection: &Connection,
    project_id: &str,
    language: &str,
) -> Result<Vec<ProjectAssetLink>, String> {
    let locale = if language == "en" { "en" } else { "zh-CN" };
    let fallback = if locale == "en" { "zh-CN" } else { "en" };
    let sql = format!("SELECT pa.asset_id,COALESCE(NULLIF(lr.name,''),NULLIF(lf.name,''),a.name),
      (SELECT i.id FROM images i WHERE i.asset_id=a.id ORDER BY i.is_cover DESC,i.sort_order LIMIT 1),c.name,pa.status,pa.purpose,pa.note,
      COALESCE((SELECT json_group_array(unit_id) FROM project_asset_units pau WHERE pau.project_id=pa.project_id AND pau.asset_id=pa.asset_id),'[]'),
      CASE WHEN a.deleted_at IS NULL THEN 0 ELSE 1 END,pa.updated_at
      FROM project_assets pa JOIN assets a ON a.id=pa.asset_id LEFT JOIN categories c ON c.id=a.category_id
      LEFT JOIN asset_localizations lr ON lr.asset_id=a.id AND lr.locale='{locale}' LEFT JOIN asset_localizations lf ON lf.asset_id=a.id AND lf.locale='{fallback}'
      WHERE pa.project_id=?1 ORDER BY pa.updated_at DESC");
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let values = statement
        .query_map([project_id], |row| {
            Ok(ProjectAssetLink {
                asset_id: row.get(0)?,
                name: row.get(1)?,
                cover_image_id: row.get(2)?,
                category_name: row.get(3)?,
                status: row.get(4)?,
                purpose: row.get(5)?,
                note: row.get(6)?,
                unit_ids: json_vec(row.get(7)?),
                unavailable: row.get::<_, i64>(8)? != 0,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn list_boards(connection: &Connection, project_id: &str) -> Result<Vec<ProjectBoardLink>, String> {
    let mut statement = connection.prepare("SELECT prb.board_id,rb.name,prb.is_main,prb.sort_order,(SELECT COUNT(*) FROM reference_board_items rbi WHERE rbi.board_id=rb.id AND rbi.deleted_at IS NULL) FROM project_reference_boards prb JOIN reference_boards rb ON rb.id=prb.board_id WHERE prb.project_id=?1 ORDER BY prb.is_main DESC,prb.sort_order,rb.name").map_err(|e| e.to_string())?;
    let values = statement
        .query_map([project_id], |row| {
            Ok(ProjectBoardLink {
                board_id: row.get(0)?,
                name: row.get(1)?,
                is_main: row.get::<_, i64>(2)? != 0,
                sort_order: row.get(3)?,
                item_count: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn list_paths(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<ProjectPathShortcut>, String> {
    let mut statement = connection.prepare("SELECT id,project_id,kind,label,path,path_type,sort_order,created_at,updated_at FROM project_paths WHERE project_id=?1 ORDER BY sort_order,label").map_err(|e| e.to_string())?;
    let values = statement
        .query_map([project_id], |row| {
            let value: String = row.get(4)?;
            Ok(ProjectPathShortcut {
                id: row.get(0)?,
                project_id: row.get(1)?,
                kind: row.get(2)?,
                label: row.get(3)?,
                available: path_matches(&value, &row.get::<_, String>(5)?),
                path: value,
                path_type: row.get(5)?,
                sort_order: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

pub fn get_project(
    connection: &Connection,
    id: &str,
    language: &str,
    mark_opened: bool,
) -> Result<CreativeProject, String> {
    if mark_opened {
        connection
            .execute(
                "UPDATE projects SET last_opened_at=?1 WHERE id=?2",
                params![now(), id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(CreativeProject {
        summary: get_summary(connection, id)?,
        units: list_units(connection, id)?,
        tasks: list_tasks(connection, id)?,
        assets: list_assets(connection, id, language)?,
        boards: list_boards(connection, id)?,
        paths: list_paths(connection, id)?,
    })
}

pub fn upsert_project(
    connection: &mut Connection,
    input: ProjectInput,
) -> Result<CreativeProject, String> {
    valid_project_input(&input)?;
    if let Some(cover) = input.cover_asset_id.as_deref() {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
                [cover],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            != 0;
        if !exists {
            return Err("封面素材不存在".into());
        }
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    let archived_at = if input.status == "archived" {
        Some(timestamp.clone())
    } else {
        None
    };
    connection.execute("INSERT INTO projects(id,name,description,project_type,status,target_tools_json,versions_json,resolution_width,resolution_height,frame_rate,cover_asset_id,created_at,updated_at,archived_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?13)
      ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,project_type=excluded.project_type,status=excluded.status,target_tools_json=excluded.target_tools_json,versions_json=excluded.versions_json,resolution_width=excluded.resolution_width,resolution_height=excluded.resolution_height,frame_rate=excluded.frame_rate,cover_asset_id=excluded.cover_asset_id,updated_at=excluded.updated_at,archived_at=CASE WHEN excluded.status='archived' THEN COALESCE(projects.archived_at,excluded.archived_at) ELSE NULL END",
      params![id,input.name.trim(),input.description.trim(),input.project_type,input.status,json_text(&input.target_tools),json_text(&input.versions),input.resolution_width,input.resolution_height,input.frame_rate,input.cover_asset_id,timestamp,archived_at]).map_err(|e| format!("保存项目失败：{e}"))?;
    get_project(connection, &id, "zh-CN", false)
}

pub fn archive_project(connection: &Connection, id: &str, archived: bool) -> Result<(), String> {
    let changed = connection
        .execute(
            "UPDATE projects SET status=?1,archived_at=?2,updated_at=?3 WHERE id=?4",
            params![
                if archived { "archived" } else { "planning" },
                if archived { Some(now()) } else { None },
                now(),
                id
            ],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("项目不存在".into());
    }
    Ok(())
}

pub fn delete_project(connection: &Connection, id: &str) -> Result<(), String> {
    let status = connection
        .query_row("SELECT status FROM projects WHERE id=?1", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("项目不存在")?;
    if status != "archived" {
        return Err("只有已归档项目才能永久删除".into());
    }
    connection
        .execute("DELETE FROM projects WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn add_project_assets(
    connection: &mut Connection,
    project_id: &str,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    if asset_ids.is_empty() {
        return Ok(0);
    }
    if asset_ids.len() > 5000 {
        return Err("一次最多加入 5000 项素材".into());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    ensure_project(&transaction, project_id)?;
    let timestamp = now();
    let mut count = 0;
    for id in HashSet::<String>::from_iter(asset_ids) {
        let exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
                [&id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            != 0;
        if !exists {
            continue;
        }
        count += transaction.execute("INSERT OR IGNORE INTO project_assets(project_id,asset_id,status,created_at,updated_at) VALUES(?1,?2,'candidate',?3,?3)", params![project_id,id,timestamp]).map_err(|e| e.to_string())?;
    }
    transaction
        .execute(
            "UPDATE projects SET updated_at=?1 WHERE id=?2",
            params![timestamp, project_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(count)
}

pub fn update_project_assets(
    connection: &mut Connection,
    update: ProjectAssetUpdate,
) -> Result<usize, String> {
    if update.asset_ids.is_empty() {
        return Ok(0);
    }
    if let Some(status) = update.status.as_deref() {
        if !ASSET_STATUSES.contains(&status) {
            return Err("项目素材状态无效".into());
        }
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    ensure_project(&transaction, &update.project_id)?;
    if let Some(unit_ids) = update.unit_ids.as_ref() {
        validate_units(&transaction, &update.project_id, unit_ids)?;
    }
    let timestamp = now();
    let mut changed = 0;
    for asset_id in HashSet::<String>::from_iter(update.asset_ids) {
        changed += transaction.execute("UPDATE project_assets SET status=COALESCE(?1,status),purpose=COALESCE(?2,purpose),note=COALESCE(?3,note),updated_at=?4 WHERE project_id=?5 AND asset_id=?6", params![update.status,update.purpose,update.note,timestamp,update.project_id,asset_id]).map_err(|e| e.to_string())?;
        if let Some(unit_ids) = update.unit_ids.as_ref() {
            transaction
                .execute(
                    "DELETE FROM project_asset_units WHERE project_id=?1 AND asset_id=?2",
                    params![update.project_id, asset_id],
                )
                .map_err(|e| e.to_string())?;
            for unit_id in unit_ids {
                transaction.execute("INSERT INTO project_asset_units(project_id,asset_id,unit_id) VALUES(?1,?2,?3)", params![update.project_id,asset_id,unit_id]).map_err(|e| e.to_string())?;
            }
        }
    }
    transaction
        .execute(
            "UPDATE projects SET updated_at=?1 WHERE id=?2",
            params![timestamp, update.project_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

pub fn remove_project_assets(
    connection: &mut Connection,
    project_id: &str,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    if asset_ids.is_empty() {
        return Ok(0);
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let sql = format!(
        "DELETE FROM project_assets WHERE project_id=? AND asset_id IN ({})",
        placeholders(asset_ids.len())
    );
    let mut values = vec![Value::Text(project_id.into())];
    values.extend(asset_ids.into_iter().map(Value::Text));
    let changed = transaction
        .execute(&sql, params_from_iter(values.iter()))
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE projects SET updated_at=?1 WHERE id=?2",
            params![now(), project_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

fn ensure_project(connection: &Connection, id: &str) -> Result<(), String> {
    if connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        == 0
    {
        Err("项目不存在".into())
    } else {
        Ok(())
    }
}

fn validate_units(connection: &Connection, project_id: &str, ids: &[String]) -> Result<(), String> {
    for id in ids {
        let valid = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_units WHERE id=?1 AND project_id=?2)",
                params![id, project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            != 0;
        if !valid {
            return Err("关联的场景或镜头不存在".into());
        }
    }
    Ok(())
}

pub fn upsert_project_unit(
    connection: &Connection,
    input: ProjectUnitInput,
) -> Result<ProjectUnit, String> {
    if input.name.trim().is_empty() {
        return Err("场景或镜头名称不能为空".into());
    }
    if !matches!(input.kind.as_str(), "scene" | "shot") {
        return Err("项目单元类型无效".into());
    }
    ensure_project(connection, &input.project_id)?;
    if let Some(id) = input.id.as_deref() {
        let owner = connection
            .query_row(
                "SELECT project_id FROM project_units WHERE id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if owner
            .as_deref()
            .is_some_and(|value| value != input.project_id.as_str())
        {
            return Err("不能把场景或镜头转移到其他项目".into());
        }
    }
    if input.kind == "scene" && input.parent_id.is_some() {
        return Err("场景不能拥有父级".into());
    }
    if input.kind == "shot" {
        let parent = input.parent_id.as_deref().ok_or("镜头必须属于一个场景")?;
        let valid = connection.query_row("SELECT EXISTS(SELECT 1 FROM project_units WHERE id=?1 AND project_id=?2 AND kind='scene')", params![parent,input.project_id], |row| row.get::<_, i64>(0)).map_err(|e| e.to_string())? != 0;
        if !valid {
            return Err("镜头的父场景不存在".into());
        }
    }
    if let (Some(start), Some(end)) = (input.start_frame, input.end_frame) {
        if end < start {
            return Err("结束帧不能小于起始帧".into());
        }
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    let sort_order = input.sort_order.unwrap_or_else(|| connection.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM project_units WHERE project_id=?1 AND parent_id IS ?2", params![input.project_id,input.parent_id], |row| row.get(0)).unwrap_or(0));
    connection.execute("INSERT INTO project_units(id,project_id,parent_id,kind,name,description,start_frame,end_frame,resolution_width,resolution_height,frame_rate,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)
      ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,kind=excluded.kind,name=excluded.name,description=excluded.description,start_frame=excluded.start_frame,end_frame=excluded.end_frame,resolution_width=excluded.resolution_width,resolution_height=excluded.resolution_height,frame_rate=excluded.frame_rate,sort_order=excluded.sort_order,updated_at=excluded.updated_at",
      params![id,input.project_id,input.parent_id,input.kind,input.name.trim(),input.description.trim(),input.start_frame,input.end_frame,input.resolution_width,input.resolution_height,input.frame_rate,sort_order,timestamp]).map_err(|e| e.to_string())?;
    connection.query_row("SELECT id,project_id,parent_id,kind,name,description,start_frame,end_frame,resolution_width,resolution_height,frame_rate,sort_order,created_at,updated_at FROM project_units WHERE id=?1", [&id], unit_from_row).map_err(|e| e.to_string())
}

pub fn delete_project_unit(connection: &Connection, id: &str) -> Result<(i64, i64), String> {
    let impact = connection.query_row("WITH RECURSIVE tree(id) AS (SELECT id FROM project_units WHERE id=?1 UNION ALL SELECT u.id FROM project_units u JOIN tree t ON u.parent_id=t.id) SELECT COUNT(*),(SELECT COUNT(*) FROM project_tasks WHERE unit_id IN tree) FROM tree", [id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|e| e.to_string())?.ok_or("场景或镜头不存在")?;
    connection
        .execute("DELETE FROM project_units WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(impact)
}

pub fn reorder_project_units(
    connection: &mut Connection,
    project_id: &str,
    parent_id: Option<String>,
    ids: Vec<String>,
) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for (index, id) in ids.iter().enumerate() {
        let changed = transaction.execute("UPDATE project_units SET sort_order=?1,updated_at=?2 WHERE id=?3 AND project_id=?4 AND parent_id IS ?5", params![index as i64,now(),id,project_id,parent_id]).map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("场景或镜头排序列表无效".into());
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

pub fn upsert_project_task(
    connection: &mut Connection,
    input: ProjectTaskInput,
) -> Result<ProjectTask, String> {
    if input.title.trim().is_empty() {
        return Err("任务标题不能为空".into());
    }
    if !TASK_STATUSES.contains(&input.status.as_str())
        || !TASK_PRIORITIES.contains(&input.priority.as_str())
    {
        return Err("任务状态或优先级无效".into());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    ensure_project(&transaction, &input.project_id)?;
    if let Some(id) = input.id.as_deref() {
        let owner = transaction
            .query_row(
                "SELECT project_id FROM project_tasks WHERE id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if owner
            .as_deref()
            .is_some_and(|value| value != input.project_id.as_str())
        {
            return Err("不能把任务转移到其他项目".into());
        }
    }
    if let Some(unit) = input.unit_id.as_deref() {
        validate_units(&transaction, &input.project_id, &[unit.into()])?;
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    let sort_order = input.sort_order.unwrap_or_else(|| transaction.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM project_tasks WHERE project_id=?1 AND status=?2", params![input.project_id,input.status], |row| row.get(0)).unwrap_or(0));
    transaction.execute("INSERT INTO project_tasks(id,project_id,unit_id,title,description,status,priority,due_date,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)
      ON CONFLICT(id) DO UPDATE SET unit_id=excluded.unit_id,title=excluded.title,description=excluded.description,status=excluded.status,priority=excluded.priority,due_date=excluded.due_date,sort_order=excluded.sort_order,updated_at=excluded.updated_at",
      params![id,input.project_id,input.unit_id,input.title.trim(),input.description.trim(),input.status,input.priority,input.due_date,sort_order,timestamp]).map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM project_task_assets WHERE task_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    for asset_id in HashSet::<String>::from_iter(input.asset_ids) {
        let linked = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_assets WHERE project_id=?1 AND asset_id=?2)",
                params![input.project_id, asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            != 0;
        if linked {
            transaction
                .execute(
                    "INSERT INTO project_task_assets(task_id,asset_id) VALUES(?1,?2)",
                    params![id, asset_id],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())?;
    list_tasks(connection, &input.project_id)?
        .into_iter()
        .find(|value| value.id == id)
        .ok_or_else(|| "任务保存失败".into())
}

pub fn delete_project_task(connection: &Connection, id: &str) -> Result<(), String> {
    if connection
        .execute("DELETE FROM project_tasks WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        Err("任务不存在".into())
    } else {
        Ok(())
    }
}

pub fn move_project_task(
    connection: &mut Connection,
    id: &str,
    status: &str,
    target_index: i64,
) -> Result<(), String> {
    if !TASK_STATUSES.contains(&status) {
        return Err("任务状态无效".into());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let project_id = transaction
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id=?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("任务不存在")?;
    let mut ids = {
        let mut statement = transaction.prepare("SELECT id FROM project_tasks WHERE project_id=?1 AND status=?2 AND id<>?3 ORDER BY sort_order,created_at").map_err(|e| e.to_string())?;
        let values = statement
            .query_map(params![project_id, status, id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    ids.insert((target_index.max(0) as usize).min(ids.len()), id.into());
    let timestamp = now();
    for (index, task_id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE project_tasks SET status=?1,sort_order=?2,updated_at=?3 WHERE id=?4",
                params![status, index as i64, timestamp, task_id],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction
        .execute(
            "UPDATE projects SET updated_at=?1 WHERE id=?2",
            params![timestamp, project_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())
}

pub fn set_project_task_assets(
    connection: &mut Connection,
    task_id: &str,
    asset_ids: Vec<String>,
) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let project_id = transaction
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id=?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("任务不存在")?;
    transaction
        .execute(
            "DELETE FROM project_task_assets WHERE task_id=?1",
            [task_id],
        )
        .map_err(|e| e.to_string())?;
    for asset_id in HashSet::<String>::from_iter(asset_ids) {
        let linked = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_assets WHERE project_id=?1 AND asset_id=?2)",
                params![project_id, asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            != 0;
        if linked {
            transaction
                .execute(
                    "INSERT INTO project_task_assets(task_id,asset_id) VALUES(?1,?2)",
                    params![task_id, asset_id],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

pub fn link_project_board(
    connection: &mut Connection,
    project_id: &str,
    board_id: &str,
    main: bool,
) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    ensure_project(&transaction, project_id)?;
    let board = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM reference_boards WHERE id=?1)",
            [board_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        != 0;
    if !board {
        return Err("参考板不存在".into());
    }
    if main {
        transaction
            .execute(
                "UPDATE project_reference_boards SET is_main=0 WHERE project_id=?1",
                [project_id],
            )
            .map_err(|e| e.to_string())?;
    }
    let order: i64 = transaction.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM project_reference_boards WHERE project_id=?1", [project_id], |row| row.get(0)).unwrap_or(0);
    transaction.execute("INSERT INTO project_reference_boards(project_id,board_id,is_main,sort_order,created_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(project_id,board_id) DO UPDATE SET is_main=excluded.is_main", params![project_id,board_id,main as i64,order,now()]).map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())
}

pub fn unlink_project_board(
    connection: &Connection,
    project_id: &str,
    board_id: &str,
) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM project_reference_boards WHERE project_id=?1 AND board_id=?2",
            params![project_id, board_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn set_main_project_board(
    connection: &mut Connection,
    project_id: &str,
    board_id: &str,
) -> Result<(), String> {
    link_project_board(connection, project_id, board_id, true)
}

fn validate_path(input: &ProjectPathInput) -> Result<(), String> {
    if !PATH_KINDS.contains(&input.kind.as_str()) {
        return Err("快捷方式类型无效".into());
    }
    if input.label.trim().is_empty() {
        return Err("快捷方式名称不能为空".into());
    }
    if !matches!(input.path_type.as_str(), "file" | "directory") {
        return Err("路径类型无效".into());
    }
    let path = Path::new(input.path.trim());
    if !path.is_absolute() {
        return Err("只允许保存绝对路径".into());
    }
    if input.path_type == "file" {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !FILE_EXTENSIONS.contains(&extension.as_str()) {
            return Err("不支持该工程文件格式，禁止保存可执行文件或脚本".into());
        }
    }
    Ok(())
}

fn path_matches(value: &str, path_type: &str) -> bool {
    let path = Path::new(value);
    if path_type == "directory" {
        path.is_dir()
    } else {
        path.is_file()
    }
}

pub fn upsert_project_path(
    connection: &Connection,
    input: ProjectPathInput,
) -> Result<ProjectPathShortcut, String> {
    validate_path(&input)?;
    ensure_project(connection, &input.project_id)?;
    if let Some(id) = input.id.as_deref() {
        let owner = connection
            .query_row(
                "SELECT project_id FROM project_paths WHERE id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if owner
            .as_deref()
            .is_some_and(|value| value != input.project_id.as_str())
        {
            return Err("不能把快捷方式转移到其他项目".into());
        }
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    let order = input.sort_order.unwrap_or_else(|| {
        connection
            .query_row(
                "SELECT COALESCE(MAX(sort_order),-1)+1 FROM project_paths WHERE project_id=?1",
                [&input.project_id],
                |row| row.get(0),
            )
            .unwrap_or(0)
    });
    connection.execute("INSERT INTO project_paths(id,project_id,kind,label,path,path_type,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,label=excluded.label,path=excluded.path,path_type=excluded.path_type,sort_order=excluded.sort_order,updated_at=excluded.updated_at", params![id,input.project_id,input.kind,input.label.trim(),input.path.trim(),input.path_type,order,timestamp]).map_err(|e| e.to_string())?;
    list_paths(connection, &input.project_id)?
        .into_iter()
        .find(|value| value.id == id)
        .ok_or_else(|| "快捷方式保存失败".into())
}

pub fn delete_project_path(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM project_paths WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn check_project_path(connection: &Connection, id: &str) -> Result<ProjectPathCheck, String> {
    let value = connection
        .query_row(
            "SELECT path,path_type FROM project_paths WHERE id=?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("快捷方式不存在")?;
    let available = path_matches(&value.0, &value.1);
    Ok(ProjectPathCheck {
        id: id.into(),
        available,
        message: if available {
            "路径可用".into()
        } else {
            "路径已失效，请重新定位".into()
        },
    })
}

pub fn open_project_path(connection: &Connection, id: &str) -> Result<(), String> {
    let value = connection
        .query_row(
            "SELECT project_id,kind,label,path,path_type FROM project_paths WHERE id=?1",
            [id],
            |row| {
                Ok(ProjectPathInput {
                    id: Some(id.into()),
                    project_id: row.get(0)?,
                    kind: row.get(1)?,
                    label: row.get(2)?,
                    path: row.get(3)?,
                    path_type: row.get(4)?,
                    sort_order: None,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("快捷方式不存在")?;
    validate_path(&value)?;
    if !path_matches(&value.path, &value.path_type) {
        return Err("路径已失效，请重新定位".into());
    }
    open::that(&value.path).map_err(|e| format!("无法打开路径：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn project() -> ProjectInput {
        ProjectInput {
            id: None,
            name: "镜头测试".into(),
            description: String::new(),
            project_type: "animation".into(),
            status: "active".into(),
            target_tools: vec!["Unreal Engine".into()],
            versions: vec!["5.5".into()],
            resolution_width: Some(1920),
            resolution_height: Some(1080),
            frame_rate: Some(24.0),
            cover_asset_id: None,
        }
    }

    #[test]
    fn v9_project_crud_and_archive_guard() {
        let dir = tempdir().unwrap();
        let mut connection = db::open_database(&dir.path().join("library.sqlite3")).unwrap();
        let saved = upsert_project(&mut connection, project()).unwrap();
        assert_eq!(saved.summary.name, "镜头测试");
        assert!(delete_project(&connection, &saved.summary.id).is_err());
        archive_project(&connection, &saved.summary.id, true).unwrap();
        delete_project(&connection, &saved.summary.id).unwrap();
        assert!(list_projects(&connection, true).unwrap().is_empty());
    }

    #[test]
    fn project_units_and_tasks_cascade() {
        let dir = tempdir().unwrap();
        let mut connection = db::open_database(&dir.path().join("library.sqlite3")).unwrap();
        let id = upsert_project(&mut connection, project())
            .unwrap()
            .summary
            .id;
        let scene = upsert_project_unit(
            &connection,
            ProjectUnitInput {
                id: None,
                project_id: id.clone(),
                parent_id: None,
                kind: "scene".into(),
                name: "场景一".into(),
                description: String::new(),
                start_frame: None,
                end_frame: None,
                resolution_width: None,
                resolution_height: None,
                frame_rate: None,
                sort_order: None,
            },
        )
        .unwrap();
        let shot = upsert_project_unit(
            &connection,
            ProjectUnitInput {
                id: None,
                project_id: id.clone(),
                parent_id: Some(scene.id.clone()),
                kind: "shot".into(),
                name: "SH010".into(),
                description: String::new(),
                start_frame: Some(1),
                end_frame: Some(100),
                resolution_width: None,
                resolution_height: None,
                frame_rate: None,
                sort_order: None,
            },
        )
        .unwrap();
        upsert_project_task(
            &mut connection,
            ProjectTaskInput {
                id: None,
                project_id: id.clone(),
                unit_id: Some(shot.id),
                title: "灯光".into(),
                description: String::new(),
                status: "todo".into(),
                priority: "high".into(),
                due_date: None,
                sort_order: None,
                asset_ids: vec![],
            },
        )
        .unwrap();
        let impact = delete_project_unit(&connection, &scene.id).unwrap();
        assert_eq!(impact, (2, 1));
        assert!(get_project(&connection, &id, "zh-CN", false)
            .unwrap()
            .tasks
            .is_empty());
    }

    #[test]
    fn rejects_executable_shortcuts() {
        let input = ProjectPathInput {
            id: None,
            project_id: "p".into(),
            kind: "project_file".into(),
            label: "危险".into(),
            path: if cfg!(windows) {
                "C:\\temp\\run.exe".into()
            } else {
                "/tmp/run.exe".into()
            },
            path_type: "file".into(),
            sort_order: None,
        };
        assert!(validate_path(&input).is_err());
    }

    #[test]
    fn asset_links_survive_soft_delete_and_cascade_on_purge() {
        let dir = tempdir().unwrap();
        let mut connection = db::open_database(&dir.path().join("library.sqlite3")).unwrap();
        connection.execute("INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES('asset-1','岩石','','now','now')", []).unwrap();
        let first = upsert_project(&mut connection, project())
            .unwrap()
            .summary
            .id;
        let mut second_input = project();
        second_input.name = "第二项目".into();
        let second = upsert_project(&mut connection, second_input)
            .unwrap()
            .summary
            .id;
        assert_eq!(
            add_project_assets(&mut connection, &first, vec!["asset-1".into()]).unwrap(),
            1
        );
        assert_eq!(
            add_project_assets(&mut connection, &second, vec!["asset-1".into()]).unwrap(),
            1
        );
        connection
            .execute("UPDATE assets SET deleted_at='now' WHERE id='asset-1'", [])
            .unwrap();
        assert!(
            get_project(&connection, &first, "zh-CN", false)
                .unwrap()
                .assets[0]
                .unavailable
        );
        connection
            .execute("UPDATE assets SET deleted_at=NULL WHERE id='asset-1'", [])
            .unwrap();
        assert!(
            !get_project(&connection, &first, "zh-CN", false)
                .unwrap()
                .assets[0]
                .unavailable
        );
        connection
            .execute("DELETE FROM assets WHERE id='asset-1'", [])
            .unwrap();
        assert!(get_project(&connection, &first, "zh-CN", false)
            .unwrap()
            .assets
            .is_empty());
        assert!(get_project(&connection, &second, "zh-CN", false)
            .unwrap()
            .assets
            .is_empty());
    }
}
