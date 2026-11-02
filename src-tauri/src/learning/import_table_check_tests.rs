use super::{import_snapshot::snapshot_synthetic, import_table_check::verify_synthetic_tables};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, time::Duration};

const MANIFEST: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.json");
const DATABASE: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.sqlite");

fn sample() -> Value {
    serde_json::from_slice(MANIFEST).unwrap()
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    assert_eq!(
        format!("{:x}", Sha256::digest(DATABASE)),
        "dba49e0512521ce4f4f859a90e013c394dd6846858cc7c23b1c10e4140692723"
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    fs::write(&path, DATABASE).unwrap();
    (temp, path)
}

fn verify(db: &Connection, manifest: &Value) -> Result<usize, String> {
    verify_synthetic_tables(db, &serde_json::to_vec(manifest).unwrap())
}

#[test]
fn learning_import_table_check_compares_real_fixture_without_using_row_or_column_order() {
    let (_temp, path) = fixture();
    let db = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(verify(&db, &sample()).unwrap(), 22);
    let mut rearranged = sample();
    rearranged["tables"]["series_items"]["rows"]
        .as_array_mut()
        .unwrap()
        .reverse();
    rearranged["tables"]["series_items"]["fields"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(verify(&db, &rearranged).unwrap(), 22);
    assert_eq!(fs::read(path).unwrap(), DATABASE);
}

#[test]
fn learning_import_table_check_rejects_changed_values_types_and_multiplicity() {
    let (_temp, path) = fixture();
    let db = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    for (field, value) in [
        ("name", json!("changed")),
        ("description", json!("")),
        ("created_at", json!(1.0)),
        ("created_at", json!(true)),
    ] {
        let mut changed = sample();
        changed["tables"]["categories"]["rows"][0][field] = value;
        assert!(verify(&db, &changed).is_err());
    }
    let mut duplicated = sample();
    let row = duplicated["tables"]["categories"]["rows"][0].clone();
    duplicated["tables"]["categories"]["rows"]
        .as_array_mut()
        .unwrap()
        .push(row);
    assert!(verify(&db, &duplicated).is_err());
    let mut deleted = sample();
    deleted["tables"]["categories"]["rows"]
        .as_array_mut()
        .unwrap()
        .clear();
    assert!(verify(&db, &deleted).is_err());
}

#[test]
fn learning_import_table_check_rejects_database_schema_changes_and_blob_values() {
    for sql in [
        "ALTER TABLE categories ADD COLUMN unexpected TEXT",
        "ALTER TABLE categories RENAME COLUMN name TO renamed",
        "UPDATE categories SET name=x'4142' WHERE id='cat-1'",
        "DROP TABLE knowledge_links; CREATE TABLE knowledge_links(source_path TEXT, target TEXT); INSERT INTO knowledge_links VALUES('complex.md','链接#标题'),('complex.md','链接#标题')",
        "CREATE TABLE ai_usage_copy AS SELECT id,analysis_id,operation,model,CAST(input_tokens AS REAL) AS input_tokens,output_tokens,cached_tokens,usage_json,created_at FROM ai_usage; DROP TABLE ai_usage; ALTER TABLE ai_usage_copy RENAME TO ai_usage",
    ] {
        let (_temp, path) = fixture();
        let writer = Connection::open(&path).unwrap();
        writer.execute_batch(sql).unwrap();
        drop(writer);
        let db = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
        assert!(verify(&db, &sample()).is_err(), "mutation accepted: {sql}");
    }
}

#[test]
fn learning_import_table_check_rejects_unsafe_declared_column_before_query() {
    let (_temp, path) = fixture();
    let db = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    let mut invalid = sample();
    invalid["tables"]["categories"]["fields"][0] = json!("id\"; DROP TABLE series;--");
    assert!(verify(&db, &invalid).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM series", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn learning_import_table_check_rejects_matching_virtual_table() {
    let (_temp, path) = fixture();
    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("ALTER TABLE categories RENAME TO archived_categories;
        CREATE VIRTUAL TABLE categories USING rtree(id, name, description, icon, color, sort_order, created_at, +updated_at);
        INSERT INTO categories VALUES(1,0,1,0,1,0,1,1);").unwrap();
    drop(writer);
    let db = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    let mut matching = sample();
    matching["tables"]["categories"]["rows"][0] = json!({
        "id": 1, "name": 0.0, "description": 1.0, "icon": 0.0,
        "color": 1.0, "sort_order": 0.0, "created_at": 1.0, "updated_at": 1
    });
    assert!(
        verify(&db, &matching).is_err(),
        "virtual table was accepted as an ordinary source table"
    );
}
