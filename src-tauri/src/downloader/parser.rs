use super::models::{DownloadInfo, DownloadProgress};
pub fn info(url: &str, value: &serde_json::Value) -> Result<DownloadInfo, String> {
    let url = super::domain::validate_url(url)?;
    let title = value["title"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("源站未返回有效标题")?
        .chars()
        .take(500)
        .collect();
    let mut subtitle_languages = std::collections::BTreeSet::new();
    for key in ["subtitles", "automatic_captions"] {
        if let Some(tracks) = value[key].as_object() {
            for language in tracks.keys() {
                if language.len() <= 80
                    && language
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                {
                    subtitle_languages.insert(language.clone());
                }
            }
        }
    }
    let entries = value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .take(500)
        .filter_map(|item| {
            let raw = item["webpage_url"]
                .as_str()
                .or_else(|| item["url"].as_str())?;
            let raw = if !raw.contains("://")
                && super::domain::platform(&url) == "YouTube"
                && raw
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            {
                format!("https://www.youtube.com/watch?v={raw}")
            } else {
                raw.into()
            };
            Some(super::models::PlaylistEntry {
                url: super::domain::validate_url(&raw).ok()?,
                title: item["title"]
                    .as_str()
                    .unwrap_or("未命名视频")
                    .chars()
                    .take(500)
                    .collect(),
                duration: item["duration"].as_f64(),
            })
        })
        .collect();
    Ok(DownloadInfo {
        platform: super::domain::platform(&url),
        url,
        title,
        thumbnail: value["thumbnail"]
            .as_str()
            .and_then(|s| super::domain::validate_url(s).ok()),
        duration: value["duration"].as_f64(),
        author: value["uploader"]
            .as_str()
            .or_else(|| value["channel"].as_str())
            .unwrap_or_default()
            .chars()
            .take(255)
            .collect(),
        formats: super::formats::parse(value),
        subtitle_languages: subtitle_languages.into_iter().collect(),
        entries,
    })
}
pub fn progress(value: &serde_json::Value) -> DownloadProgress {
    let downloaded = value["downloaded_bytes"].as_u64().unwrap_or_default();
    let total = value["total_bytes"]
        .as_u64()
        .or_else(|| value["total_bytes_estimate"].as_u64())
        .filter(|v| *v > 0);
    let positive = |key: &str| value[key].as_f64().filter(|n| n.is_finite() && *n >= 0.0);
    DownloadProgress {
        downloaded,
        total,
        speed: positive("speed"),
        eta: positive("eta"),
        percent: total.map(|n| (downloaded as f64 * 100.0 / n as f64).clamp(0.0, 100.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn download_metadata_preserves_formats_and_playlist_children() {
        let value = json!({"title":"视频", "uploader":"作者", "duration":10.5, "formats":[{"format_id":"137","ext":"mp4","width":1920,"height":1080,"fps":60,"vcodec":"avc1","acodec":"none","filesize":4000},{"format_id":"140","ext":"m4a","acodec":"mp4a","vcodec":"none","abr":128,"asr":44100}],"subtitles":{"zh-CN":[],"en":[]},"entries":[{"url":"https://example.com/a","title":"A"},{"url":"file:///secret","title":"Unsafe"}]});
        let result = info("https://example.com/list", &value).unwrap();
        assert_eq!(result.formats.len(), 2);
        assert_eq!(result.formats[1].sample_rate, Some(44100));
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.subtitle_languages, vec!["en", "zh-CN"]);
        assert!(info("https://example.com/a", &json!({})).is_err());
    }
    #[test]
    fn download_progress_unknown_total_is_not_zero_percent() {
        assert_eq!(progress(&json!({"downloaded_bytes":4})).percent, None);
        let value =
            progress(&json!({"downloaded_bytes":50,"total_bytes_estimate":100,"speed":12,"eta":4}));
        assert_eq!(value.percent, Some(50.0));
        assert_eq!(value.speed, Some(12.0));
    }
}
