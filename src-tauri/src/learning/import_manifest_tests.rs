use super::import_manifest::{validate_synthetic_manifest, MAX_SYNTHETIC_MANIFEST};
use serde_json::{json, Value};

const SAMPLE: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.json");

fn sample() -> Value {
    serde_json::from_slice(SAMPLE).unwrap()
}

fn rejected(value: &Value) {
    assert!(validate_synthetic_manifest(&serde_json::to_vec(value).unwrap()).is_err());
}

#[test]
fn learning_import_manifest_accepts_synthetic_shape_without_claiming_file_verification() {
    let summary = validate_synthetic_manifest(SAMPLE).unwrap();
    assert_eq!(summary.declared_tables, 20);
    assert_eq!(summary.declared_files, 5);
    assert_eq!(summary.declared_objects, 9);
    assert_eq!(summary.declared_unresolved_references, 1);

    // A hint is never an authority to touch disk. Its contents are deliberately ignored.
    let mut hint = sample();
    hint["sampleRootHint"] = json!("C:\\not-an-authorized-root\\private");
    assert!(validate_synthetic_manifest(&serde_json::to_vec(&hint).unwrap()).is_ok());
    // Unknown target kinds have not been mapped merely because their shape is acceptable.
    hint["objects"][0]["targetKind"] = json!("future-kind");
    assert!(validate_synthetic_manifest(&serde_json::to_vec(&hint).unwrap()).is_ok());
    // Invented declarations can pass shape checks: real bytes need a separate verifier.
    hint["snapshot"]["sha256"] = json!("0".repeat(64));
    hint["snapshot"]["bytes"] = json!(1);
    assert!(validate_synthetic_manifest(&serde_json::to_vec(&hint).unwrap()).is_ok());
}

#[test]
fn learning_import_manifest_rejects_ambiguous_json_and_oversized_input() {
    let text = std::str::from_utf8(SAMPLE).unwrap();
    for invalid in [
        text.replacen("\"version\": 1,", "\"version\": 1, \"version\": 1,", 1),
        text.replacen(
            "\"version\": 1,",
            "\"version\": 1, \"ver\\u0073ion\": 1,",
            1,
        ),
        text.replacen(
            "\"name\": \"合成分类\",",
            "\"name\": \"一\", \"name\": \"二\",",
            1,
        ),
    ] {
        assert!(validate_synthetic_manifest(invalid.as_bytes()).is_err());
    }
    assert!(validate_synthetic_manifest(&vec![b' '; MAX_SYNTHETIC_MANIFEST + 1]).is_err());
    assert!(validate_synthetic_manifest(&[0xff]).is_err());
}

#[test]
fn learning_import_manifest_rejects_invalid_envelope_schema_and_table_shapes() {
    for (path, bad) in [
        (vec!["format"], json!("other")),
        (vec!["version"], json!(2)),
        (vec!["synthetic"], json!(false)),
        (vec!["sourceProfile", "id"], json!("not-a-uuid")),
        (
            vec!["sourceProfile", "id"],
            json!("A1000000-0000-4000-8000-000000000001"),
        ),
        (vec!["sourceProfile", "binding"], json!("user-selected")),
        (vec!["snapshot", "schemaVersion"], json!(7)),
        (vec!["snapshot", "userVersion"], json!(9)),
        (vec!["snapshot", "consistentBackup"], json!(false)),
    ] {
        let mut v = sample();
        let mut at = &mut v;
        for key in &path[..path.len() - 1] {
            at = &mut at[*key];
        }
        at[path[path.len() - 1]] = bad;
        rejected(&v);
    }
    let mut v = sample();
    v["tables"].as_object_mut().unwrap().remove("series");
    rejected(&v);
    let mut v = sample();
    v["tables"]["schema_version"]["rows"][0]["version"] = json!(7);
    rejected(&v);
    let mut v = sample();
    v["tables"]["categories"]["rows"][0]
        .as_object_mut()
        .unwrap()
        .remove("name");
    rejected(&v);
    let mut v = sample();
    v["tables"]["categories"]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("name"));
    rejected(&v);
}

#[test]
fn learning_import_manifest_rejects_unsafe_or_colliding_declared_paths() {
    for path in [
        "../outside.md",
        "vault/../outside.md",
        "/vault/a.md",
        "C:/outside.md",
        "//server/share/a.md",
        "vault\\a.md",
        "vault/a:stream",
        "vault/./a.md",
        "vault//a.md",
        "vault/trailing. ",
        "vault/CON.txt",
        "vault/COM¹.log",
        "vault/bad?.md",
        "vault/line\nbreak.md",
    ] {
        let mut v = sample();
        v["files"][0]["relativePath"] = json!(path);
        rejected(&v);
    }
    let mut v = sample();
    v["files"][1]["relativePath"] = json!("VAULT/PLAIN.MD");
    rejected(&v);
    let mut v = sample();
    v["snapshot"]["relativePath"] = json!("vault/plain.md");
    rejected(&v);
}

#[test]
fn learning_import_manifest_rejects_inconsistent_digests_and_source_keys() {
    let mut v = sample();
    v["files"][0]["afterSha256"] = json!("0".repeat(64));
    rejected(&v);
    let mut v = sample();
    v["snapshot"]["sha256"] = json!("x".repeat(64));
    rejected(&v);
    let mut v = sample();
    v["objects"][0]["sourceKey"][0] = json!("b1000000-0000-4000-8000-000000000001");
    rejected(&v);
    let mut v = sample();
    let duplicate = v["objects"][0].clone();
    v["objects"].as_array_mut().unwrap().push(duplicate);
    rejected(&v);
    let mut v = sample();
    v["objects"][0]["mapping"] = json!("imported");
    rejected(&v);
}
