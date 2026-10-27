//! Synthetic fixture shape checks only. This does not authorize or read files.
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

pub(super) const MAX_SYNTHETIC_MANIFEST: usize = 16 * 1024 * 1024;
const TABLES: [&str; 20] = [
    "schema_version",
    "categories",
    "series",
    "series_items",
    "playback_progress",
    "video_notes",
    "learning_settings",
    "video_analyses",
    "transcript_segments",
    "analysis_chunks",
    "analysis_keyframes",
    "ai_usage",
    "knowledge_index",
    "knowledge_links",
    "knowledge_tags",
    "ai_template_versions",
    "ai_template_preferences",
    "knowledge_note_metadata",
    "knowledge_note_versions",
    "knowledge_attachments",
];

// serde_json::Value otherwise silently keeps the last value for a duplicate key.
struct UniqueJson;
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Check;
        impl<'de> Visitor<'de> for Check {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<UniqueJson, A::Error> {
                while seq.next_element::<UniqueJson>()?.is_some() {}
                Ok(UniqueJson)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<UniqueJson, A::Error> {
                let mut keys = HashSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    map.next_value::<UniqueJson>()?;
                }
                Ok(UniqueJson)
            }
        }
        deserializer.deserialize_any(Check)
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u64,
    synthetic: bool,
    source_profile: Profile,
    snapshot: Snapshot,
    tables: BTreeMap<String, Table>,
    files: Vec<File>,
    objects: Vec<Object>,
    unresolved_references: Vec<Value>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    application: String,
    source_code: String,
    vault_identity: String,
    binding: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    relative_path: String,
    bytes: u64,
    sha256: String,
    schema_version: u64,
    user_version: u64,
    consistent_backup: bool,
}

#[derive(serde::Deserialize)]
struct Table {
    fields: Vec<String>,
    rows: Vec<BTreeMap<String, Value>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    relative_path: String,
    sha256: String,
    before_sha256: String,
    after_sha256: String,
    #[serde(rename = "bytes")]
    _bytes: u64,
    #[serde(rename = "required")]
    _required: bool,
    kind: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Object {
    source_key: Vec<String>,
    target_kind: String,
    mapping: String,
}

pub(super) struct SyntheticManifestSummary {
    pub declared_tables: usize,
    pub declared_files: usize,
    pub declared_objects: usize,
    pub declared_unresolved_references: usize,
}

fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn relative_path(value: &str) -> bool {
    text(value)
        && !value.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        && value.split('/').all(|part| {
            if part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' ']) {
                return false;
            }
            let name = part
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if matches!(
                name.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) {
                return false;
            }
            if let Some(suffix) = name
                .strip_prefix("COM")
                .or_else(|| name.strip_prefix("LPT"))
            {
                if matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                ) {
                    return false;
                }
            }
            true
        })
}

/// Checks only the shape of the synthetic v1 contract; nothing here verifies file bytes.
pub(super) fn validate_synthetic_manifest(
    bytes: &[u8],
) -> Result<SyntheticManifestSummary, String> {
    let invalid = || "合成迁入清单格式无效".to_string();
    if bytes.len() > MAX_SYNTHETIC_MANIFEST {
        return Err(invalid());
    }
    serde_json::from_slice::<UniqueJson>(bytes).map_err(|_| invalid())?;
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let profile = &manifest.source_profile;
    let canonical_id = uuid::Uuid::parse_str(&profile.id)
        .ok()
        .filter(|id| !id.is_nil() && id.hyphenated().to_string() == profile.id)
        .is_some();
    if manifest.format != "yingliu-migration-input"
        || manifest.version != 1
        || !manifest.synthetic
        || !canonical_id
        || profile.application != "yingliu"
        || profile.binding != "synthetic-only"
        || !hex(&profile.source_code, 40)
        || !text(&profile.vault_identity)
        || manifest.snapshot.schema_version != 8
        || manifest.snapshot.user_version != 8
        || !manifest.snapshot.consistent_backup
        || manifest.snapshot.bytes == 0
        || !hex(&manifest.snapshot.sha256, 64)
        || !relative_path(&manifest.snapshot.relative_path)
    {
        return Err(invalid());
    }
    if manifest.tables.len() != TABLES.len()
        || TABLES
            .iter()
            .any(|name| !manifest.tables.contains_key(*name))
    {
        return Err(invalid());
    }
    for table in manifest.tables.values() {
        let fields: HashSet<&str> = table.fields.iter().map(String::as_str).collect();
        if fields.is_empty()
            || fields.len() != table.fields.len()
            || fields.iter().any(|field| {
                field.is_empty()
                    || !field
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            || table.rows.iter().any(|row| {
                row.len() != fields.len()
                    || row.keys().any(|key| !fields.contains(key.as_str()))
                    || row
                        .values()
                        .any(|value| value.is_array() || value.is_object())
            })
        {
            return Err(invalid());
        }
    }
    let version = &manifest.tables["schema_version"].rows;
    if version.len() != 1 || version[0].get("version").and_then(Value::as_u64) != Some(8) {
        return Err(invalid());
    }
    let mut paths = HashSet::new();
    paths.insert(manifest.snapshot.relative_path.to_lowercase());
    for file in &manifest.files {
        if !relative_path(&file.relative_path)
            || !paths.insert(file.relative_path.to_lowercase())
            || !hex(&file.sha256, 64)
            || file.sha256 != file.before_sha256
            || file.sha256 != file.after_sha256
            || !text(&file.kind)
        {
            return Err(invalid());
        }
    }
    let mut keys = HashSet::new();
    for object in &manifest.objects {
        if object.source_key.len() < 3
            || object.source_key[0] != profile.id
            || object.source_key.iter().any(|part| !text(part))
            || !keys.insert(object.source_key.clone())
            || !text(&object.target_kind)
            || object.mapping != "not-imported"
        {
            return Err(invalid());
        }
    }
    Ok(SyntheticManifestSummary {
        declared_tables: manifest.tables.len(),
        declared_files: manifest.files.len(),
        declared_objects: manifest.objects.len(),
        declared_unresolved_references: manifest.unresolved_references.len(),
    })
}
