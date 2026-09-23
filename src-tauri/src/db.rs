use crate::{fab, images, models::*, search_syntax};
use chrono::Utc;
use rusqlite::{
    params, params_from_iter, types::Value, Connection, OptionalExtension, Row, Transaction,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
};
use url::Url;
use uuid::Uuid;

pub fn open_database(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")
        .map_err(|e| format!("配置数据库失败：{e}"))?;
    connection
        .query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))
        .map_err(|e| format!("启用 WAL 失败：{e}"))?;
    connection
        .execute_batch(include_str!("schema.sql"))
        .map_err(|e| format!("初始化数据库失败：{e}"))?;
    migrate_to_v2(&connection)?;
    migrate_to_v3(&connection)?;
    migrate_to_v4(&connection)?;
    migrate_to_v5(&connection)?;
    migrate_to_v6(&connection)?;
    migrate_to_v7(&connection)?;
    migrate_to_v8(&connection)?;
    migrate_to_v9(&connection)?;
    migrate_to_v10(&connection)?;
    migrate_to_v11(&connection)?;
    migrate_to_v12(&connection)?;
    Ok(connection)
}

fn migrate_to_v2(connection: &Connection) -> Result<(), String> {
    let has_normalized = {
        let mut statement = connection
            .prepare("PRAGMA table_info(assets)")
            .map_err(|e| e.to_string())?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        names.iter().any(|name| name == "normalized_share_url")
    };
    if !has_normalized {
        connection
            .execute(
                "ALTER TABLE assets ADD COLUMN normalized_share_url TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("升级数据库到 v2 失败：{e}"))?;
    }
    connection
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_assets_normalized_share_url ON assets(normalized_share_url)",
            [],
        )
        .map_err(|e| e.to_string())?;
    let mut statement = connection
        .prepare("SELECT id,share_url FROM assets WHERE normalized_share_url='' ")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(statement);
    for (id, url) in rows {
        connection
            .execute(
                "UPDATE assets SET normalized_share_url=?1 WHERE id=?2",
                params![
                    normalize_share_url(&url).unwrap_or_else(|| url.trim().to_lowercase()),
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
    }
    connection
        .execute(
            "INSERT INTO schema_meta(key,value) VALUES('schema_version','2') ON CONFLICT(key) DO UPDATE SET value=CASE WHEN CAST(value AS INTEGER)<2 THEN '2' ELSE value END",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn valid_language(language: &str) -> &str {
    if language == "en" {
        "en"
    } else {
        "zh-CN"
    }
}

fn other_language(language: &str) -> &str {
    if valid_language(language) == "en" {
        "zh-CN"
    } else {
        "en"
    }
}

fn contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|ch| matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

fn detected_language(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        valid_language(fallback).into()
    } else if contains_cjk(value) {
        "zh-CN".into()
    } else {
        "en".into()
    }
}

fn migrate_to_v3(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if version
        .as_deref()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        >= 3
    {
        return Ok(());
    }
    let assets = {
        let mut statement = connection
            .prepare("SELECT id,name,description,license FROM assets")
            .map_err(|e| e.to_string())?;
        let collected = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        collected
    };
    for (id, name, description, license) in assets {
        let primary = detected_language(&name, "zh-CN");
        for locale in ["zh-CN", "en"] {
            let localized_name = if detected_language(&name, &primary) == locale {
                name.as_str()
            } else {
                ""
            };
            let localized_description = if !description.trim().is_empty()
                && detected_language(&description, &primary) == locale
            {
                description.as_str()
            } else {
                ""
            };
            let localized_license =
                if !license.trim().is_empty() && detected_language(&license, &primary) == locale {
                    license.as_str()
                } else {
                    ""
                };
            if !localized_name.is_empty()
                || !localized_description.is_empty()
                || !localized_license.is_empty()
            {
                connection.execute("INSERT OR IGNORE INTO asset_localizations(asset_id,locale,name,description,license) VALUES(?1,?2,?3,?4,?5)", params![id,locale,localized_name,localized_description,localized_license]).map_err(|e| format!("升级双语内容失败：{e}"))?;
            }
        }
        let tags = query_strings_with_param(
            connection,
            "SELECT t.name FROM asset_tags at JOIN tags t ON t.id=at.tag_id WHERE at.asset_id=?1",
            &id,
        )?;
        for tag in tags {
            let locale = detected_language(&tag, &primary);
            insert_localized_tag(connection, &id, &locale, &tag)?;
        }
    }
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','3')",
            [],
        )
        .map_err(|e| e.to_string())?;
    let ids = query_strings(connection, "SELECT id FROM assets")?;
    for id in ids {
        reindex_asset(connection, &id)?;
    }
    Ok(())
}

fn migrate_to_v4(connection: &Connection) -> Result<(), String> {
    let columns = {
        let mut statement = connection
            .prepare("PRAGMA table_info(assets)")
            .map_err(|e| e.to_string())?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        names
    };
    if !columns.iter().any(|name| name == "link_check_status") {
        connection
            .execute(
                "ALTER TABLE assets ADD COLUMN link_check_status TEXT NOT NULL DEFAULT 'unknown'",
                [],
            )
            .map_err(|e| format!("升级数据库到 v4 失败：{e}"))?;
    }
    if !columns.iter().any(|name| name == "link_checked_at") {
        connection
            .execute("ALTER TABLE assets ADD COLUMN link_checked_at TEXT", [])
            .map_err(|e| format!("升级数据库到 v4 失败：{e}"))?;
    }
    if !columns.iter().any(|name| name == "link_check_message") {
        connection
            .execute(
                "ALTER TABLE assets ADD COLUMN link_check_message TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("升级数据库到 v4 失败：{e}"))?;
    }
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','4')",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_to_v5(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','5')",
            [],
        )
        .map_err(|e| format!("升级数据库到 v5 失败：{e}"))?;
    Ok(())
}

fn migrate_to_v6(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','6')",
            [],
        )
        .map_err(|e| format!("升级数据库到 v6 失败：{e}"))?;
    Ok(())
}

fn migrate_to_v7(connection: &Connection) -> Result<(), String> {
    for (table, column, definition) in [
        ("assets", "deleted_at", "TEXT"),
        ("assets", "delete_batch_id", "TEXT"),
        ("categories", "deleted_at", "TEXT"),
        ("categories", "delete_batch_id", "TEXT"),
    ] {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if !columns.iter().any(|value| value == column) {
            connection
                .execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                    [],
                )
                .map_err(|e| format!("升级数据库到 v7 失败：{e}"))?;
        }
    }
    connection
        .execute_batch(
            "DROP INDEX IF EXISTS idx_assets_share_url;
             CREATE INDEX IF NOT EXISTS idx_assets_normalized_share_url ON assets(normalized_share_url);
             CREATE INDEX IF NOT EXISTS idx_assets_deleted ON assets(deleted_at,delete_batch_id);
             CREATE INDEX IF NOT EXISTS idx_categories_deleted ON categories(deleted_at,delete_batch_id);
             CREATE TABLE IF NOT EXISTS deletion_batches(
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('assets','category')),
               label TEXT NOT NULL,
               asset_count INTEGER NOT NULL DEFAULT 0,
               category_count INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL
             );",
        )
        .map_err(|e| format!("升级数据库到 v7 失败：{e}"))?;
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','7')",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_to_v8(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if version >= 8 {
        return Ok(());
    }
    let columns = {
        let mut statement = connection
            .prepare("PRAGMA table_info(assets)")
            .map_err(|error| error.to_string())?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        values
    };
    if !columns.iter().any(|name| name == "fab_listing_id") {
        connection
            .execute(
                "ALTER TABLE assets ADD COLUMN fab_listing_id TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|error| format!("升级数据库到 v8 失败：{error}"))?;
    }
    let assets = {
        let mut statement = connection
            .prepare("SELECT id,source_url FROM assets WHERE fab_listing_id='' AND source_url<>''")
            .map_err(|error| error.to_string())?;
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        values
    };
    for (id, source_url) in assets {
        if let Ok(listing_id) = fab::listing_id_from_url(&source_url) {
            connection
                .execute(
                    "UPDATE assets SET fab_listing_id=?1 WHERE id=?2",
                    params![listing_id.to_string(), id],
                )
                .map_err(|error| format!("回填 Fab 商品 ID 失败：{error}"))?;
        }
    }
    connection
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_assets_fab_listing_id ON assets(fab_listing_id);
             INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','8');",
        )
        .map_err(|error| format!("升级数据库到 v8 失败：{error}"))?;
    Ok(())
}

fn migrate_to_v9(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if version >= 9 {
        return Ok(());
    }
    // Tables are created by schema.sql before migrations. Keeping the version
    // step explicit makes restored v8 backups upgrade predictably.
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','9')",
            [],
        )
        .map_err(|error| format!("升级数据库到 v9 失败：{error}"))?;
    Ok(())
}

fn migrate_to_v10(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if version >= 10 {
        return Ok(());
    }
    let columns = {
        let mut statement = connection
            .prepare("PRAGMA table_info(assets)")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    if !columns.iter().any(|name| name == "rating") {
        connection
            .execute(
                "ALTER TABLE assets ADD COLUMN rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5)",
                [],
            )
            .map_err(|error| format!("升级数据库到 v10 失败：{error}"))?;
    }
    connection
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_assets_rating ON assets(rating DESC,updated_at DESC);
             INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','10');",
        )
        .map_err(|error| format!("升级数据库到 v10 失败：{error}"))?;
    Ok(())
}

fn migrate_to_v11(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    if version >= 11 {
        return Ok(());
    }
    let batch_columns = {
        let mut statement = connection
            .prepare("PRAGMA table_info(deletion_batches)")
            .map_err(|e| e.to_string())?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    if !batch_columns.iter().any(|name| name == "media_count") {
        connection.execute_batch("ALTER TABLE deletion_batches RENAME TO deletion_batches_v10;
          CREATE TABLE deletion_batches(id TEXT PRIMARY KEY,kind TEXT NOT NULL CHECK(kind IN ('assets','category','media','mediaFolder')),label TEXT NOT NULL,asset_count INTEGER NOT NULL DEFAULT 0,category_count INTEGER NOT NULL DEFAULT 0,media_count INTEGER NOT NULL DEFAULT 0,media_folder_count INTEGER NOT NULL DEFAULT 0,created_at TEXT NOT NULL);
          INSERT INTO deletion_batches(id,kind,label,asset_count,category_count,created_at) SELECT id,kind,label,asset_count,category_count,created_at FROM deletion_batches_v10;
          DROP TABLE deletion_batches_v10;").map_err(|e|format!("升级回收站到 v11 失败：{e}"))?;
    }
    connection
        .execute_batch(
            "DROP INDEX IF EXISTS idx_assets_rating;
         INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','11');",
        )
        .map_err(|e| format!("升级数据库到 v11 失败：{e}"))?;
    Ok(())
}

fn migrate_to_v12(connection: &Connection) -> Result<(), String> {
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    if version >= 12 {
        return Ok(());
    }
    for (table, column, definition) in [
        (
            "projects",
            "cover_media_entry_id",
            "TEXT REFERENCES media_entries(id) ON DELETE SET NULL",
        ),
        (
            "project_units",
            "video_media_entry_id",
            "TEXT REFERENCES media_entries(id) ON DELETE SET NULL",
        ),
    ] {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if !names.iter().any(|name| name == column) {
            connection
                .execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN {column} {definition}"
                ))
                .map_err(|e| format!("升级数据库到 v12 失败：{e}"))?;
        }
    }
    connection
        .execute_batch(
            "INSERT OR REPLACE INTO schema_meta(key,value) VALUES('schema_version','12')",
        )
        .map_err(|e| format!("升级数据库到 v12 失败：{e}"))?;
    Ok(())
}

fn insert_localized_tag(
    connection: &Connection,
    asset_id: &str,
    locale: &str,
    tag: &str,
) -> Result<(), String> {
    let normalized = tag.trim().to_lowercase();
    if normalized.is_empty() {
        return Ok(());
    }
    let tag_id = connection
        .query_row(
            "SELECT id FROM localized_tags WHERE locale=?1 AND normalized_name=?2",
            params![valid_language(locale), normalized],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    connection.execute("INSERT OR IGNORE INTO localized_tags(id,locale,name,normalized_name) VALUES(?1,?2,?3,?4)", params![tag_id,valid_language(locale),tag.trim(),normalized]).map_err(|e| e.to_string())?;
    connection
        .execute(
            "INSERT OR IGNORE INTO asset_localized_tags(asset_id,tag_id) VALUES(?1,?2)",
            params![asset_id, tag_id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn normalize_share_url(raw: &str) -> Option<String> {
    let mut parsed = Url::parse(raw.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let _ = parsed.set_scheme("https");
    parsed.set_fragment(None);
    let kept = parsed
        .query_pairs()
        .filter(|(key, _)| {
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "pwd" | "password" | "code"
            )
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    parsed.set_query(None);
    if !kept.is_empty() {
        parsed.query_pairs_mut().extend_pairs(kept);
    }
    let trimmed_path = parsed.path().trim_end_matches('/').to_string();
    parsed.set_path(if trimmed_path.is_empty() {
        "/"
    } else {
        &trimmed_path
    });
    Some(parsed.to_string().trim_end_matches('/').to_string())
}

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn json_vec(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}
fn json(values: &[String]) -> Result<String, String> {
    serde_json::to_string(values).map_err(|e| e.to_string())
}
fn placeholders(count: usize) -> String {
    std::iter::repeat("?")
        .take(count)
        .collect::<Vec<_>>()
        .join(",")
}
fn unique_values(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .filter(|v| seen.insert(v.to_lowercase()))
        .map(ToString::to_string)
        .collect()
}

pub fn library_meta(
    connection: &Connection,
    content_language: &str,
) -> Result<LibraryMeta, String> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.name, c.parent_id, c.sort_order,
         (SELECT COUNT(*) FROM assets a WHERE a.category_id = c.id AND a.deleted_at IS NULL)
         FROM categories c WHERE c.deleted_at IS NULL ORDER BY c.sort_order, c.name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let categories = statement
        .query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                sort_order: row.get(3)?,
                asset_count: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let total_assets = connection
        .query_row(
            "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let language = valid_language(content_language);
    let tags = query_strings_with_param(
        connection,
        "SELECT DISTINCT t.name FROM localized_tags t JOIN asset_localized_tags at ON at.tag_id=t.id JOIN assets a ON a.id=at.asset_id WHERE t.locale=?1 AND a.deleted_at IS NULL ORDER BY t.name COLLATE NOCASE",
        language,
    )?;
    let licenses = query_strings_with_param(connection, "SELECT DISTINCT l.license FROM asset_localizations l JOIN assets a ON a.id=l.asset_id WHERE l.locale=?1 AND trim(l.license)<>'' AND a.deleted_at IS NULL ORDER BY l.license COLLATE NOCASE", language)?;
    let (dcc_tools, versions, formats) = collect_json_options(connection)?;
    Ok(LibraryMeta {
        categories,
        filters: FilterOptions {
            tags,
            dcc_tools,
            versions,
            formats,
            licenses,
        },
        total_assets,
    })
}

fn query_strings(connection: &Connection, sql: &str) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn collect_json_options(
    connection: &Connection,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), String> {
    let mut tools = BTreeSet::new();
    let mut versions = BTreeSet::new();
    let mut formats = BTreeSet::new();
    let mut statement = connection
        .prepare("SELECT dcc_tools_json, versions_json, formats_json FROM assets WHERE deleted_at IS NULL")
        .map_err(|e| e.to_string())?;
    let mut rows = statement.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        for value in json_vec(row.get(0).map_err(|e| e.to_string())?) {
            tools.insert(value);
        }
        for value in json_vec(row.get(1).map_err(|e| e.to_string())?) {
            versions.insert(value);
        }
        for value in json_vec(row.get(2).map_err(|e| e.to_string())?) {
            formats.insert(value);
        }
    }
    Ok((
        tools.into_iter().collect(),
        versions.into_iter().collect(),
        formats.into_iter().collect(),
    ))
}

fn category_descendants(connection: &Connection, roots: &[String]) -> Result<Vec<String>, String> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("WITH RECURSIVE tree(id) AS (SELECT id FROM categories WHERE deleted_at IS NULL AND id IN ({}) UNION ALL SELECT c.id FROM categories c JOIN tree t ON c.parent_id=t.id WHERE c.deleted_at IS NULL) SELECT DISTINCT id FROM tree", placeholders(roots.len()));
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params_from_iter(roots.iter()), |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn search_assets(
    connection: &Connection,
    base_dir: &Path,
    request: &SearchRequest,
) -> Result<Page<AssetCard>, String> {
    let query = request.query.trim();
    let parsed_query = search_syntax::parse(query)?;
    let simple_query = search_syntax::simple_fts(&parsed_query);
    let use_fts = simple_query.is_some_and(|value| value.chars().count() >= 3);
    let language = valid_language(&request.content_language);
    let fallback = other_language(language);
    let mut from_clause = format!(" FROM assets a LEFT JOIN categories c ON c.id=a.category_id LEFT JOIN asset_localizations lr ON lr.asset_id=a.id AND lr.locale='{language}' LEFT JOIN asset_localizations lf ON lf.asset_id=a.id AND lf.locale='{fallback}'");
    if use_fts {
        from_clause.push_str(" JOIN asset_search ON asset_search.asset_id=a.id");
    }
    let mut conditions: Vec<String> = vec!["a.deleted_at IS NULL".into()];
    let mut values: Vec<Value> = Vec::new();
    if !query.is_empty() {
        if use_fts {
            conditions.push("asset_search.text MATCH ?".into());
            values.push(Value::Text(format!(
                "\"{}\"",
                simple_query.unwrap_or(query).replace('"', "\"\"")
            )));
        } else if let Some(condition) = search_syntax::sql(&parsed_query, language, &mut values)? {
            conditions.push(condition);
        }
    }
    let categories = category_descendants(connection, &request.category_ids)?;
    push_in_condition(
        &mut conditions,
        &mut values,
        "a.category_id",
        &categories,
        false,
    );
    if !request.tag_ids.is_empty() {
        let marks = placeholders(request.tag_ids.len());
        conditions.push(format!("EXISTS (SELECT 1 FROM asset_localized_tags at WHERE at.asset_id=a.id AND at.tag_id IN ({marks}))"));
        values.extend(request.tag_ids.iter().map(|v| Value::Text(v.clone())));
    } else if !request.tags.is_empty() {
        let marks = placeholders(request.tags.len());
        conditions.push(format!("EXISTS (SELECT 1 FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=a.id AND t.locale='{language}' AND t.normalized_name IN ({marks}))"));
        values.extend(request.tags.iter().map(|v| Value::Text(v.to_lowercase())));
    }
    push_json_condition(
        &mut conditions,
        &mut values,
        "a.dcc_tools_json",
        &request.dcc_tools,
    );
    push_json_condition(
        &mut conditions,
        &mut values,
        "a.versions_json",
        &request.versions,
    );
    push_json_condition(
        &mut conditions,
        &mut values,
        "a.formats_json",
        &request.formats,
    );
    push_in_condition(
        &mut conditions,
        &mut values,
        "lower(COALESCE(NULLIF(lr.license,''),NULLIF(lf.license,''),a.license))",
        &request.licenses,
        true,
    );
    if request.favorite_only {
        conditions.push("a.favorite=1".into());
    }
    if request.recent_only {
        conditions.push("a.last_viewed_at IS NOT NULL".into());
    }
    if let Some(project_id) = request
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut clause =
            "EXISTS (SELECT 1 FROM project_assets pa WHERE pa.project_id=? AND pa.asset_id=a.id"
                .to_string();
        values.push(Value::Text(project_id.to_string()));
        if let Some(status) = request
            .project_asset_status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            clause.push_str(" AND pa.status=?");
            values.push(Value::Text(status.to_string()));
        }
        clause.push(')');
        conditions.push(clause);
    }
    if let Some(issue) = request
        .health_issue
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let ids = health_issue_ids(connection, base_dir, issue)?;
        if ids.is_empty() {
            conditions.push("1=0".into());
        } else {
            push_in_condition(&mut conditions, &mut values, "a.id", &ids, false);
        }
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*){from_clause}{where_clause}");
    let total = connection
        .query_row(&count_sql, params_from_iter(values.iter()), |row| {
            row.get(0)
        })
        .map_err(|e| format!("统计搜索结果失败：{e}"))?;

    let order = match request.sort.as_str() {
        "name" => "COALESCE(NULLIF(lr.name,''),NULLIF(lf.name,''),a.name) COLLATE NOCASE ASC",
        "created" => "a.created_at DESC",
        "recent" => "a.last_viewed_at DESC NULLS LAST",
        "favorite" => "a.favorite DESC, a.updated_at DESC",
        "relevance" if use_fts => "CASE WHEN lower(COALESCE(lr.name,'') || ' ' || COALESCE(lr.description,'') || ' ' || COALESCE(lr.license,'')) LIKE ? ESCAPE '\\' THEN 0 ELSE 1 END, bm25(asset_search), a.updated_at DESC",
        _ => "a.updated_at DESC",
    };
    let sql = format!("SELECT a.id,COALESCE(NULLIF(lr.name,''),NULLIF(lf.name,''),a.name),c.name,a.dcc_tools_json,a.versions_json,a.formats_json,a.favorite,a.updated_at,a.last_viewed_at,
      (SELECT i.id FROM images i WHERE i.asset_id=a.id ORDER BY i.is_cover DESC,i.sort_order LIMIT 1),
      (SELECT m.id FROM asset_media m WHERE m.asset_id=a.id ORDER BY m.is_cover DESC,m.sort_order LIMIT 1),
      (SELECT m.kind FROM asset_media m WHERE m.asset_id=a.id ORDER BY m.is_cover DESC,m.sort_order LIMIT 1),
      COALESCE((SELECT json_group_array(t.name) FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=a.id AND t.locale=CASE WHEN EXISTS(SELECT 1 FROM asset_localized_tags ax JOIN localized_tags tx ON tx.id=ax.tag_id WHERE ax.asset_id=a.id AND tx.locale='{language}') THEN '{language}' ELSE '{fallback}' END),'[]'),
      CASE WHEN COALESCE(lr.name,'')<>'' THEN '{language}' ELSE '{fallback}' END,
      CASE WHEN COALESCE(lr.name,'')<>'' THEN 0 ELSE 1 END,
      a.link_check_status,a.link_checked_at,a.link_check_message,CASE WHEN trim(a.share_url)<>'' THEN 1 ELSE 0 END
      {from_clause}{where_clause} ORDER BY {order} LIMIT ? OFFSET ?");
    let mut page_values = values;
    if request.sort == "relevance" && use_fts {
        page_values.push(Value::Text(format!(
            "%{}%",
            escape_like(&query.to_lowercase())
        )));
    }
    page_values.push(Value::Integer(request.limit.clamp(1, 200)));
    page_values.push(Value::Integer(request.offset.max(0)));
    let mut statement = connection
        .prepare(&sql)
        .map_err(|e| format!("准备搜索失败：{e}"))?;
    let items = statement
        .query_map(params_from_iter(page_values.iter()), card_from_row)
        .map_err(|e| format!("执行搜索失败：{e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page {
        items,
        total,
        offset: request.offset.max(0),
        limit: request.limit.clamp(1, 200),
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn push_in_condition(
    conditions: &mut Vec<String>,
    params: &mut Vec<Value>,
    column: &str,
    values: &[String],
    lowercase: bool,
) {
    if values.is_empty() {
        return;
    }
    conditions.push(format!("{column} IN ({})", placeholders(values.len())));
    params.extend(values.iter().map(|value| {
        Value::Text(if lowercase {
            value.to_lowercase()
        } else {
            value.clone()
        })
    }));
}

fn push_json_condition(
    conditions: &mut Vec<String>,
    params: &mut Vec<Value>,
    column: &str,
    values: &[String],
) {
    if values.is_empty() {
        return;
    }
    conditions.push(format!(
        "EXISTS (SELECT 1 FROM json_each({column}) j WHERE lower(j.value) IN ({}))",
        placeholders(values.len())
    ));
    params.extend(values.iter().map(|value| Value::Text(value.to_lowercase())));
}

fn card_from_row(row: &Row<'_>) -> rusqlite::Result<AssetCard> {
    Ok(AssetCard {
        id: row.get(0)?,
        name: row.get(1)?,
        category_name: row.get(2)?,
        dcc_tools: json_vec(row.get(3)?),
        versions: json_vec(row.get(4)?),
        formats: json_vec(row.get(5)?),
        favorite: row.get::<_, i64>(6)? != 0,
        updated_at: row.get(7)?,
        last_viewed_at: row.get(8)?,
        cover_image_id: row.get(9)?,
        cover_media_id: row.get(10)?,
        cover_media_kind: row.get(11)?,
        tags: json_vec(row.get(12)?),
        content_language: row.get(13)?,
        language_fallback: row.get::<_, i64>(14)? != 0,
        link_check_status: row.get(15)?,
        link_checked_at: row.get(16)?,
        link_check_message: row.get(17)?,
        has_share_link: row.get::<_, i64>(18)? != 0,
    })
}

pub fn get_asset(
    connection: &Connection,
    id: &str,
    mark_viewed: bool,
    content_language: &str,
) -> Result<AssetDetail, String> {
    if mark_viewed {
        connection
            .execute(
                "UPDATE assets SET last_viewed_at=?1 WHERE id=?2 AND deleted_at IS NULL",
                params![now(), id],
            )
            .map_err(|e| e.to_string())?;
    }
    let language = valid_language(content_language);
    let fallback = other_language(language);
    let sql = format!("SELECT a.id,COALESCE(NULLIF(lr.name,''),NULLIF(lf.name,''),a.name),c.name,a.dcc_tools_json,a.versions_json,a.formats_json,a.favorite,a.updated_at,a.last_viewed_at,
      (SELECT i.id FROM images i WHERE i.asset_id=a.id ORDER BY i.is_cover DESC,i.sort_order LIMIT 1),
      (SELECT m.id FROM asset_media m WHERE m.asset_id=a.id ORDER BY m.is_cover DESC,m.sort_order LIMIT 1),
      (SELECT m.kind FROM asset_media m WHERE m.asset_id=a.id ORDER BY m.is_cover DESC,m.sort_order LIMIT 1),
      COALESCE((SELECT json_group_array(t.name) FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=a.id AND t.locale=CASE WHEN EXISTS(SELECT 1 FROM asset_localized_tags ax JOIN localized_tags tx ON tx.id=ax.tag_id WHERE ax.asset_id=a.id AND tx.locale='{language}') THEN '{language}' ELSE '{fallback}' END),'[]'),
      CASE WHEN COALESCE(lr.name,'')<>'' THEN '{language}' ELSE '{fallback}' END,
      CASE WHEN COALESCE(lr.name,'')<>'' THEN 0 ELSE 1 END,
      a.link_check_status,a.link_checked_at,a.link_check_message,CASE WHEN trim(a.share_url)<>'' THEN 1 ELSE 0 END,
      COALESCE(NULLIF(lr.description,''),NULLIF(lf.description,''),a.description),a.category_id,a.size_bytes,a.author,a.source_url,COALESCE(NULLIF(lr.license,''),NULLIF(lf.license,''),a.license),a.share_url,a.extraction_code,a.created_at,a.fab_listing_id
      FROM assets a LEFT JOIN categories c ON c.id=a.category_id LEFT JOIN asset_localizations lr ON lr.asset_id=a.id AND lr.locale='{language}' LEFT JOIN asset_localizations lf ON lf.asset_id=a.id AND lf.locale='{fallback}' WHERE a.id=?1 AND a.deleted_at IS NULL");
    let fields = connection
        .query_row(&sql, [id], |row| {
            Ok((
                card_from_row(row)?,
                row.get::<_, String>(19)?,
                row.get::<_, Option<String>>(20)?,
                row.get::<_, Option<i64>>(21)?,
                row.get::<_, String>(22)?,
                row.get::<_, String>(23)?,
                row.get::<_, String>(24)?,
                row.get::<_, String>(25)?,
                row.get::<_, String>(26)?,
                row.get::<_, String>(27)?,
                row.get::<_, String>(28)?,
            ))
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("素材不存在")?;
    let mut statement = connection.prepare("SELECT id,original_name,sort_order,is_cover FROM images WHERE asset_id=?1 ORDER BY sort_order").map_err(|e| e.to_string())?;
    let images = statement
        .query_map([id], |row| {
            Ok(AssetImage {
                id: row.get(0)?,
                original_name: row.get(1)?,
                sort_order: row.get(2)?,
                is_cover: row.get::<_, i64>(3)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let media = crate::media::list(connection, id)?;
    let media_library = {
        let mut statement = connection.prepare(
            "SELECT e.id,e.kind,e.name,m.original_name,m.logical_path,e.primary_file_id,
                    COALESCE((SELECT id FROM media_files WHERE entry_id=e.id AND role='proxy' ORDER BY sort_order LIMIT 1),e.primary_file_id),
                    (SELECT id FROM media_files WHERE entry_id=e.id AND role IN ('thumbnail','waveform') ORDER BY CASE role WHEN 'thumbnail' THEN 0 ELSE 1 END LIMIT 1),
                    e.processing_status
             FROM media_entry_assets x
             JOIN media_entries e ON e.id=x.entry_id AND e.deleted_at IS NULL
             JOIN media_files m ON m.id=e.primary_file_id
             WHERE x.asset_id=?1 ORDER BY e.updated_at DESC",
        ).map_err(|error| error.to_string())?;
        let values = statement
            .query_map([id], |row| {
                let kind = match row.get::<_, String>(1)?.as_str() {
                    "image" => MediaKind::Image,
                    "model" => MediaKind::Model,
                    "audio" => MediaKind::Audio,
                    "video" => MediaKind::Video,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok(LinkedMediaPreview {
                    id: row.get(0)?,
                    kind,
                    name: row.get(2)?,
                    original_name: row.get(3)?,
                    logical_path: row.get(4)?,
                    primary_file_id: row.get(5)?,
                    stream_file_id: row.get(6)?,
                    thumbnail_file_id: row.get(7)?,
                    processing_status: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        values
    };
    let mut localizations = HashMap::new();
    for locale in ["zh-CN", "en"] {
        let value = connection.query_row("SELECT name,description,license FROM asset_localizations WHERE asset_id=?1 AND locale=?2", params![id,locale], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).optional().map_err(|e| e.to_string())?;
        let tags = query_strings_with_params(connection, "SELECT t.name FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=?1 AND t.locale=?2 ORDER BY t.name COLLATE NOCASE", id, locale)?;
        if let Some((name, description, license)) = value {
            localizations.insert(
                locale.into(),
                LocalizedAssetText {
                    name,
                    description,
                    tags,
                    license,
                },
            );
        } else if !tags.is_empty() {
            localizations.insert(
                locale.into(),
                LocalizedAssetText {
                    tags,
                    ..Default::default()
                },
            );
        }
    }
    Ok(AssetDetail {
        card: fields.0,
        description: fields.1,
        category_id: fields.2,
        size_bytes: fields.3,
        author: fields.4,
        source_url: fields.5,
        fab_listing_id: fields.10,
        license: fields.6,
        share_url: fields.7,
        extraction_code: fields.8,
        created_at: fields.9,
        images,
        media,
        media_library,
        localizations,
    })
}

pub fn upsert_asset(
    connection: &mut Connection,
    base_dir: &Path,
    mut input: AssetInput,
) -> Result<AssetDetail, String> {
    if input.localizations.is_empty() {
        let locale = detected_language(&input.name, &input.content_language);
        input.localizations.insert(
            locale,
            LocalizedAssetText {
                name: input.name.clone(),
                description: input.description.clone(),
                tags: input.tags.clone(),
                license: input.license.clone(),
            },
        );
    }
    input
        .localizations
        .retain(|locale, _| matches!(locale.as_str(), "zh-CN" | "en"));
    for localized in input.localizations.values_mut() {
        localized.tags = unique_values(&localized.tags);
    }
    let preferred = valid_language(&input.content_language);
    let mirror = input
        .localizations
        .get(preferred)
        .filter(|v| !v.name.trim().is_empty())
        .or_else(|| input.localizations.get(other_language(preferred)))
        .cloned()
        .unwrap_or_default();
    input.name = mirror.name;
    input.description = mirror.description;
    input.tags = mirror.tags;
    input.license = mirror.license;
    validate_input(&input)?;
    input.tags = unique_values(&input.tags);
    input.dcc_tools = unique_values(&input.dcc_tools);
    input.versions = unique_values(&input.versions);
    input.formats = unique_values(&input.formats);
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let source_listing_id = fab::listing_id_from_url(&input.source_url)
        .ok()
        .map(|value| value.to_string());
    let supplied_listing_id = input
        .fab_listing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            Uuid::parse_str(value)
                .map(|value| value.to_string())
                .map_err(|_| "Fab 商品 ID 格式无效".to_string())
        })
        .transpose()?;
    let existing_listing_id = connection
        .query_row(
            "SELECT fab_listing_id FROM assets WHERE id=?1",
            [&id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .filter(|value| !value.is_empty());
    let fab_listing_id = source_listing_id
        .or(supplied_listing_id)
        .or(existing_listing_id)
        .unwrap_or_default();
    if !fab_listing_id.is_empty() {
        if let Some(duplicate) = fab_duplicate_match(connection, &fab_listing_id, Some(&id))? {
            return Err(if duplicate.location == "trash" {
                format!("该 Fab 素材已在回收站：{}", duplicate.asset_name)
            } else {
                format!("该 Fab 素材已存在：{}", duplicate.asset_name)
            });
        }
    }
    input.fab_listing_id = (!fab_listing_id.is_empty()).then(|| fab_listing_id.clone());
    let timestamp = now();
    let mut new_images = Vec::new();
    for image in input.images.iter().filter(|image| image.id.is_none()) {
        let source = image.source_path.as_ref().ok_or("新增图片缺少源路径")?;
        match images::import_image(base_dir, Path::new(source)) {
            Ok(mut stored) => {
                if let Some(name) = image
                    .original_name
                    .as_deref()
                    .and_then(|name| Path::new(name).file_name())
                    .and_then(|name| name.to_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                {
                    stored.original_name = name.chars().take(255).collect();
                }
                new_images.push((image.sort_order, image.is_cover, stored));
            }
            Err(error) => {
                for (_, _, stored) in &new_images {
                    images::remove_managed_file(base_dir, &stored.original_rel_path);
                    images::remove_managed_file(base_dir, &stored.thumbnail_rel_path);
                }
                return Err(error);
            }
        }
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let result = (|| -> Result<Vec<(String, String)>, String> {
        if input.category_id.is_none() && !input.auto_category_path.is_empty() {
            input.category_id = ensure_auto_category_path(&transaction, &input.auto_category_path)?;
        }
        let created = transaction
            .query_row("SELECT created_at FROM assets WHERE id=?1", [&id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| timestamp.clone());
        let normalized_share_url = if input.share_url.trim().is_empty() {
            String::new()
        } else {
            normalize_share_url(&input.share_url).ok_or("分享链接格式无效")?
        };
        if !normalized_share_url.is_empty() {
            let duplicate: Option<String> = transaction
                .query_row(
                    "SELECT id FROM assets WHERE normalized_share_url=?1 AND id<>?2 AND deleted_at IS NULL LIMIT 1",
                    params![normalized_share_url, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if duplicate.is_some() {
                return Err("该分享链接已存在于素材库".into());
            }
        }
        transaction.execute("INSERT INTO assets(id,name,description,category_id,dcc_tools_json,versions_json,formats_json,size_bytes,author,source_url,fab_listing_id,license,share_url,normalized_share_url,extraction_code,favorite,rating,created_at,updated_at)
          VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,0,?17,?18)
          ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,category_id=excluded.category_id,dcc_tools_json=excluded.dcc_tools_json,versions_json=excluded.versions_json,formats_json=excluded.formats_json,size_bytes=excluded.size_bytes,author=excluded.author,source_url=excluded.source_url,fab_listing_id=excluded.fab_listing_id,license=excluded.license,share_url=excluded.share_url,normalized_share_url=excluded.normalized_share_url,link_check_status=CASE WHEN assets.normalized_share_url=excluded.normalized_share_url THEN assets.link_check_status ELSE 'unknown' END,link_checked_at=CASE WHEN assets.normalized_share_url=excluded.normalized_share_url THEN assets.link_checked_at ELSE NULL END,link_check_message=CASE WHEN assets.normalized_share_url=excluded.normalized_share_url THEN assets.link_check_message ELSE '' END,extraction_code=excluded.extraction_code,favorite=excluded.favorite,updated_at=excluded.updated_at",
           params![id,input.name.trim(),input.description.trim(),input.category_id,json(&input.dcc_tools)?,json(&input.versions)?,json(&input.formats)?,input.size_bytes,input.author.trim(),input.source_url.trim(),fab_listing_id,input.license.trim(),input.share_url.trim(),normalized_share_url,input.extraction_code.trim(),input.favorite as i64,created,timestamp]).map_err(map_constraint)?;
        transaction
            .execute("DELETE FROM asset_localized_tags WHERE asset_id=?1", [&id])
            .map_err(|e| e.to_string())?;
        transaction
            .execute("DELETE FROM asset_localizations WHERE asset_id=?1", [&id])
            .map_err(|e| e.to_string())?;
        for (locale, localized) in &input.localizations {
            transaction.execute("INSERT INTO asset_localizations(asset_id,locale,name,description,license) VALUES(?1,?2,?3,?4,?5)", params![id,valid_language(locale),localized.name.trim(),localized.description.trim(),localized.license.trim()]).map_err(|e| e.to_string())?;
            for tag in &localized.tags {
                insert_localized_tag(&transaction, &id, locale, tag)?;
            }
        }
        transaction
            .execute("DELETE FROM asset_tags WHERE asset_id=?1", [&id])
            .map_err(|e| e.to_string())?;
        for tag in &input.tags {
            let normalized = tag.to_lowercase();
            let tag_id = transaction
                .query_row(
                    "SELECT id FROM tags WHERE normalized_name=?1",
                    [&normalized],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            transaction
                .execute(
                    "INSERT OR IGNORE INTO tags(id,name,normalized_name) VALUES(?1,?2,?3)",
                    params![tag_id, tag, normalized],
                )
                .map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "INSERT INTO asset_tags(asset_id,tag_id) VALUES(?1,?2)",
                    params![id, tag_id],
                )
                .map_err(|e| e.to_string())?;
        }
        let keep: HashSet<String> = input
            .images
            .iter()
            .filter_map(|image| image.id.clone())
            .collect();
        let mut removed = Vec::new();
        let mut stmt = transaction
            .prepare("SELECT id,original_rel_path,thumbnail_rel_path FROM images WHERE asset_id=?1")
            .map_err(|e| e.to_string())?;
        let existing = stmt
            .query_map([&id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        for (image_id, original, thumb) in existing {
            if !keep.contains(&image_id) {
                transaction
                    .execute("DELETE FROM images WHERE id=?1", [&image_id])
                    .map_err(|e| e.to_string())?;
                removed.push((original, thumb));
            }
        }
        for image in input.images.iter().filter(|image| image.id.is_some()) {
            transaction
                .execute(
                    "UPDATE images SET sort_order=?1,is_cover=?2 WHERE id=?3 AND asset_id=?4",
                    params![image.sort_order, image.is_cover as i64, image.id, id],
                )
                .map_err(|e| e.to_string())?;
        }
        for (sort_order, is_cover, stored) in &new_images {
            transaction.execute("INSERT INTO images(id,asset_id,original_name,original_rel_path,thumbnail_rel_path,sort_order,is_cover,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![stored.id,id,stored.original_name,stored.original_rel_path,stored.thumbnail_rel_path,sort_order,*is_cover as i64,timestamp]).map_err(|e| e.to_string())?;
        }
        if input.images.iter().any(|image| image.is_cover) {
            transaction
                .execute("UPDATE asset_media SET is_cover=0 WHERE asset_id=?1", [&id])
                .map_err(|e| e.to_string())?;
        }
        normalize_cover(&transaction, &id)?;
        reindex_asset(&transaction, &id)?;
        Ok(removed)
    })();
    match result {
        Ok(removed) => {
            transaction.commit().map_err(|e| e.to_string())?;
            for (original, thumb) in removed {
                images::remove_managed_file(base_dir, &original);
                images::remove_managed_file(base_dir, &thumb);
            }
            get_asset(connection, &id, false, &input.content_language)
        }
        Err(error) => {
            drop(transaction);
            for (_, _, stored) in new_images {
                images::remove_managed_file(base_dir, &stored.original_rel_path);
                images::remove_managed_file(base_dir, &stored.thumbnail_rel_path);
            }
            Err(error)
        }
    }
}

fn validate_input(input: &AssetInput) -> Result<(), String> {
    if input
        .localizations
        .values()
        .all(|value| value.name.trim().is_empty())
    {
        return Err("素材名称不能为空".into());
    }
    if input
        .localizations
        .values()
        .any(|value| value.name.chars().count() > 200)
    {
        return Err("素材名称不能超过 200 个字符".into());
    }
    validate_http_url(&input.share_url, false)?;
    if !input.source_url.trim().is_empty() {
        validate_http_url(&input.source_url, false)?;
    }
    if input.size_bytes.is_some_and(|size| size < 0) {
        return Err("素材大小不能为负数".into());
    }
    Ok(())
}

fn validate_http_url(raw: &str, required: bool) -> Result<(), String> {
    if !required && raw.trim().is_empty() {
        return Ok(());
    }
    let url = Url::parse(raw.trim()).map_err(|_| "链接格式无效")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只允许 http/https 链接".into());
    }
    Ok(())
}

fn map_constraint(error: rusqlite::Error) -> String {
    let text = error.to_string();
    if text.contains("assets.share_url") {
        "该分享链接已存在于素材库".into()
    } else {
        text
    }
}

fn normalize_cover(transaction: &Transaction<'_>, asset_id: &str) -> Result<(), String> {
    let media_cover = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM asset_media WHERE asset_id=?1 AND is_cover=1)",
            [asset_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        != 0;
    if media_cover {
        transaction
            .execute("UPDATE images SET is_cover=0 WHERE asset_id=?1", [asset_id])
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let cover = transaction
        .query_row(
            "SELECT id FROM images WHERE asset_id=?1 ORDER BY is_cover DESC,sort_order LIMIT 1",
            [asset_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    transaction
        .execute("UPDATE images SET is_cover=0 WHERE asset_id=?1", [asset_id])
        .map_err(|e| e.to_string())?;
    if let Some(id) = cover {
        transaction
            .execute("UPDATE images SET is_cover=1 WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn category_path(connection: &Connection, category_id: Option<&str>) -> Result<String, String> {
    let Some(mut current) = category_id.map(ToString::to_string) else {
        return Ok(String::new());
    };
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let value = connection
            .query_row(
                "SELECT name,parent_id FROM categories WHERE id=?1",
                [&current],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match value {
            Some((name, parent)) => {
                names.push(name);
                if let Some(parent) = parent {
                    current = parent
                } else {
                    break;
                }
            }
            None => break,
        }
    }
    names.reverse();
    Ok(names.join(" / "))
}

pub fn fab_duplicate_match(
    connection: &Connection,
    listing_id: &str,
    exclude_asset_id: Option<&str>,
) -> Result<Option<FabDuplicateMatch>, String> {
    let listing_id = Uuid::parse_str(listing_id.trim())
        .map_err(|_| "Fab 商品 ID 格式无效")?
        .to_string();
    let value = connection
        .query_row(
            "SELECT id,name,category_id,deleted_at FROM assets WHERE fab_listing_id=?1 AND id<>COALESCE(?2,'') ORDER BY CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END,updated_at DESC LIMIT 1",
            params![listing_id, exclude_asset_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((asset_id, asset_name, category_id, deleted_at)) = value else {
        return Ok(None);
    };
    Ok(Some(FabDuplicateMatch {
        asset_id,
        asset_name,
        category_path: category_path(connection, category_id.as_deref())?,
        location: if deleted_at.is_some() {
            "trash"
        } else {
            "library"
        }
        .into(),
    }))
}

pub fn auto_category_path_exists(connection: &Connection, path: &[String]) -> Result<bool, String> {
    if path.is_empty() || path.len() > 2 {
        return Ok(false);
    }
    let mut parent: Option<String> = None;
    for name in path {
        let id = connection
            .query_row(
                "SELECT id FROM categories WHERE name=?1 COLLATE NOCASE AND parent_id IS ?2 AND deleted_at IS NULL LIMIT 1",
                params![name.trim(), parent],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some(id) = id else {
            return Ok(false);
        };
        parent = Some(id);
    }
    Ok(true)
}

fn ensure_auto_category_path(
    transaction: &Transaction<'_>,
    path: &[String],
) -> Result<Option<String>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    if path.len() > 2 {
        return Err("自动分类最多支持两层".into());
    }
    let mut parent: Option<String> = None;
    for raw_name in path {
        let name = raw_name.trim();
        if name.is_empty()
            || name.chars().count() > 60
            || name
                .chars()
                .any(|value| value.is_control() || matches!(value, '/' | '\\'))
        {
            return Err("自动分类名称不安全".into());
        }
        let existing = transaction
            .query_row(
                "SELECT id,deleted_at,delete_batch_id FROM categories WHERE name=?1 COLLATE NOCASE AND parent_id IS ?2 LIMIT 1",
                params![name, parent],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let id = if let Some((id, deleted_at, batch_id)) = existing {
            if deleted_at.is_some() {
                transaction
                    .execute(
                        "UPDATE categories SET deleted_at=NULL,delete_batch_id=NULL WHERE id=?1",
                        [&id],
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(batch_id) = batch_id {
                    transaction
                        .execute(
                            "UPDATE deletion_batches SET category_count=MAX(category_count-1,0) WHERE id=?1",
                            [&batch_id],
                        )
                        .map_err(|error| error.to_string())?;
                    transaction
                        .execute(
                            "DELETE FROM deletion_batches WHERE id=?1 AND asset_count=0 AND category_count=0",
                            [&batch_id],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            id
        } else {
            let id = Uuid::new_v4().to_string();
            transaction
                .execute(
                    "INSERT INTO categories(id,name,parent_id,sort_order,created_at) VALUES(?1,?2,?3,COALESCE((SELECT MAX(sort_order)+1 FROM categories WHERE parent_id IS ?3 AND deleted_at IS NULL),0),?4)",
                    params![id, name, parent, now()],
                )
                .map_err(map_constraint)?;
            id
        };
        parent = Some(id);
    }
    Ok(parent)
}

pub fn reindex_asset(connection: &Connection, asset_id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM asset_search WHERE asset_id=?1", [asset_id])
        .map_err(|e| e.to_string())?;
    let active_clause = if table_has_column(connection, "assets", "deleted_at")? {
        " AND deleted_at IS NULL"
    } else {
        ""
    };
    let sql = format!("SELECT name,description,category_id,dcc_tools_json,versions_json,formats_json,author,source_url,license FROM assets WHERE id=?1{active_clause}");
    let Some(row) = connection
        .query_row(&sql, [asset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .optional()
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    let tags = query_strings_with_param(
        connection,
        "SELECT t.name FROM asset_tags at JOIN tags t ON t.id=at.tag_id WHERE at.asset_id=?1",
        asset_id,
    )?;
    let localized = query_strings_with_param(connection, "SELECT name || ' ' || description || ' ' || license FROM asset_localizations WHERE asset_id=?1", asset_id)?;
    let localized_tags = query_strings_with_param(connection, "SELECT t.name FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=?1", asset_id)?;
    let text = [
        row.0,
        row.1,
        category_path(connection, row.2.as_deref())?,
        json_vec(row.3).join(" "),
        json_vec(row.4).join(" "),
        json_vec(row.5).join(" "),
        row.6,
        row.7,
        row.8,
        tags.join(" "),
        localized.join(" "),
        localized_tags.join(" "),
    ]
    .join(" ")
    .to_lowercase();
    connection
        .execute(
            "INSERT INTO asset_search(asset_id,text) VALUES(?1,?2)",
            params![asset_id, text],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(columns.iter().any(|value| value == column))
}

fn query_strings_with_param(
    connection: &Connection,
    sql: &str,
    value: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([value], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn query_strings_with_params(
    connection: &Connection,
    sql: &str,
    first: &str,
    second: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![first, second], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn set_favorite(connection: &Connection, id: &str, favorite: bool) -> Result<(), String> {
    if connection
        .execute(
            "UPDATE assets SET favorite=?1,updated_at=?2 WHERE id=?3 AND deleted_at IS NULL",
            params![favorite as i64, now(), id],
        )
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("素材不存在".into());
    }
    Ok(())
}

pub fn delete_asset(connection: &mut Connection, _base_dir: &Path, id: &str) -> Result<(), String> {
    delete_library_items(
        connection,
        DeleteRequest {
            asset_ids: vec![id.into()],
            category_id: None,
        },
    )?;
    Ok(())
}

pub fn select_asset_ids(
    connection: &Connection,
    base_dir: &Path,
    request: &SearchRequest,
) -> Result<AssetSelection, String> {
    let mut ids = Vec::new();
    let mut offset = 0;
    loop {
        let mut page_request = request.clone();
        page_request.offset = offset;
        page_request.limit = 200;
        let page = search_assets(connection, base_dir, &page_request)?;
        ids.extend(page.items.into_iter().map(|item| item.id));
        if ids.len() as i64 >= page.total || page.limit == 0 {
            break;
        }
        offset = ids.len() as i64;
    }
    Ok(AssetSelection {
        total: ids.len(),
        ids,
    })
}

fn delete_impact_inner(
    connection: &Connection,
    request: &DeleteRequest,
) -> Result<(String, Vec<String>, Vec<String>), String> {
    if let Some(category_id) = request.category_id.as_deref() {
        let label = connection
            .query_row(
                "SELECT name FROM categories WHERE id=?1 AND deleted_at IS NULL",
                [category_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("分类不存在")?;
        let categories = query_strings_with_param(connection,
            "WITH RECURSIVE tree(id) AS (SELECT id FROM categories WHERE id=?1 AND deleted_at IS NULL UNION ALL SELECT c.id FROM categories c JOIN tree t ON c.parent_id=t.id WHERE c.deleted_at IS NULL) SELECT id FROM tree",
            category_id)?;
        let mut assets = Vec::new();
        for category in &categories {
            assets.extend(query_strings_with_param(
                connection,
                "SELECT id FROM assets WHERE category_id=?1 AND deleted_at IS NULL",
                category,
            )?);
        }
        return Ok((label, unique_values(&assets), categories));
    }
    let requested = unique_values(&request.asset_ids);
    if requested.is_empty() {
        return Err("请先选择要删除的素材或分类".into());
    }
    let mut assets = Vec::new();
    let mut first_name = String::new();
    for id in requested {
        if let Some(name) = connection
            .query_row(
                "SELECT name FROM assets WHERE id=?1 AND deleted_at IS NULL",
                [&id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            if first_name.is_empty() {
                first_name = name;
            }
            assets.push(id);
        }
    }
    if assets.is_empty() {
        return Err("素材不存在或已在回收站".into());
    }
    let label = if assets.len() == 1 {
        first_name
    } else {
        format!("{} 等 {} 项素材", first_name, assets.len())
    };
    Ok((label, assets, Vec::new()))
}

pub fn delete_impact(
    connection: &Connection,
    request: &DeleteRequest,
) -> Result<DeleteResult, String> {
    let (label, assets, categories) = delete_impact_inner(connection, request)?;
    Ok(DeleteResult {
        batch_id: String::new(),
        label,
        asset_count: assets.len(),
        category_count: categories.len(),
        media_count: 0,
        media_folder_count: 0,
    })
}

pub fn delete_library_items(
    connection: &mut Connection,
    request: DeleteRequest,
) -> Result<DeleteResult, String> {
    let (label, assets, categories) = delete_impact_inner(connection, &request)?;
    let batch_id = Uuid::new_v4().to_string();
    let timestamp = now();
    let kind = if categories.is_empty() {
        "assets"
    } else {
        "category"
    };
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction.execute("INSERT INTO deletion_batches(id,kind,label,asset_count,category_count,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![batch_id,kind,label,assets.len() as i64,categories.len() as i64,timestamp]).map_err(|e| e.to_string())?;
    for id in &assets {
        transaction.execute("UPDATE assets SET deleted_at=?1,delete_batch_id=?2 WHERE id=?3 AND deleted_at IS NULL", params![timestamp,batch_id,id]).map_err(|e| e.to_string())?;
        transaction
            .execute("DELETE FROM asset_search WHERE asset_id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    for id in &categories {
        transaction.execute("UPDATE categories SET deleted_at=?1,delete_batch_id=?2 WHERE id=?3 AND deleted_at IS NULL", params![timestamp,batch_id,id]).map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(DeleteResult {
        batch_id,
        label,
        asset_count: assets.len(),
        category_count: categories.len(),
        media_count: 0,
        media_folder_count: 0,
    })
}

pub fn list_trash(
    connection: &Connection,
    offset: i64,
    limit: i64,
) -> Result<Page<TrashBatch>, String> {
    let offset = offset.max(0);
    let limit = limit.clamp(1, 200);
    let total = connection
        .query_row("SELECT COUNT(*) FROM deletion_batches", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    let mut statement = connection.prepare("SELECT id,kind,label,asset_count,category_count,media_count,media_folder_count,created_at FROM deletion_batches ORDER BY created_at DESC LIMIT ?1 OFFSET ?2").map_err(|e| e.to_string())?;
    let items = statement
        .query_map(params![limit, offset], |row| {
            Ok(TrashBatch {
                id: row.get(0)?,
                kind: row.get(1)?,
                label: row.get(2)?,
                asset_count: row.get(3)?,
                category_count: row.get(4)?,
                media_count: row.get(5)?,
                media_folder_count: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page {
        items,
        total,
        offset,
        limit,
    })
}

pub fn restore_trash_batch(
    connection: &mut Connection,
    batch_id: &str,
) -> Result<DeleteResult, String> {
    let batch = connection
        .query_row(
            "SELECT kind,label,asset_count,category_count,media_count,media_folder_count FROM deletion_batches WHERE id=?1",
            [batch_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("回收站记录不存在")?;
    let ids = query_strings_with_param(
        connection,
        "SELECT id FROM assets WHERE delete_batch_id=?1",
        batch_id,
    )?;
    for id in &ids {
        let listing_id = connection
            .query_row(
                "SELECT fab_listing_id FROM assets WHERE id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        if !listing_id.is_empty() {
            let conflict: Option<String> = connection
                .query_row(
                    "SELECT name FROM assets WHERE fab_listing_id=?1 AND deleted_at IS NULL AND id<>?2 LIMIT 1",
                    params![listing_id, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(name) = conflict {
                return Err(format!("无法恢复：相同 Fab 素材已存在于素材库（{name}）"));
            }
        }
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE categories SET deleted_at=NULL,delete_batch_id=NULL WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE assets SET deleted_at=NULL,delete_batch_id=NULL WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute("UPDATE media_folders SET deleted_at=NULL,delete_batch_id=NULL WHERE delete_batch_id=?1",[batch_id]).map_err(|e|e.to_string())?;
    transaction.execute("UPDATE media_entries SET deleted_at=NULL,delete_batch_id=NULL WHERE delete_batch_id=?1",[batch_id]).map_err(|e|e.to_string())?;
    transaction
        .execute("DELETE FROM deletion_batches WHERE id=?1", [batch_id])
        .map_err(|e| e.to_string())?;
    for id in &ids {
        reindex_asset(&transaction, id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(DeleteResult {
        batch_id: batch_id.into(),
        label: batch.1,
        asset_count: batch.2 as usize,
        category_count: batch.3 as usize,
        media_count: batch.4 as usize,
        media_folder_count: batch.5 as usize,
    })
}

pub fn purge_trash_batch(
    connection: &mut Connection,
    base_dir: &Path,
    batch_id: &str,
) -> Result<(), String> {
    let mut statement = connection.prepare("SELECT i.original_rel_path,i.thumbnail_rel_path FROM images i JOIN assets a ON a.id=i.asset_id WHERE a.delete_batch_id=?1").map_err(|e| e.to_string())?;
    let paths = statement
        .query_map([batch_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(statement);
    let mut media_statement = connection.prepare("SELECT m.original_rel_path,m.proxy_rel_path,m.thumbnail_rel_path FROM asset_media m JOIN assets a ON a.id=m.asset_id WHERE a.delete_batch_id=?1").map_err(|e| e.to_string())?;
    let media_paths = media_statement
        .query_map([batch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(media_statement);
    let independent_media_paths = {
        let mut statement=connection.prepare("SELECT f.rel_path FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE e.delete_batch_id=?1").map_err(|e|e.to_string())?;
        let values = statement
            .query_map([batch_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction.execute("DELETE FROM asset_search WHERE asset_id IN (SELECT id FROM assets WHERE delete_batch_id=?1)", [batch_id]).map_err(|e| e.to_string())?;
    transaction
        .execute("DELETE FROM assets WHERE delete_batch_id=?1", [batch_id])
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE categories SET parent_id=NULL WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "DELETE FROM categories WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute("DELETE FROM media_search WHERE entry_id IN (SELECT id FROM media_entries WHERE delete_batch_id=?1)",[batch_id]).map_err(|e|e.to_string())?;
    transaction
        .execute(
            "DELETE FROM media_entries WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE media_folders SET parent_id=NULL WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    transaction
        .execute(
            "DELETE FROM media_folders WHERE delete_batch_id=?1",
            [batch_id],
        )
        .map_err(|e| e.to_string())?;
    if transaction
        .execute("DELETE FROM deletion_batches WHERE id=?1", [batch_id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("回收站记录不存在".into());
    }
    transaction.execute("DELETE FROM tags WHERE NOT EXISTS(SELECT 1 FROM asset_tags at WHERE at.tag_id=tags.id)", []).map_err(|e| e.to_string())?;
    transaction.execute("DELETE FROM localized_tags WHERE NOT EXISTS(SELECT 1 FROM asset_localized_tags at WHERE at.tag_id=localized_tags.id)", []).map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    for (original, thumb) in paths {
        images::remove_managed_file(base_dir, &original);
        images::remove_managed_file(base_dir, &thumb);
    }
    for (original, proxy, thumb) in media_paths {
        images::remove_managed_file(base_dir, &original);
        if let Some(path) = proxy {
            images::remove_managed_file(base_dir, &path);
        }
        if let Some(path) = thumb {
            images::remove_managed_file(base_dir, &path);
        }
    }
    for path in independent_media_paths {
        images::remove_managed_file(base_dir, &path);
    }
    Ok(())
}

pub fn empty_trash(connection: &mut Connection, base_dir: &Path) -> Result<usize, String> {
    let ids = query_strings(
        connection,
        "SELECT id FROM deletion_batches ORDER BY created_at",
    )?;
    let count = ids.len();
    for id in ids {
        purge_trash_batch(connection, base_dir, &id)?;
    }
    Ok(count)
}

pub fn prepare_reference_cover_ids(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    for id in unique_values(ids) {
        let image = connection.query_row("SELECT i.id FROM images i JOIN assets a ON a.id=i.asset_id WHERE a.id=?1 AND a.deleted_at IS NULL ORDER BY i.is_cover DESC,i.sort_order LIMIT 1", [&id], |row| row.get::<_,String>(0)).optional().map_err(|error| error.to_string())?;
        if let Some(image) = image {
            result.push(image);
        }
    }
    Ok(result)
}

pub fn image_path(
    connection: &Connection,
    image_id: &str,
    thumbnail: bool,
) -> Result<String, String> {
    let column = if thumbnail {
        "thumbnail_rel_path"
    } else {
        "original_rel_path"
    };
    connection
        .query_row(
            &format!("SELECT {column} FROM images WHERE id=?1"),
            [image_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("图片不存在".into())
}

pub fn share_info(connection: &Connection, id: &str) -> Result<(String, String), String> {
    connection
        .query_row(
            "SELECT share_url,extraction_code FROM assets WHERE id=?1 AND deleted_at IS NULL",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("素材不存在".into())
}

pub fn upsert_category(
    connection: &Connection,
    id: Option<String>,
    name: String,
    parent_id: Option<String>,
    preserve_parent: bool,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分类名称不能为空".into());
    }
    if let Some(parent_id) = parent_id.as_deref() {
        ensure_category_exists(connection, parent_id)?;
    }
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if preserve_parent {
        connection
            .execute(
                "UPDATE categories SET name=?1 WHERE id=?2 AND deleted_at IS NULL",
                params![name, id],
            )
            .map_err(map_constraint)?;
    } else {
        connection.execute("INSERT INTO categories(id,name,parent_id,sort_order,created_at) VALUES(?1,?2,?3,COALESCE((SELECT MAX(sort_order)+1 FROM categories WHERE parent_id IS ?3),0),?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,parent_id=excluded.parent_id", params![id,name,parent_id,now()]).map_err(map_constraint)?;
    }
    let asset_ids = query_strings(connection, "SELECT id FROM assets")?;
    for asset_id in asset_ids {
        reindex_asset(connection, &asset_id)?;
    }
    Ok(id)
}

pub fn duplicate_match(
    connection: &Connection,
    raw_url: &str,
) -> Result<Option<DuplicateMatch>, String> {
    if raw_url.trim().is_empty() {
        return Ok(None);
    }
    let normalized = normalize_share_url(raw_url).ok_or("分享链接格式无效")?;
    connection
        .query_row(
            "SELECT id,name,share_url FROM assets WHERE normalized_share_url=?1 AND deleted_at IS NULL LIMIT 1",
            [normalized],
            |row| {
                Ok(DuplicateMatch {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    share_url: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
}

pub fn batch_update_assets(
    connection: &mut Connection,
    update: BatchAssetUpdate,
) -> Result<BatchUpdateReport, String> {
    let ids = unique_values(&update.ids);
    if ids.is_empty() {
        return Err("请先选择素材".into());
    }
    let add_tags = unique_values(&update.add_tags);
    let remove_tags = unique_values(&update.remove_tags)
        .into_iter()
        .map(|tag| tag.to_lowercase())
        .collect::<HashSet<_>>();
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let locale = valid_language(&update.content_language).to_string();
    let timestamp = now();
    let mut updated = 0;
    for id in &ids {
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            continue;
        }
        if update.clear_category {
            transaction
                .execute("UPDATE assets SET category_id=NULL WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        } else if let Some(category_id) = &update.category_id {
            transaction
                .execute(
                    "UPDATE assets SET category_id=?1 WHERE id=?2",
                    params![category_id, id],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(favorite) = update.favorite {
            transaction
                .execute(
                    "UPDATE assets SET favorite=?1 WHERE id=?2",
                    params![favorite as i64, id],
                )
                .map_err(|e| e.to_string())?;
        }
        for tag in &add_tags {
            insert_localized_tag(&transaction, id, &locale, tag)?;
            let normalized = tag.to_lowercase();
            let tag_id = transaction
                .query_row(
                    "SELECT id FROM tags WHERE normalized_name=?1",
                    [&normalized],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            transaction
                .execute(
                    "INSERT OR IGNORE INTO tags(id,name,normalized_name) VALUES(?1,?2,?3)",
                    params![tag_id, tag, normalized],
                )
                .map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO asset_tags(asset_id,tag_id) VALUES(?1,?2)",
                    params![id, tag_id],
                )
                .map_err(|e| e.to_string())?;
        }
        for normalized in &remove_tags {
            transaction.execute("DELETE FROM asset_localized_tags WHERE asset_id=?1 AND tag_id IN (SELECT id FROM localized_tags WHERE locale=?2 AND normalized_name=?3)", params![id,locale,normalized]).map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN (SELECT id FROM tags WHERE normalized_name=?2)",
                    params![id, normalized],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction
            .execute(
                "UPDATE assets SET updated_at=?1 WHERE id=?2",
                params![timestamp, id],
            )
            .map_err(|e| e.to_string())?;
        reindex_asset(&transaction, id)?;
        updated += 1;
    }
    transaction
        .execute(
            "DELETE FROM tags WHERE NOT EXISTS(SELECT 1 FROM asset_tags WHERE asset_tags.tag_id=tags.id)",
            [],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute("DELETE FROM localized_tags WHERE NOT EXISTS(SELECT 1 FROM asset_localized_tags WHERE asset_localized_tags.tag_id=localized_tags.id)", []).map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(BatchUpdateReport {
        requested: ids.len(),
        updated,
    })
}

#[derive(Debug, Clone)]
pub struct AssetMoveUndo {
    pub previous: Vec<(String, Option<String>)>,
    pub expected_category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryPosition {
    pub id: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone)]
pub struct CategoryMoveUndo {
    pub before: Vec<CategoryPosition>,
    pub after: Vec<CategoryPosition>,
}

pub struct AssetMoveOutcome {
    pub moved: usize,
    pub undo: Option<AssetMoveUndo>,
}

pub struct CategoryMoveOutcome {
    pub changed: bool,
    pub name: String,
    pub undo: Option<CategoryMoveUndo>,
}

fn ensure_category_exists(connection: &Connection, id: &str) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1 AND deleted_at IS NULL)",
            [id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        Ok(())
    } else {
        Err("目标分类不存在".into())
    }
}

pub fn move_assets_to_category(
    connection: &mut Connection,
    ids: Vec<String>,
    category_id: Option<String>,
) -> Result<AssetMoveOutcome, String> {
    let ids = unique_values(&ids);
    if ids.is_empty() {
        return Err("没有可移动的素材".into());
    }
    if let Some(category_id) = category_id.as_deref() {
        ensure_category_exists(connection, category_id)?;
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let mut previous = Vec::new();
    for id in &ids {
        let old_category = transaction
            .query_row(
                "SELECT category_id FROM assets WHERE id=?1 AND deleted_at IS NULL",
                [id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("素材不存在：{id}"))?;
        if old_category != category_id {
            previous.push((id.clone(), old_category));
        }
    }
    if previous.is_empty() {
        transaction.commit().map_err(|e| e.to_string())?;
        return Ok(AssetMoveOutcome {
            moved: 0,
            undo: None,
        });
    }
    let timestamp = now();
    for (id, _) in &previous {
        transaction
            .execute(
                "UPDATE assets SET category_id=?1,updated_at=?2 WHERE id=?3",
                params![category_id, timestamp, id],
            )
            .map_err(|e| e.to_string())?;
        reindex_asset(&transaction, id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(AssetMoveOutcome {
        moved: previous.len(),
        undo: Some(AssetMoveUndo {
            previous,
            expected_category: category_id,
        }),
    })
}

pub fn undo_asset_move(connection: &mut Connection, undo: &AssetMoveUndo) -> Result<usize, String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for (id, _) in &undo.previous {
        let current = transaction
            .query_row("SELECT category_id FROM assets WHERE id=?1", [id], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("素材已不存在，无法撤销")?;
        if current != undo.expected_category {
            return Err("素材分类已经再次变化，无法撤销上一次移动".into());
        }
    }
    let timestamp = now();
    for (id, category_id) in &undo.previous {
        transaction
            .execute(
                "UPDATE assets SET category_id=?1,updated_at=?2 WHERE id=?3",
                params![category_id, timestamp, id],
            )
            .map_err(|e| e.to_string())?;
        reindex_asset(&transaction, id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(undo.previous.len())
}

fn category_positions(connection: &Connection) -> Result<Vec<CategoryPosition>, String> {
    let mut statement = connection
        .prepare("SELECT id,parent_id,sort_order FROM categories ORDER BY id")
        .map_err(|e| e.to_string())?;
    let positions = statement
        .query_map([], |row| {
            Ok(CategoryPosition {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                sort_order: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(positions)
}

fn sibling_ids(
    connection: &Connection,
    parent_id: Option<&str>,
    exclude_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT id FROM categories WHERE parent_id IS ?1 AND id<>?2 ORDER BY sort_order,name COLLATE NOCASE,id")
        .map_err(|e| e.to_string())?;
    let ids = statement
        .query_map(params![parent_id, exclude_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

fn write_sibling_order(
    connection: &Connection,
    parent_id: Option<&str>,
    ids: &[String],
) -> Result<(), String> {
    for (index, id) in ids.iter().enumerate() {
        connection
            .execute(
                "UPDATE categories SET parent_id=?1,sort_order=?2 WHERE id=?3",
                params![parent_id, index as i64, id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn is_category_descendant(
    connection: &Connection,
    ancestor_id: &str,
    possible_descendant_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "WITH RECURSIVE descendants(id) AS (SELECT id FROM categories WHERE parent_id=?1 UNION ALL SELECT c.id FROM categories c JOIN descendants d ON c.parent_id=d.id) SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2)",
            params![ancestor_id, possible_descendant_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
}

fn asset_ids_in_category_subtrees(
    connection: &Connection,
    roots: &[String],
) -> Result<Vec<String>, String> {
    let categories = category_descendants(connection, roots)?;
    if categories.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT id FROM assets WHERE category_id IN ({})",
        placeholders(categories.len())
    );
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let ids = statement
        .query_map(params_from_iter(categories.iter()), |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

pub fn move_category(
    connection: &mut Connection,
    request: MoveCategoryRequest,
) -> Result<CategoryMoveOutcome, String> {
    let (name, old_parent) = connection
        .query_row(
            "SELECT name,parent_id FROM categories WHERE id=?1",
            [&request.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("要移动的分类不存在")?;
    if let Some(parent_id) = request.target_parent_id.as_deref() {
        ensure_category_exists(connection, parent_id)?;
        if parent_id == request.id || is_category_descendant(connection, &request.id, parent_id)? {
            return Err("不能把分类移动到自身或其子分类中".into());
        }
    }
    let before = category_positions(connection)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    if old_parent != request.target_parent_id {
        let old_siblings = sibling_ids(&transaction, old_parent.as_deref(), &request.id)?;
        write_sibling_order(&transaction, old_parent.as_deref(), &old_siblings)?;
    }
    let mut target_siblings = sibling_ids(
        &transaction,
        request.target_parent_id.as_deref(),
        &request.id,
    )?;
    let target_index = request.target_index.min(target_siblings.len());
    target_siblings.insert(target_index, request.id.clone());
    write_sibling_order(
        &transaction,
        request.target_parent_id.as_deref(),
        &target_siblings,
    )?;
    if old_parent != request.target_parent_id {
        for asset_id in
            asset_ids_in_category_subtrees(&transaction, std::slice::from_ref(&request.id))?
        {
            reindex_asset(&transaction, &asset_id)?;
        }
    }
    let after = category_positions(&transaction)?;
    transaction.commit().map_err(|e| e.to_string())?;
    let changed = before != after;
    Ok(CategoryMoveOutcome {
        changed,
        name,
        undo: changed.then_some(CategoryMoveUndo { before, after }),
    })
}

pub fn undo_category_move(
    connection: &mut Connection,
    undo: &CategoryMoveUndo,
) -> Result<usize, String> {
    if category_positions(connection)? != undo.after {
        return Err("分类结构已经再次变化，无法撤销上一次移动".into());
    }
    let moved_roots: Vec<String> = undo
        .before
        .iter()
        .filter(|before| {
            undo.after
                .iter()
                .find(|after| after.id == before.id)
                .is_some_and(|after| after.parent_id != before.parent_id)
        })
        .map(|position| position.id.clone())
        .collect();
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute("UPDATE categories SET parent_id=NULL", [])
        .map_err(|e| e.to_string())?;
    for position in &undo.before {
        transaction
            .execute(
                "UPDATE categories SET parent_id=?1,sort_order=?2 WHERE id=?3",
                params![position.parent_id, position.sort_order, position.id],
            )
            .map_err(|e| e.to_string())?;
    }
    for asset_id in asset_ids_in_category_subtrees(&transaction, &moved_roots)? {
        reindex_asset(&transaction, &asset_id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(1)
}

fn health_issue_ids(
    connection: &Connection,
    base_dir: &Path,
    issue: &str,
) -> Result<Vec<String>, String> {
    if issue.starts_with("mediaLibrary") {
        return Ok(Vec::new());
    }
    let sql = match issue {
        "noPreview" => Some("SELECT id FROM assets a WHERE a.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM images i WHERE i.asset_id=a.id) AND NOT EXISTS(SELECT 1 FROM asset_media m WHERE m.asset_id=a.id)"),
        "noCover" => Some("SELECT id FROM assets a WHERE a.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM images i WHERE i.asset_id=a.id AND i.is_cover=1) AND NOT EXISTS(SELECT 1 FROM asset_media m WHERE m.asset_id=a.id AND m.is_cover=1)"),
        "mediaProcessingError" => Some("SELECT DISTINCT a.id FROM assets a JOIN asset_media m ON m.asset_id=a.id WHERE a.deleted_at IS NULL AND m.processing_status='error'"),
        "uncategorized" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND category_id IS NULL"),
        "noTags" => Some("SELECT id FROM assets a WHERE a.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM asset_localized_tags at WHERE at.asset_id=a.id)"),
        "missingVersion" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND versions_json='[]'"),
        "missingFormat" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND formats_json='[]'"),
        "nonBaidu" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND normalized_share_url<>'' AND normalized_share_url NOT LIKE 'https://pan.baidu.com/%' AND normalized_share_url NOT LIKE 'https://%.pan.baidu.com/%'"),
        "duplicateLink" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND normalized_share_url<>'' AND normalized_share_url IN (SELECT normalized_share_url FROM assets WHERE deleted_at IS NULL GROUP BY normalized_share_url HAVING COUNT(*)>1)"),
        "duplicateFab" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND fab_listing_id<>'' AND fab_listing_id IN (SELECT fab_listing_id FROM assets WHERE deleted_at IS NULL AND fab_listing_id<>'' GROUP BY fab_listing_id HAVING COUNT(*)>1)"),
        "missingZh" => Some("SELECT id FROM assets a WHERE a.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM asset_localizations l WHERE l.asset_id=a.id AND l.locale='zh-CN' AND trim(l.name)<>'')"),
        "missingEn" => Some("SELECT id FROM assets a WHERE a.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM asset_localizations l WHERE l.asset_id=a.id AND l.locale='en' AND trim(l.name)<>'')"),
        "invalidLink" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND normalized_share_url<>'' AND link_check_status='invalid'"),
        "linkCheckError" => Some("SELECT id FROM assets WHERE deleted_at IS NULL AND normalized_share_url<>'' AND link_check_status='error'"),
        "missingFiles" | "missingMediaFiles" => None,
        _ => return Err("未知的素材库检查类型".into()),
    };
    if let Some(sql) = sql {
        return query_strings(connection, sql);
    }
    if issue == "missingMediaFiles" {
        let mut statement = connection.prepare("SELECT m.asset_id,m.original_rel_path,m.proxy_rel_path,m.thumbnail_rel_path FROM asset_media m JOIN assets a ON a.id=m.asset_id WHERE a.deleted_at IS NULL").map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let (asset_id, original, proxy, thumb) = row.map_err(|e| e.to_string())?;
            if !base_dir.join(original).is_file()
                || proxy.is_some_and(|path| !base_dir.join(path).is_file())
                || thumb.is_some_and(|path| !base_dir.join(path).is_file())
            {
                ids.insert(asset_id);
            }
        }
        return Ok(ids.into_iter().collect());
    }
    let mut statement = connection
        .prepare("SELECT i.asset_id,i.original_rel_path,i.thumbnail_rel_path FROM images i JOIN assets a ON a.id=i.asset_id WHERE a.deleted_at IS NULL")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    for row in rows {
        let (asset_id, original, thumb) = row.map_err(|e| e.to_string())?;
        if !base_dir.join(original).is_file() || !base_dir.join(thumb).is_file() {
            ids.insert(asset_id);
        }
    }
    Ok(ids.into_iter().collect())
}

pub fn health_summary(connection: &Connection, base_dir: &Path) -> Result<HealthSummary, String> {
    let specs = [
        ("noPreview", "无预览图"),
        ("noCover", "无封面"),
        ("missingFiles", "图片文件缺失"),
        ("missingMediaFiles", "预览原件或代理缺失"),
        ("mediaProcessingError", "媒体处理失败"),
        ("uncategorized", "未分类"),
        ("noTags", "无标签"),
        ("missingVersion", "缺少版本"),
        ("missingFormat", "缺少格式"),
        ("nonBaidu", "非百度网盘链接"),
        ("duplicateLink", "重复分享链接"),
        ("duplicateFab", "重复 Fab 素材"),
        ("missingZh", "缺少中文版本"),
        ("missingEn", "缺少英文版本"),
        ("invalidLink", "网盘链接已失效"),
        ("linkCheckError", "网盘链接检查失败"),
    ];
    let mut counts = Vec::new();
    let mut total_issues = 0;
    for (issue, label) in specs {
        let count = health_issue_ids(connection, base_dir, issue)?.len() as i64;
        total_issues += count;
        counts.push(HealthCount {
            issue: issue.into(),
            label: label.into(),
            count,
        });
    }
    let mut missing_original = 0;
    let mut missing_dependency = 0;
    {
        let mut statement=connection.prepare("SELECT role,rel_path FROM media_files f JOIN media_entries e ON e.id=f.entry_id WHERE e.deleted_at IS NULL AND f.role IN ('main','dependency')").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (role, path) = row.map_err(|e| e.to_string())?;
            if !base_dir.join(path).is_file() {
                if role == "main" {
                    missing_original += 1
                } else {
                    missing_dependency += 1
                }
            }
        }
    }
    let media_specs=[
      ("mediaLibraryMissingOriginal","媒体库原件缺失",missing_original),
      ("mediaLibraryMissingDependency","模型依赖缺失",missing_dependency),
      ("mediaLibraryProxyError","媒体代理或解析失败",connection.query_row("SELECT count(*) FROM media_entries WHERE deleted_at IS NULL AND processing_status='error'",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?),
      ("mediaLibraryNoThumbnail","媒体库无缩略图",connection.query_row("SELECT count(*) FROM media_entries e WHERE e.deleted_at IS NULL AND e.kind IN ('image','audio','video') AND NOT EXISTS(SELECT 1 FROM media_files f WHERE f.entry_id=e.id AND f.role IN ('thumbnail','waveform'))",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?),
      ("mediaLibraryOrphanLink","媒体库孤立关联",connection.query_row("SELECT (SELECT count(*) FROM media_entry_assets x JOIN assets a ON a.id=x.asset_id WHERE a.deleted_at IS NOT NULL)+(SELECT count(*) FROM project_media_entries x LEFT JOIN projects p ON p.id=x.project_id WHERE p.id IS NULL)",[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?),
    ];
    for (issue, label, count) in media_specs {
        total_issues += count;
        counts.push(HealthCount {
            issue: issue.into(),
            label: label.into(),
            count,
        });
    }
    Ok(HealthSummary {
        total_issues,
        counts,
    })
}

pub fn list_health_issues(
    connection: &Connection,
    base_dir: &Path,
    request: HealthIssueRequest,
) -> Result<Page<AssetCard>, String> {
    search_assets(
        connection,
        base_dir,
        &SearchRequest {
            query: String::new(),
            category_ids: Vec::new(),
            tags: Vec::new(),
            tag_ids: Vec::new(),
            dcc_tools: Vec::new(),
            versions: Vec::new(),
            formats: Vec::new(),
            licenses: Vec::new(),
            favorite_only: false,
            recent_only: false,
            health_issue: Some(request.issue),
            smart_collection_id: None,
            project_id: None,
            project_asset_status: None,
            sort: "updated".into(),
            offset: request.offset,
            limit: request.limit,
            content_language: default_content_language(),
        },
    )
}

pub fn all_link_check_targets(connection: &Connection) -> Result<Vec<LinkCheckTarget>, String> {
    let mut statement = connection
        .prepare("SELECT id,share_url,normalized_share_url FROM assets WHERE deleted_at IS NULL AND normalized_share_url<>'' ORDER BY id")
        .map_err(|e| e.to_string())?;
    let targets = statement
        .query_map([], |row| {
            Ok(LinkCheckTarget {
                id: row.get(0)?,
                share_url: row.get(1)?,
                normalized_share_url: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(targets)
}

pub fn link_check_target(connection: &Connection, id: &str) -> Result<LinkCheckTarget, String> {
    connection
        .query_row(
            "SELECT id,share_url,normalized_share_url FROM assets WHERE id=?1 AND deleted_at IS NULL AND normalized_share_url<>''",
            [id],
            |row| {
                Ok(LinkCheckTarget {
                    id: row.get(0)?,
                    share_url: row.get(1)?,
                    normalized_share_url: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "素材不存在".into())
}

pub fn save_link_check_result(
    connection: &Connection,
    ids: &[String],
    status: &str,
    checked_at: &str,
    message: &str,
) -> Result<(), String> {
    if !matches!(status, "valid" | "invalid" | "error") {
        return Err("无效的链接检查状态".into());
    }
    for id in ids {
        connection
            .execute(
                "UPDATE assets SET link_check_status=?1,link_checked_at=?2,link_check_message=?3 WHERE id=?4",
                params![status, checked_at, message, id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("images/originals")).unwrap();
        fs::create_dir_all(dir.path().join("images/thumbnails")).unwrap();
        let connection = open_database(&dir.path().join("test.db")).unwrap();
        (dir, connection)
    }

    #[test]
    fn migrates_v11_project_media_columns_without_losing_existing_rows() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            INSERT INTO schema_meta VALUES('schema_version','11');
            CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL);
            CREATE TABLE project_units(id TEXT PRIMARY KEY,project_id TEXT NOT NULL);
            INSERT INTO projects VALUES('project-1','旧项目');
            INSERT INTO project_units VALUES('shot-1','project-1');",
            )
            .unwrap();
        migrate_to_v12(&connection).unwrap();
        migrate_to_v12(&connection).unwrap();
        let version: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cover: Option<String> = connection
            .query_row(
                "SELECT cover_media_entry_id FROM projects WHERE id='project-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let video: Option<String> = connection
            .query_row(
                "SELECT video_media_entry_id FROM project_units WHERE id='shot-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "12");
        assert_eq!(cover, None);
        assert_eq!(video, None);
    }

    #[test]
    fn category_lifecycle_and_meta() {
        let (_dir, mut connection) = setup();
        let id = upsert_category(&connection, None, "环境".into(), None, false).unwrap();
        let meta = library_meta(&connection, "zh-CN").unwrap();
        assert_eq!(meta.categories[0].name, "环境");
        delete_library_items(
            &mut connection,
            DeleteRequest {
                asset_ids: Vec::new(),
                category_id: Some(id),
            },
        )
        .unwrap();
        assert!(library_meta(&connection, "zh-CN")
            .unwrap()
            .categories
            .is_empty());
    }

    #[test]
    fn moves_multiple_assets_transactionally_and_undoes() {
        let (_dir, mut connection) = setup();
        let first_category =
            upsert_category(&connection, Some("c1".into()), "环境".into(), None, false).unwrap();
        let second_category =
            upsert_category(&connection, Some("c2".into()), "角色".into(), None, false).unwrap();
        connection.execute_batch("INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES('a1','素材1','https://pan.baidu.com/s/a1','now','now'); INSERT INTO assets(id,name,category_id,share_url,created_at,updated_at) VALUES('a2','素材2','c1','https://pan.baidu.com/s/a2','now','now');").unwrap();
        let outcome = move_assets_to_category(
            &mut connection,
            vec!["a1".into(), "a2".into()],
            Some(second_category.clone()),
        )
        .unwrap();
        assert_eq!(outcome.moved, 2);
        assert_eq!(
            query_strings(&connection, "SELECT category_id FROM assets ORDER BY id").unwrap(),
            vec![second_category.clone(), second_category]
        );
        assert_eq!(
            undo_asset_move(&mut connection, &outcome.undo.unwrap()).unwrap(),
            2
        );
        let restored: Vec<(String, Option<String>)> = {
            let mut statement = connection
                .prepare("SELECT id,category_id FROM assets ORDER BY id")
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            restored,
            vec![("a1".into(), None), ("a2".into(), Some(first_category))]
        );

        assert!(move_assets_to_category(
            &mut connection,
            vec!["a1".into(), "missing".into()],
            Some("c2".into())
        )
        .is_err());
        assert!(move_assets_to_category(
            &mut connection,
            vec!["a1".into()],
            Some("missing-category".into())
        )
        .is_err());
        let unchanged: Option<String> = connection
            .query_row("SELECT category_id FROM assets WHERE id='a1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(unchanged.is_none());
    }

    #[test]
    fn moves_reorders_and_undoes_category_subtrees() {
        let (_dir, mut connection) = setup();
        upsert_category(&connection, Some("a".into()), "A".into(), None, false).unwrap();
        upsert_category(&connection, Some("b".into()), "B".into(), None, false).unwrap();
        upsert_category(
            &connection,
            Some("child".into()),
            "Child".into(),
            Some("a".into()),
            false,
        )
        .unwrap();
        upsert_category(
            &connection,
            Some("grand".into()),
            "Grand".into(),
            Some("child".into()),
            false,
        )
        .unwrap();

        let nested = move_category(
            &mut connection,
            MoveCategoryRequest {
                id: "b".into(),
                target_parent_id: Some("a".into()),
                target_index: 0,
            },
        )
        .unwrap();
        assert!(nested.changed);
        let children = query_strings(
            &connection,
            "SELECT id FROM categories WHERE parent_id='a' ORDER BY sort_order",
        )
        .unwrap();
        assert_eq!(children, vec!["b", "child"]);
        assert!(move_category(
            &mut connection,
            MoveCategoryRequest {
                id: "a".into(),
                target_parent_id: Some("grand".into()),
                target_index: 0
            }
        )
        .is_err());
        assert!(move_category(
            &mut connection,
            MoveCategoryRequest {
                id: "b".into(),
                target_parent_id: Some("missing".into()),
                target_index: 0
            }
        )
        .is_err());

        let moved_to_root = move_category(
            &mut connection,
            MoveCategoryRequest {
                id: "child".into(),
                target_parent_id: None,
                target_index: 0,
            },
        )
        .unwrap();
        assert!(moved_to_root.changed);
        let parent: Option<String> = connection
            .query_row(
                "SELECT parent_id FROM categories WHERE id='grand'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent.as_deref(), Some("child"));
        undo_category_move(&mut connection, &moved_to_root.undo.unwrap()).unwrap();
        let restored_parent: Option<String> = connection
            .query_row(
                "SELECT parent_id FROM categories WHERE id='child'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(restored_parent.as_deref(), Some("a"));
    }

    #[test]
    fn rejects_unsafe_links() {
        assert!(validate_http_url("file:///secret", true).is_err());
        assert!(validate_http_url("https://pan.baidu.com/s/demo", true).is_ok());
    }

    #[test]
    fn normalizes_share_links_and_migrates_v1() {
        assert_eq!(
            normalize_share_url("http://PAN.BAIDU.com/s/demo/?pwd=A1B2#part").unwrap(),
            "https://pan.baidu.com/s/demo"
        );
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let legacy = Connection::open(&path).unwrap();
        legacy.execute_batch("CREATE TABLE assets(id TEXT PRIMARY KEY,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',category_id TEXT,dcc_tools_json TEXT NOT NULL DEFAULT '[]',versions_json TEXT NOT NULL DEFAULT '[]',formats_json TEXT NOT NULL DEFAULT '[]',size_bytes INTEGER,author TEXT NOT NULL DEFAULT '',source_url TEXT NOT NULL DEFAULT '',license TEXT NOT NULL DEFAULT '',share_url TEXT NOT NULL,extraction_code TEXT NOT NULL DEFAULT '',favorite INTEGER NOT NULL DEFAULT 0,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,last_viewed_at TEXT); INSERT INTO assets(id,name,source_url,share_url,created_at,updated_at) VALUES('1','旧素材','https://www.fab.com/zh-cn/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a?lang=zh-CN','http://pan.baidu.com/s/old?pwd=1234','now','now');").unwrap();
        drop(legacy);
        let migrated = open_database(&path).unwrap();
        let value: String = migrated
            .query_row(
                "SELECT normalized_share_url FROM assets WHERE id='1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "https://pan.baidu.com/s/old");
        let version: String = migrated
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let localized: String = migrated
            .query_row(
                "SELECT name FROM asset_localizations WHERE asset_id='1' AND locale='zh-CN'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let link_status: String = migrated
            .query_row(
                "SELECT link_check_status FROM assets WHERE id='1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let fab_listing_id: String = migrated
            .query_row(
                "SELECT fab_listing_id FROM assets WHERE id='1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "12");
        assert_eq!(fab_listing_id, "06003f78-9a59-4fb8-abbc-14dc276f0b4a");
        assert_eq!(link_status, "unknown");
        assert_eq!(localized, "旧素材");
    }

    #[test]
    fn stores_searches_and_switches_bilingual_content() {
        let (dir, mut connection) = setup();
        let localizations = HashMap::from([
            (
                "zh-CN".into(),
                LocalizedAssetText {
                    name: "森林遗迹".into(),
                    description: "适用于虚幻引擎".into(),
                    tags: vec!["环境".into()],
                    license: "商用授权".into(),
                },
            ),
            (
                "en".into(),
                LocalizedAssetText {
                    name: "Forest Ruins".into(),
                    description: "Made for Unreal Engine".into(),
                    tags: vec!["Environment".into()],
                    license: "Commercial".into(),
                },
            ),
        ]);
        let saved = upsert_asset(
            &mut connection,
            dir.path(),
            AssetInput {
                id: None,
                name: String::new(),
                description: String::new(),
                category_id: None,
                tags: Vec::new(),
                dcc_tools: vec!["Unreal Engine".into()],
                versions: vec!["5.5".into()],
                formats: vec!["uasset".into()],
                size_bytes: None,
                author: String::new(),
                source_url: String::new(),
                fab_listing_id: None,
                auto_category_path: Vec::new(),
                license: String::new(),
                share_url: "https://pan.baidu.com/s/bilingual".into(),
                extraction_code: String::new(),
                favorite: false,
                images: Vec::new(),
                localizations,
                content_language: "zh-CN".into(),
            },
        )
        .unwrap();
        assert_eq!(saved.card.name, "森林遗迹");
        let english = get_asset(&connection, &saved.card.id, false, "en").unwrap();
        assert_eq!(english.card.name, "Forest Ruins");
        assert_eq!(english.card.tags, vec!["Environment"]);
        let page = search_assets(
            &connection,
            dir.path(),
            &SearchRequest {
                query: "森林".into(),
                category_ids: Vec::new(),
                tags: Vec::new(),
                tag_ids: Vec::new(),
                dcc_tools: Vec::new(),
                versions: Vec::new(),
                formats: Vec::new(),
                licenses: Vec::new(),
                favorite_only: false,
                recent_only: false,
                health_issue: None,
                smart_collection_id: None,
                project_id: None,
                project_asset_status: None,
                sort: "relevance".into(),
                offset: 0,
                limit: 10,
                content_language: "en".into(),
            },
        )
        .unwrap();
        assert_eq!(page.items[0].name, "Forest Ruins");
    }

    #[test]
    fn keeps_link_result_for_same_url_and_resets_it_when_url_changes() {
        let (dir, mut connection) = setup();
        let mut input = AssetInput {
            id: None,
            name: "测试素材".into(),
            description: String::new(),
            category_id: None,
            tags: Vec::new(),
            dcc_tools: Vec::new(),
            versions: Vec::new(),
            formats: Vec::new(),
            size_bytes: None,
            author: String::new(),
            source_url: String::new(),
            fab_listing_id: None,
            auto_category_path: Vec::new(),
            license: String::new(),
            share_url: "https://pan.baidu.com/s/first?pwd=1234".into(),
            extraction_code: "1234".into(),
            favorite: false,
            images: Vec::new(),
            localizations: HashMap::new(),
            content_language: "zh-CN".into(),
        };
        let saved = upsert_asset(&mut connection, dir.path(), input.clone()).unwrap();
        save_link_check_result(
            &connection,
            std::slice::from_ref(&saved.card.id),
            "invalid",
            "2026-09-18T00:00:00Z",
            "分享已取消",
        )
        .unwrap();
        input.id = Some(saved.card.id.clone());
        let unchanged = upsert_asset(&mut connection, dir.path(), input.clone()).unwrap();
        assert_eq!(unchanged.card.link_check_status, "invalid");
        assert_eq!(unchanged.card.link_check_message, "分享已取消");

        input.share_url = "https://pan.baidu.com/s/second".into();
        let changed = upsert_asset(&mut connection, dir.path(), input).unwrap();
        assert_eq!(changed.card.link_check_status, "unknown");
        assert!(changed.card.link_checked_at.is_none());
        assert!(changed.card.link_check_message.is_empty());
    }

    #[test]
    fn batch_update_and_health_detection() {
        let (dir, mut connection) = setup();
        let make_input = |name: &str, url: &str| AssetInput {
            id: None,
            name: name.into(),
            description: String::new(),
            category_id: None,
            tags: Vec::new(),
            dcc_tools: vec!["Unreal Engine".into()],
            versions: Vec::new(),
            formats: Vec::new(),
            size_bytes: None,
            author: String::new(),
            source_url: String::new(),
            fab_listing_id: None,
            auto_category_path: Vec::new(),
            license: String::new(),
            share_url: url.into(),
            extraction_code: String::new(),
            favorite: false,
            images: Vec::new(),
            localizations: HashMap::new(),
            content_language: "zh-CN".into(),
        };
        let first = upsert_asset(
            &mut connection,
            dir.path(),
            make_input("A", "https://pan.baidu.com/s/demo?pwd=1111"),
        )
        .unwrap();
        let second = upsert_asset(
            &mut connection,
            dir.path(),
            make_input("B", "https://pan.baidu.com/s/other"),
        )
        .unwrap();
        connection.execute("UPDATE assets SET share_url='http://pan.baidu.com/s/demo?pwd=2222',normalized_share_url='https://pan.baidu.com/s/demo' WHERE id=?1", [&second.card.id]).unwrap();
        let report = batch_update_assets(
            &mut connection,
            BatchAssetUpdate {
                ids: vec![first.card.id.clone()],
                category_id: None,
                clear_category: false,
                add_tags: vec!["Nanite".into()],
                remove_tags: Vec::new(),
                favorite: Some(true),
                content_language: "zh-CN".into(),
            },
        )
        .unwrap();
        assert_eq!(report.updated, 1);
        let updated = get_asset(&connection, &first.card.id, false, "zh-CN").unwrap();
        assert!(updated.card.favorite);
        assert_eq!(updated.card.tags, vec!["Nanite"]);
        let health = health_summary(&connection, dir.path()).unwrap();
        assert_eq!(
            health
                .counts
                .iter()
                .find(|item| item.issue == "duplicateLink")
                .unwrap()
                .count,
            2
        );
    }

    #[test]
    fn allows_empty_links_and_restores_soft_deleted_assets() {
        let (dir, mut connection) = setup();
        let input = |name: &str| AssetInput {
            id: None,
            name: name.into(),
            description: String::new(),
            category_id: None,
            tags: Vec::new(),
            dcc_tools: Vec::new(),
            versions: Vec::new(),
            formats: Vec::new(),
            size_bytes: None,
            author: String::new(),
            source_url: String::new(),
            fab_listing_id: None,
            auto_category_path: Vec::new(),
            license: String::new(),
            share_url: String::new(),
            extraction_code: String::new(),
            favorite: false,
            images: Vec::new(),
            localizations: HashMap::new(),
            content_language: "zh-CN".into(),
        };
        let first = upsert_asset(&mut connection, dir.path(), input("无链接 A")).unwrap();
        upsert_asset(&mut connection, dir.path(), input("无链接 B")).unwrap();
        let deletion = delete_library_items(
            &mut connection,
            DeleteRequest {
                asset_ids: vec![first.card.id.clone()],
                category_id: None,
            },
        )
        .unwrap();
        assert!(get_asset(&connection, &first.card.id, false, "zh-CN").is_err());
        assert_eq!(list_trash(&connection, 0, 20).unwrap().total, 1);
        restore_trash_batch(&mut connection, &deletion.batch_id).unwrap();
        assert_eq!(
            get_asset(&connection, &first.card.id, false, "zh-CN")
                .unwrap()
                .card
                .name,
            "无链接 A"
        );
    }

    #[test]
    fn auto_creates_categories_and_rejects_fab_url_variants() {
        let (dir, mut connection) = setup();
        let make_input = |name: &str, source_url: &str| AssetInput {
            id: None,
            name: name.into(),
            description: String::new(),
            category_id: None,
            tags: Vec::new(),
            dcc_tools: vec!["Unreal Engine".into()],
            versions: vec!["5.5".into()],
            formats: vec!["uasset".into()],
            size_bytes: None,
            author: String::new(),
            source_url: source_url.into(),
            fab_listing_id: None,
            auto_category_path: vec!["环境".into(), "沙漠".into()],
            license: String::new(),
            share_url: String::new(),
            extraction_code: String::new(),
            favorite: false,
            images: Vec::new(),
            localizations: HashMap::new(),
            content_language: "zh-CN".into(),
        };
        let id = "06003f78-9a59-4fb8-abbc-14dc276f0b4a";
        let saved = upsert_asset(
            &mut connection,
            dir.path(),
            make_input(
                "沙漠",
                &format!("https://www.fab.com/listings/{id}?lang=zh-CN"),
            ),
        )
        .unwrap();
        assert_eq!(saved.card.category_name.as_deref(), Some("沙漠"));
        assert_eq!(
            category_path(&connection, saved.category_id.as_deref()).unwrap(),
            "环境 / 沙漠"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM categories", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let duplicate = upsert_asset(
            &mut connection,
            dir.path(),
            make_input(
                "重复",
                &format!("http://fab.com/zh-cn/listings/{id}/#details"),
            ),
        )
        .unwrap_err();
        assert!(duplicate.contains("已存在"));
        assert_eq!(
            fab_duplicate_match(&connection, id, None)
                .unwrap()
                .unwrap()
                .asset_id,
            saved.card.id
        );
        let manual_id = upsert_category(
            &connection,
            Some("manual".into()),
            "我的分类".into(),
            None,
            false,
        )
        .unwrap();
        let mut manual = make_input(
            "手动分类",
            "https://www.fab.com/listings/916aa5ba-df73-47d7-be11-dc8a01e12a43",
        );
        manual.category_id = Some(manual_id.clone());
        let manually_saved = upsert_asset(&mut connection, dir.path(), manual).unwrap();
        assert_eq!(
            manually_saved.category_id.as_deref(),
            Some(manual_id.as_str())
        );
        assert_eq!(
            manually_saved.card.category_name.as_deref(),
            Some("我的分类")
        );
    }
}
