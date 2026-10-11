use rusqlite::Connection;

pub fn migrate(db: &Connection) -> Result<(), String> {
    if !db.is_autocommit() {
        return Err("资产迁移需要独立事务".into());
    }
    let upgrade:bool=db.query_row("SELECT NOT EXISTS(SELECT 1 FROM pragma_table_info('asset_core') WHERE name='entity_id')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    if upgrade {
        db.execute_batch("PRAGMA foreign_keys=OFF;")
            .map_err(|e| e.to_string())?;
    }
    let result = (|| {
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(include_str!("schema.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute_batch(include_str!("intelligence_schema.sql"))
            .map_err(|e| e.to_string())?;
        if upgrade {
            let triggers = {
                let mut s=tx.prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND instr(lower(sql),'asset_core')>0").map_err(|e|e.to_string())?;
                let rows = s
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?
            };
            for (name, _) in &triggers {
                tx.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
                    .map_err(|e| e.to_string())?;
            }
            tx.execute_batch("CREATE TABLE asset_core_next(id TEXT PRIMARY KEY,legacy_asset_id TEXT UNIQUE REFERENCES assets(id) ON DELETE CASCADE,media_entry_id TEXT UNIQUE REFERENCES media_entries(id) ON DELETE CASCADE,entity_id TEXT UNIQUE REFERENCES asset_entities(id) ON DELETE CASCADE,created_at TEXT NOT NULL,CHECK((legacy_asset_id IS NOT NULL)+(media_entry_id IS NOT NULL)+(entity_id IS NOT NULL)=1)); INSERT INTO asset_core_next(id,legacy_asset_id,media_entry_id,created_at) SELECT id,legacy_asset_id,media_entry_id,created_at FROM asset_core;").map_err(|e|e.to_string())?;
            let mismatch:bool=tx.query_row("SELECT EXISTS(SELECT id,legacy_asset_id,media_entry_id,created_at FROM asset_core EXCEPT SELECT id,legacy_asset_id,media_entry_id,created_at FROM asset_core_next) OR (SELECT count(*) FROM asset_core)<>(SELECT count(*) FROM asset_core_next)",[],|r|r.get(0)).map_err(|e|e.to_string())?;
            if mismatch {
                return Err("迁移身份校验失败，已回滚".into());
            }
            tx.execute_batch(
                "DROP TABLE asset_core; ALTER TABLE asset_core_next RENAME TO asset_core;",
            )
            .map_err(|e| e.to_string())?;
            for (_, sql) in triggers {
                tx.execute_batch(&sql).map_err(|e| e.to_string())?;
            }
        }
        super::migrate_origins(&tx)?;
        {
            let broken: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if broken {
                return Err("资产外键校验失败，迁移已回滚".into());
            }
        }
        tx.commit().map_err(|e| e.to_string())
    })();
    let restore = db
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string());
    result.and(restore)
}
