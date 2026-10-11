use super::models::DownloadFormat;
pub fn parse(value: &serde_json::Value) -> Vec<DownloadFormat> {
    value["formats"]
        .as_array()
        .into_iter()
        .flatten()
        .take(1000)
        .filter_map(|item| {
            let id = item["format_id"].as_str()?;
            if !super::domain::valid_format(id) {
                return None;
            }
            let codec = |key: &str| {
                item[key]
                    .as_str()
                    .filter(|s| !s.is_empty() && *s != "none")
                    .map(str::to_string)
            };
            Some(DownloadFormat {
                id: id.into(),
                extension: item["ext"].as_str().unwrap_or_default().into(),
                width: item["width"].as_u64(),
                height: item["height"].as_u64(),
                fps: item["fps"].as_f64(),
                video_codec: codec("vcodec"),
                audio_codec: codec("acodec"),
                size: item["filesize"]
                    .as_u64()
                    .or_else(|| item["filesize_approx"].as_u64()),
                audio_bitrate: item["abr"].as_f64(),
                sample_rate: item["asr"].as_u64(),
            })
        })
        .collect()
}
