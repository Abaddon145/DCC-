//! Compare only declared synthetic fixture tables, not source identity or files.
use super::import_manifest::validate_synthetic_manifest;
use rusqlite::{types::ValueRef, Connection};
use serde_json::Value;

fn sql_value(value: ValueRef<'_>) -> Option<Value> {
    match value {
        ValueRef::Null => Some(Value::Null),
        ValueRef::Integer(number) => Some(Value::from(number)),
        ValueRef::Real(number) => serde_json::Number::from_f64(number).map(Value::Number),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .map(|s| Value::String(s.into())),
        ValueRef::Blob(_) => None,
    }
}

fn declared_value(value: &Value) -> bool {
    match value {
        Value::Null | Value::String(_) => true,
        Value::Number(number) => {
            number.as_i64().is_some()
                || number
                    .as_f64()
                    .is_some_and(|v| number.is_f64() && v.is_finite())
        }
        _ => false,
    }
}

pub(super) fn verify_synthetic_tables(db: &Connection, bytes: &[u8]) -> Result<usize, String> {
    let failed = || "合成快照与清单不一致".to_string();
    validate_synthetic_manifest(bytes).map_err(|_| failed())?;
    let manifest: Value = serde_json::from_slice(bytes).map_err(|_| failed())?;
    let tables = manifest
        .get("tables")
        .and_then(Value::as_object)
        .ok_or_else(failed)?;
    let mut total = 0usize;
    for (name, table) in tables {
        // Table names are the fixed whitelist checked by validate_synthetic_manifest.
        let kind: String = db
            .query_row(
                "SELECT type FROM pragma_table_list WHERE schema='main' AND name=?1 COLLATE BINARY",
                [name],
                |row| row.get(0),
            )
            .map_err(|_| failed())?;
        if kind != "table" {
            return Err(failed());
        }
        let mut fields: Vec<&str> = table["fields"]
            .as_array()
            .ok_or_else(failed)?
            .iter()
            .map(|v| v.as_str().ok_or_else(failed))
            .collect::<Result<_, _>>()?;
        fields.sort_unstable();
        let mut schema = db
            .prepare(&format!("PRAGMA table_xinfo(\"{name}\")"))
            .map_err(|_| failed())?;
        let mut columns = schema.query([]).map_err(|_| failed())?;
        let mut actual_fields = Vec::new();
        while let Some(row) = columns.next().map_err(|_| failed())? {
            let field: String = row.get(1).map_err(|_| failed())?;
            let hidden: i64 = row.get(6).map_err(|_| failed())?;
            if hidden != 0 {
                return Err(failed());
            }
            actual_fields.push(field);
        }
        actual_fields.sort_unstable();
        if actual_fields
            .iter()
            .map(String::as_str)
            .ne(fields.iter().copied())
        {
            return Err(failed());
        }
        let declared = table["rows"].as_array().ok_or_else(failed)?;
        let mut expected_rows = Vec::with_capacity(declared.len());
        for row in declared {
            let row = row.as_object().ok_or_else(failed)?;
            let values = fields
                .iter()
                .map(|field| row.get(*field).ok_or_else(failed))
                .collect::<Result<Vec<_>, _>>()?;
            if values.iter().any(|value| !declared_value(value)) {
                return Err(failed());
            }
            expected_rows.push(serde_json::to_vec(&values).map_err(|_| failed())?);
        }
        // Columns are ASCII identifiers checked above; quoting also supports SQL keywords.
        let select = fields
            .iter()
            .map(|field| format!("\"{field}\""))
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = db
            .prepare(&format!("SELECT {select} FROM \"{name}\""))
            .map_err(|_| failed())?;
        let mut rows = statement.query([]).map_err(|_| failed())?;
        let mut actual_rows = Vec::with_capacity(expected_rows.len());
        while let Some(row) = rows.next().map_err(|_| failed())? {
            if actual_rows.len() >= expected_rows.len() {
                return Err(failed());
            }
            let mut values = Vec::with_capacity(fields.len());
            for index in 0..fields.len() {
                values
                    .push(sql_value(row.get_ref(index).map_err(|_| failed())?).ok_or_else(failed)?);
            }
            actual_rows.push(serde_json::to_vec(&values).map_err(|_| failed())?);
        }
        expected_rows.sort_unstable();
        actual_rows.sort_unstable();
        if actual_rows != expected_rows {
            return Err(failed());
        }
        total = total.checked_add(actual_rows.len()).ok_or_else(failed)?;
    }
    Ok(total)
}
