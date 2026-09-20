use crate::models::{CollectionMutationReport, ManualCollection, ManualCollectionInput};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use uuid::Uuid;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn normalize_siblings(connection: &Connection, parent_id: Option<&str>) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT id FROM manual_collections WHERE parent_id IS ?1 ORDER BY sort_order,name COLLATE NOCASE,id")
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map([parent_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (index, id) in ids.iter().enumerate() {
        connection
            .execute(
                "UPDATE manual_collections SET sort_order=?1 WHERE id=?2",
                params![index as i64, id],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn list(connection: &Connection) -> Result<Vec<ManualCollection>, String> {
    let mut statement = connection.prepare(
        "SELECT c.id,c.parent_id,c.name,c.description,c.cover_asset_id,
         (SELECT i.id FROM images i WHERE i.asset_id=c.cover_asset_id ORDER BY i.is_cover DESC,i.sort_order LIMIT 1),
         c.sort_order,
         (SELECT COUNT(*) FROM manual_collection_assets m WHERE m.collection_id=c.id),
         (WITH RECURSIVE tree(id) AS (SELECT c.id UNION ALL SELECT child.id FROM manual_collections child JOIN tree t ON child.parent_id=t.id)
          SELECT COUNT(DISTINCT m.asset_id) FROM manual_collection_assets m WHERE m.collection_id IN (SELECT id FROM tree)),
         c.created_at,c.updated_at
         FROM manual_collections c ORDER BY c.parent_id,c.sort_order,c.name COLLATE NOCASE")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ManualCollection {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                cover_asset_id: row.get(4)?,
                cover_image_id: row.get(5)?,
                sort_order: row.get(6)?,
                direct_asset_count: row.get(7)?,
                descendant_asset_count: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn ensure_parent(connection: &Connection, id: &str, parent_id: Option<&str>) -> Result<(), String> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    if parent_id == id {
        return Err("合集不能放入自身".into());
    }
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM manual_collections WHERE id=?1)",
            [parent_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if !exists {
        return Err("目标合集不存在".into());
    }
    let cycle = connection.query_row(
        "WITH RECURSIVE tree(id) AS (SELECT id FROM manual_collections WHERE parent_id=?1 UNION ALL SELECT c.id FROM manual_collections c JOIN tree t ON c.parent_id=t.id) SELECT EXISTS(SELECT 1 FROM tree WHERE id=?2)",
        params![id,parent_id], |row| row.get::<_,i64>(0)).map_err(|error| error.to_string())? != 0;
    if cycle {
        return Err("合集不能移动到自己的子合集".into());
    }
    Ok(())
}

fn ensure_unique_name(
    connection: &Connection,
    id: &str,
    parent_id: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let duplicate = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM manual_collections WHERE parent_id IS ?1 AND lower(name)=lower(?2) AND id<>?3)",
        params![parent_id,name,id], |row| row.get::<_,i64>(0)).map_err(|error| error.to_string())? != 0;
    if duplicate {
        Err("同级已存在同名合集".into())
    } else {
        Ok(())
    }
}

pub fn upsert(
    connection: &mut Connection,
    input: ManualCollectionInput,
) -> Result<ManualCollection, String> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("合集名称需为 1–100 个字符".into());
    }
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    ensure_parent(connection, &id, input.parent_id.as_deref())?;
    ensure_unique_name(connection, &id, input.parent_id.as_deref(), name)?;
    if let Some(asset_id) = input.cover_asset_id.as_deref() {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
                [asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?
            != 0;
        if !exists {
            return Err("封面素材不存在".into());
        }
    }
    let timestamp = now();
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let existing: Option<(Option<String>, i64, String)> = transaction
        .query_row(
            "SELECT parent_id,sort_order,created_at FROM manual_collections WHERE id=?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let old_parent = existing.as_ref().and_then(|value| value.0.clone());
    let order = existing.as_ref().map(|value| value.1).unwrap_or_else(|| transaction.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM manual_collections WHERE parent_id IS ?1", [input.parent_id.as_deref()], |row| row.get(0)).unwrap_or(0));
    let created = existing
        .map(|value| value.2)
        .unwrap_or_else(|| timestamp.clone());
    transaction.execute("INSERT INTO manual_collections(id,parent_id,name,description,cover_asset_id,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,name=excluded.name,description=excluded.description,cover_asset_id=excluded.cover_asset_id,updated_at=excluded.updated_at", params![id,input.parent_id,name,input.description.trim(),input.cover_asset_id,order,created,timestamp]).map_err(|error| error.to_string())?;
    normalize_siblings(&transaction, old_parent.as_deref())?;
    normalize_siblings(&transaction, input.parent_id.as_deref())?;
    let affected_assets = {
        let mut statement = transaction
            .prepare("SELECT asset_id FROM manual_collection_assets WHERE collection_id=?1")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([&id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for asset_id in affected_assets {
        crate::db::reindex_asset(&transaction, &asset_id)?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    list(connection)?
        .into_iter()
        .find(|value| value.id == id)
        .ok_or_else(|| "合集保存失败".into())
}

pub fn move_collection(
    connection: &mut Connection,
    id: &str,
    target_parent_id: Option<&str>,
    target_index: usize,
) -> Result<(), String> {
    ensure_parent(connection, id, target_parent_id)?;
    let (old_parent, name): (Option<String>, String) = connection
        .query_row(
            "SELECT parent_id,name FROM manual_collections WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or("合集不存在")?;
    ensure_unique_name(connection, id, target_parent_id, &name)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE manual_collections SET parent_id=?1,updated_at=?2 WHERE id=?3",
            params![target_parent_id, now(), id],
        )
        .map_err(|error| error.to_string())?;
    normalize_siblings(&transaction, old_parent.as_deref())?;
    let mut ids = {
        let mut statement = transaction.prepare("SELECT id FROM manual_collections WHERE parent_id IS ?1 AND id<>?2 ORDER BY sort_order,name COLLATE NOCASE,id").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![target_parent_id, id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    ids.insert(target_index.min(ids.len()), id.to_string());
    for (index, value) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE manual_collections SET sort_order=?1 WHERE id=?2",
                params![index as i64, value],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn delete(connection: &mut Connection, id: &str) -> Result<(), String> {
    let parent: Option<String> = connection
        .query_row(
            "SELECT parent_id FROM manual_collections WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or("合集不存在")?;
    let has_children = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM manual_collections WHERE parent_id=?1)",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if has_children {
        return Err("请先删除或移动子合集".into());
    }
    connection
        .execute("DELETE FROM manual_collections WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    normalize_siblings(connection, parent.as_deref())
}

pub fn add_assets(
    connection: &mut Connection,
    collection_id: &str,
    asset_ids: Vec<String>,
) -> Result<CollectionMutationReport, String> {
    if asset_ids.len() > 5000 {
        return Err("一次最多处理 5000 项素材".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM manual_collections WHERE id=?1)",
            [collection_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if !exists {
        return Err("合集不存在".into());
    }
    let mut order: i64 = transaction.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM manual_collection_assets WHERE collection_id=?1", [collection_id], |row| row.get(0)).map_err(|error| error.to_string())?;
    let mut affected = 0usize;
    let timestamp = now();
    for id in HashSet::<String>::from_iter(asset_ids) {
        let changed = transaction.execute("INSERT OR IGNORE INTO manual_collection_assets(collection_id,asset_id,sort_order,added_at) SELECT ?1,id,?2,?3 FROM assets WHERE id=?4 AND deleted_at IS NULL", params![collection_id,order,timestamp,id]).map_err(|error| error.to_string())?;
        affected += changed;
        order += changed as i64;
        if changed > 0 {
            crate::db::reindex_asset(&transaction, &id)?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(CollectionMutationReport {
        affected_assets: affected,
    })
}

pub fn remove_assets(
    connection: &mut Connection,
    collection_id: &str,
    asset_ids: Vec<String>,
) -> Result<CollectionMutationReport, String> {
    let unique = HashSet::<String>::from_iter(asset_ids);
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let mut affected = 0;
    for id in unique {
        affected += transaction
            .execute(
                "DELETE FROM manual_collection_assets WHERE collection_id=?1 AND asset_id=?2",
                params![collection_id, id],
            )
            .map_err(|error| error.to_string())?;
        crate::db::reindex_asset(&transaction, &id)?;
    }
    let ids = {
        let mut statement = transaction.prepare("SELECT asset_id FROM manual_collection_assets WHERE collection_id=?1 ORDER BY sort_order,added_at").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([collection_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (index, id) in ids.iter().enumerate() {
        transaction.execute("UPDATE manual_collection_assets SET sort_order=?1 WHERE collection_id=?2 AND asset_id=?3", params![index as i64,collection_id,id]).map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(CollectionMutationReport {
        affected_assets: affected,
    })
}

pub fn reorder_assets(
    connection: &mut Connection,
    collection_id: &str,
    ids: Vec<String>,
) -> Result<(), String> {
    let existing = {
        let mut statement = connection.prepare("SELECT asset_id FROM manual_collection_assets WHERE collection_id=?1 ORDER BY sort_order").map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([collection_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    if existing.len() != ids.len()
        || HashSet::<_>::from_iter(existing.iter()) != HashSet::<_>::from_iter(ids.iter())
    {
        return Err("排序列表必须包含合集中的全部素材".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for (index, id) in ids.iter().enumerate() {
        transaction.execute("UPDATE manual_collection_assets SET sort_order=?1 WHERE collection_id=?2 AND asset_id=?3", params![index as i64,collection_id,id]).map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn descendant_ids(connection: &Connection, id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare("WITH RECURSIVE tree(id) AS (SELECT id FROM manual_collections WHERE id=?1 UNION ALL SELECT c.id FROM manual_collections c JOIN tree t ON c.parent_id=t.id) SELECT id FROM tree").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([id], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn asset_collection_names(
    connection: &Connection,
    asset_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare("SELECT c.name FROM manual_collection_assets m JOIN manual_collections c ON c.id=m.collection_id WHERE m.asset_id=?1 ORDER BY c.name COLLATE NOCASE").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([asset_id], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_database;
    use tempfile::tempdir;

    #[test]
    fn tree_rejects_cycles_and_membership_is_independent() {
        let dir = tempdir().unwrap();
        let mut db = open_database(&dir.path().join("library.db")).unwrap();
        let root = upsert(
            &mut db,
            ManualCollectionInput {
                id: None,
                parent_id: None,
                name: "灵感".into(),
                description: "".into(),
                cover_asset_id: None,
            },
        )
        .unwrap();
        let child = upsert(
            &mut db,
            ManualCollectionInput {
                id: None,
                parent_id: Some(root.id.clone()),
                name: "城市".into(),
                description: "".into(),
                cover_asset_id: None,
            },
        )
        .unwrap();
        assert!(move_collection(&mut db, &root.id, Some(&child.id), 0).is_err());
        assert_eq!(list(&db).unwrap().len(), 2);
        for id in ["asset-a", "asset-b"] {
            db.execute("INSERT INTO assets(id,name,description,dcc_tools_json,versions_json,formats_json,author,source_url,license,share_url,normalized_share_url,extraction_code,favorite,rating,created_at,updated_at) VALUES(?1,?1,'','[]','[]','[]','','','','','','',0,0,'now','now')",[id]).unwrap();
        }
        let report = add_assets(
            &mut db,
            &root.id,
            vec!["asset-a".into(), "asset-b".into(), "asset-a".into()],
        )
        .unwrap();
        assert_eq!(report.affected_assets, 2);
        reorder_assets(&mut db, &root.id, vec!["asset-b".into(), "asset-a".into()]).unwrap();
        let removed = remove_assets(&mut db, &root.id, vec!["asset-a".into()]).unwrap();
        assert_eq!(removed.affected_assets, 1);
        assert_eq!(
            list(&db)
                .unwrap()
                .into_iter()
                .find(|item| item.id == root.id)
                .unwrap()
                .direct_asset_count,
            1
        );
    }
}
