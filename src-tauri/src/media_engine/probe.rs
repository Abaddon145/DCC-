#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub duration: Option<f64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub codec: String,
    pub fps: Option<f64>,
    pub format: String,
}
pub fn parse(json: &serde_json::Value) -> Result<MediaInfo, String> {
    let streams = json["streams"].as_array().ok_or("无法解析媒体流")?;
    let stream = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .or_else(|| streams.iter().find(|s| s["codec_type"] == "audio"))
        .ok_or("文件不包含可用的视频或音频")?;
    let fraction = stream["avg_frame_rate"]
        .as_str()
        .and_then(|s| s.split_once('/'))
        .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        .filter(|(_, b)| *b > 0.0)
        .map(|(a, b)| a / b)
        .filter(|n| n.is_finite() && *n > 0.0);
    Ok(MediaInfo {
        duration: json["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n >= 0.0),
        width: stream["width"].as_u64(),
        height: stream["height"].as_u64(),
        codec: stream["codec_name"].as_str().ok_or("媒体编码缺失")?.into(),
        fps: fraction,
        format: json["format"]["format_name"]
            .as_str()
            .unwrap_or_default()
            .into(),
    })
}
pub fn analyze(
    tool: &std::path::Path,
    file: &std::path::Path,
    stop: &dyn Fn() -> bool,
) -> Result<MediaInfo, String> {
    if !file.is_file()
        || std::fs::metadata(file)
            .map_err(|_| "无法读取下载文件")?
            .len()
            == 0
    {
        return Err("下载文件缺失或为空".into());
    }
    let args = [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ]
    .map(str::to_string)
    .into_iter()
    .chain(Some(file.to_string_lossy().into()))
    .collect::<Vec<_>>();
    let output = crate::runtime::process::capture(
        tool,
        &args,
        std::time::Duration::from_secs(30),
        stop,
        &mut |_| {},
    )
    .map_err(|_| {
        if stop() {
            "媒体校验已停止"
        } else {
            "文件校验失败：无法解析音视频；下载文件已保留，可重试"
        }
    })?;
    parse(&serde_json::from_str(&output).map_err(|_| "媒体信息无法解析")?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_probe_requires_decodable_stream_and_parses_fps() {
        let value = serde_json::json!({"format":{"duration":"12.5","format_name":"mov,mp4"},"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001"}]});
        let info = parse(&value).unwrap();
        assert_eq!(info.duration, Some(12.5));
        assert_eq!(info.width, Some(1920));
        assert!((info.fps.unwrap() - 29.97).abs() < 0.01);
        assert!(parse(&serde_json::json!({"streams":[]})).is_err());
    }
}
