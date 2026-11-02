use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CourseMediaBinding {
    pub source_item_id: String,
    pub media_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Progress {
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub rate: f64,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct SourceProgressMetadata {
    pub last_played_at: Option<i64>,
    pub updated_at: i64,
    pub last_sequence: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TargetCourse {
    pub id: String,
    pub progress: Progress,
    pub revision: i64,
    pub memberships: Vec<TargetMembership>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct TargetMembership {
    pub series_id: String,
    pub series_revision: i64,
    pub position: i64,
}

#[derive(Debug, Clone)]
pub(super) struct CourseItem {
    pub id: String,
    pub title: String,
    pub requested_media_id: Option<String>,
    pub media_id: Option<String>,
    pub source_progress: Option<Progress>,
    pub source_progress_metadata: Option<SourceProgressMetadata>,
    pub source_duration_ms: Option<i64>,
    pub source_item_hash: String,
    pub origin: Option<super::course_origins::CourseOriginState>,
    pub origin_token: Option<String>,
    pub target: Option<TargetCourse>,
}

#[derive(Debug, Clone)]
pub(super) struct SourceSeries {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub source_series_hash: String,
    pub origin: Option<super::course_origins::CourseOriginState>,
    pub origin_token: Option<String>,
    pub items: Vec<CourseItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConflictKind {
    SharedMedia,
    RepeatedSeriesMembership,
    DifferentSourceProgress,
    DifferentTargetProgress,
}

#[derive(Debug, Clone)]
pub(super) struct Conflict {
    pub kind: ConflictKind,
    pub media_id: String,
    pub source_item_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CourseImportPreview {
    pub series: Vec<SourceSeries>,
    pub conflicts: Vec<Conflict>,
    pub source_fingerprint: String,
    pub target_fingerprint: String,
    pub source_binding: Option<String>,
}

/// Compares only previewed fields; this does not authorize or atomically commit an import.
pub(super) fn recheck_preview(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
    expected: &CourseImportPreview,
) -> Result<CourseImportPreview, String> {
    let current = preview_courses(source, target, bindings)?;
    compare_preview(current, expected)
}

fn compare_preview(
    current: CourseImportPreview,
    expected: &CourseImportPreview,
) -> Result<CourseImportPreview, String> {
    if current.source_fingerprint != expected.source_fingerprint {
        return Err("课程来源已变化，请重新预检".into());
    }
    if current.target_fingerprint != expected.target_fingerprint {
        return Err("目标课程或媒体绑定已变化，请重新预检".into());
    }
    Ok(current)
}

/// Profile identity alone grants no access to a source database.
pub(super) fn preview_courses_from_profile(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
    profile: &str,
) -> Result<CourseImportPreview, String> {
    preview_courses_inner(source, target, bindings, Some(profile), None)
}

pub(super) fn preview_courses_from_profile_selected(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
    profile: &str,
    series: &[String],
) -> Result<CourseImportPreview, String> {
    preview_courses_inner(source, target, bindings, Some(profile), Some(series))
}

pub(super) fn recheck_preview_from_profile(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
    profile: &str,
    expected: &CourseImportPreview,
) -> Result<CourseImportPreview, String> {
    let current = preview_courses_from_profile(source, target, bindings, profile)?;
    compare_preview(current, expected)
}

pub(super) fn preview_courses(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<CourseImportPreview, String> {
    preview_courses_inner(source, target, bindings, None, None)
}

pub(super) fn check_source_schema(source: &Connection) -> Result<(), String> {
    let failed = || "课程来源快照无效".to_string();
    let required: &[(&str, &[&str])] = &[
        ("schema_version", &["version"]),
        ("series", &["id", "title", "description", "sort_order"]),
        (
            "series_items",
            &["id", "series_id", "title", "duration_ms", "sort_order"],
        ),
        (
            "playback_progress",
            &[
                "item_id",
                "position_ms",
                "duration_ms",
                "playback_rate",
                "completed",
                "last_played_at",
                "updated_at",
                "last_sequence",
            ],
        ),
    ];
    for (table, columns) in required {
        let kind: String = source.query_row(
            "SELECT type FROM pragma_table_list() WHERE schema='main' AND name=?1 COLLATE BINARY",
            [table], |row| row.get(0),
        ).map_err(|_| failed())?;
        if kind != "table" {
            return Err(failed());
        }
        let mut stmt = source
            .prepare("SELECT name FROM pragma_table_xinfo(?1,'main') WHERE hidden=0")
            .map_err(|_| failed())?;
        let actual = stmt
            .query_map([table], |row| row.get::<_, String>(0))
            .map_err(|_| failed())?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|_| failed())?;
        if columns.iter().any(|column| !actual.contains(*column)) {
            return Err(failed());
        }
    }
    Ok(())
}

fn check_source_budget(
    source: &Connection,
    selected_series: Option<&[String]>,
) -> Result<(), String> {
    // ponytail: bound the current whole-library preview; page batches before raising this limit.
    const MAX_ROWS: i64 = 10_000;
    const MAX_BYTES: i64 = 2 * 1024 * 1024;
    let failed = || "课程来源快照无效".to_string();
    let queries = [
        "SELECT count(*),coalesce(sum(coalesce(octet_length(id),0)+coalesce(octet_length(title),0)+coalesce(octet_length(description),0)+coalesce(octet_length(sort_order),0)),0) FROM (SELECT id,title,description,sort_order FROM main.series WHERE ?1 IS NULL OR id COLLATE BINARY IN (SELECT value FROM json_each(?1)) LIMIT 10001)",
        "SELECT count(*),coalesce(sum(coalesce(octet_length(id),0)+coalesce(octet_length(series_id),0)+coalesce(octet_length(title),0)+coalesce(octet_length(duration_ms),0)+coalesce(octet_length(sort_order),0)),0) FROM (SELECT id,series_id,title,duration_ms,sort_order FROM main.series_items WHERE ?1 IS NULL OR series_id COLLATE BINARY IN (SELECT value FROM json_each(?1)) LIMIT 10001)",
        "SELECT count(*),coalesce(sum(coalesce(octet_length(item_id),0)+coalesce(octet_length(position_ms),0)+coalesce(octet_length(duration_ms),0)+coalesce(octet_length(playback_rate),0)+coalesce(octet_length(completed),0)+coalesce(octet_length(last_played_at),0)+coalesce(octet_length(updated_at),0)+coalesce(octet_length(last_sequence),0)),0) FROM (SELECT item_id,position_ms,duration_ms,playback_rate,completed,last_played_at,updated_at,last_sequence FROM main.playback_progress WHERE ?1 IS NULL OR item_id COLLATE BINARY IN (SELECT id FROM main.series_items WHERE series_id COLLATE BINARY IN (SELECT value FROM json_each(?1))) LIMIT 10001)",
    ];
    let mut rows = 0i64;
    let mut bytes = 0i64;
    let selected_json = selected_series
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| failed())?;
    for query in queries {
        let (count, size): (i64, i64) = source
            .query_row(query, [&selected_json], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|_| failed())?;
        rows = rows.checked_add(count).ok_or_else(failed)?;
        bytes = bytes.checked_add(size).ok_or_else(failed)?;
        if rows > MAX_ROWS || bytes > MAX_BYTES {
            return Err("课程来源过大，当前版本无法一次预检".into());
        }
    }
    Ok(())
}

fn preview_courses_inner(
    source: &Connection,
    target: &Connection,
    bindings: &[CourseMediaBinding],
    profile: Option<&str>,
    selected_series: Option<&[String]>,
) -> Result<CourseImportPreview, String> {
    let failed = || "课程来源快照无效".to_string();
    let source_tx = source.unchecked_transaction().map_err(|_| failed())?;
    check_source_schema(&source_tx)?;
    let user_version: i64 = source_tx
        .query_row("PRAGMA main.user_version", [], |r| r.get(0))
        .map_err(|_| failed())?;
    let versions: (i64, i64, i64) = source_tx.query_row("SELECT count(*), coalesce(min(version),0), coalesce(max(version),0) FROM (SELECT version FROM main.schema_version LIMIT 2)", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|_| failed())?;
    if user_version != 8 || versions != (1, 8, 8) {
        return Err(failed());
    }
    if selected_series.is_some_and(|ids| ids.len() > 10000) {
        return Err("所选系列过多".into());
    }
    check_source_budget(&source_tx, selected_series)?;
    let invalid_relations: bool = source_tx.query_row("SELECT EXISTS(SELECT 1 FROM main.series GROUP BY id COLLATE BINARY HAVING count(*)>1) OR EXISTS(SELECT 1 FROM main.series_items GROUP BY id COLLATE BINARY HAVING count(*)>1) OR EXISTS(SELECT 1 FROM main.playback_progress GROUP BY item_id COLLATE BINARY HAVING count(*)>1) OR EXISTS(SELECT 1 FROM main.series_items i LEFT JOIN main.series s ON s.id COLLATE BINARY=i.series_id COLLATE BINARY WHERE s.id IS NULL) OR EXISTS(SELECT 1 FROM main.playback_progress p LEFT JOIN main.series_items i ON i.id COLLATE BINARY=p.item_id COLLATE BINARY WHERE i.id IS NULL)", [], |r| r.get(0)).map_err(|_| failed())?;
    if invalid_relations {
        return Err(failed());
    }
    let mut binding_map = HashMap::new();
    for binding in bindings {
        if !valid_id(&binding.source_item_id)
            || binding_map
                .insert(binding.source_item_id.as_str(), binding.media_id.as_str())
                .is_some()
        {
            return Err(failed());
        }
    }
    let tx = target
        .unchecked_transaction()
        .map_err(|_| "目标库读取失败".to_string())?;
    let profile_state = profile
        .map(|id| super::source_profiles::get_in(&tx, id))
        .transpose()?;
    let source_binding = profile
        .map(|id| super::source_profiles::sqlite_binding_in(&tx, id))
        .transpose()?
        .flatten();
    let selected_json = selected_series
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| failed())?;
    let mut series_stmt = source_tx
        .prepare(
            "SELECT id,title,description FROM main.series WHERE ?1 IS NULL OR id COLLATE BINARY IN (SELECT value FROM json_each(?1)) ORDER BY sort_order,id COLLATE BINARY",
        )
        .map_err(|_| failed())?;
    let source_series = series_stmt
        .query_map([&selected_json], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|_| failed())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| failed())?;
    let chosen = if let Some(ids) = selected_series {
        let set: HashSet<&str> = ids.iter().map(String::as_str).collect();
        if ids.is_empty()
            || set.len() != ids.len()
            || ids.iter().any(|id| !valid_id(id))
            || !set
                .iter()
                .all(|id| source_series.iter().any(|series| series.0 == *id))
        {
            return Err("所选系列已变化，请重新选择".into());
        }
        Some(set)
    } else {
        None
    };
    let mut item_stmt = source_tx
        .prepare("SELECT id,title,duration_ms FROM main.series_items WHERE series_id COLLATE BINARY=?1 ORDER BY sort_order,id COLLATE BINARY")
        .map_err(|_| failed())?;
    let mut seen = HashSet::new();
    let mut series = Vec::new();
    for (id, title, description) in source_series {
        if !valid_id(&id) {
            return Err(failed());
        }
        if chosen
            .as_ref()
            .is_some_and(|set| !set.contains(id.as_str()))
        {
            continue;
        }
        let ids = item_stmt
            .query_map([&id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })
            .map_err(|_| failed())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| failed())?;
        let mut items = Vec::new();
        // ponytail: per-item lookups; batch them if large-library profiling shows a bottleneck.
        for (item_id, item_title, source_duration_ms) in ids {
            if !valid_id(&item_id) || !seen.insert(item_id.clone()) {
                return Err(failed());
            }
            if source_duration_ms.is_some_and(|v| v < 0) {
                return Err(failed());
            }
            let raw: Option<(i64, Option<i64>, f64, i64, Option<i64>, i64, i64)> = source_tx.query_row("SELECT position_ms,duration_ms,playback_rate,completed,last_played_at,updated_at,last_sequence FROM main.playback_progress WHERE item_id COLLATE BINARY=?1", [&item_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(|_| failed())?;
            let (source_progress, source_progress_metadata) = match raw {
                Some((
                    position_ms,
                    duration_ms,
                    rate,
                    completed,
                    last_played_at,
                    updated_at,
                    last_sequence,
                )) => (
                    Some(checked_progress(position_ms, duration_ms, rate, completed)?),
                    Some(SourceProgressMetadata {
                        last_played_at,
                        updated_at,
                        last_sequence,
                    }),
                ),
                None => (None, None),
            };
            let media_id = if let Some(media) = binding_map.get(item_id.as_str()) {
                let available: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM media_entries WHERE id=?1 AND kind='video' AND deleted_at IS NULL)", [media], |r| r.get(0)).map_err(|_| "目标库读取失败".to_string())?;
                available.then(|| (*media).to_owned())
            } else {
                None
            };
            let target_course = if let Some(media) = &media_id {
                let raw: Option<(String,i64,Option<i64>,f64,i64,i64)> = tx.query_row("SELECT id,position_ms,duration_ms,rate,completed,progress_revision FROM learning_courses WHERE media_entry_id=?1", [media], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(|_| "目标库读取失败".to_string())?;
                raw.map(|(id, pos, duration, rate, complete, revision)| {
                    if revision < 0 {
                        return Err("目标课程进度无效".to_string());
                    }
                    let mut memberships = tx.prepare("SELECT s.id,s.revision,m.position FROM learning_series_courses m JOIN learning_series s ON s.id=m.series_id WHERE m.course_id=?1 ORDER BY s.id,m.position").map_err(|_| "目标库读取失败".to_string())?;
                    let memberships = memberships.query_map([&id], |r| Ok(TargetMembership { series_id: r.get(0)?, series_revision: r.get(1)?, position: r.get(2)? })).map_err(|_| "目标库读取失败".to_string())?.collect::<Result<Vec<_>,_>>().map_err(|_| "目标库读取失败".to_string())?;
                    Ok(TargetCourse {
                        id,
                        progress: checked_progress(pos, duration, rate, complete)?,
                        revision,
                        memberships,
                    })
                })
                .transpose()?
            } else {
                None
            };
            let requested_media_id = binding_map.get(item_id.as_str()).map(|v| (*v).to_owned());
            let source_item_hash = fingerprint(
                "yingliu.course-origin.source.v1",
                &(
                    &id,
                    &item_id,
                    &item_title,
                    source_duration_ms,
                    &source_progress,
                    &source_progress_metadata,
                ),
            )?;
            let origin = profile
                .map(|profile| {
                    let key = super::course_origins::CourseOriginKey::new(profile, &item_id)?;
                    super::course_origins::inspect(&tx, &key, &source_item_hash)
                })
                .transpose()?;
            let origin_token = profile
                .map(|profile| {
                    let key = super::course_origins::CourseOriginKey::new(profile, &item_id)?;
                    super::course_origins::preview_token(&tx, &key, &source_item_hash)
                })
                .transpose()?;
            if let (Some(media), Some(origin)) = (&media_id, &origin) {
                use super::course_origins::CourseOriginState::*;
                let mapped = match origin {
                    Unmapped => None,
                    Unchanged(id)
                    | SourceChanged(id)
                    | TargetChanged(id)
                    | BothChanged(id)
                    | TargetUnavailable(id) => Some(id),
                };
                if let Some(mapped) = mapped {
                    let mapped_media: Option<String> = tx
                        .query_row(
                            "SELECT media_entry_id FROM learning_courses WHERE id=?1",
                            [mapped],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|_| "无法检查课程来源")?;
                    if mapped_media.as_deref() != Some(media) {
                        return Err("课程来源已映射其他媒体，请重新检查".into());
                    }
                }
            }
            items.push(CourseItem {
                id: item_id,
                title: item_title,
                requested_media_id,
                media_id,
                source_progress,
                source_progress_metadata,
                source_duration_ms,
                source_item_hash,
                origin,
                origin_token,
                target: target_course,
            });
        }
        let source_series_hash = fingerprint(
            "yingliu.series-origin.source.v1",
            &(
                &id,
                &title,
                &description,
                items.iter().map(|i| &i.id).collect::<Vec<_>>(),
            ),
        )?;
        let origin = profile
            .map(|profile| {
                let key = super::course_origins::SeriesOriginKey::new(profile, &id)?;
                super::course_origins::inspect_series(&tx, &key, &source_series_hash)
            })
            .transpose()?;
        let origin_token = profile
            .map(|profile| {
                let key = super::course_origins::SeriesOriginKey::new(profile, &id)?;
                super::course_origins::preview_series_token(&tx, &key, &source_series_hash)
            })
            .transpose()?;
        series.push(SourceSeries {
            id,
            title,
            description,
            source_series_hash,
            origin,
            origin_token,
            items,
        });
    }
    if binding_map.keys().any(|id| !seen.contains(*id)) {
        return Err(failed());
    }
    let mut groups: BTreeMap<&str, Vec<(&str, &CourseItem)>> = BTreeMap::new();
    for s in &series {
        for item in &s.items {
            if let Some(media) = item.media_id.as_deref() {
                groups.entry(media).or_default().push((&s.id, item));
            }
        }
    }
    let mut conflicts = Vec::new();
    for (media, group) in groups {
        if group.len() > 1 {
            conflicts.push(Conflict {
                kind: ConflictKind::SharedMedia,
                media_id: media.into(),
                source_item_ids: group.iter().map(|(_, i)| i.id.clone()).collect(),
            });
        }
        let mut per_series: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (series_id, item) in &group {
            per_series
                .entry(series_id)
                .or_default()
                .push(item.id.clone());
        }
        for ids in per_series.into_values().filter(|v| v.len() > 1) {
            conflicts.push(Conflict {
                kind: ConflictKind::RepeatedSeriesMembership,
                media_id: media.into(),
                source_item_ids: ids,
            });
        }
        let known: Vec<_> = group
            .iter()
            .filter_map(|(_, i)| i.source_progress.as_ref())
            .collect();
        if known
            .first()
            .is_some_and(|first| known.iter().any(|p| p != first))
        {
            conflicts.push(Conflict {
                kind: ConflictKind::DifferentSourceProgress,
                media_id: media.into(),
                source_item_ids: group
                    .iter()
                    .filter(|(_, i)| i.source_progress.is_some())
                    .map(|(_, i)| i.id.clone())
                    .collect(),
            });
        }
        let different: Vec<String> = group.iter().filter(|(_, i)| matches!((&i.source_progress, &i.target), (Some(s),Some(t)) if *s != t.progress)).map(|(_, i)| i.id.clone()).collect();
        if !different.is_empty() {
            conflicts.push(Conflict {
                kind: ConflictKind::DifferentTargetProgress,
                media_id: media.into(),
                source_item_ids: different,
            });
        }
    }
    let source_view: Vec<_> = series
        .iter()
        .map(|s| {
            (
                &s.id,
                &s.title,
                &s.description,
                s.items
                    .iter()
                    .map(|i| {
                        (
                            &i.id,
                            &i.title,
                            &i.source_duration_ms,
                            &i.source_progress,
                            &i.source_progress_metadata,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let target_view: Vec<_> = series
        .iter()
        .map(|s| {
            (
                &s.id,
                &s.origin,
                &s.origin_token,
                s.items
                    .iter()
                    .map(|i| {
                        (
                            &i.id,
                            &i.requested_media_id,
                            &i.media_id,
                            &i.target,
                            &i.origin,
                            &i.origin_token,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let source_fingerprint = fingerprint("yingliu.course-preview.source.v2", &source_view)?;
    let profile_binding = profile_state.as_ref().map(|p| (&p.id, &p.access_epoch));
    let target_fingerprint = if let Some(binding) = &source_binding {
        fingerprint(
            "yingliu.course-preview.target.v5",
            &(profile_binding, target_view, binding),
        )?
    } else {
        fingerprint(
            "yingliu.course-preview.target.v4",
            &(profile_binding, target_view),
        )?
    };
    drop(item_stmt);
    drop(series_stmt);
    source_tx.commit().map_err(|_| failed())?;
    tx.commit().map_err(|_| "目标库读取失败".to_string())?;
    Ok(CourseImportPreview {
        series,
        conflicts,
        source_fingerprint,
        target_fingerprint,
        source_binding,
    })
}

// These cover only the fields used by this preview, not file identity or source authorization.
fn fingerprint<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(domain, value)).map_err(|_| "无法计算课程预检指纹")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
}

fn checked_progress(
    position_ms: i64,
    duration_ms: Option<i64>,
    rate: f64,
    completed: i64,
) -> Result<Progress, String> {
    if position_ms < 0
        || duration_ms.is_some_and(|v| v < 0)
        || !rate.is_finite()
        || !(0.25..=4.0).contains(&rate)
        || !(0..=1).contains(&completed)
    {
        return Err("课程进度无效".into());
    }
    Ok(Progress {
        position_ms,
        duration_ms,
        rate,
        completed: completed == 1,
    })
}
