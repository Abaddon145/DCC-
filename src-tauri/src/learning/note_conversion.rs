//! Pure source conversion; publication and identity checks belong to the caller.
use super::note_nodes::{LearningReference, TimeRange};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Options, Parser, Tag};
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Copy)]
pub enum AttachmentKind {
    Image,
    File,
}
pub struct AttachmentTarget {
    pub attachment_id: String,
    pub kind: AttachmentKind,
}
pub type AttachmentMap = HashMap<String, AttachmentTarget>;
pub struct MediaTarget {
    pub course_id: String,
    pub media_id: String,
    pub fingerprint: String,
    pub duration_ms: u64,
}
pub type MediaMap = HashMap<String, MediaTarget>;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservedSpan {
    pub placeholder_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub label: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionPreview {
    pub document: Option<Value>,
    pub archive_hash: String,
    pub warnings: Vec<String>,
    pub unsupported: Vec<PreservedSpan>,
    pub pending: bool,
}
pub fn convert_source_note(
    bytes: &[u8],
    attachments: &AttachmentMap,
    media: &MediaMap,
) -> Result<ConversionPreview, String> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("原始笔记超过 5 MiB，请拆分后迁入".into());
    }
    let mut preview = ConversionPreview {
        document: None,
        archive_hash: format!("{:x}", Sha256::digest(bytes)),
        warnings: Vec::new(),
        unsupported: Vec::new(),
        pending: false,
    };
    let Ok(source) = std::str::from_utf8(bytes) else {
        preview.warn("原文不是有效 UTF-8，保留原字节，尚未生成可编辑正文");
        return Ok(preview);
    };
    if source.lines().take(50_001).count() > 50_000 {
        preview.whole(source, "笔记超过 50,000 行，保留完整原文");
        return Ok(preview);
    }
    // Parse the entire source once so later reference definitions can resolve earlier links.
    // Only the BOM is skipped; every offset is translated back to the original byte slice.
    let input = source.strip_prefix('\u{feff}').unwrap_or(source);
    let base = source.len() - input.len();
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut output = Vec::new();
    let mut cursor = base;
    let mut root = 0..0;
    let mut unsupported = None;
    for (count, (event, offset)) in Parser::new_ext(input, options)
        .into_offset_iter()
        .enumerate()
    {
        if count >= 100_000 || stack.len() >= 64 && matches!(event, Event::Start(_)) {
            preview.whole(source, "笔记事件或嵌套超过安全上限，保留完整原文");
            return Ok(preview);
        }
        let range = offset.start + base..offset.end + base;
        if stack.is_empty() {
            preserve_gap(&mut preview, &mut output, source, cursor..range.start);
            root = range.clone();
            unsupported = None;
        }
        root.end = root.end.max(range.end);
        match event {
            Event::Start(tag) => {
                match &tag {
                    Tag::HtmlBlock => unsupported = Some("嵌入式 HTML"),
                    Tag::MetadataBlock(_) => unsupported = Some("原始属性区"),
                    Tag::FootnoteDefinition(_) => unsupported = Some("脚注"),
                    Tag::DefinitionList
                    | Tag::DefinitionListTitle
                    | Tag::DefinitionListDefinition => unsupported = Some("定义列表"),
                    Tag::BlockQuote(Some(_)) => unsupported = Some("特殊引用块"),
                    Tag::Heading {
                        id, classes, attrs, ..
                    } if id.is_some() || !classes.is_empty() || !attrs.is_empty() => {
                        unsupported = Some("标题特殊属性")
                    }
                    Tag::Table(alignments) if alignments.iter().any(|v| *v != Alignment::None) => {
                        unsupported = Some("表格对齐属性")
                    }
                    Tag::Link {
                        link_type: LinkType::WikiLink { .. },
                        ..
                    }
                    | Tag::Image {
                        link_type: LinkType::WikiLink { .. },
                        ..
                    } => unsupported = Some("Wiki 语法"),
                    _ => {}
                }
                stack.push(Frame {
                    tag,
                    children: Vec::new(),
                    checked: None,
                });
            }
            Event::End(_) => {
                let frame = stack.pop().ok_or("Markdown 事件未配对")?;
                if unsupported.is_none() {
                    let header = stack
                        .iter()
                        .any(|frame| matches!(frame.tag, Tag::TableHead));
                    match frame.finish(header, attachments, media) {
                        Ok(nodes) => {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.extend(nodes);
                            } else {
                                output.extend(nodes);
                            }
                        }
                        Err(reason) => unsupported = Some(reason),
                    }
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => unsupported = Some("嵌入式 HTML"),
            Event::InlineMath(_) | Event::DisplayMath(_) => unsupported = Some("公式"),
            Event::FootnoteReference(_) => unsupported = Some("脚注"),
            Event::TaskListMarker(checked) => {
                if let Some(item) = stack
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame.tag, Tag::Item))
                {
                    item.checked = Some(checked);
                }
            }
            Event::Text(text) if unsupported.is_none() => {
                push_inline(&mut stack, text_node(&text, None))
            }
            Event::Code(text) if unsupported.is_none() => {
                push_inline(&mut stack, text_node(&text, Some(json!({"type":"code"}))))
            }
            Event::SoftBreak if unsupported.is_none() => {
                push_inline(&mut stack, text_node("\n", None))
            }
            Event::HardBreak if unsupported.is_none() => {
                push_inline(&mut stack, json!({"type":"hardBreak"}))
            }
            Event::Rule if unsupported.is_none() => {
                let node = json!({"type":"horizontalRule"});
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    output.push(node);
                }
            }
            _ => {}
        }
        if stack.is_empty() {
            if let Some(reason) = unsupported {
                // A closing fence's parser range excludes its line ending. Keep that
                // original CRLF/LF in the archived block rather than regenerating it.
                if !source[..root.end].ends_with('\n') {
                    let tail = &source[root.end..];
                    if tail.starts_with("\r\n") {
                        root.end += 2;
                    } else if tail.starts_with('\n') {
                        root.end += 1;
                    }
                }
                // The outer block is replaced as a whole, never just the unrecognised inline tag.
                output.push(preview.preserve(root.clone(), reason));
            }
            cursor = root.end;
        }
    }
    preserve_gap(&mut preview, &mut output, source, cursor..source.len());
    if output.is_empty() {
        output.push(json!({"type":"paragraph"}));
    }
    let document = json!({"type":"doc", "content":output});
    if crate::knowledge::inspect_document(&document).is_err() {
        preview.whole(source, "转换结果超过正文限制或无法安全表示，保留完整原文");
    } else {
        preview.document = Some(document);
    }
    Ok(preview)
}

impl ConversionPreview {
    fn warn(&mut self, reason: &str) {
        self.pending = true;
        if !self.warnings.iter().any(|v| v == reason) {
            self.warnings.push(reason.into());
        }
    }
    fn preserve(&mut self, range: Range<usize>, reason: &str) -> Value {
        self.warn(reason);
        let id = uuid::Uuid::new_v4().to_string();
        let label = format!("保留原文：{reason}");
        let node = json!({"type":"preservedMarkdown","attrs":{"archiveItemId":id,"label":label}});
        self.unsupported.push(PreservedSpan {
            placeholder_id: id,
            start_byte: range.start,
            end_byte: range.end,
            label,
            reason: reason.into(),
        });
        node
    }
    fn whole(&mut self, source: &str, reason: &str) {
        self.unsupported.clear();
        self.warnings.clear();
        let node = self.preserve(0..source.len(), reason);
        self.document = Some(json!({"type":"doc","content":[node]}));
    }
}
fn preserve_gap(
    preview: &mut ConversionPreview,
    output: &mut Vec<Value>,
    source: &str,
    range: Range<usize>,
) {
    if range.start < range.end && !source[range.clone()].trim().is_empty() {
        output.push(preview.preserve(range, "未转换的源定义或语法"));
    }
}
struct Frame<'a> {
    tag: Tag<'a>,
    children: Vec<Value>,
    checked: Option<bool>,
}
fn push_inline(stack: &mut [Frame<'_>], node: Value) {
    if let Some(frame) = stack.last_mut() {
        // Merge neighbouring plain text (including soft breaks) without growing one JSON node per lexer token.
        if node["type"] == "text" {
            if let Some(last) = frame.children.last_mut() {
                if last["type"] == "text" && last.get("marks") == node.get("marks") {
                    if let Value::String(text) = &mut last["text"] {
                        text.push_str(node["text"].as_str().unwrap_or(""));
                    }
                    return;
                }
            }
        }
        frame.children.push(node);
    }
}
fn text_node(text: &str, mark: Option<Value>) -> Value {
    let mut node = json!({"type":"text","text":text});
    if let Some(mark) = mark {
        node["marks"] = json!([mark]);
    }
    node
}
fn container(kind: &str, children: Vec<Value>) -> Value {
    if children.is_empty() {
        json!({"type":kind})
    } else {
        json!({"type":kind,"content":children})
    }
}
fn is_inline(node: &Value) -> bool {
    matches!(node["type"].as_str(), Some("text" | "hardBreak"))
}
fn blocks(children: Vec<Value>) -> Vec<Value> {
    let mut output = Vec::new();
    let mut inline = Vec::new();
    for node in children {
        if is_inline(&node) {
            inline.push(node);
        } else {
            if !inline.is_empty() {
                output.push(container("paragraph", std::mem::take(&mut inline)));
            }
            output.push(node);
        }
    }
    if !inline.is_empty() {
        output.push(container("paragraph", inline));
    }
    output
}
fn mark(mut children: Vec<Value>, mark: Value) -> Vec<Value> {
    for node in &mut children {
        if node["type"] == "text" {
            let marks = node
                .as_object_mut()
                .unwrap()
                .entry("marks")
                .or_insert_with(|| json!([]));
            marks.as_array_mut().unwrap().insert(0, mark.clone());
        }
    }
    children
}
fn plain(children: &[Value]) -> String {
    children.iter().filter_map(|v| v["text"].as_str()).collect()
}
fn canonical_uuid(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id)
}
impl Frame<'_> {
    fn finish(
        self,
        header: bool,
        attachments: &AttachmentMap,
        media: &MediaMap,
    ) -> Result<Vec<Value>, &'static str> {
        let children = self.children;
        let node = match self.tag {
            Tag::Paragraph => {
                let mut nodes = blocks(children);
                if nodes.is_empty() {
                    nodes.push(container("paragraph", Vec::new()));
                }
                return Ok(nodes);
            }
            Tag::Heading { level, .. } => {
                if children.iter().any(|v| !is_inline(v)) {
                    return Err("标题中的块引用");
                }
                let mut node = container("heading", children);
                node["attrs"] = json!({"level":level as u32});
                node
            }
            Tag::BlockQuote(_) => container("blockquote", blocks(children)),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let mut tokens = info.split_whitespace();
                        let language = tokens.next().map(str::to_owned);
                        if tokens.next().is_some() {
                            return Err("代码块额外属性");
                        }
                        language
                    }
                    CodeBlockKind::Indented => None,
                };
                let mut node = container("codeBlock", children);
                node["attrs"] = json!({"language":language});
                node
            }
            Tag::Item => {
                let mut content = blocks(children);
                if content.first().is_none_or(|v| v["type"] != "paragraph") {
                    content.insert(0, container("paragraph", Vec::new()));
                }
                let mut node = container(
                    if self.checked.is_some() {
                        "taskItem"
                    } else {
                        "listItem"
                    },
                    content,
                );
                if let Some(checked) = self.checked {
                    node["attrs"] = json!({"checked":checked});
                }
                node
            }
            Tag::List(start) => {
                if start.is_some() && children.iter().any(|v| v["type"] == "taskItem") {
                    return Err("有序任务列表");
                }
                if let Some(start) = start {
                    let mut node = container("orderedList", children);
                    node["attrs"] = json!({"start":start});
                    node
                } else {
                    let mut lists: Vec<Value> = Vec::new();
                    for item in children {
                        let kind = if item["type"] == "taskItem" {
                            "taskList"
                        } else {
                            "bulletList"
                        };
                        if let Some(last) = lists.last_mut().filter(|v| v["type"] == kind) {
                            last["content"].as_array_mut().unwrap().push(item);
                        } else {
                            lists.push(container(kind, vec![item]));
                        }
                    }
                    return Ok(lists);
                }
            }
            Tag::Table(_) => container("table", children),
            Tag::TableHead | Tag::TableRow => container("tableRow", children),
            Tag::TableCell => {
                let mut content = blocks(children);
                if content.is_empty() {
                    content.push(container("paragraph", Vec::new()));
                }
                let mut node = container(if header { "tableHeader" } else { "tableCell" }, content);
                node["attrs"] = json!({"colspan":1,"rowspan":1,"colwidth":null});
                node
            }
            Tag::Emphasis => return Ok(mark(children, json!({"type":"italic"}))),
            Tag::Strong => return Ok(mark(children, json!({"type":"bold"}))),
            Tag::Strikethrough => return Ok(mark(children, json!({"type":"strike"}))),
            Tag::Link {
                dest_url, title, ..
            } => {
                if children.iter().any(|v| !is_inline(v)) {
                    return Err("链接中的块内容");
                }
                if let Some(target) = attachments.get(dest_url.as_ref()) {
                    attachment(target, &plain(&children), &title)?
                } else if dest_url.starts_with("yingliu:") {
                    reference(&dest_url, &plain(&children), media)?
                } else {
                    let parsed =
                        url::Url::parse(&dest_url).map_err(|_| "未登记的本地附件或链接")?;
                    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                        return Err("不支持的链接协议");
                    }
                    return Ok(mark(
                        children,
                        json!({"type":"link","attrs":{"href":dest_url.as_ref(),"title":if title.is_empty(){None}else{Some(title.as_ref())}}}),
                    ));
                }
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                let target = attachments
                    .get(dest_url.as_ref())
                    .ok_or("图片尚未登记为托管附件")?;
                if !matches!(target.kind, AttachmentKind::Image) {
                    return Err("图片附件类型不匹配");
                }
                attachment(target, &plain(&children), &title)?
            }
            _ => return Err("不支持的 Markdown 结构"),
        };
        Ok(vec![node])
    }
}
fn attachment(target: &AttachmentTarget, label: &str, title: &str) -> Result<Value, &'static str> {
    if !canonical_uuid(&target.attachment_id) {
        return Err("附件身份无效");
    }
    Ok(match target.kind {
        AttachmentKind::Image => {
            json!({"type":"image","attrs":{"src":format!("http://dcc-media.localhost/note/{}",target.attachment_id),"alt":label,"title":if title.is_empty(){None}else{Some(title)}}})
        }
        AttachmentKind::File => {
            json!({"type":"attachment","attrs":{"attachmentId":target.attachment_id,"label":if label.is_empty(){"附件"}else{label}}})
        }
    })
}
fn reference(destination: &str, label: &str, media: &MediaMap) -> Result<Value, &'static str> {
    let url = url::Url::parse(destination).map_err(|_| "课程链接格式无效")?;
    let query = url.query_pairs().collect::<Vec<_>>();
    if url.scheme() != "yingliu"
        || url.host_str() != Some("video")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || query.len() != 1
        || query[0].0 != "t"
        || query[0].1.is_empty()
        || !query[0].1.bytes().all(|v| v.is_ascii_digit())
    {
        return Err("课程时间点格式无效");
    }
    let id = url.path().strip_prefix('/').ok_or("课程身份缺失")?;
    // URL parsers normalise dot segments and percent-encoded query keys. Neither is
    // permission to resolve a different source identity than the literal saved link.
    let (literal_id, literal_time) = destination
        .strip_prefix("yingliu://video/")
        .and_then(|v| v.split_once("?t="))
        .ok_or("课程链接格式无效")?;
    if literal_id != id || literal_time != query[0].1 {
        return Err("课程链接需保留明确的来源身份与时间");
    }
    let target = media.get(id).ok_or("课程身份尚未映射")?;
    let start = query[0].1.parse::<u64>().map_err(|_| "课程时间点无效")?;
    let end = start
        .checked_add(1)
        .filter(|end| *end <= target.duration_ms)
        .ok_or("课程时间点超出时长")?;
    let value = LearningReference {
        course_id: target.course_id.clone(),
        media_id: target.media_id.clone(),
        fingerprint: target.fingerprint.clone(),
        ranges: vec![TimeRange {
            start_ms: start,
            end_ms: end,
        }],
        label: if label.is_empty() {
            "课程时间点".into()
        } else {
            label.into()
        },
    };
    value
        .validate()
        .map_err(|_| "课程引用身份、摘要或范围无效")?;
    Ok(json!({"type":"learningReference","attrs":value}))
}
