use crate::{knowledge, knowledge_files};
use rusqlite::Connection;
use serde_json::Value;
use std::{io::Write, path::Path};
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn map_svg(data: &Value) -> Result<String, String> {
    let mut rows = Vec::new();
    let mut stack = vec![(&data["nodeData"], 0usize, None::<usize>)];
    while let Some((node, depth, parent)) = stack.pop() {
        if rows.len() >= 1000 || depth > 32 {
            return Err("思维导图过大".into());
        }
        let topic = node["topic"].as_str().ok_or("导图节点无效")?;
        let index = rows.len();
        rows.push((topic, depth, parent));
        if let Some(children) = node["children"].as_array() {
            stack.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (child, depth + 1, Some(index))),
            );
        }
    }
    let width = rows
        .iter()
        .map(|(_, depth, _)| depth * 220 + 260)
        .max()
        .unwrap_or(360)
        .max(360);
    let height = rows.len() * 56 + 32;
    let scale = 1f64
        .min(1600f64 / width as f64)
        .min(1600f64 / height as f64);
    let mut svg=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {width} {height}\"><rect width=\"{width}\" height=\"{height}\" fill=\"#11151B\"/>",(width as f64*scale).round() as usize,(height as f64*scale).round() as usize);
    for (i, (topic, depth, parent)) in rows.iter().enumerate() {
        if let Some(parent) = parent {
            svg.push_str(&format!(
                "<path d=\"M {} {} H {} V {}\" fill=\"none\" stroke=\"#6C7B90\"/>",
                rows[*parent].1 * 220 + 230,
                parent * 56 + 40,
                depth * 220 + 8,
                i * 56 + 40
            ));
        }
        svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"208\" height=\"40\" rx=\"6\" fill=\"#202936\" stroke=\"#64748B\"/><text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"14\" fill=\"#E4E8EF\">{}</text>",depth*220+16,i*56+20,depth*220+26,i*56+45,escape(&topic.chars().take(20).collect::<String>())));
    }
    svg.push_str("</svg>");
    Ok(svg)
}
fn render(db: &Connection, base: &Path, node: &Value, budget: &mut u64) -> Result<String, String> {
    let mut content = String::new();
    if let Some(children) = node["content"].as_array() {
        for child in children {
            content.push_str(&render(db, base, child, budget)?);
        }
    }
    let kind = node["type"].as_str().unwrap_or("");
    if kind == "text" {
        content = escape(node["text"].as_str().unwrap_or(""));
        if let Some(marks) = node["marks"].as_array() {
            for mark in marks {
                content = match mark["type"].as_str() {
                    Some("bold") => format!("<strong>{content}</strong>"),
                    Some("italic") => format!("<em>{content}</em>"),
                    Some("code") => format!("<code>{content}</code>"),
                    Some("strike") => format!("<s>{content}</s>"),
                    Some("link") => {
                        let href = mark["attrs"]["href"].as_str().unwrap_or("");
                        if href.starts_with("https://") || href.starts_with("http://") {
                            format!(
                                "<a href=\"{}\" rel=\"noreferrer noopener\">{content}</a>",
                                escape(href)
                            )
                        } else {
                            content
                        }
                    }
                    _ => content,
                };
            }
        }
        return Ok(content);
    }
    if kind == "image" {
        use base64::Engine;
        let id = node["attrs"]["src"]
            .as_str()
            .unwrap_or("")
            .strip_prefix("http://dcc-media.localhost/note/")
            .ok_or("图片地址无效")?;
        let (path, mime) = knowledge_files::path(db, base, id)?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        *budget = budget.checked_add(size).ok_or("图片过大")?;
        if size > 25 * 1024 * 1024 || *budget > 100 * 1024 * 1024 {
            return Err("离线 HTML 图片超过 100 MB，请使用笔记包".into());
        }
        if !mime.starts_with("image/") {
            return Err("图片类型无效".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        return Ok(format!(
            "<img src=\"data:{mime};base64,{}\" alt=\"{}\"/>",
            base64::engine::general_purpose::STANDARD.encode(bytes),
            escape(node["attrs"]["alt"].as_str().unwrap_or(""))
        ));
    }
    if kind == "mindMap" {
        return map_svg(&node["attrs"]["data"]);
    }
    if kind == "hardBreak" {
        return Ok("<br/>".into());
    }
    if kind == "horizontalRule" {
        return Ok("<hr/>".into());
    }
    if kind == "reference" || kind == "attachment" || kind == "remoteImage" {
        return Ok(format!(
            "<p>{}</p>",
            escape(
                node["attrs"]["label"]
                    .as_str()
                    .or(node["attrs"]["alt"].as_str())
                    .unwrap_or("附件 / 引用")
            )
        ));
    }
    let tag = match kind {
        "heading" => match node["attrs"]["level"].as_u64() {
            Some(1) => "h1",
            Some(2) => "h2",
            Some(3) => "h3",
            Some(4) => "h4",
            Some(5) => "h5",
            _ => "h6",
        },
        "paragraph" => "p",
        "bulletList" | "taskList" => "ul",
        "orderedList" => "ol",
        "listItem" | "taskItem" => "li",
        "blockquote" => "blockquote",
        "codeBlock" => "pre",
        "table" => "table",
        "tableRow" => "tr",
        "tableCell" => "td",
        "tableHeader" => "th",
        _ => "",
    };
    Ok(if tag.is_empty() {
        content
    } else {
        format!("<{tag}>{content}</{tag}>")
    })
}
pub fn html(db: &Connection, base: &Path, id: &str, revision: i64) -> Result<String, String> {
    let note = knowledge::get(db, id)?;
    if note.revision != revision {
        return Err("笔记已变化，请重新导出".into());
    }
    knowledge::inspect_document(&note.document)?;
    let body = render(db, base, &note.document, &mut 0)?;
    Ok(format!("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>body{{max-width:860px;margin:48px auto;padding:24px;font:16px/1.8 system-ui;color:#242a33;background:#fff}}img,svg{{max-width:100%;height:auto}}pre{{white-space:pre-wrap;background:#f2f4f7;padding:16px}}table{{border-collapse:collapse}}td,th{{border:1px solid #bbb;padding:8px}}blockquote{{border-left:3px solid #bbb;margin-left:0;padding-left:18px}}</style></head><body><h1>{}</h1>{body}</body></html>",escape(&note.title),escape(&note.title)))
}
pub fn export(
    db: &Connection,
    base: &Path,
    id: &str,
    revision: i64,
    format: &str,
    path: &Path,
) -> Result<(), String> {
    if !["html", "txt"].contains(&format)
        || path.extension().and_then(|s| s.to_str()) != Some(format)
        || path.exists()
    {
        return Err("请选择尚不存在的 HTML/TXT 文件".into());
    }
    let note = knowledge::get(db, id)?;
    if note.revision != revision {
        return Err("笔记已变化，请重新导出".into());
    }
    let text = if format == "html" {
        html(db, base, id, revision)?
    } else {
        format!("{}\n\n{}", note.title, note.plain_text)
    };
    let mut stage = tempfile::NamedTempFile::new_in(path.parent().ok_or("导出目录无效")?)
        .map_err(|e| e.to_string())?;
    stage
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage.persist_noclobber(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn export_map(
    db: &Connection,
    id: &str,
    revision: i64,
    map_id: &str,
    format: &str,
    png: Option<Vec<u8>>,
    path: &Path,
) -> Result<(), String> {
    uuid::Uuid::parse_str(map_id).map_err(|_| "导图 ID 无效")?;
    if !["svg", "png"].contains(&format)
        || path.extension().and_then(|v| v.to_str()) != Some(format)
        || path.exists()
    {
        return Err("请选择尚不存在的 PNG/SVG 文件".into());
    }
    let note = knowledge::get(db, id)?;
    if note.revision != revision {
        return Err("笔记已变化，请重新导出".into());
    }
    let mut nodes = vec![&note.document];
    let mut data = None;
    while let Some(node) = nodes.pop() {
        if node["type"] == "mindMap" && node["attrs"]["id"] == map_id {
            data = Some(&node["attrs"]["data"]);
            break;
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children);
        }
    }
    let data = data.ok_or("笔记中未找到该导图")?;
    let bytes = if format == "svg" {
        map_svg(data)?.into_bytes()
    } else {
        let bytes = png.ok_or("缺少 PNG 图像")?;
        if bytes.len() > 10 * 1024 * 1024 {
            return Err("PNG 超过 10 MB".into());
        }
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1600);
        limits.max_image_height = Some(1600);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map_err(|_| "PNG 无效或超过 1600px")?;
        bytes
    };
    let mut stage = tempfile::NamedTempFile::new_in(path.parent().ok_or("导出目录无效")?)
        .map_err(|e| e.to_string())?;
    stage.write_all(&bytes).map_err(|e| e.to_string())?;
    stage.as_file().sync_all().map_err(|e| e.to_string())?;
    stage.persist_noclobber(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn svg_escapes_topics_and_has_bounded_raster_dimensions() {
        let svg = map_svg(&json!({"nodeData":{"id":"root","topic":"<script>alert(1)"}})).unwrap();
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("&lt;script&gt;"));
    }
    #[test]
    fn map_persists_indexes_and_exports_without_active_html() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&temp.path().join("library.db")).unwrap();
        let map_id = uuid::Uuid::new_v4().to_string();
        let document = json!({"type":"doc","content":[{"type":"mindMap","attrs":{"id":map_id,"data":{"direction":1,"nodeData":{"id":"root","topic":"<script>材质学习","children":[{"id":"child","topic":"参数"}]}}}}]});
        let note = knowledge::save(
            &mut db,
            knowledge::NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: None,
                title: "学习".into(),
                document,
                tags: vec![],
                favorite: false,
                journal_date: None,
            },
        )
        .unwrap();
        assert!(note.plain_text.contains("参数"));
        let result = html(&db, temp.path(), &note.id, note.revision).unwrap();
        assert!(!result.contains("<script>"));
        assert!(result.contains("&lt;script&gt;"));
        let path = temp.path().join("learn.html");
        export(&db, temp.path(), &note.id, note.revision, "html", &path).unwrap();
        assert!(path.exists());
        assert!(export(&db, temp.path(), &note.id, note.revision, "html", &path).is_err());
        let md = temp.path().join("learn.md");
        crate::note_markdown::export(
            &db,
            temp.path(),
            &note.id,
            note.revision,
            &format!("![思维导图](dccmap://{map_id})"),
            &md,
        )
        .unwrap();
        assert!(!std::fs::read_to_string(md).unwrap().contains("dccmap://"));
    }
}
