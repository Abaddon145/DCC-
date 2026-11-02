//! Local-only snapshots and reversible, allowlisted writes. No SQL comes from the model.
use rusqlite::{
    params_from_iter,
    types::{Value as SqlValue, ValueRef},
    Connection,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

type Row = Map<String, Value>;
fn error(e: rusqlite::Error) -> String {
    e.to_string()
}
fn sql_value(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Number(n) if n.is_i64() => SqlValue::Integer(n.as_i64().unwrap()),
        Value::Number(n) => SqlValue::Real(n.as_f64().unwrap_or_default()),
        Value::String(s) => SqlValue::Text(s.clone()),
        _ => SqlValue::Text(v.to_string()),
    }
}
pub fn rows(c: &Connection, sql: &str, values: &[Value]) -> Result<Vec<Row>, String> {
    let mut stmt = c.prepare(sql).map_err(error)?;
    let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
    let result = stmt
        .query_map(params_from_iter(values.iter().map(sql_value)), |r| {
            let mut item = Row::new();
            for (i, name) in names.iter().enumerate() {
                let value = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                    ValueRef::Blob(_) => return Err(rusqlite::Error::InvalidQuery),
                };
                item.insert(name.clone(), value);
            }
            Ok(item)
        })
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scope {
    pub kind: String,
    pub id: String,
}
pub fn snapshot(c: &Connection, s: &Scope) -> Result<Value, String> {
    let v = [json!(s.id)];
    let mut result = Map::new();
    let queries:&[(&str,&str)]=match s.kind.as_str() {
        "asset"=>&[("asset","SELECT * FROM assets WHERE id=?"),("texts","SELECT * FROM asset_localizations WHERE asset_id=? ORDER BY locale"),("tags","SELECT t.* FROM localized_tags t JOIN asset_localized_tags l ON l.tag_id=t.id WHERE l.asset_id=? ORDER BY t.id")],
        "image"|"model"|"audio"|"video"=>&[("media","SELECT * FROM media_entries WHERE id=?"),("tags","SELECT t.* FROM localized_tags t JOIN media_entry_tags l ON l.tag_id=t.id WHERE l.entry_id=? ORDER BY t.id")],
        "project"=>&[("project","SELECT * FROM projects WHERE id=?"),("units","SELECT * FROM project_units WHERE project_id=? ORDER BY id"),("tasks","SELECT * FROM project_tasks WHERE project_id=? ORDER BY id"),("assets","SELECT * FROM project_assets WHERE project_id=? ORDER BY asset_id"),("assetUnits","SELECT * FROM project_asset_units WHERE project_id=? ORDER BY asset_id,unit_id"),("taskAssets","SELECT a.* FROM project_task_assets a JOIN project_tasks t ON t.id=a.task_id WHERE t.project_id=? ORDER BY a.task_id,a.asset_id"),("media","SELECT * FROM project_media_entries WHERE project_id=? ORDER BY entry_id"),("boards","SELECT * FROM project_reference_boards WHERE project_id=? ORDER BY board_id"),("paths","SELECT * FROM project_paths WHERE project_id=? ORDER BY id")],
        _=>return Err("无效的 AI 数据范围".into())
    };
    for (name, sql) in queries {
        let mut data = rows(c, sql, &v)?;
        for row in &mut data {
            row.remove("last_viewed_at");
            row.remove("last_opened_at");
        }
        result.insert(name.to_string(), json!(data));
    }
    Ok(Value::Object(result))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mutation {
    table: String,
    key: Row,
    before: Option<Row>,
    after: Option<Row>,
}
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Journal {
    changes: Vec<Mutation>,
}
fn allowed(table: &str) -> bool {
    matches!(
        table,
        "assets"
            | "media_entries"
            | "asset_localizations"
            | "localized_tags"
            | "asset_localized_tags"
            | "media_entry_tags"
            | "categories"
            | "media_folders"
            | "projects"
            | "project_tasks"
            | "project_units"
            | "project_assets"
            | "project_media_entries"
    )
}
fn predicate(key: &Row) -> String {
    key.keys()
        .map(|k| format!("\"{k}\" IS ?"))
        .collect::<Vec<_>>()
        .join(" AND ")
}
fn read(c: &Connection, table: &str, key: &Row) -> Result<Option<Row>, String> {
    if !allowed(table) || key.is_empty() || key.keys().any(|name| !identifier(name)) {
        return Err("无效的 AI 撤销目标".into());
    }
    Ok(rows(
        c,
        &format!("SELECT * FROM {table} WHERE {}", predicate(key)),
        &key.values().cloned().collect::<Vec<_>>(),
    )?
    .into_iter()
    .next())
}
fn write(c: &Connection, table: &str, key: &Row, row: Option<&Row>) -> Result<(), String> {
    if !allowed(table) || key.is_empty() {
        return Err("无效的 AI 写入目标".into());
    }
    if let Some(row) = row {
        if row.keys().any(|name| !identifier(name)) {
            return Err("无效的 AI 字段名".into());
        }
        let cols = row.keys().map(|s| format!("\"{s}\"")).collect::<Vec<_>>();
        let sql = if read(c, table, key)?.is_some() {
            format!(
                "UPDATE {table} SET {} WHERE {}",
                cols.iter()
                    .map(|s| format!("{s}=?"))
                    .collect::<Vec<_>>()
                    .join(","),
                predicate(key)
            )
        } else {
            format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                cols.join(","),
                vec!["?"; cols.len()].join(",")
            )
        };
        let mut values: Vec<Value> = row.values().cloned().collect();
        if sql.starts_with("UPDATE") {
            values.extend(key.values().cloned());
        }
        c.execute(&sql, params_from_iter(values.iter().map(sql_value)))
            .map_err(error)?;
    } else {
        c.execute(
            &format!("DELETE FROM {table} WHERE {}", predicate(key)),
            params_from_iter(key.values().map(sql_value)),
        )
        .map_err(error)?;
    }
    Ok(())
}
fn identifier(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
impl Journal {
    pub fn put(
        &mut self,
        c: &Connection,
        table: &str,
        key: Value,
        patch: Value,
    ) -> Result<(), String> {
        let key = key.as_object().ok_or("无效主键")?.clone();
        let before = read(c, table, &key)?;
        let mut row = before.clone().unwrap_or_else(|| key.clone());
        row.extend(patch.as_object().ok_or("无效变更")?.clone());
        write(c, table, &key, Some(&row))?;
        let after = read(c, table, &key)?;
        self.changes.push(Mutation {
            table: table.into(),
            key,
            before,
            after,
        });
        Ok(())
    }
    pub fn undo(&self, c: &Connection) -> Result<(), String> {
        // Reverse in the same transaction: later mutations may touch the same row.
        for m in self.changes.iter().rev() {
            let mut current = read(c, &m.table, &m.key)?;
            let mut expected = m.after.clone();
            for row in [&mut current, &mut expected].into_iter().flatten() {
                row.remove("last_viewed_at");
                row.remove("last_opened_at");
            }
            if current != expected {
                return Err("应用后数据已被修改，无法安全撤销；未覆盖你的修改".into());
            }
            if m.before.is_none() {
                let id = m.key.get("id").cloned().unwrap_or(Value::Null);
                let sql=match m.table.as_str() {
                    "localized_tags"=>Some("SELECT 1 FROM asset_localized_tags WHERE tag_id=?1 UNION ALL SELECT 1 FROM media_entry_tags WHERE tag_id=?1 LIMIT 1"),
                    "categories"=>Some("SELECT 1 FROM assets WHERE category_id=?1 UNION ALL SELECT 1 FROM categories WHERE parent_id=?1 LIMIT 1"),
                    "media_folders"=>Some("SELECT 1 FROM media_entries WHERE folder_id=?1 UNION ALL SELECT 1 FROM media_folders WHERE parent_id=?1 LIMIT 1"),
                    _=>None,
                };
                if let Some(sql) = sql {
                    if !rows(c, sql, &[id])?.is_empty() {
                        return Err("新建的分类或标签已被其他内容使用，无法撤销".into());
                    }
                }
            }
            // Keep view history accumulated since apply.
            let mut before = m.before.clone();
            if let Some(row) = &mut before {
                if let Some(current) = read(c, &m.table, &m.key)? {
                    for name in ["last_viewed_at", "last_opened_at"] {
                        if let Some(v) = current.get(name) {
                            row.insert(name.into(), v.clone());
                        }
                    }
                }
            }
            write(c, &m.table, &m.key, before.as_ref())?;
        }
        Ok(())
    }
}
