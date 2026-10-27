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
