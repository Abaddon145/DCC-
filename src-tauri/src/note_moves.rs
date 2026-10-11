use crate::knowledge::{self, NoteInput};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
#[derive(Clone)]
struct Undo {
    token: String,
    library: String,
    before: Value,
    after: Value,
    note: Option<(String, Option<String>, i64)>,
}
static UNDO: OnceLock<Mutex<Option<Undo>>> = OnceLock::new();
pub fn clear_undo() {
    if let Ok(mut undo) = UNDO.get_or_init(|| Mutex::new(None)).lock() {
        *undo = None;
    }
}
fn snapshot(db: &Connection) -> Result<Value, String> {
    let mut q = db
        .prepare("SELECT id,parent_id,sort_order FROM knowledge_notebooks ORDER BY id")
        .map_err(|e| e.to_string())?;
    let v=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"parent":r.get::<_,Option<String>>(1)?,"order":r.get::<_,i64>(2)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(json!(v))
}
fn input(note: knowledge::Note, parent: Option<String>) -> NoteInput {
    NoteInput {
        id: Some(note.id),
        expected_revision: Some(note.revision),
        notebook_id: parent,
        title: note.title,
        document: note.document,
        tags: note.tags,
        favorite: note.favorite,
        journal_date: note.journal_date,
    }
}
pub fn move_item(
    db: &mut Connection,
    library: &str,
    kind: &str,
    id: &str,
    parent: Option<String>,
    index: usize,
) -> Result<String, String> {
    let mut undo = UNDO
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "撤销锁损坏")?;
    let before = snapshot(db)?;
    let mut note_undo = None;
    if kind == "note" {
        let note = knowledge::get(db, id)?;
        if note.deleted_at.is_some() {
            return Err("不能移动回收站笔记".into());
        }
        let previous = note.notebook_id.clone();
        let saved = knowledge::save(db, input(note, parent))?;
        note_undo = Some((saved.id, previous, saved.revision));
    } else if kind == "notebook" {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let old: Option<String> = tx
            .query_row(
                "SELECT parent_id FROM knowledge_notebooks WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|_| "笔记本不存在")?;
        if let Some(p) = &parent {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM knowledge_notebooks WHERE id=?1)",
                    [p],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                return Err("目标笔记本不存在".into());
            }
        }
        let cycle:bool=tx.query_row("WITH RECURSIVE descendants(id) AS (SELECT ?1 UNION SELECT n.id FROM knowledge_notebooks n JOIN descendants d ON n.parent_id=d.id) SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2)",params![id,parent],|r|r.get(0)).map_err(|e|e.to_string())?;
        if cycle {
            return Err("不能移动到自身或后代笔记本".into());
        }
        let siblings = |parent: &Option<String>| -> Result<Vec<String>, String> {
            let mut q=tx.prepare("SELECT id FROM knowledge_notebooks WHERE parent_id IS ?1 AND id<>?2 ORDER BY sort_order,name,id").map_err(|e|e.to_string())?;
            let result = q
                .query_map(params![parent, id], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string());
            result
        };
        let mut next = siblings(&parent)?;
        next.insert(index.min(next.len()), id.into());
        if old != parent {
            for (i, sibling) in siblings(&old)?.iter().enumerate() {
                tx.execute(
                    "UPDATE knowledge_notebooks SET sort_order=?2 WHERE id=?1",
                    params![sibling, i as i64],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        for (i, sibling) in next.iter().enumerate() {
            tx.execute(
                "UPDATE knowledge_notebooks SET parent_id=?2,sort_order=?3 WHERE id=?1",
                params![sibling, parent, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
    } else {
        return Err("未知拖拽类型".into());
    }
    let token = uuid::Uuid::new_v4().to_string();
    *undo = Some(Undo {
        token: token.clone(),
        library: library.into(),
        before,
        after: snapshot(db)?,
        note: note_undo,
    });
    Ok(token)
}
pub fn undo(db: &mut Connection, library: &str, token: &str) -> Result<(), String> {
    let mut slot = UNDO
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "撤销锁损坏")?;
    let record = slot.as_ref().ok_or("没有可撤销的移动")?;
    if record.token != token || record.library != library {
        return Err("撤销记录已失效或素材库已切换".into());
    }
    if snapshot(db)? != record.after {
        return Err("笔记本结构已变化，不能撤销旧移动".into());
    }
    if let Some((id, parent, revision)) = &record.note {
        let note = knowledge::get(db, id)?;
        if note.revision != *revision || note.deleted_at.is_some() {
            return Err("笔记已修改，不能撤销旧移动".into());
        }
        knowledge::save(db, input(note, parent.clone()))?;
    } else {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for row in record.before.as_array().ok_or("撤销快照无效")? {
            tx.execute(
                "UPDATE knowledge_notebooks SET parent_id=?2,sort_order=?3 WHERE id=?1",
                params![
                    row["id"].as_str(),
                    row["parent"].as_str(),
                    row["order"].as_i64()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    *slot = None;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notebook_moves_reorder_reject_cycles_and_undo() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&temp.path().join("library.db")).unwrap();
        let a = knowledge::upsert_notebook(&mut db, None, None, "a", 0).unwrap();
        let b = knowledge::upsert_notebook(&mut db, None, Some(&a.id), "b", 0).unwrap();
        let before = snapshot(&db).unwrap();
        assert!(move_item(&mut db, "test", "notebook", &a.id, Some(b.id.clone()), 0).is_err());
        assert_eq!(snapshot(&db).unwrap(), before);
        let token = move_item(&mut db, "test", "notebook", &b.id, None, 0).unwrap();
        assert!(undo(&mut db, "other", &token).is_err());
        undo(&mut db, "test", &token).unwrap();
        assert_eq!(snapshot(&db).unwrap(), before);
    }
}
