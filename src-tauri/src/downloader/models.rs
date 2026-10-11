use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadRequest {
    pub url: String,
    pub format: String,
    pub quality: String,
    pub audio_only: bool,
    pub subtitles: bool,
    pub output_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFormat {
    pub id: String,
    pub extension: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub size: Option<u64>,
    pub audio_bitrate: Option<f64>,
    pub sample_rate: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub author: String,
    pub platform: String,
    pub formats: Vec<DownloadFormat>,
    pub subtitle_languages: Vec<String>,
    pub entries: Vec<PlaylistEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadSettings {
    pub directory: String,
    pub concurrency: usize,
    pub proxy: String,
    pub cookie_source: String,
    pub default_quality: String,
}
impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            directory: String::new(),
            concurrency: 2,
            proxy: String::new(),
            cookie_source: "none".into(),
            default_quality: "best".into(),
        }
    }
}
impl DownloadSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=4).contains(&self.concurrency)
            || !matches!(
                self.cookie_source.as_str(),
                "none" | "file" | "edge" | "chrome"
            )
            || !matches!(self.default_quality.as_str(), "best" | "1080p" | "720p")
        {
            return Err("下载设置无效".into());
        }
        if !self.directory.is_empty()
            && (!std::path::Path::new(&self.directory).is_absolute()
                || self.directory.chars().any(char::is_control))
        {
            return Err("请选择绝对路径下载目录".into());
        }
        if !self.proxy.is_empty() {
            let url = url::Url::parse(&self.proxy).map_err(|_| "代理地址无效")?;
            if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.path() != "/" && !url.path().is_empty()
            {
                return Err("代理仅接受HTTP/SOCKS地址，不在设置中明文保存账号密码".into());
            }
        }
        Ok(())
    }
}
