use super::note_conversion::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const IMAGE: &str = "596d257d-3e5c-46c4-a464-187cf180c6bd";
const FILE: &str = "cdf480cf-7779-4557-a1c2-74a5d7676efc";
const COURSE: &str = "cedbfc58-32fd-4bd2-a5c1-fd0c707edfe6";
const MEDIA: &str = "5804245d-e1d3-4297-b08c-053a123706bd";
fn convert(text: &str) -> ConversionPreview {
    convert_source_note(text.as_bytes(), &AttachmentMap::new(), &MediaMap::new()).unwrap()
}
fn nodes<'a>(document: &'a Value, kind: &str) -> Vec<&'a Value> {
    let mut stack = vec![document];
    let mut found = Vec::new();
    while let Some(node) = stack.pop() {
        if node["type"] == kind {
            found.push(node);
        }
        if let Some(children) = node["content"].as_array() {
            stack.extend(children.iter().rev());
        }
    }
    found
}
fn doc(preview: &ConversionPreview) -> &Value {
    let doc = preview.document.as_ref().unwrap();
    crate::knowledge::inspect_document(doc).unwrap();
    doc
}
fn assert_raw(preview: &ConversionPreview, source: &str, needle: &str) {
    assert!(preview.pending);
    assert!(!preview.warnings.is_empty());
    let pieces = preview
        .unsupported
        .iter()
        .map(|span| {
            assert_eq!(
                uuid::Uuid::parse_str(&span.placeholder_id)
                    .unwrap()
                    .to_string(),
                span.placeholder_id
            );
            assert!(span.start_byte < span.end_byte);
            assert!(!span.reason.is_empty());
            &source[span.start_byte..span.end_byte]
        })
        .collect::<Vec<_>>();
    assert!(
        pieces.iter().any(|piece| piece.contains(needle)),
        "missing raw {needle:?} in {} preserved spans",
        pieces.len()
    );
    assert_eq!(
        nodes(doc(preview), "preservedMarkdown").len(),
        preview.unsupported.len()
    );
    for span in &preview.unsupported {
        assert!(nodes(doc(preview), "preservedMarkdown")
            .iter()
            .any(|node| node["attrs"]["archiveItemId"] == span.placeholder_id));
    }
}

#[test]
fn learning_note_conversion_commonmark_keeps_structure_marks_and_literal_code() {
    let preview = convert("# UE5\n\n正文 **粗体** *斜体* ~~删除~~ `inline` [官方](https://example.com)\n换行  \n硬换行\n\n> 引用\n\n- 第一\n  - 下级\n\n3. 有序\n4. 继续\n\n- [ ] 待办\n- [x] 完成\n\n| A | B |\n| --- | --- |\n| 内容 | **加粗** |\n\n```html\n<script>literal()</script>\n```\n\n---\n");
    assert!(!preview.pending, "{:?}", preview.warnings);
    let value = doc(&preview);
    for kind in [
        "heading",
        "blockquote",
        "bulletList",
        "orderedList",
        "taskList",
        "table",
        "hardBreak",
        "horizontalRule",
    ] {
        assert!(!nodes(value, kind).is_empty(), "missing {kind}: {value}");
    }
    assert_eq!(nodes(value, "orderedList")[0]["attrs"]["start"], 3);
    assert_eq!(nodes(value, "taskItem")[0]["attrs"]["checked"], false);
    assert_eq!(nodes(value, "taskItem")[1]["attrs"]["checked"], true);
    assert_eq!(nodes(value, "codeBlock")[0]["attrs"]["language"], "html");
    assert!(nodes(value, "codeBlock")[0]
        .to_string()
        .contains("<script>literal()</script>"));
    let text_nodes = nodes(value, "text");
    for mark in ["bold", "italic", "strike", "code", "link"] {
        assert!(
            text_nodes.iter().any(|node| node["marks"]
                .as_array()
                .is_some_and(|marks| marks.iter().any(|v| v["type"] == mark))),
            "missing mark {mark}"
        );
    }
}

#[test]
fn learning_note_conversion_preserves_unsupported_outer_blocks_and_original_offsets() {
    for raw in [
        "<script>alert(1)</script>\r\n",
        "正文 <b>HTML</b>\r\n",
        "<!-- 注释 😀 -->\r\n",
        "---\r\nname: test\r\n---\r\n",
        "[[概念|链接]]\r\n",
        "![[图.png]]\r\n",
        "$E=mc^2$\r\n",
        "> [!NOTE]\r\n> 特殊引用\r\n",
        "| A |\r\n| :---: |\r\n| 对齐 |\r\n",
    ] {
        let source = format!("{raw}\r\n# 后续\r\n\r\n普通正文\r\n");
        let preview = convert(&source);
        assert_raw(&preview, &source, raw.trim_end());
        assert_eq!(
            nodes(doc(&preview), "heading")[0]["content"][0]["text"],
            "后续"
        );
        for pair in preview.unsupported.windows(2) {
            assert!(pair[0].end_byte <= pair[1].start_byte);
        }
    }
    let source = "\u{feff}# 😀 开始\r\n\r\n前😀 <b>原文</b>\r\n\r\n结束\r\n";
    let preview = convert(source);
    assert_raw(&preview, source, "前😀 <b>原文</b>\r\n");
    assert_eq!(
        preview.archive_hash,
        format!("{:x}", Sha256::digest(source.as_bytes()))
    );
    assert_eq!(
        nodes(doc(&preview), "heading")[0]["content"][0]["text"],
        "😀 开始"
    );
    // A footnote reference paragraph and its definition are two distinct outer blocks.
    let source = "正文[^a]\r\n\r\n[^a]: 注脚\r\n";
    let preview = convert(source);
    assert_raw(&preview, source, "正文[^a]\r\n");
    assert_raw(&preview, source, "[^a]: 注脚\r\n");
}

#[test]
fn learning_note_conversion_attachment_mapping_is_explicit_and_no_remote_downloads() {
    let attachments = AttachmentMap::from([
        (
            "images/a.png".into(),
            AttachmentTarget {
                attachment_id: IMAGE.into(),
                kind: AttachmentKind::Image,
            },
        ),
        (
            "docs/file.pdf".into(),
            AttachmentTarget {
                attachment_id: FILE.into(),
                kind: AttachmentKind::File,
            },
        ),
    ]);
    let source = "之前 ![示意](images/a.png \"图\") 之后\n\n[课件](docs/file.pdf)\n";
    let preview = convert_source_note(source.as_bytes(), &attachments, &MediaMap::new()).unwrap();
    assert!(!preview.pending);
    let value = doc(&preview);
    assert_eq!(
        nodes(value, "image")[0]["attrs"]["src"],
        format!("http://dcc-media.localhost/note/{IMAGE}")
    );
    assert_eq!(nodes(value, "image")[0]["attrs"]["alt"], "示意");
    assert_eq!(nodes(value, "attachment")[0]["attrs"]["attachmentId"], FILE);
    assert_eq!(
        value["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["paragraph", "image", "paragraph", "attachment"]
    );
    for raw in [
        "![缺失](images/missing.png)",
        "![远程](https://example.com/a.png)",
        "[文件](file:///c:/secret.txt)",
        "[脚本](javascript:alert%281%29)",
    ] {
        assert_raw(&convert(raw), raw, raw);
    }
    let bad = AttachmentMap::from([(
        "images/a.png".into(),
        AttachmentTarget {
            attachment_id: "not-uuid".into(),
            kind: AttachmentKind::Image,
        },
    )]);
    let raw = "![示意](images/a.png)";
    assert_raw(
        &convert_source_note(raw.as_bytes(), &bad, &MediaMap::new()).unwrap(),
        raw,
        raw,
    );
}

#[test]
fn learning_note_conversion_time_point_requires_exact_trusted_media_identity() {
    let media = MediaMap::from([(
        "video-1".into(),
        MediaTarget {
            course_id: COURSE.into(),
            media_id: MEDIA.into(),
            fingerprint: "a".repeat(64),
            duration_ms: 90_000,
        },
    )]);
    let source = "前文 [关键帧](yingliu://video/video-1?t=12000) 后文\n";
    let preview = convert_source_note(source.as_bytes(), &AttachmentMap::new(), &media).unwrap();
    assert!(!preview.pending, "{:?}", preview.warnings);
    let node = nodes(doc(&preview), "learningReference")[0];
    assert_eq!(node["attrs"]["courseId"], COURSE);
    assert_eq!(node["attrs"]["mediaId"], MEDIA);
    assert_eq!(node["attrs"]["fingerprint"], "a".repeat(64));
    assert_eq!(
        node["attrs"]["ranges"],
        json!([{"startMs":12000,"endMs":12001}])
    );
    for url in [
        "yingliu://video/missing?t=1",
        "yingliu://video/video-1?t=90000",
        "yingliu://video/video-1?t=-1",
        "yingliu://video/video-1?t=12.5",
        "yingliu://video/video-1?t=1&t=2",
        "yingliu://video/video-1?t=1&other=1",
        "yingliu://video/discard/../video-1?t=1",
        "yingliu://video/video-1?%74=1",
        "yingliu://video/video-1?t=%31",
    ] {
        let raw = format!("[时间点]({url})");
        assert_raw(
            &convert_source_note(raw.as_bytes(), &AttachmentMap::new(), &media).unwrap(),
            &raw,
            &raw,
        );
    }
}

#[test]
fn learning_note_conversion_unsupported_nested_content_keeps_the_entire_list_or_quote() {
    for raw in [
        "- 已知\r\n- 子项\r\n  - 特殊 <b>HTML</b>\r\n",
        "> 已知\r\n>\r\n> [[不支持]]\r\n",
        "```hlsl title=shader.hlsl\r\nfloat x;\r\n```\r\n",
    ] {
        let source = format!("# 前面\r\n\r\n{raw}\r\n# 后面\r\n");
        let preview = convert(&source);
        assert_raw(&preview, &source, raw);
        assert_eq!(preview.unsupported.len(), 1);
        assert_eq!(nodes(doc(&preview), "heading").len(), 2);
    }
    // Explicit maps cannot silently discard the outer click target of a linked image.
    let source = "[![图](a.png)](https://example.com/page)";
    let attachments = AttachmentMap::from([(
        "a.png".into(),
        AttachmentTarget {
            attachment_id: IMAGE.into(),
            kind: AttachmentKind::Image,
        },
    )]);
    let preview = convert_source_note(source.as_bytes(), &attachments, &MediaMap::new()).unwrap();
    assert_raw(&preview, source, source);
}

#[test]
fn learning_note_conversion_reference_definitions_resolve_across_blocks_without_losing_raw() {
    let source = "[官方][later]\r\n\r\n正文\r\n\r\n[later]: https://example.com \"引用标题\"\r\n";
    let preview = convert(source);
    let text = nodes(doc(&preview), "text");
    assert!(text
        .iter()
        .any(|node| node["text"] == "官方"
            && node["marks"][0]["attrs"]["href"] == "https://example.com"));
    assert_raw(
        &preview,
        source,
        "[later]: https://example.com \"引用标题\"",
    );
}

#[test]
fn learning_note_conversion_large_plain_text_is_converted_but_invalid_encoding_is_not_faked() {
    let source = "600KiB😀 ".repeat(60_000);
    assert!(source.len() > 600 * 1024);
    let preview = convert(&source);
    assert!(!preview.pending);
    assert_eq!(
        crate::knowledge::inspect_document(doc(&preview))
            .unwrap()
            .0
            .trim(),
        source.trim()
    );
    let invalid =
        convert_source_note(&[0xff, 0, 0x80], &AttachmentMap::new(), &MediaMap::new()).unwrap();
    assert!(invalid.pending && invalid.document.is_none());
    assert_eq!(
        invalid.archive_hash,
        format!("{:x}", Sha256::digest([0xff, 0, 0x80]))
    );
    assert!(convert_source_note(
        &vec![b'a'; 5 * 1024 * 1024 + 1],
        &AttachmentMap::new(),
        &MediaMap::new()
    )
    .is_err());
    assert_eq!(doc(&convert(""))["content"][0]["type"], "paragraph");
}

#[test]
fn learning_note_conversion_structure_limits_preserve_the_whole_body() {
    for source in [
        "x\n".repeat(50_001),
        format!("{}deep\n", "> ".repeat(70)),
        "\"\\".repeat(1_100_000),
        "*x* ".repeat(30_000),
    ] {
        let preview = convert(&source);
        assert_raw(&preview, &source, &source);
        assert_eq!(preview.unsupported.len(), 1);
        assert_eq!(preview.unsupported[0].start_byte, 0);
        assert_eq!(preview.unsupported[0].end_byte, source.len());
    }
}

#[test]
fn learning_note_conversion_matches_manual_import_common_fixtures() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../src/lib/fixtures/learningMarkdown.json"
    ))
    .unwrap();
    fn semantic(node: &Value) -> Value {
        let mut out = json!({"type":node["type"]});
        let key = match node["type"].as_str() {
            Some("heading") => Some("level"),
            Some("orderedList") => Some("start"),
            Some("taskItem") => Some("checked"),
            _ => None,
        };
        if let Some(key) = key {
            out["attrs"] = json!({key:node["attrs"][key]});
        }
        if let Some(text) = node.get("text") {
            out["text"] = text.clone();
        }
        if let Some(children) = node["content"].as_array().filter(|v| !v.is_empty()) {
            out["content"] = json!(children.iter().map(semantic).collect::<Vec<_>>());
        }
        if let Some(marks) = node["marks"].as_array().filter(|v| !v.is_empty()) {
            out["marks"] = json!(marks
                .iter()
                .map(|mark| if mark["type"] == "link" {
                    json!({"type":"link","attrs":{"href":mark["attrs"]["href"]}})
                } else {
                    json!({"type":mark["type"]})
                })
                .collect::<Vec<_>>());
        }
        out
    }
    for fixture in fixtures.as_array().unwrap() {
        let preview = convert(fixture["markdown"].as_str().unwrap());
        assert!(!preview.pending);
        assert_eq!(
            semantic(doc(&preview)),
            fixture["document"],
            "{}",
            fixture["name"]
        );
    }
}
