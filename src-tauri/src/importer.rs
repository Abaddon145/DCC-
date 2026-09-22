use crate::{db, fab, models::*};
use calamine::{open_workbook_auto, Reader};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::Path,
};
use url::Url;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const CANONICAL: &[(&str, &[&str])] = &[
    ("fab_url", &["fab_url", "Fab URL", "Fab网址", "Fab 链接"]),
    ("baidu_url", &["baidu_url", "百度网盘链接", "百度分享文本"]),
    ("ue_versions", &["ue_versions", "UE版本", "UE 版本"]),
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
    let mut duplicate_count = 0;
    let mut seen_fab = HashMap::<String, usize>::new();
    for row in &rows {
        let mut result = validate_row(connection, row)?;
        if row.values.contains_key("fab_url") && result.status != "error" {
            let raw_url = value(row, "fab_url");
            if let Ok(listing_id) = fab::listing_id_from_url(raw_url) {
                let listing_id = listing_id.to_string();
                if let Some(first_row) = seen_fab.get(&listing_id) {
                    result.status = "duplicate".into();
                    result.duplicate_source = Some("file".into());
                    result
                        .messages
                        .push(format!("与第 {first_row} 行是同一 Fab 商品，导入时将跳过"));
                } else {
                    seen_fab.insert(listing_id.clone(), row.row);
                    if let Some(duplicate) = db::fab_duplicate_match(connection, &listing_id, None)?
                    {
                        result.status = "duplicate".into();
                        result.duplicate_asset_id = Some(duplicate.asset_id);
                        result.duplicate_source = Some(duplicate.location.clone());
                        result.messages.push(if duplicate.location == "trash" {
                            format!("Fab 素材“{}”已在回收站，导入时将跳过", duplicate.asset_name)
                        } else {
                            format!("Fab 素材“{}”已存在，导入时将跳过", duplicate.asset_name)
                        });
                    }
                }
            }
        }
        match result.status.as_str() {
            "error" => error_count += 1,
            "duplicate" => duplicate_count += 1,
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
        duplicate_count,
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
        let exists: bool = !normalized.is_empty() && connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE normalized_share_url=?1 AND deleted_at IS NULL)",
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
            fab_listing_id: None,
            auto_category_path: Vec::new(),
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
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("xlsx"))
    {
        return Err("导入模板必须保存为 .xlsx 文件".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = File::create(path).map_err(|error| format!("创建 Excel 模板失败：{error}"))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let parts = [
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_string()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string()),
        ("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Fab批量导入" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_string()),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_string()),
        ("xl/worksheets/sheet1.xml", template_sheet_xml()),
    ];
    for (name, content) in parts {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn template_sheet_xml() -> String {
    let rows = [
        ["fab_url", "baidu_url", "ue_versions"],
        [
            "https://www.fab.com/listings/00000000-0000-0000-0000-000000000000",
            "https://pan.baidu.com/s/example?pwd=a1b2",
            "5.4;5.5",
        ],
    ];
    let mut sheet = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cols><col min="1" max="1" width="72" customWidth="1"/><col min="2" max="2" width="48" customWidth="1"/><col min="3" max="3" width="22" customWidth="1"/></cols><sheetData>"#,
    );
    for (row_index, row) in rows.iter().enumerate() {
        sheet.push_str(&format!("<row r=\"{}\">", row_index + 1));
        for (column_index, value) in row.iter().enumerate() {
            let column = (b'A' + column_index as u8) as char;
            sheet.push_str(&format!(
                "<c r=\"{column}{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                row_index + 1,
                xml_escape(value)
            ));
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    sheet
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, Clone)]
pub struct FabImportRow {
    pub row: usize,
    pub fab_url: String,
    pub baidu_text: String,
    pub ue_versions: Vec<String>,
}

pub fn is_fab_import(mapping: &HashMap<String, String>) -> bool {
    mapping.contains_key("fab_url")
}

pub fn fab_rows(
    path: &Path,
    mapping: HashMap<String, String>,
) -> Result<Vec<FabImportRow>, String> {
    let (headers, raw_rows) = read_table(path)?;
    let effective = if mapping.is_empty() {
        suggest_mapping(&headers)
    } else {
        mapping
    };
    Ok(mapped_rows(&headers, raw_rows, &effective)
        .into_iter()
        .map(|row| FabImportRow {
            row: row.row,
            fab_url: value(&row, "fab_url").to_string(),
            baidu_text: value(&row, "baidu_url").to_string(),
            ue_versions: split(value(&row, "ue_versions")),
        })
        .collect())
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
            let headers: Vec<String> = iter
                .next()
                .ok_or("表格为空")?
                .iter()
                .map(|c| c.to_string())
                .collect();
            let mut rows: Vec<Vec<String>> = iter
                .map(|row| row.iter().map(|c| c.to_string()).collect())
                .collect();
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("xlsx"))
            {
                apply_url_hyperlinks(path, &headers, &mut rows);
            }
            Ok((headers, rows))
        }
        _ => Err("仅支持 .xlsx 和 .csv 文件".into()),
    }
}

fn apply_url_hyperlinks(path: &Path, headers: &[String], rows: &mut [Vec<String>]) {
    let Ok(hyperlinks) = xlsx_hyperlinks(path) else {
        return;
    };
    let mapping = suggest_mapping(headers);
    for canonical in ["fab_url", "baidu_url", "share_url", "source_url"] {
        let Some(header) = mapping.get(canonical) else {
            continue;
        };
        let Some(column) = headers.iter().position(|value| value == header) else {
            continue;
        };
        for (row_index, row) in rows.iter_mut().enumerate() {
            if let Some(target) = hyperlinks.get(&(row_index + 2, column + 1)) {
                if Url::parse(target)
                    .ok()
                    .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
                {
                    if row.len() <= column {
                        row.resize(column + 1, String::new());
                    }
                    row[column] = target.clone();
                }
            }
        }
    }
}

fn xlsx_hyperlinks(path: &Path) -> Result<HashMap<(usize, usize), String>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let workbook_xml = zip_text(&mut archive, "xl/workbook.xml")?;
    let workbook_rels = zip_text(&mut archive, "xl/_rels/workbook.xml.rels")?;
    let first_sheet = Regex::new(r#"<sheet\b[^>]*>"#)
        .map_err(|error| error.to_string())?
        .find(&workbook_xml)
        .ok_or_else(|| "Excel 工作簿没有工作表".to_string())?
        .as_str();
    let relationship_id =
        xml_attribute(first_sheet, "r:id").ok_or_else(|| "Excel 工作表关系无效".to_string())?;
    let sheet_target = relationship_target(&workbook_rels, &relationship_id)
        .ok_or_else(|| "Excel 工作表路径无效".to_string())?;
    let sheet_path = if sheet_target.starts_with('/') {
        sheet_target.trim_start_matches('/').to_string()
    } else {
        format!("xl/{}", sheet_target.trim_start_matches("../"))
    };
    let (sheet_directory, sheet_file) = sheet_path
        .rsplit_once('/')
        .ok_or_else(|| "Excel 工作表路径无效".to_string())?;
    let sheet_rels_path = format!("{sheet_directory}/_rels/{sheet_file}.rels");
    let sheet_xml = zip_text(&mut archive, &sheet_path)?;
    let sheet_rels = match zip_text(&mut archive, &sheet_rels_path) {
        Ok(value) => value,
        Err(_) => return Ok(HashMap::new()),
    };
    let mut targets = HashMap::new();
    let relationship_tag =
        Regex::new(r#"<Relationship\b[^>]*>"#).map_err(|error| error.to_string())?;
    for item in relationship_tag.find_iter(&sheet_rels) {
        let tag = item.as_str();
        if let (Some(id), Some(target)) = (xml_attribute(tag, "Id"), xml_attribute(tag, "Target")) {
            targets.insert(id, xml_unescape(&target));
        }
    }
    let hyperlink_tag = Regex::new(r#"<hyperlink\b[^>]*>"#).map_err(|error| error.to_string())?;
    let mut result = HashMap::new();
    for item in hyperlink_tag.find_iter(&sheet_xml) {
        let tag = item.as_str();
        let Some(reference) = xml_attribute(tag, "ref") else {
            continue;
        };
        let Some(id) = xml_attribute(tag, "r:id") else {
            continue;
        };
        if let (Some(cell), Some(target)) = (cell_position(&reference), targets.get(&id)) {
            result.insert(cell, target.clone());
        }
    }
    Ok(result)
}

fn zip_text(archive: &mut ZipArchive<File>, name: &str) -> Result<String, String> {
    let mut value = String::new();
    archive
        .by_name(name)
        .map_err(|error| error.to_string())?
        .read_to_string(&mut value)
        .map_err(|error| error.to_string())?;
    Ok(value)
}

fn xml_attribute(tag: &str, name: &str) -> Option<String> {
    Regex::new(&format!(r#"\b{}="([^"]*)""#, regex::escape(name)))
        .ok()?
        .captures(tag)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
}

fn relationship_target(xml: &str, id: &str) -> Option<String> {
    let relationship_tag = Regex::new(r#"<Relationship\b[^>]*>"#).ok()?;
    let result = relationship_tag.find_iter(xml).find_map(|item| {
        let tag = item.as_str();
        (xml_attribute(tag, "Id").as_deref() == Some(id))
            .then(|| xml_attribute(tag, "Target"))
            .flatten()
            .map(|value| xml_unescape(&value))
    });
    result
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn cell_position(reference: &str) -> Option<(usize, usize)> {
    let letters = reference
        .chars()
        .take_while(|value| value.is_ascii_alphabetic())
        .collect::<String>();
    let digits = reference
        .chars()
        .skip_while(|value| value.is_ascii_alphabetic())
        .collect::<String>();
    let row = digits.parse::<usize>().ok()?;
    let column = letters.chars().try_fold(0usize, |value, letter| {
        value
            .checked_mul(26)?
            .checked_add((letter.to_ascii_uppercase() as u8 - b'A' + 1) as usize)
    })?;
    (row > 0 && column > 0).then_some((row, column))
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
    if row.values.contains_key("fab_url") {
        let fab_url = value(row, "fab_url");
        let mut messages = Vec::new();
        let valid = fab::listing_id_from_url(fab_url).is_ok();
        if !valid {
            messages.push("Fab URL 无效或缺失".into());
        }
        return Ok(ImportRowResult {
            row: row.row,
            name: fab_url.into(),
            status: if valid {
                "valid".into()
            } else {
                "error".into()
            },
            messages,
            suggested_category_path: None,
            actual_category_path: None,
            duplicate_asset_id: None,
            duplicate_source: None,
        });
    }
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
    if !link.is_empty() {
        if Url::parse(link)
            .ok()
            .is_none_or(|u| !matches!(u.scheme(), "http" | "https"))
        {
            messages.push("分享链接格式无效".into());
            status = "error";
        } else {
            let normalized = db::normalize_share_url(link).unwrap_or_default();
            let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE normalized_share_url=?1 AND deleted_at IS NULL)",
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
        suggested_category_path: None,
        actual_category_path: None,
        duplicate_asset_id: None,
        duplicate_source: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_hyperlink_fixture(path: &Path) {
        let file = File::create(path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let parts = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Fab批量导入" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>fab_url</t></is></c><c r="B1" t="inlineStr"><is><t>baidu_url</t></is></c><c r="C1" t="inlineStr"><is><t>ue_versions</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>https://www.fab.com/listings/11111111-1111-1111-1111-111111111111</t></is></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Futuristic Rooftop City Environment | Fab</t></is></c><c r="B3" t="inlineStr"><is><t>通过网盘分享的文件：City.rar&#10;链接: https://pan.baidu.com/s/demo?pwd=6666 提取码: 6666</t></is></c></row></sheetData><hyperlinks><hyperlink ref="A3" r:id="rId1"/></hyperlinks></worksheet>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://www.fab.com/listings/6b1e9d82-2d94-45d0-993e-d5e730eac7de" TargetMode="External"/></Relationships>"#,
            ),
        ];
        for (name, content) in parts {
            archive.start_file(name, options).unwrap();
            archive.write_all(content.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn reads_the_simplified_fab_template_and_user_versions() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("fab.csv");
        std::fs::write(&path, "fab_url,baidu_url,ue_versions\nhttps://www.fab.com/listings/11111111-1111-1111-1111-111111111111,https://pan.baidu.com/s/demo?pwd=a1b2,5.4;5.5\n").unwrap();
        let mapping =
            suggest_mapping(&["fab_url".into(), "baidu_url".into(), "ue_versions".into()]);
        assert!(is_fab_import(&mapping));
        let rows = fab_rows(&path, mapping).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ue_versions, vec!["5.4", "5.5"]);
        assert!(rows[0].baidu_text.contains("pwd=a1b2"));
    }

    #[test]
    fn exports_a_real_excel_template_that_can_be_imported_again() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("fab-template.xlsx");
        export_template(&path).unwrap();

        let rows = fab_rows(&path, HashMap::new()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].fab_url,
            "https://www.fab.com/listings/00000000-0000-0000-0000-000000000000"
        );
        assert!(rows[0].baidu_text.contains("pwd=a1b2"));
        assert_eq!(rows[0].ue_versions, vec!["5.4", "5.5"]);
    }

    #[test]
    fn reads_fab_url_from_a_titled_excel_hyperlink() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("linked-template.xlsx");
        write_hyperlink_fixture(&path);

        let rows = fab_rows(&path, HashMap::new()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].row, 3);
        assert_eq!(
            rows[1].fab_url,
            "https://www.fab.com/listings/6b1e9d82-2d94-45d0-993e-d5e730eac7de"
        );
        assert!(rows[1].baidu_text.contains("提取码: 6666"));
    }

    #[test]
    fn preview_marks_fab_url_variants_in_the_same_file_as_duplicates() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("duplicates.csv");
        std::fs::write(
            &path,
            "fab_url\nhttps://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a\nhttp://fab.com/zh-cn/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a/?lang=zh-CN#details\n",
        )
        .unwrap();
        let connection = db::open_database(&directory.path().join("library.db")).unwrap();
        let result = preview(&connection, &path, HashMap::new()).unwrap();
        assert_eq!(result.valid_count, 1);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(result.rows[1].status, "duplicate");
        assert_eq!(result.rows[1].duplicate_source.as_deref(), Some("file"));
    }
}
