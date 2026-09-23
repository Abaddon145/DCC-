use crate::{db, models::*};
use chrono::Utc;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use std::collections::HashSet;
use uuid::Uuid;

pub fn normalize_global(mut value: GlobalPreferences) -> Result<GlobalPreferences, String> {
    if !matches!(value.theme.as_str(), "graphite" | "ue-slate" | "midnight") {
        value.theme = "graphite".into();
    }
    if !is_hex_color(&value.accent_color) {
        return Err("强调色必须是 #RRGGBB 格式".into());
    }
    if !matches!(value.density.as_str(), "comfortable" | "compact") {
        value.density = "comfortable".into();
    }
    value.sidebar_width = value.sidebar_width.clamp(210, 360);
    value.detail_width = value.detail_width.clamp(340, 640);
    let defaults = default_shortcuts();
    for (command, shortcut) in defaults {
        value.shortcuts.entry(command).or_insert(shortcut);
    }
    let mut used = HashSet::new();
    for shortcut in value.shortcuts.values() {
        let normalized = shortcut.trim().to_ascii_lowercase();
        if normalized.is_empty() || !used.insert(normalized) {
            return Err("快捷键不能为空或重复".into());
        }
    }
    Ok(value)
}

pub fn normalize_library(mut value: LibraryPreferences) -> LibraryPreferences {
    const GROUPS: [&[&str]; 4] = [
        &[
            "library",
            "imageLibrary",
            "modelLibrary",
            "audioLibrary",
            "videoLibrary",
        ],
        &["projects", "reference"],
        &["smartCollections", "favorites", "recent"],
        &["tagManager", "health", "trash"],
    ];
    let requested = value
        .module_order
        .drain(..)
        .filter(|id| id != "collections")
        .collect::<Vec<_>>();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for group in GROUPS {
        for id in requested.iter().filter(|id| group.contains(&id.as_str())) {
            if seen.insert(id.clone()) {
                order.push(id.clone());
            }
        }
        for id in group {
            if seen.insert(id.to_string()) {
                order.push(id.to_string());
            }
        }
    }
    for id in requested {
        if id == "collections" {
            continue;
        }
        if seen.insert(id.clone()) {
            order.push(id);
        }
    }
    if let Some(position) = order.iter().position(|id| id == "library") {
        order.remove(position);
    }
    order.insert(0, "library".into());
    value.module_order = order;
    value.disabled_modules.retain(|id| id != "library");
    value.disabled_modules.sort();
    value.disabled_modules.dedup();
    value.disabled_modules.retain(|id| id != "collections");
    if value.startup_module == "collections"
        || value.disabled_modules.contains(&value.startup_module)
        || !value.module_order.contains(&value.startup_module)
    {
        value.startup_module = "library".into();
    }
    if !matches!(value.asset_view.as_str(), "grid" | "list") {
        value.asset_view = "grid".into();
    }
    if !matches!(value.card_size.as_str(), "small" | "medium" | "large") {
        value.card_size = "medium".into();
    }
    if !matches!(value.cover_fit.as_str(), "cover" | "contain") {
        value.cover_fit = "cover".into();
    }
    if !matches!(
        value.default_sort.as_str(),
        "updated" | "name" | "created" | "recent" | "favorite"
    ) {
        value.default_sort = "updated".into();
    }
    let defaults = LibraryPreferences::default().context_pane_widths;
    for (id, width) in defaults {
        value.context_pane_widths.entry(id).or_insert(width);
    }
    value.context_pane_widths.retain(|id, width| {
        if !matches!(
            id.as_str(),
            "library" | "imageLibrary" | "modelLibrary" | "audioLibrary" | "videoLibrary"
        ) {
            return false;
        }
        *width = (*width).clamp(180, 420);
        true
    });
    value.collapsed_context_panes.retain(|id| {
        matches!(
            id.as_str(),
            "library" | "imageLibrary" | "modelLibrary" | "audioLibrary" | "videoLibrary"
        )
    });
    value.collapsed_context_panes.sort();
    value.collapsed_context_panes.dedup();
    value.collapsed_module_groups.retain(|id| {
        matches!(
            id.as_str(),
            "content" | "mediaLibraries" | "creation" | "manage"
        )
    });
    value.collapsed_module_groups.sort();
    value.collapsed_module_groups.dedup();
    value
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn load_library_preferences(connection: &Connection) -> Result<LibraryPreferences, String> {
    let raw = connection
        .query_row(
            "SELECT settings_json FROM library_preferences WHERE id=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let value = raw
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    Ok(normalize_library(value))
}

pub fn save_library_preferences(
    connection: &Connection,
    value: LibraryPreferences,
) -> Result<LibraryPreferences, String> {
    let value = normalize_library(value);
    let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    connection.execute(
        "INSERT INTO library_preferences(id,settings_json,updated_at) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,updated_at=excluded.updated_at",
        params![json, Utc::now().to_rfc3339()],
    ).map_err(|e| e.to_string())?;
    Ok(value)
}

fn collection_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SmartCollection> {
    let raw: String = row.get(4)?;
    let legacy: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let mut invalid_conditions = Vec::new();
    let has_collection = legacy
        .get("manualCollectionId")
        .is_some_and(|value| !value.is_null() && value.as_str().is_some_and(|v| !v.is_empty()));
    let has_rating = legacy
        .get("ratings")
        .and_then(|value| value.as_array())
        .is_some_and(|values| !values.is_empty())
        || legacy.get("sort").and_then(|value| value.as_str()) == Some("rating");
    let query = legacy
        .get("query")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if has_collection || query.contains("collection:") {
        invalid_conditions.push("手动合集条件已在 v0.13 取消".into());
    }
    if has_rating || query.contains("rating:") {
        invalid_conditions.push("评分条件已在 v0.13 取消".into());
    }
    Ok(SmartCollection {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        color: row.get(3)?,
        rule: serde_json::from_str(&raw).unwrap_or_default(),
        sort_order: row.get(5)?,
        invalid_conditions,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn validate_rule_refs(
    connection: &Connection,
    rule: &SmartCollectionRule,
) -> Result<Vec<String>, String> {
    let mut invalid = Vec::new();
    for id in &rule.category_ids {
        if !exists(connection, "SELECT 1 FROM categories WHERE id=?1", id)? {
            invalid.push(format!("分类已删除：{}", short_id(id)));
        }
    }
    for id in &rule.tag_ids {
        if !exists(connection, "SELECT 1 FROM localized_tags WHERE id=?1", id)? {
            invalid.push(format!("标签已删除：{}", short_id(id)));
        }
    }
    Ok(invalid)
}

fn exists(connection: &Connection, sql: &str, id: &str) -> Result<bool, String> {
    connection
        .query_row(sql, [id], |_| Ok(()))
        .optional()
        .map(|v| v.is_some())
        .map_err(|e| e.to_string())
}

fn short_id(value: &str) -> &str {
    &value[..value.len().min(8)]
}

pub fn list_smart_collections(connection: &Connection) -> Result<Vec<SmartCollection>, String> {
    let mut statement = connection.prepare("SELECT id,name,icon,color,rule_json,sort_order,created_at,updated_at FROM smart_collections ORDER BY sort_order,name COLLATE NOCASE").map_err(|e| e.to_string())?;
    let mut values = statement
        .query_map([], collection_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for value in &mut values {
        value
            .invalid_conditions
            .extend(validate_rule_refs(connection, &value.rule)?);
    }
    Ok(values)
}

pub fn upsert_smart_collection(
    connection: &Connection,
    input: SmartCollectionInput,
) -> Result<SmartCollection, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("集合名称不能为空".into());
    }
    if !is_hex_color(&input.color) {
        return Err("集合颜色必须是 #RRGGBB 格式".into());
    }
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = Utc::now().to_rfc3339();
    let created = connection
        .query_row(
            "SELECT created_at FROM smart_collections WHERE id=?1",
            [&id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| timestamp.clone());
    let sort_order = connection
        .query_row(
            "SELECT sort_order FROM smart_collections WHERE id=?1",
            [&id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| {
            connection
                .query_row(
                    "SELECT COALESCE(MAX(sort_order),-1)+1 FROM smart_collections",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0)
        });
    let rule_json = serde_json::to_string(&input.rule).map_err(|e| e.to_string())?;
    connection.execute("INSERT INTO smart_collections(id,name,icon,color,rule_json,sort_order,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET name=excluded.name,icon=excluded.icon,color=excluded.color,rule_json=excluded.rule_json,updated_at=excluded.updated_at", params![id,name,input.icon,input.color,rule_json,sort_order,created,timestamp]).map_err(|e| if e.to_string().contains("UNIQUE") { "已存在同名智能集合".into() } else { e.to_string() })?;
    get_smart_collection(connection, &id)
}

pub fn get_smart_collection(connection: &Connection, id: &str) -> Result<SmartCollection, String> {
    let mut value = connection.query_row("SELECT id,name,icon,color,rule_json,sort_order,created_at,updated_at FROM smart_collections WHERE id=?1", [id], collection_from_row).optional().map_err(|e| e.to_string())?.ok_or("智能集合不存在")?;
    value
        .invalid_conditions
        .extend(validate_rule_refs(connection, &value.rule)?);
    Ok(value)
}

pub fn duplicate_smart_collection(
    connection: &Connection,
    id: &str,
) -> Result<SmartCollection, String> {
    let source = get_smart_collection(connection, id)?;
    let base = format!("{} 副本", source.name);
    let mut name = base.clone();
    let mut number = 2;
    while connection
        .query_row(
            "SELECT 1 FROM smart_collections WHERE name=?1",
            [&name],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        name = format!("{base} {number}");
        number += 1;
    }
    upsert_smart_collection(
        connection,
        SmartCollectionInput {
            id: None,
            name,
            icon: source.icon,
            color: source.color,
            rule: source.rule,
        },
    )
}

pub fn delete_smart_collection(connection: &Connection, id: &str) -> Result<(), String> {
    if connection
        .execute("DELETE FROM smart_collections WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("智能集合不存在".into());
    }
    normalize_collection_order(connection)
}

pub fn reorder_smart_collections(
    connection: &mut Connection,
    ids: Vec<String>,
) -> Result<(), String> {
    let existing = list_smart_collections(connection)?
        .into_iter()
        .map(|v| v.id)
        .collect::<Vec<_>>();
    if ids.len() != existing.len()
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
        || ids.iter().any(|id| !existing.contains(id))
    {
        return Err("智能集合排序列表不完整".into());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE smart_collections SET sort_order=?1 WHERE id=?2",
                params![index as i64, id],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())
}

fn normalize_collection_order(connection: &Connection) -> Result<(), String> {
    for (index, value) in list_smart_collections(connection)?.iter().enumerate() {
        connection
            .execute(
                "UPDATE smart_collections SET sort_order=?1 WHERE id=?2",
                params![index as i64, value.id],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn resolve_smart_collection(
    connection: &Connection,
    request: &SearchRequest,
) -> Result<Option<SearchRequest>, String> {
    let Some(id) = request.smart_collection_id.as_deref() else {
        return Ok(Some(request.clone()));
    };
    let collection = get_smart_collection(connection, id)?;
    if !collection.invalid_conditions.is_empty() {
        return Ok(None);
    }
    Ok(Some(SearchRequest {
        query: collection.rule.query,
        category_ids: collection.rule.category_ids,
        tags: vec![],
        tag_ids: collection.rule.tag_ids,
        dcc_tools: collection.rule.dcc_tools,
        versions: collection.rule.versions,
        formats: collection.rule.formats,
        licenses: collection.rule.licenses,
        favorite_only: collection.rule.favorite_only,
        recent_only: collection.rule.recent_only,
        health_issue: collection.rule.health_issue,
        smart_collection_id: Some(id.into()),
        project_id: request.project_id.clone(),
        project_asset_status: request.project_asset_status.clone(),
        sort: collection.rule.sort,
        offset: request.offset,
        limit: request.limit,
        content_language: request.content_language.clone(),
    }))
}

pub fn list_tags(
    connection: &Connection,
    locale: &str,
    query: &str,
    offset: i64,
    limit: i64,
) -> Result<Page<TagUsage>, String> {
    let locale = if locale == "en" { "en" } else { "zh-CN" };
    let pattern = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
    let total = connection
        .query_row(
            "SELECT COUNT(*) FROM localized_tags WHERE locale=?1 AND name LIKE ?2 ESCAPE '\\'",
            params![locale, pattern],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut statement = connection.prepare("SELECT t.id,t.locale,t.name,COUNT(a.id),COUNT(DISTINCT a.id) FROM localized_tags t LEFT JOIN asset_localized_tags at ON at.tag_id=t.id LEFT JOIN assets a ON a.id=at.asset_id AND a.deleted_at IS NULL WHERE t.locale=?1 AND t.name LIKE ?2 ESCAPE '\\' GROUP BY t.id ORDER BY COUNT(a.id) DESC,t.name COLLATE NOCASE LIMIT ?3 OFFSET ?4").map_err(|e| e.to_string())?;
    let items = statement
        .query_map(
            params![locale, pattern, limit.clamp(1, 200), offset.max(0)],
            |row| {
                Ok(TagUsage {
                    id: row.get(0)?,
                    locale: row.get(1)?,
                    name: row.get(2)?,
                    usage_count: row.get(3)?,
                    asset_count: row.get(4)?,
                })
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page {
        items,
        total,
        offset: offset.max(0),
        limit: limit.clamp(1, 200),
    })
}

pub fn rename_tag(
    connection: &mut Connection,
    id: &str,
    name: &str,
) -> Result<TagMutationReport, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("标签名称不能为空".into());
    }
    let (locale, old): (String, String) = connection
        .query_row(
            "SELECT locale,name FROM localized_tags WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("标签不存在")?;
    let normalized = name.to_lowercase();
    if let Some(existing) = connection
        .query_row(
            "SELECT id FROM localized_tags WHERE locale=?1 AND normalized_name=?2 AND id<>?3",
            params![locale, normalized, id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "标签“{name}”已存在，请改用合并操作（目标 {existing}）"
        ));
    }
    let affected = tag_asset_ids(connection, &[id.to_string()])?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute(
            "UPDATE localized_tags SET name=?1,normalized_name=?2 WHERE id=?3",
            params![name, normalized, id],
        )
        .map_err(|e| e.to_string())?;
    for asset_id in &affected {
        db::reindex_asset(&transaction, asset_id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    let _ = old;
    Ok(TagMutationReport {
        affected_assets: affected.len(),
        removed_tags: 0,
    })
}

pub fn merge_tags(
    connection: &mut Connection,
    source_ids: Vec<String>,
    target_id: &str,
) -> Result<TagMutationReport, String> {
    let target_locale: String = connection
        .query_row(
            "SELECT locale FROM localized_tags WHERE id=?1",
            [target_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("目标标签不存在")?;
    let sources = source_ids
        .into_iter()
        .filter(|id| id != target_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Ok(TagMutationReport {
            affected_assets: 0,
            removed_tags: 0,
        });
    }
    for source in &sources {
        let locale: String = connection
            .query_row(
                "SELECT locale FROM localized_tags WHERE id=?1",
                [source],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("源标签不存在")?;
        if locale != target_locale {
            return Err("不允许跨语言合并标签".into());
        }
    }
    let affected = tag_asset_ids(connection, &sources)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for source in &sources {
        transaction.execute("INSERT OR IGNORE INTO asset_localized_tags(asset_id,tag_id) SELECT asset_id,?1 FROM asset_localized_tags WHERE tag_id=?2", params![target_id,source]).map_err(|e| e.to_string())?;
        transaction
            .execute("DELETE FROM localized_tags WHERE id=?1", [source])
            .map_err(|e| e.to_string())?;
    }
    rewrite_collection_tag_ids(&transaction, &sources, Some(target_id))?;
    for asset_id in &affected {
        db::reindex_asset(&transaction, asset_id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(TagMutationReport {
        affected_assets: affected.len(),
        removed_tags: sources.len(),
    })
}

pub fn delete_tags(
    connection: &mut Connection,
    ids: Vec<String>,
) -> Result<TagMutationReport, String> {
    let ids = ids
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(TagMutationReport {
            affected_assets: 0,
            removed_tags: 0,
        });
    }
    let affected = tag_asset_ids(connection, &ids)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for id in &ids {
        transaction
            .execute("DELETE FROM localized_tags WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    for asset_id in &affected {
        db::reindex_asset(&transaction, asset_id)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(TagMutationReport {
        affected_assets: affected.len(),
        removed_tags: ids.len(),
    })
}

pub fn delete_unused_tags(
    connection: &Connection,
    locale: &str,
) -> Result<TagMutationReport, String> {
    let locale = if locale == "en" { "en" } else { "zh-CN" };
    let removed = connection.execute("DELETE FROM localized_tags WHERE locale=?1 AND NOT EXISTS(SELECT 1 FROM asset_localized_tags at WHERE at.tag_id=localized_tags.id)", [locale]).map_err(|e| e.to_string())?;
    Ok(TagMutationReport {
        affected_assets: 0,
        removed_tags: removed,
    })
}

fn tag_asset_ids(connection: &Connection, ids: &[String]) -> Result<Vec<String>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let marks = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = connection
        .prepare(&format!(
            "SELECT DISTINCT asset_id FROM asset_localized_tags WHERE tag_id IN ({marks})"
        ))
        .map_err(|e| e.to_string())?;
    let values = statement
        .query_map(
            params_from_iter(ids.iter().map(|v| Value::Text(v.clone()))),
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(values)
}

fn rewrite_collection_tag_ids(
    connection: &Connection,
    sources: &[String],
    target: Option<&str>,
) -> Result<(), String> {
    let rows = {
        let mut statement = connection
            .prepare("SELECT id,rule_json FROM smart_collections")
            .map_err(|e| e.to_string())?;
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        values
    };
    let source_set = sources.iter().collect::<HashSet<_>>();
    for (id, raw) in rows {
        let mut rule: SmartCollectionRule = serde_json::from_str(&raw).unwrap_or_default();
        if !rule.tag_ids.iter().any(|id| source_set.contains(id)) {
            continue;
        }
        if let Some(target) = target {
            for id in &mut rule.tag_ids {
                if source_set.contains(id) {
                    *id = target.into();
                }
            }
            rule.tag_ids.sort();
            rule.tag_ids.dedup();
        }
        connection
            .execute(
                "UPDATE smart_collections SET rule_json=?1,updated_at=?2 WHERE id=?3",
                params![
                    serde_json::to_string(&rule).map_err(|e| e.to_string())?,
                    Utc::now().to_rfc3339(),
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    #[test]
    fn library_preferences_keep_library_first_and_unknown_modules() {
        let mut value = LibraryPreferences::default();
        value.module_order = vec!["future".into(), "favorites".into(), "library".into()];
        value.disabled_modules = vec!["library".into(), "future".into()];
        value.context_pane_widths.insert("library".into(), 900);
        value.collapsed_module_groups =
            vec!["manage".into(), "mediaLibraries".into(), "unknown".into()];
        let value = normalize_library(value);
        assert_eq!(value.module_order[0], "library");
        assert!(value.module_order.contains(&"future".into()));
        assert!(!value.disabled_modules.contains(&"library".into()));
        assert_eq!(value.context_pane_widths["library"], 420);
        assert_eq!(
            value.collapsed_module_groups,
            vec!["manage", "mediaLibraries"]
        );
    }
    #[test]
    fn rejects_duplicate_shortcuts() {
        let mut value = GlobalPreferences::default();
        value.shortcuts.insert("settings".into(), "Ctrl+P".into());
        assert!(normalize_global(value).is_err());
    }

    #[test]
    fn smart_collection_keeps_stable_tag_reference_through_merge() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(include_str!("schema.sql"))
            .unwrap();
        let now = Utc::now().to_rfc3339();
        connection.execute("INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES('a','Asset','https://pan.baidu.com/s/a',?1,?1)", [&now]).unwrap();
        connection.execute("INSERT INTO localized_tags(id,locale,name,normalized_name) VALUES('source','en','Tree','tree'),('target','en','Trees','trees')", []).unwrap();
        connection
            .execute(
                "INSERT INTO asset_localized_tags(asset_id,tag_id) VALUES('a','source')",
                [],
            )
            .unwrap();
        db::reindex_asset(&connection, "a").unwrap();
        let created = upsert_smart_collection(
            &connection,
            SmartCollectionInput {
                id: None,
                name: "Trees".into(),
                icon: "sparkles".into(),
                color: "#D99A42".into(),
                rule: SmartCollectionRule {
                    tag_ids: vec!["source".into()],
                    sort: "updated".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert!(created.invalid_conditions.is_empty());
        let report = merge_tags(&mut connection, vec!["source".into()], "target").unwrap();
        assert_eq!(report.affected_assets, 1);
        let updated = get_smart_collection(&connection, &created.id).unwrap();
        assert_eq!(updated.rule.tag_ids, vec!["target"]);
        assert!(updated.invalid_conditions.is_empty());
        delete_tags(&mut connection, vec!["target".into()]).unwrap();
        assert_eq!(
            get_smart_collection(&connection, &created.id)
                .unwrap()
                .invalid_conditions
                .len(),
            1
        );
    }
}
