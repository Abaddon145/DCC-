use crate::models::{LocalizedAssetText, TranslationPreview, TranslationRequest};
use std::collections::HashMap;

pub fn has_han(value: &str) -> bool {
    value
        .chars()
        .any(|ch| matches!(ch as u32, 0x3400..=0x9fff | 0xf900..=0xfaff))
}

pub fn fab_localizations(fields: LocalizedAssetText) -> HashMap<String, LocalizedAssetText> {
    let mut result = HashMap::new();
    let mut zh = LocalizedAssetText::default();
    let mut en = LocalizedAssetText::default();
    for (value, zh_slot, en_slot) in [
        (&fields.name, &mut zh.name, &mut en.name),
        (
            &fields.description,
            &mut zh.description,
            &mut en.description,
        ),
        (&fields.license, &mut zh.license, &mut en.license),
    ] {
        if has_han(value) {
            *zh_slot = value.clone();
        } else {
            *en_slot = value.clone();
        }
    }
    for tag in fields.tags {
        if has_han(&tag) {
            zh.tags.push(tag);
        } else {
            en.tags.push(tag);
        }
    }
    result.insert("zh-CN".into(), zh);
    result.insert("en".into(), en);
    result
}

pub fn missing_request(
    localizations: &HashMap<String, LocalizedAssetText>,
    source: &str,
    target: &str,
) -> Option<TranslationRequest> {
    let source_fields = localizations.get(source)?;
    let target_fields = localizations.get(target).cloned().unwrap_or_default();
    let fields = LocalizedAssetText {
        name: if target_fields.name.trim().is_empty() {
            source_fields.name.clone()
        } else {
            String::new()
        },
        description: if target_fields.description.trim().is_empty() {
            source_fields.description.clone()
        } else {
            String::new()
        },
        tags: if target_fields.tags.is_empty() {
            source_fields.tags.clone()
        } else {
            Vec::new()
        },
        license: if target_fields.license.trim().is_empty() {
            source_fields.license.clone()
        } else {
            String::new()
        },
    };
    if fields.name.trim().is_empty()
        && fields.description.trim().is_empty()
        && fields.tags.is_empty()
        && fields.license.trim().is_empty()
    {
        return None;
    }
    Some(TranslationRequest {
        source_language: source.into(),
        target_language: target.into(),
        fields,
    })
}

pub fn merge_preview(
    localizations: &mut HashMap<String, LocalizedAssetText>,
    target: &str,
    preview: TranslationPreview,
) -> Vec<String> {
    let mut warnings = preview.warnings;
    let translated = preview.fields;
    let existing = localizations.entry(target.into()).or_default();
    if existing.name.trim().is_empty() && !translated.name.trim().is_empty() {
        existing.name = translated.name;
    }
    if existing.description.trim().is_empty() && !translated.description.trim().is_empty() {
        existing.description = translated.description;
    }
    if existing.tags.is_empty() && !translated.tags.is_empty() {
        existing.tags = translated.tags;
    }
    if existing.license.trim().is_empty() && !translated.license.trim().is_empty() {
        existing.license = translated.license;
    }
    warnings.extend(
        preview
            .failed_fields
            .into_iter()
            .map(|field| format!("{field} 翻译失败")),
    );
    warnings
}

pub fn fill_empty(existing: &mut LocalizedAssetText, translated: LocalizedAssetText) -> bool {
    let mut changed = false;
    if existing.name.trim().is_empty() && !translated.name.trim().is_empty() {
        existing.name = translated.name;
        changed = true;
    }
    if existing.description.trim().is_empty() && !translated.description.trim().is_empty() {
        existing.description = translated.description;
        changed = true;
    }
    if existing.tags.is_empty() && !translated.tags.is_empty() {
        existing.tags = translated.tags;
        changed = true;
    }
    if existing.license.trim().is_empty() && !translated.license.trim().is_empty() {
        existing.license = translated.license;
        changed = true;
    }
    changed
}

pub fn pending_fields(localizations: &HashMap<String, LocalizedAssetText>) -> Vec<String> {
    let mut pending = Vec::new();
    for (source, target, label) in [("en", "zh-CN", "中文"), ("zh-CN", "en", "英文")] {
        if let Some(request) = missing_request(localizations, source, target) {
            if !request.fields.name.trim().is_empty() {
                pending.push(format!("{label}名称"));
            }
            if !request.fields.description.trim().is_empty() {
                pending.push(format!("{label}描述"));
            }
            if !request.fields.tags.is_empty() {
                pending.push(format!("{label}标签"));
            }
            if !request.fields.license.trim().is_empty() {
                pending.push(format!("{label}许可"));
            }
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fab_fields_are_not_mislabeled_as_english_when_source_is_chinese() {
        let localized = fab_localizations(LocalizedAssetText {
            name: "沙漠场景".into(),
            description: "Desert environment".into(),
            tags: vec!["Landscape".into(), "沙漠".into()],
            license: String::new(),
        });
        assert_eq!(localized["zh-CN"].name, "沙漠场景");
        assert_eq!(localized["en"].description, "Desert environment");
        assert_eq!(localized["zh-CN"].tags, ["沙漠"]);
        assert!(pending_fields(&localized).contains(&"英文名称".to_string()));
    }

    #[test]
    fn partial_translation_keeps_existing_content_and_remains_pending() {
        let mut localized = HashMap::from([
            (
                "en".into(),
                LocalizedAssetText {
                    name: "Skeletal Mesh".into(),
                    description: "Mesh description".into(),
                    ..Default::default()
                },
            ),
            (
                "zh-CN".into(),
                LocalizedAssetText {
                    name: "手工名称".into(),
                    ..Default::default()
                },
            ),
        ]);
        let request = missing_request(&localized, "en", "zh-CN").unwrap();
        assert!(request.fields.name.is_empty());
        assert_eq!(request.fields.description, "Mesh description");
        merge_preview(
            &mut localized,
            "zh-CN",
            TranslationPreview {
                fields: LocalizedAssetText {
                    name: "错误覆盖".into(),
                    ..Default::default()
                },
                failed_fields: vec!["description".into()],
                ..Default::default()
            },
        );
        assert_eq!(localized["zh-CN"].name, "手工名称");
        assert_eq!(pending_fields(&localized), ["中文描述"]);
    }
}
