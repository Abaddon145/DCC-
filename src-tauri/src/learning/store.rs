use super::models::*;
use rusqlite::{params, Connection, OptionalExtension};
const COURSE_SELECT:&str="SELECT c.id,c.media_entry_id,COALESCE(NULLIF(c.title_override,''),e.name),c.position_ms,c.duration_ms,c.rate,c.completed,c.progress_revision FROM learning_courses c JOIN media_entries e ON e.id=c.media_entry_id WHERE e.kind='video' AND e.deleted_at IS NULL";
fn uuid(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "课程或系列ID无效".into())
}
fn course_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Course> {
    Ok(Course {
        id: r.get(0)?,
        media_id: r.get(1)?,
        title: r.get(2)?,
        position_ms: r.get(3)?,
        duration_ms: r.get(4)?,
        rate: r.get(5)?,
        completed: r.get(6)?,
        progress_revision: r.get(7)?,
    })
}
pub fn ensure_course(db: &Connection, media: &str) -> Result<Course, String> {
    uuid(media)?;
    let eligible:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM media_entries WHERE id=?1 AND kind='video' AND deleted_at IS NULL)",[media],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !eligible {
        return Err("当前库中没有可用的此视频".into());
    }
    db.execute("INSERT INTO learning_courses(id,media_entry_id,duration_ms,created_at,updated_at) SELECT ?1,e.id,CASE WHEN f.duration_ms>=0 THEN f.duration_ms ELSE NULL END,?2,?2 FROM media_entries e JOIN media_files f ON f.id=e.primary_file_id WHERE e.id=?3 ON CONFLICT(media_entry_id) DO NOTHING",params![uuid::Uuid::new_v4().to_string(),chrono::Utc::now().to_rfc3339(),media]).map_err(|e|e.to_string())?;
    db.query_row(
        &format!("{COURSE_SELECT} AND c.media_entry_id=?1"),
        [media],
        course_row,
    )
    .map_err(|_| "课程创建失败".into())
}
pub fn course(db: &Connection, id: &str) -> Result<Course, String> {
    uuid(id)?;
    db.query_row(&format!("{COURSE_SELECT} AND c.id=?1"), [id], course_row)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "课程不存在、已删除或不属于当前库".into())
}
pub fn list_courses(
    db: &Connection,
    offset: i64,
    limit: i64,
) -> Result<crate::models::Page<Course>, String> {
    let total=db.query_row("SELECT count(*) FROM learning_courses c JOIN media_entries e ON e.id=c.media_entry_id WHERE e.kind='video' AND e.deleted_at IS NULL",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let offset = offset.max(0);
    let limit = limit.clamp(1, 200);
    let mut stmt = db
        .prepare(&format!(
            "{COURSE_SELECT} ORDER BY c.updated_at DESC,c.id LIMIT ?1 OFFSET ?2"
        ))
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params![limit, offset], course_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(crate::models::Page {
        items,
        total,
        offset,
        limit,
    })
}
pub fn series(db: &Connection, id: &str) -> Result<Series, String> {
    uuid(id)?;
    let mut value = db
        .query_row(
            "SELECT id,name,description,revision FROM learning_series WHERE id=?1",
            [id],
            |r| {
                Ok(Series {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    revision: r.get(3)?,
                    ordered_course_ids: vec![],
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("系列不存在或不属于当前库")?;
    let mut stmt=db.prepare("SELECT course_id FROM learning_series_courses WHERE series_id=?1 ORDER BY position,course_id").map_err(|e|e.to_string())?;
    value.ordered_course_ids = stmt
        .query_map([id], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn list_series(db: &Connection) -> Result<Vec<Series>, String> {
    let mut stmt = db
        .prepare("SELECT id FROM learning_series ORDER BY updated_at DESC,id")
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    ids.iter().map(|id| series(db, id)).collect()
}
pub fn upsert_series(
    db: &mut Connection,
    id: Option<&str>,
    name: &str,
    description: &str,
    expected: Option<i64>,
) -> Result<Series, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 || description.len() > 20000 {
        return Err("系列名称需1—200字，说明不能超过20KB".into());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = if let Some(id) = id {
        uuid(id)?;
        if db.execute("UPDATE learning_series SET name=?1,description=?2,revision=revision+1,updated_at=?3 WHERE id=?4 AND revision=?5",params![name,description,now,id,expected.ok_or("缺少系列修订号")?]).map_err(|e|e.to_string())?!=1 {return Err("系列已变化，请重新读取".into());}
        id.to_owned()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO learning_series(id,name,description,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",params![id,name,description,now]).map_err(|e|e.to_string())?;
        id
    };
    series(db, &id)
}
pub fn set_series_members(
    db: &mut Connection,
    id: &str,
    revision: i64,
    courses: &[String],
) -> Result<Series, String> {
    uuid(id)?;
    if courses.len() > 10000 {
        return Err("单系列最多10000课程".into());
    }
    let unique = courses.iter().collect::<std::collections::HashSet<_>>();
    if unique.len() != courses.len() {
        return Err("系列课程不能重复".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for id in courses {
        course(&tx, id)?;
    }
    if tx.execute("UPDATE learning_series SET revision=revision+1,updated_at=?1 WHERE id=?2 AND revision=?3",params![chrono::Utc::now().to_rfc3339(),id,revision]).map_err(|e|e.to_string())?!=1 {return Err("系列顺序已变化，请重新读取".into());}
    tx.execute(
        "DELETE FROM learning_series_courses WHERE series_id=?1",
        [id],
    )
    .map_err(|e| e.to_string())?;
    {
        let mut stmt=tx.prepare("INSERT INTO learning_series_courses(series_id,course_id,position) VALUES(?1,?2,?3)").map_err(|e|e.to_string())?;
        for (position, course) in courses.iter().enumerate() {
            stmt.execute(params![id, course, position as i64])
                .map_err(|e| e.to_string())?;
        }
    }
    let result = series(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn delete_series(db: &Connection, id: &str, revision: i64) -> Result<(), String> {
    uuid(id)?;
    if db
        .execute(
            "DELETE FROM learning_series WHERE id=?1 AND revision=?2",
            params![id, revision],
        )
        .map_err(|e| e.to_string())?
        != 1
    {
        return Err("系列已变化，请重新读取".into());
    }
    Ok(())
}

#[cfg(test)]
mod learning_store_tests {
    use super::*;
    fn media(db: &Connection) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let file = uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','same title',?2,'now','now')",rusqlite::params![id,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4',?3,1000,'now')",rusqlite::params![file,id,format!("media-library/video/{id}/video.mp4")]).unwrap();
        id
    }
    #[test]
    fn learning_course_reuses_media_across_series() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        let media_id = media(&db);
        let a = ensure_course(&db, &media_id).unwrap();
        assert_eq!(ensure_course(&db, &media_id).unwrap().id, a.id);
        let same_title = ensure_course(&db, &media(&db)).unwrap();
        assert_ne!(a.id, same_title.id);
        let x = upsert_series(&mut db, None, "UE入门", "", None).unwrap();
        let y = upsert_series(&mut db, None, "场景练习", "", None).unwrap();
        let x =
            set_series_members(&mut db, &x.id, 0, &[a.id.clone(), same_title.id.clone()]).unwrap();
        let y = set_series_members(&mut db, &y.id, 0, &[a.id.clone()]).unwrap();
        assert_eq!(x.ordered_course_ids[0], y.ordered_course_ids[0]);
        assert_eq!(a.duration_ms, Some(1000));
        assert_eq!(
            crate::media_library::delete_entries(&mut db, vec![media_id.clone()]).unwrap(),
            1
        );
        assert_eq!(course(&db, &a.id).unwrap().id, a.id);
        let batch = crate::db::list_trash(&db, 0, 10).unwrap().items[0]
            .id
            .clone();
        crate::db::restore_trash_batch(&mut db, &batch).unwrap();
        assert_eq!(course(&db, &a.id).unwrap().id, a.id);
        // Simulate an older client changing media outside the current guard.
        db.execute_batch("DROP TRIGGER media_workspace_protect_learning_trash")
            .unwrap();
        db.execute(
            "UPDATE media_entries SET deleted_at='now' WHERE id=?1",
            [&media_id],
        )
        .unwrap();
        assert!(course(&db, &a.id).is_err());
    }
    #[test]
    fn learning_store_rejects_foreign_media_and_order_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
        assert!(ensure_course(&db, &uuid::Uuid::new_v4().to_string()).is_err());
        let a = ensure_course(&db, &media(&db)).unwrap();
        let b = ensure_course(&db, &media(&db)).unwrap();
        let s = upsert_series(&mut db, None, "教程", "", None).unwrap();
        let s = set_series_members(&mut db, &s.id, 0, &[a.id.clone(), b.id.clone()]).unwrap();
        assert!(set_series_members(&mut db, &s.id, 0, &[b.id.clone(), a.id.clone()]).is_err());
        assert!(set_series_members(&mut db, &s.id, 1, &[a.id.clone(), a.id.clone()]).is_err());
        assert!(
            set_series_members(&mut db, &s.id, 1, &[uuid::Uuid::new_v4().to_string()]).is_err()
        );
        let s = set_series_members(&mut db, &s.id, 1, &[b.id.clone(), a.id.clone()]).unwrap();
        assert_eq!(s.ordered_course_ids, vec![b.id, a.id]);
        assert_eq!(s.revision, 2);
    }
    #[test]
    fn learning_store_backup_keeps_course_order_and_existing_project_links() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(&dir.path().join("library"));
        let archive = dir.path().join("learning.dccassetlib");
        let (course_id,series_id,core_id)=state.with_library_mut(|db,base| {
            let media_id=media(db);let c=ensure_course(db,&media_id)?;
            let s=upsert_series(db,None,"Backup","",None)?;
            set_series_members(db,&s.id,0,&[c.id.clone()])?;
            db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES('project','Existing','scene','active','now','now')",[]).unwrap();
            db.execute("INSERT INTO project_media_entries(project_id,entry_id,created_at) VALUES('project',?1,'now')",[&media_id]).unwrap();
            let core=crate::asset::repository::register_origin(db,&crate::asset::models::CoreOrigin{kind:"media".into(),id:media_id.clone()})?;
            crate::asset::repository::register_files(db,&core)?;
            crate::backup::export_library(db,base,&archive)?;
            Ok((c.id,s.id,core))
        }).unwrap();
        state
            .with_active_mut(|active| crate::backup::restore_library(active, &archive))
            .unwrap();
        state.with_library(|db,_| {
            assert_eq!(series(db,&series_id)?.ordered_course_ids,vec![course_id.clone()]);
            let c=course(db,&course_id)?;
            assert_eq!(list_courses(db,0,1)?.items[0].id,c.id);
            assert_eq!(db.query_row("SELECT media_entry_id FROM asset_core WHERE id=?1",[&core_id],|r|r.get::<_,String>(0)).unwrap(),c.media_id);
            assert_eq!(db.query_row("SELECT count(*) FROM project_media_entries WHERE project_id='project' AND entry_id=?1",[&c.media_id],|r|r.get::<_,i64>(0)).unwrap(),1);
            crate::learning::migrate(db)?;
            assert_eq!(ensure_course(db,&c.media_id)?.id,course_id);
            delete_series(db,&series_id,1)?;
            assert_eq!(course(db,&course_id)?.id,course_id,"deleting a series must not delete media/course");
            Ok(())
        }).unwrap();
    }
}
