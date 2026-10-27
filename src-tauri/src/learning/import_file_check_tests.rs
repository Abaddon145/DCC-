use super::import_file_check::verify_synthetic_files;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const MANIFEST: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.json");
const DB: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.sqlite");
const PLAIN: &[u8] = include_bytes!("fixtures/yingliu-files/plain.md");
const COMPLEX: &[u8] = include_bytes!("fixtures/yingliu-files/complex.md");
const IMAGE: &[u8] = include_bytes!("fixtures/yingliu-files/test.png");
const MEDIA: &[u8] = include_bytes!("fixtures/yingliu-files/source.bin");

fn manifest() -> Value {
    serde_json::from_slice(MANIFEST).unwrap()
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("vault/Attachments")).unwrap();
    fs::create_dir_all(temp.path().join("media")).unwrap();
    fs::write(temp.path().join("learning.snapshot.db"), DB).unwrap();
    fs::write(temp.path().join("vault/plain.md"), PLAIN).unwrap();
    fs::write(temp.path().join("vault/complex.md"), COMPLEX).unwrap();
    fs::write(temp.path().join("vault/Attachments/test.png"), IMAGE).unwrap();
    fs::write(temp.path().join("media/source.bin"), MEDIA).unwrap();
    let long = format!(
        "# 长文\n\n{}",
        "普通学习段落，包含可选择和修改的正文。\n\n".repeat(16000)
    );
    fs::write(temp.path().join("vault/long.md"), long).unwrap();
    temp
}
fn verify(
    root: &Path,
    value: &Value,
) -> Result<super::import_file_check::SyntheticFilesReport, String> {
    verify_synthetic_files(root, &serde_json::to_vec(value).unwrap())
}

#[test]
fn learning_import_file_check_matches_all_synthetic_bytes_and_ignores_root_hint() {
    assert_eq!(
        format!("{:x}", Sha256::digest(MANIFEST)),
        "5618b1423558f3b40759886d93dde207802741458cfec305117d076cbf1a98db"
    );
    let temp = fixture();
    let mut value = manifest();
    value["sampleRootHint"] = json!("C:\\untrusted-path");
    let report = verify(temp.path(), &value).unwrap();
    assert_eq!(
        (
            report.verified,
            report.missing_required,
            report.missing_optional
        ),
        (6, 0, 0)
    );
    assert_eq!(fs::read(temp.path().join("vault/plain.md")).unwrap(), PLAIN);
    assert_eq!(
        fs::read(temp.path().join("learning.snapshot.db")).unwrap(),
        DB
    );
}

#[test]
fn learning_import_file_check_reports_required_and_optional_missing_items() {
    let temp = fixture();
    fs::remove_file(temp.path().join("vault/plain.md")).unwrap();
    let report = verify(temp.path(), &manifest()).unwrap();
    assert_eq!(
        (
            report.verified,
            report.missing_required,
            report.missing_optional
        ),
        (5, 1, 0)
    );
    let mut optional = manifest();
    optional["files"][0]["required"] = json!(false);
    let report = verify(temp.path(), &optional).unwrap();
    assert_eq!(
        (
            report.verified,
            report.missing_required,
            report.missing_optional
        ),
        (5, 0, 1)
    );
    fs::remove_file(temp.path().join("learning.snapshot.db")).unwrap();
    let report = verify(temp.path(), &optional).unwrap();
    assert_eq!(
        (
            report.verified,
            report.missing_required,
            report.missing_optional
        ),
        (4, 1, 1)
    );
}

#[test]
fn learning_import_file_check_rejects_changed_content_length_and_digest() {
    let temp = fixture();
    let mut changed = PLAIN.to_vec();
    changed[0] ^= 1;
    fs::write(temp.path().join("vault/plain.md"), changed).unwrap();
    let error = verify(temp.path(), &manifest()).unwrap_err();
    assert!(!error.contains("plain.md"));
    assert!(!error.contains("第一段"));
    fs::write(temp.path().join("vault/plain.md"), PLAIN).unwrap();
    let mut wrong_size = manifest();
    wrong_size["files"][0]["bytes"] = json!(44);
    assert!(verify(temp.path(), &wrong_size).is_err());
    fs::write(temp.path().join("learning.snapshot.db"), b"bad").unwrap();
    assert!(verify(temp.path(), &manifest()).is_err());
}

#[test]
fn learning_import_file_check_rejects_paths_missing_root_and_oversize_claim() {
    let temp = fixture();
    let mut outside = manifest();
    outside["files"][0]["relativePath"] = json!("../private.md");
    assert!(verify(temp.path(), &outside).is_err());
    let mut duplicate = manifest();
    duplicate["files"][0]["relativePath"] = json!("vault/complex.md");
    assert!(verify(temp.path(), &duplicate).is_err());
    let mut huge = manifest();
    huge["files"][0]["bytes"] = json!(u64::MAX);
    assert!(verify(temp.path(), &huge).is_err());
    assert!(verify(&temp.path().join("missing-root"), &manifest()).is_err());
}

#[test]
fn learning_import_file_check_bounds_actual_read_length() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sample.bin");
    fs::write(&path, b"abcd").unwrap();
    let mut file = fs::File::open(&path).unwrap();
    assert!(super::import_file_check::digest(&mut file, 3).is_err());
    assert!(super::import_file_check::digest(&mut file, 4).is_ok());
}

#[cfg(windows)]
#[test]
fn learning_import_file_check_refuses_conflicting_writer_and_recovers_after_close() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = fixture();
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(temp.path().join("vault/plain.md"))
        .unwrap();
    assert!(verify(temp.path(), &manifest()).is_err());
    drop(writer);
    assert_eq!(verify(temp.path(), &manifest()).unwrap().verified, 6);
}

#[derive(Debug, PartialEq, Eq)]
enum FixturePreflight {
    Incomplete {
        required: usize,
        optional: usize,
    },
    Compared {
        files: usize,
        rows: usize,
        declared_objects: usize,
        declared_unresolved: usize,
    },
}

// Test-only composition. It is not an authorization or a production import preview.
fn check_fixture_preflight(
    root: &Path,
    value: &Value,
    between: impl FnOnce(&Path),
) -> Result<FixturePreflight, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "合成清单无法编码")?;
    check_fixture_preflight_bytes(root, &bytes, between)
}

fn check_fixture_preflight_bytes(
    root: &Path,
    bytes: &[u8],
    between: impl FnOnce(&Path),
) -> Result<FixturePreflight, String> {
    let declared = super::import_manifest::validate_synthetic_manifest(bytes)?;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "合成清单无法解析")?;
    if value["snapshot"]["relativePath"] != "learning.snapshot.db" {
        return Err("合成快照名称无效".into());
    }
    let check_wal = || -> Result<(), String> {
        let wal = root.join("learning.snapshot.db-wal");
        crate::media_access::reject_links(&wal).map_err(|_| "合成快照日志不可用")?;
        match fs::metadata(wal) {
            Ok(meta) if meta.is_file() && meta.len() == 0 => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("合成固定快照包含活动日志".into()),
        }
    };
    let before = verify_synthetic_files(root, bytes)?;
    if before.missing_required != 0 || before.missing_optional != 0 {
        return Ok(FixturePreflight::Incomplete {
            required: before.missing_required,
            optional: before.missing_optional,
        });
    }
    check_wal()?;
    let snapshot = super::import_snapshot::snapshot_synthetic(
        &root.join("learning.snapshot.db"),
        std::time::Duration::from_secs(2),
    )?;
    let rows = super::import_table_check::verify_synthetic_tables(&snapshot, bytes)?;
    between(root);
    check_wal()?;
    let after = verify_synthetic_files(root, bytes)?;
    if after.missing_required != 0
        || after.missing_optional != 0
        || after.verified != before.verified
    {
        return Err("合成来源在检查期间发生变化".into());
    }
    Ok(FixturePreflight::Compared {
        files: after.verified,
        rows,
        declared_objects: declared.declared_objects,
        declared_unresolved: declared.declared_unresolved_references,
    })
}

#[test]
fn learning_import_preflight_combines_fixed_fixture_without_touching_target() {
    let source = fixture();
    let target = tempfile::tempdir().unwrap();
    let target_path = target.path().join("target.db");
    let target_db = rusqlite::Connection::open(&target_path).unwrap();
    target_db
        .execute_batch(
            "CREATE TABLE saved_note(body TEXT); INSERT INTO saved_note VALUES('preserve');",
        )
        .unwrap();
    drop(target_db);
    let before_target = fs::read(&target_path).unwrap();
    let before_source = fs::read(source.path().join("learning.snapshot.db")).unwrap();
    assert_eq!(
        check_fixture_preflight(source.path(), &manifest(), |_| {}).unwrap(),
        FixturePreflight::Compared {
            files: 6,
            rows: 22,
            declared_objects: 9,
            declared_unresolved: 1,
        }
    );
    assert_eq!(fs::read(&target_path).unwrap(), before_target);
    assert_eq!(
        fs::read(source.path().join("learning.snapshot.db")).unwrap(),
        before_source
    );
}

#[test]
fn learning_import_preflight_reports_missing_instead_of_claiming_a_complete_comparison() {
    for name in [
        "vault/plain.md",
        "vault/Attachments/test.png",
        "learning.snapshot.db",
    ] {
        let source = fixture();
        fs::remove_file(source.path().join(name)).unwrap();
        assert_eq!(
            check_fixture_preflight(source.path(), &manifest(), |_| {}).unwrap(),
            FixturePreflight::Incomplete {
                required: 1,
                optional: 0
            }
        );
    }
    let source = fixture();
    fs::remove_file(source.path().join("vault/plain.md")).unwrap();
    let mut optional = manifest();
    optional["files"][0]["required"] = json!(false);
    assert_eq!(
        check_fixture_preflight(source.path(), &optional, |_| {}).unwrap(),
        FixturePreflight::Incomplete {
            required: 0,
            optional: 1
        }
    );
}

#[test]
fn learning_import_preflight_rejects_row_file_and_between_stage_changes() {
    let source = fixture();
    let mut changed_row = manifest();
    changed_row["tables"]["categories"]["rows"][0]["name"] = json!("different");
    assert!(check_fixture_preflight(source.path(), &changed_row, |_| {}).is_err());
    assert!(check_fixture_preflight(source.path(), &manifest(), |root| {
        let mut changed = PLAIN.to_vec();
        changed[0] ^= 1;
        fs::write(root.join("vault/plain.md"), changed).unwrap();
    })
    .is_err());
    fs::write(source.path().join("vault/plain.md"), PLAIN).unwrap();
    assert!(check_fixture_preflight(source.path(), &manifest(), |root| {
        fs::remove_file(root.join("vault/plain.md")).unwrap();
    })
    .is_err());
}

#[test]
fn learning_import_preflight_rejects_nonempty_or_changing_wal_and_bad_manifest_first() {
    let source = fixture();
    let wal = source.path().join("learning.snapshot.db-wal");
    fs::write(&wal, b"not-a-fixed-snapshot").unwrap();
    assert!(check_fixture_preflight(source.path(), &manifest(), |_| {}).is_err());
    fs::write(&wal, b"").unwrap();
    assert!(check_fixture_preflight(source.path(), &manifest(), |root| {
        fs::write(root.join("learning.snapshot.db-wal"), b"changed").unwrap();
    })
    .is_err());
    let mut invalid = manifest();
    invalid["version"] = json!(2);
    let error =
        check_fixture_preflight(&source.path().join("missing-root"), &invalid, |_| {}).unwrap_err();
    assert!(error.contains("清单格式"));
}

#[test]
fn learning_import_preflight_rejects_raw_duplicate_keys_before_root_access() {
    let source = fixture();
    let text = std::str::from_utf8(MANIFEST).unwrap();
    let duplicate = text.replacen("\"version\": 1,", "\"version\": 1, \"version\": 1,", 1);
    let error = check_fixture_preflight_bytes(
        &source.path().join("missing-root"),
        duplicate.as_bytes(),
        |_| {},
    )
    .unwrap_err();
    assert!(error.contains("清单格式"));
}

#[test]
fn learning_import_preflight_rejects_committed_wal_not_covered_by_main_file_hash() {
    let source = fixture();
    let path = source.path().join("learning.snapshot.db");
    let main_before = fs::read(&path).unwrap();
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    let _: i64 = writer
        .query_row("PRAGMA wal_autocheckpoint=0", [], |r| r.get(0))
        .unwrap();
    writer.execute("INSERT INTO video_notes(id,item_id,time_ms,content,created_at,updated_at) VALUES('wal-note','course-1',30000,'wal only',1,1)", []).unwrap();
    drop(writer);
    let wal = source.path().join("learning.snapshot.db-wal");
    let wal_before = fs::read(&wal).unwrap();
    assert!(!wal_before.is_empty());
    assert_eq!(fs::read(&path).unwrap(), main_before);
    let error = check_fixture_preflight(source.path(), &manifest(), |_| {}).unwrap_err();
    assert!(error.contains("活动日志"));
    assert_eq!(fs::read(&path).unwrap(), main_before);
    assert_eq!(fs::read(wal).unwrap(), wal_before);
}
