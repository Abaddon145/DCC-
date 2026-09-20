use crate::models::{FabMetadata, FabPreviewImage};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use reqwest::{header, Client};
use serde_json::Value;
use std::{collections::HashSet, fs, path::PathBuf, time::Duration};
use url::Url;
use uuid::Uuid;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;
const MAX_PREVIEW_IMAGES: usize = 30;

pub async fn fetch_metadata(raw_url: &str) -> Result<FabMetadata, String> {
    let listing_id = listing_id_from_url(raw_url)?;
    let canonical_url = format!("https://www.fab.com/listings/{listing_id}");
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("无法初始化网络请求：{error}"))?;

    let api_url = format!("https://www.fab.com/i/listings/{listing_id}");
    let listing = get_json(&client, &api_url, &canonical_url).await?;
    let mut metadata = metadata_from_listing(&listing, &canonical_url)?;

    if let Some(asset_formats) = listing.get("assetFormats").and_then(Value::as_array) {
        for asset_format in asset_formats.iter().take(12) {
            let Some(code) = asset_format
                .pointer("/assetFormatType/code")
                .and_then(Value::as_str)
                .filter(|code| !code.is_empty())
            else {
                continue;
            };
            let detail_url =
                format!("https://www.fab.com/i/listings/{listing_id}/asset-formats/{code}");
            if let Ok(detail) = get_json(&client, &detail_url, &canonical_url).await {
                collect_versions(&detail, &mut metadata.versions);
            }
        }
    }
    metadata.versions = unique(metadata.versions);
    let candidates = preview_candidates(&listing);
    let (preview_images, failed) = download_previews(&client, &candidates).await;
    metadata.preview_images = preview_images;
    metadata.image_warning = if failed > 0 || candidates.len() > MAX_PREVIEW_IMAGES {
        let omitted = candidates.len().saturating_sub(MAX_PREVIEW_IMAGES);
        Some(format!(
            "{} 张图片读取失败{}",
            failed,
            if omitted > 0 {
                format!("，另有 {omitted} 张超过单次导入上限")
            } else {
                String::new()
            }
        ))
    } else {
        None
    };
    Ok(metadata)
}

pub fn listing_id_from_url(raw_url: &str) -> Result<Uuid, String> {
    let parsed = Url::parse(raw_url.trim()).map_err(|_| "请输入完整的 Fab 商品网址")?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Fab 网址必须使用 http 或 https".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "fab.com" && host != "www.fab.com" {
        return Err("只支持 fab.com 的商品网址".into());
    }
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let id = segments
        .windows(2)
        .find(|parts| parts[0] == "listings")
        .map(|parts| parts[1])
        .ok_or("网址中没有找到 Fab 商品 ID")?;
    Uuid::parse_str(id).map_err(|_| "Fab 商品 ID 格式无效".into())
}

async fn get_json(client: &Client, url: &str, referer: &str) -> Result<Value, String> {
    let response = client
        .get(url)
        .header(header::ACCEPT, "application/json, text/plain, */*")
        .header(header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.7")
        .header(header::REFERER, referer)
        .header("X-Requested-With", "XMLHttpRequest")
        .header(
            header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/140.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|error| format!("连接 Fab 失败：{error}"))?;
    if response.status().is_redirection() {
        return Err("Fab 返回了不安全的重定向，已停止读取".into());
    }
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取 Fab 数据失败：{error}"))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("Fab 返回的数据过大，已停止读取".into());
    }
    let body = String::from_utf8_lossy(&bytes);
    if body.contains("cf_challenge") || body.contains("challenge-platform") {
        return Err("Fab 要求安全验证，暂时无法自动读取；请稍后重试或更换网络".into());
    }
    if !status.is_success() {
        return Err(format!("Fab 返回错误状态：{status}"));
    }
    serde_json::from_slice(&bytes).map_err(|_| "Fab 返回的数据格式无法识别".into())
}

fn metadata_from_listing(value: &Value, canonical_url: &str) -> Result<FabMetadata, String> {
    let name = string_at(value, "/title");
    if name.is_empty() {
        return Err("Fab 商品数据中缺少名称".into());
    }
    let category = string_at(value, "/category/name");
    let category_path = string_at(value, "/category/path");
    let listing_type = string_at(value, "/listingType");
    let suggested_category_path = classify_category(&category_path, &category, &listing_type);
    let listing_id = listing_id_from_url(canonical_url)?.to_string();
    let mut tags = names_from_array(value.get("tags"));
    if !category.is_empty() {
        tags.push(category.clone());
    }

    let mut dcc_tools = Vec::new();
    let mut formats = Vec::new();
    if let Some(items) = value.get("assetFormats").and_then(Value::as_array) {
        for item in items {
            if let Some(format_type) = item.get("assetFormatType") {
                push_string(format_type.get("name"), &mut dcc_tools);
                if let Some(extensions) = format_type.get("extensions").and_then(Value::as_array) {
                    for extension in extensions {
                        push_string(Some(extension), &mut formats);
                    }
                }
            }
        }
    }

    Ok(FabMetadata {
        canonical_url: canonical_url.to_string(),
        listing_id,
        category_path,
        listing_type,
        suggested_category_path,
        name,
        description: html_to_text(&string_at(value, "/description")),
        author: string_at(value, "/user/sellerName"),
        category,
        tags: unique(tags),
        dcc_tools: unique(dcc_tools),
        versions: Vec::new(),
        formats: unique(formats),
        license: license_name(value),
        preview_images: Vec::new(),
        image_warning: None,
    })
}

pub fn classify_category(
    category_path: &str,
    category_name: &str,
    listing_type: &str,
) -> Vec<String> {
    let first = category_path
        .split('/')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_ascii_lowercase();
    let listing_type = listing_type.trim().to_ascii_lowercase();
    let root = match first.as_str() {
        "environments" | "buildings-architecture" | "nature-plants" | "landscapes" => Some("环境"),
        "characters-creatures" | "clothing-accessories" | "metahuman" | "characters" => {
            Some("角色与生物")
        }
        "tools-objects-decor"
        | "electronics-technology"
        | "food-drink"
        | "furniture-fixtures"
        | "props" => Some("道具"),
        "vehicles-transportation" | "vehicles" => Some("载具"),
        "weapons-combat" | "weapons" => Some("武器"),
        "material" | "materials" | "materials-textures" | "textures" | "decal" | "decals" => {
            Some("材质与纹理")
        }
        "animation" | "animations" => Some("动画"),
        "audio" | "music" | "sound-effects" => Some("音频"),
        "vfx" | "visual-effects" => Some("特效"),
        "ui" | "2d" | "2d-assets" | "sprites-flipbooks" | "brushes" => Some("UI与2D"),
        "hdri" | "hdris" => Some("HDRI与灯光"),
        "game-systems" | "game-templates" => Some("游戏系统与模板"),
        "tools-plugins" | "tools-and-plugins" | "plugins" => Some("工具与插件"),
        "tutorials-examples" | "education-tutorials" | "tutorials" => Some("教程与示例"),
        _ => match listing_type.as_str() {
            "material" | "texture" | "decal" => Some("材质与纹理"),
            "animation" => Some("动画"),
            "audio" => Some("音频"),
            "vfx" => Some("特效"),
            "ui" | "2d-asset" | "sprite" | "brush" => Some("UI与2D"),
            "hdri" => Some("HDRI与灯光"),
            "game-system" | "game-template" => Some("游戏系统与模板"),
            "tool" | "plugin" => Some("工具与插件"),
            "tutorial" => Some("教程与示例"),
            "metahuman" => Some("角色与生物"),
            _ => None,
        },
    };
    let Some(root) = root else {
        return vec!["其他".into(), "待整理".into()];
    };
    let mut result = vec![root.to_string()];
    let has_detail = category_path
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .count()
        > 1;
    let detail = category_name.trim();
    let safe_detail = !detail.is_empty()
        && detail.chars().count() <= 60
        && !detail
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
        && !detail.eq_ignore_ascii_case(root);
    if has_detail && safe_detail {
        result.push(detail.to_string());
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewCandidate {
    url: String,
    name: String,
}

fn preview_candidates(listing: &Value) -> Vec<PreviewCandidate> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for group_name in ["thumbnails", "medias"] {
        let Some(items) = listing.get(group_name).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let media_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
            if media_type != "image" && media_type != "thumbnail" {
                continue;
            }
            let selected = item
                .get("images")
                .and_then(Value::as_array)
                .and_then(|images| {
                    images
                        .iter()
                        .filter(|image| {
                            image.get("width").and_then(Value::as_u64).unwrap_or(0) <= 1600
                        })
                        .max_by_key(|image| image.get("width").and_then(Value::as_u64).unwrap_or(0))
                        .and_then(|image| image.get("url").and_then(Value::as_str))
                })
                .or_else(|| item.get("mediaUrl").and_then(Value::as_str));
            let Some(url) = selected.filter(|url| allowed_image_url(url)) else {
                continue;
            };
            let identity = item
                .get("previewUid")
                .and_then(Value::as_str)
                .unwrap_or(url)
                .to_string();
            if !seen.insert(identity) {
                continue;
            }
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or("fab-preview.jpg")
                .to_string();
            candidates.push(PreviewCandidate {
                url: url.to_string(),
                name,
            });
        }
    }
    candidates
}

fn allowed_image_url(raw: &str) -> bool {
    Url::parse(raw)
        .ok()
        .is_some_and(|url| url.scheme() == "https" && url.host_str() == Some("media.fab.com"))
}

async fn download_previews(
    client: &Client,
    candidates: &[PreviewCandidate],
) -> (Vec<FabPreviewImage>, usize) {
    let cache_dir = match create_cache_dir() {
        Ok(path) => path,
        Err(_) => return (Vec::new(), candidates.len().min(MAX_PREVIEW_IMAGES)),
    };
    let mut images = Vec::new();
    let mut failed = 0;
    for (index, candidate) in candidates.iter().take(MAX_PREVIEW_IMAGES).enumerate() {
        match download_preview(client, candidate, &cache_dir, index).await {
            Ok(image) => images.push(image),
            Err(_) => failed += 1,
        }
    }
    (images, failed)
}

async fn download_preview(
    client: &Client,
    candidate: &PreviewCandidate,
    cache_dir: &PathBuf,
    index: usize,
) -> Result<FabPreviewImage, String> {
    let response = client
        .get(&candidate.url)
        .header(header::ACCEPT, "image/avif,image/webp,image/png,image/jpeg,image/*;q=0.8")
        .header(header::REFERER, "https://www.fab.com/")
        .header(header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/140.0 Safari/537.36")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("图片返回状态 {}", response.status()));
    }
    if response.content_length().unwrap_or(0) > MAX_IMAGE_BYTES as u64 {
        return Err("图片超过大小限制".into());
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg")
        .split(';')
        .next()
        .unwrap_or("image/jpeg")
        .to_ascii_lowercase();
    let extension = match content_type.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/jpeg" | "image/jpg" => "jpg",
        _ => return Err("响应不是受支持的图片".into()),
    };
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("图片为空或超过大小限制".into());
    }
    image::load_from_memory(&bytes).map_err(|_| "图片内容无效".to_string())?;
    let path = cache_dir.join(format!("{:02}-{}.{}", index + 1, Uuid::new_v4(), extension));
    fs::write(&path, &bytes).map_err(|error| format!("写入图片缓存失败：{error}"))?;
    Ok(FabPreviewImage {
        source_path: path.to_string_lossy().into_owned(),
        preview_data_url: format!("data:{content_type};base64,{}", STANDARD.encode(&bytes)),
        original_name: candidate.name.clone(),
        remote_url: candidate.url.clone(),
    })
}

fn create_cache_dir() -> Result<PathBuf, String> {
    let root = std::env::temp_dir()
        .join("DCCAssetLibrary")
        .join("fab-import");
    fs::create_dir_all(&root).map_err(|error| format!("创建图片缓存失败：{error}"))?;
    let directory = root.join(Uuid::new_v4().to_string());
    fs::create_dir(&directory).map_err(|error| format!("创建图片缓存失败：{error}"))?;
    Ok(directory)
}

fn collect_versions(detail: &Value, output: &mut Vec<String>) {
    let Some(versions) = detail.get("versions").and_then(Value::as_array) else {
        return;
    };
    for artifact in versions {
        let Some(engine_versions) = artifact.get("engineVersions").and_then(Value::as_array) else {
            continue;
        };
        for version in engine_versions {
            if let Some(raw) = version.as_str() {
                let cleaned = raw
                    .strip_prefix("UE_")
                    .or_else(|| raw.strip_prefix("UE "))
                    .unwrap_or(raw)
                    .replace('_', ".");
                if !cleaned.is_empty() {
                    output.push(cleaned);
                }
            }
        }
    }
}

fn license_name(value: &Value) -> String {
    let Some(licenses) = value.get("licenses").and_then(Value::as_array) else {
        return String::new();
    };
    if licenses.iter().any(|license| {
        license.get("isCc0").and_then(Value::as_bool) == Some(true)
            || license.get("slug").and_then(Value::as_str) == Some("cc0")
    }) {
        return "CC0".into();
    }
    if licenses
        .iter()
        .any(|license| license.get("group").and_then(Value::as_str) == Some("standard"))
    {
        return "Fab Standard License".into();
    }
    unique(names_from_array(Some(&Value::Array(licenses.clone())))).join(" / ")
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn names_from_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .or_else(|| item.get("name").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn push_string(value: Option<&Value>, output: &mut Vec<String>) {
    if let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        output.push(value.to_string());
    }
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_lowercase()))
        .collect()
}

fn html_to_text(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }
    let breaks = Regex::new(r"(?i)</?(p|div|li|br|h[1-6])[^>]*>").expect("valid regex");
    let tags = Regex::new(r"(?s)<[^>]*>").expect("valid regex");
    let spaced = breaks.replace_all(html, "\n");
    let plain = tags.replace_all(&spaced, "");
    plain
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_localized_fab_listing_urls() {
        let id = listing_id_from_url(
            "https://www.fab.com/zh-cn/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a?lang=zh-CN",
        )
        .unwrap();
        assert_eq!(id.to_string(), "06003f78-9a59-4fb8-abbc-14dc276f0b4a");
        assert!(listing_id_from_url(
            "https://example.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a"
        )
        .is_err());
    }

    #[test]
    fn maps_fab_listing_fields() {
        let listing = serde_json::json!({
            "title": "Garden Environment",
            "description": "<p>84 HQ meshes</p><p><strong>Nanite ready</strong></p>",
            "category": {"name": "沙漠", "path": "environments/desert"},
            "listingType": "3d-model",
            "user": {"sellerName": "Dragon Motion"},
            "tags": [{"name": "Garden"}, {"name": "garden"}],
            "assetFormats": [{"assetFormatType": {
                "code": "unreal-engine", "name": "Unreal Engine",
                "extensions": ["uasset", "uproject"]
            }}],
            "licenses": [{"group": "standard", "name": "Personal"}]
        });
        let metadata = metadata_from_listing(
            &listing,
            "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a",
        )
        .unwrap();
        assert_eq!(metadata.name, "Garden Environment");
        assert_eq!(metadata.author, "Dragon Motion");
        assert_eq!(metadata.tags, vec!["Garden", "沙漠"]);
        assert_eq!(metadata.suggested_category_path, vec!["环境", "沙漠"]);
        assert_eq!(metadata.dcc_tools, vec!["Unreal Engine"]);
        assert_eq!(metadata.formats, vec!["uasset", "uproject"]);
        assert_eq!(metadata.license, "Fab Standard License");
        assert_eq!(metadata.description, "84 HQ meshes\nNanite ready");
    }

    #[test]
    fn normalizes_url_variants_and_maps_fixed_categories() {
        let expected = "06003f78-9a59-4fb8-abbc-14dc276f0b4a";
        for url in [
            "http://FAB.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a/",
            "https://www.fab.com/zh-cn/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a?lang=zh-CN#detail",
        ] {
            assert_eq!(listing_id_from_url(url).unwrap().to_string(), expected);
        }
        assert_eq!(
            classify_category("environments/desert", "沙漠", "3d-model"),
            vec!["环境", "沙漠"]
        );
        assert_eq!(
            classify_category("future-new-type", "未知", "unknown"),
            vec!["其他", "待整理"]
        );
        assert_eq!(classify_category("", "", "material"), vec!["材质与纹理"]);
    }

    #[test]
    fn reads_engine_versions() {
        let detail = serde_json::json!({"versions": [
            {"engineVersions": ["UE_5.3", "UE_5.4"]},
            {"engineVersions": ["UE_5.4", "UE_5.5"]}
        ]});
        let mut versions = Vec::new();
        collect_versions(&detail, &mut versions);
        assert_eq!(unique(versions), vec!["5.3", "5.4", "5.5"]);
    }

    #[test]
    fn selects_largest_safe_fab_preview_and_deduplicates() {
        let listing = serde_json::json!({
            "thumbnails": [{"type": "thumbnail", "previewUid": "cover", "name": "cover.png", "images": [
                {"width": 320, "url": "https://media.fab.com/cover-320.jpg"},
                {"width": 640, "url": "https://media.fab.com/cover-640.jpg"}
            ]}],
            "medias": [
                {"type": "image", "previewUid": "one", "name": "one.png", "images": [
                    {"width": 960, "url": "https://media.fab.com/one-960.jpg"},
                    {"width": 1920, "url": "https://media.fab.com/one-1920.jpg"}
                ]},
                {"type": "video", "previewUid": "video", "mediaUrl": "https://media.fab.com/demo.mp4"}
            ]
        });
        assert_eq!(
            preview_candidates(&listing),
            vec![
                PreviewCandidate {
                    url: "https://media.fab.com/cover-640.jpg".into(),
                    name: "cover.png".into()
                },
                PreviewCandidate {
                    url: "https://media.fab.com/one-960.jpg".into(),
                    name: "one.png".into()
                }
            ]
        );
    }
}
