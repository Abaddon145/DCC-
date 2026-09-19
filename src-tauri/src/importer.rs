use crate::{db, models::*};
use calamine::{open_workbook_auto, Reader};
use rusqlite::{Connection, OptionalExtension};
use std::{collections::HashMap, path::Path};
use url::Url;
use uuid::Uuid;

const CANONICAL: &[(&str, &[&str])] = &[
    ("name", &["name", "素材名称", "名称"]),
    ("name_zh", &["name_zh", "中文名称"]),
    ("name_en", &["name_en", "英文名称"]),
    ("share_url", &["share_url", "分享链接", "网盘链接"]),
    ("extraction_code", &["extraction_code", "提取码"]),
    ("description", &["description", "描述", "说明"]),
    ("description_zh", &["description_zh", "中文描述"]),
    ("description_en", &["description_en", "英文描述"]),
    ("category", &["category", "分类", "分类路径"]),
    ("tags", &["tags", "标签"]),
    ("tags_zh", &["tags_zh", "中文标签"]),
    ("tags_en", &["tags_en", "英文标签"]),
    ("dcc_tools", &["dcc_tools", "dcc软件", "软件"]),
    ("versions", &["versions", "版本"]),
    ("formats", &["formats", "格式"]),
    ("size", &["size", "素材大小", "大小"]),
    ("author", &["author", "作者", "工作室"]),
    ("source_url", &["source_url", "来源地址", "来源"]),
    ("license", &["license", "许可", "授权"]),
    ("license_zh", &["license_zh", "中文许可"]),
    ("license_en", &["license_en", "英文许可"]),
    (
        "preview_paths",
        &["preview_paths", "预览图路径", "图片路径"],
    ),
];

pub fn preview(
    connection: &Connection,
    path: &Path,
    mapping: HashMap<String, String>,
) -> Result<ImportPreview, String> {
    let (headers, raw_rows) = read_table(path)?;
    let suggested = suggest_mapping(&headers);
    let effective = if mapping.is_empty() {
        suggested.clone()
    } else {
        mapping
    };
    let rows = mapped_rows(&headers, raw_rows, &effective);
    let mut results = Vec::new();
    let mut valid_count = 0;
    let mut warning_count = 0;
    let mut error_count = 0;
    for row in &rows {
        let result = validate_row(connection, row)?;
        match result.status.as_str() {
            "error" => error_count += 1,
            "warning" => {
                warning_count += 1;
                valid_count += 1;
            }
            _ => valid_count += 1,
        }
        results.push(result);
    }
    Ok(ImportPreview {
        headers,
        suggested_mapping: suggested,
        sample: rows.iter().take(10).map(|r| r.values.clone()).collect(),
        valid_count,
        warning_count,
        error_count,
        rows: results,
    })
}

pub fn commit(
    connection: &mut Connection,
    base_dir: &Path,
    path: &Path,
    mapping: HashMap<String, String>,
) -> Result<ImportReport, String> {
    let (headers, raw_rows) = read_table(path)?;
    let effective = if mapping.is_empty() {
        suggest_mapping(&headers)
    } else {
        mapping
    };
    let rows = mapped_rows(&headers, raw_rows, &effective);
    let mut imported = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut results = Vec::new();
    for row in rows {
        let mut validation = validate_row(connection, &row)?;
        if validation.status == "error" {
            failed += 1;
            results.push(validation);
            continue;
        }
        let share_url = value(&row, "share_url");
        let normalized = db::normalize_share_url(share_url).unwrap_or_default();
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE normalized_share_url=?1)",
                [normalized],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            validation.status = "skipped".into();
            validation.messages.push("分享链接已存在".into());
            skipped += 1;
            results.push(validation);
            continue;
        }
        let category_id = ensure_category_path(connection, value(&row, "category"))?;
        let image_paths = split(value(&row, "preview_paths"))
            .into_iter()
            .filter(|source_path| Path::new(source_path).is_file())
            .map(|source_path| ImageInput {
                id: None,
                source_path: Some(source_path),
                original_name: None,
                is_cover: false,
                sort_order: 0,
            })
            .enumerate()
            .map(|(i, mut image)| {
                image.sort_order = i as i64;
                image.is_cover = i == 0;
                image
            })
            .collect();
        let legacy_name = value(&row, "name");
        let primary_locale = if legacy_name
            .chars()
            .any(|ch| matches!(ch as u32, 0x3400..=0x9FFF))
        {
            "zh-CN"
        } else {
            "en"
        };
        let mut localizations = HashMap::new();
        let zh = LocalizedAssetText {
            name: value(&row, "name_zh").into(),
            description: value(&row, "description_zh").into(),
            tags: split(value(&row, "tags_zh")),
            license: value(&row, "license_zh").into(),
        };
        let en = LocalizedAssetText {
            name: value(&row, "name_en").into(),
            description: value(&row, "description_en").into(),
            tags: split(value(&row, "tags_en")),
            license: value(&row, "license_en").into(),
        };
        if !zh.name.is_empty()
            || !zh.description.is_empty()
            || !zh.tags.is_empty()
            || !zh.license.is_empty()
        {
            localizations.insert("zh-CN".into(), zh);
        }
        if !en.name.is_empty()
            || !en.description.is_empty()
            || !en.tags.is_empty()
            || !en.license.is_empty()
        {
            localizations.insert("en".into(), en);
        }
        if localizations.is_empty() {
            localizations.insert(
                primary_locale.into(),
                LocalizedAssetText {
                    name: legacy_name.into(),
                    description: value(&row, "description").into(),
                    tags: split(value(&row, "tags")),
                    license: value(&row, "license").into(),
                },
            );
        }
        let input = AssetInput {
            id: None,
            name: value(&row, "name").into(),
            description: value(&row, "description").into(),
            category_id,
            tags: split(value(&row, "tags")),
            dcc_tools: split(value(&row, "dcc_tools")),
            versions: split(value(&row, "versions")),
            formats: split(value(&row, "formats")),
            size_bytes: parse_size(value(&row, "size")),
            author: value(&row, "author").into(),
            source_url: value(&row, "source_url").into(),
            license: value(&row, "license").into(),
            share_url: share_url.into(),
            extraction_code: value(&row, "extraction_code").into(),
            favorite: false,
            images: image_paths,
            localizations,
            content_language: primary_locale.into(),
        };
        match db::upsert_asset(connection, base_dir, input) {
            Ok(_) => {
                validation.status = "imported".into();
                imported += 1;
            }
            Err(error) => {
                validation.status = "error".into();
                validation.messages.push(error);
                failed += 1;
            }
        }
        results.push(validation);
    }
    Ok(ImportReport {
        imported,
        skipped,
        failed,
        rows: results,
    })
}

pub fn export_template(path: &Path) -> Result<(), String> {
    let mut writer = csv::WriterBuilder::new()
        .from_path(path)
        .map_err(|e| e.to_string())?;
    writer
        .write_record([
            "中文名称",
            "英文名称",
            "中文描述",
            "英文描述",
            "分类路径",
            "中文标签",
            "英文标签",
            "DCC软件",
            "版本",
            "格式",
            "素材大小",
            "作者",
            "来源地址",
            "中文许可",
            "英文许可",
            "分享链接",
            "提取码",
            "预览图路径",
        ])
        .map_err(|e| e.to_string())?;
    writer
        .write_record([
            "中世纪古堡环境包",
            "Medieval Castle Environment",
            "包含建筑与道具",
            "Includes architecture and props",
            "环境/建筑",
            "写实;Nanite",
            "Realistic;Nanite",
            "Unreal Engine",
            "5.4;5.5",
            "uasset;FBX",
            "2.4 GB",
            "示例工作室",
            "https://example.com",
            "商用需授权",
            "Commercial use requires authorization",
            "https://pan.baidu.com/s/example",
            "a1b2",
            r"D:\Previews\castle-01.jpg;D:\Previews\castle-02.jpg",
        ])
        .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

fn read_table(path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => {
            let mut reader = csv::ReaderBuilder::new()
                .flexible(true)
                .from_path(path)
                .map_err(|e| format!("读取 CSV 失败：{e}"))?;
            let headers = reader
                .headers()
                .map_err(|e| e.to_string())?
                .iter()
                .map(ToString::to_string)
                .collect();
            let rows = reader
                .records()
                .map(|record| {
                    record
                        .map(|r| r.iter().map(ToString::to_string).collect())
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((headers, rows))
        }
        "xlsx" | "xls" | "xlsb" | "ods" => {
            let mut workbook =
                open_workbook_auto(path).map_err(|e| format!("读取 Excel 失败：{e}"))?;
            let range = workbook
                .worksheet_range_at(0)
                .ok_or("表格中没有工作表")?
                .map_err(|e| e.to_string())?;
            let mut iter = range.rows();
            let headers = iter
                .next()
                .ok_or("表格为空")?
                .iter()
                .map(|c| c.to_string())
                .collect();
            let rows = iter
                .map(|row| row.iter().map(|c| c.to_string()).collect())
                .collect();
            Ok((headers, rows))
        }
        _ => Err("仅支持 .xlsx 和 .csv 文件".into()),
    }
}

fn suggest_mapping(headers: &[String]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (canonical, aliases) in CANONICAL {
        if let Some(header) = headers
            .iter()
            .find(|h| aliases.iter().any(|a| h.trim().eq_ignore_ascii_case(a)))
        {
            result.insert((*canonical).into(), header.clone());
        }
    }
    result
}

fn mapped_rows(
    headers: &[String],
    rows: Vec<Vec<String>>,
    mapping: &HashMap<String, String>,
) -> Vec<ParsedImportRow> {
    let indices: HashMap<&str, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.as_str(), i))
        .collect();
    rows.into_iter()
        .enumerate()
        .filter(|(_, cells)| cells.iter().any(|c| !c.trim().is_empty()))
        .map(|(index, cells)| {
            let values = mapping
                .iter()
                .map(|(canonical, header)| {
                    (
                        canonical.clone(),
                        indices
                            .get(header.as_str())
                            .and_then(|i| cells.get(*i))
                            .cloned()
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    )
                })
                .collect();
            ParsedImportRow {
                row: index + 2,
                values,
            }
        })
        .collect()
}

fn validate_row(connection: &Connection, row: &ParsedImportRow) -> Result<ImportRowResult, String> {
    let name = [
        value(row, "name_zh"),
        value(row, "name_en"),
        value(row, "name"),
    ]
    .into_iter()
    .find(|v| !v.is_empty())
    .unwrap_or("");
    let link = value(row, "share_url");
    let mut messages = Vec::new();
    let mut status = "valid";
    if name.is_empty() {
        messages.push("缺少素材名称".into());
        status = "error";
    }
    if link.is_empty() {
        messages.push("缺少分享链接".into());
        status = "error";
    } else if Url::parse(link)
        .ok()
        .is_none_or(|u| !matches!(u.scheme(), "http" | "https"))
    {
        messages.push("分享链接格式无效".into());
        status = "error";
    } else {
        let normalized = db::normalize_share_url(link).unwrap_or_default();
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE normalized_share_url=?1)",
                [normalized],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists && status != "error" {
            messages.push("分享链接已存在，导入时将跳过".into());
            status = "warning";
        }
        if Url::parse(link)
            .ok()
            .and_then(|u| u.host_str().map(ToString::to_string))
            .is_some_and(|h| h != "pan.baidu.com" && !h.ends_with(".pan.baidu.com"))
            && status != "error"
        {
            messages.push("不是百度网盘域名".into());
            status = "warning";
        }
    }
    for path in split(value(row, "preview_paths")) {
        if !Path::new(&path).is_file() && status != "error" {
            messages.push(format!("预览图不存在：{path}"));
            status = "warning";
        }
    }
    Ok(ImportRowResult {
        row: row.row,
        name: name.into(),
        status: status.into(),
        messages,
    })
}

fn value<'a>(row: &'a ParsedImportRow, key: &str) -> &'a str {
    row.values.get(key).map(String::as_str).unwrap_or("").trim()
}
fn split(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.split([';', '；', ',', '，'])
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter(|v| seen.insert(v.to_lowercase()))
        .map(ToString::to_string)
        .collect()
}
fn parse_size(raw: &str) -> Option<i64> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    let split_at = normalized
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(normalized.len());
    let number: f64 = normalized[..split_at].parse().ok()?;
    let unit = normalized[split_at..].trim();
    let power = match unit {
        "kb" => 1,
        "mb" => 2,
        "gb" => 3,
        "tb" => 4,
        _ => 0,
    };
    Some((number * 1024_f64.powi(power)) as i64)
}

fn ensure_category_path(connection: &Connection, raw: &str) -> Result<Option<String>, String> {
    let names: Vec<&str> = raw
        .split('/')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    if names.is_empty() {
        return Ok(None);
    }
    let mut parent: Option<String> = None;
    for name in names {
        let existing = connection
            .query_row(
                "SELECT id FROM categories WHERE name=?1 COLLATE NOCASE AND parent_id IS ?2",
                rusqlite::params![name, parent],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
        connection.execute("INSERT OR IGNORE INTO categories(id,name,parent_id,sort_order,created_at) VALUES(?1,?2,?3,0,?4)",rusqlite::params![id,name,parent,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
        parent = Some(id);
    }
    Ok(parent)
}
