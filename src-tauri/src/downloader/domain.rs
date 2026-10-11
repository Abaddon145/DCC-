use super::models::DownloadRequest;

pub fn validate_url(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.len() > 8192 || input.chars().any(char::is_control) {
        return Err("链接包含无效字符或过长".into());
    }
    let url = url::Url::parse(input).map_err(|_| "请输入完整 HTTP/HTTPS 视频链接")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("仅支持不含账号密码的 HTTP/HTTPS 链接".into());
    }
    let host = url.host_str().unwrap_or_default();
    let local = match url.host() {
        Some(url::Host::Ipv4(ip)) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
        }
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
        _ => {
            host.eq_ignore_ascii_case("localhost")
                || host.ends_with(".localhost")
                || host.ends_with(".local")
        }
    };
    if local {
        return Err("下载链接不能指向本机或局域网地址".into());
    }
    Ok(url.into())
}
pub fn validate_request(request: &DownloadRequest) -> Result<(), String> {
    validate_url(&request.url)?;
    if !valid_format(&request.format)
        || !matches!(request.quality.as_str(), "best" | "1080p" | "720p")
        || !matches!(request.output_mode.as_str(), "library" | "libraryAndFolder")
    {
        return Err("下载格式、质量或输出方式无效".into());
    }
    Ok(())
}
pub fn valid_format(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}
pub fn platform(input: &str) -> String {
    let host = url::Url::parse(input)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    if host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com") {
        "YouTube".into()
    } else if host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv" {
        "哔哩哔哩".into()
    } else {
        host
    }
}
// Do not return site stderr: signed URLs, cookie values and account names can appear there.
pub fn classify_error(input: &str) -> String {
    let text = input.to_lowercase();
    let category =
        if text.contains("no space") || text.contains("disk full") || text.contains("112") {
            "磁盘空间不足：释放空间后恢复任务"
        } else if text.contains("permission denied")
            || text.contains("access is denied")
            || text.contains("os error 5")
            || text.contains("拒绝访问")
        {
            "权限错误：请检查下载目录写入权限"
        } else if text.contains("cookie") || text.contains("sign in") || text.contains("login") {
            "Cookie错误：请更新授权 Cookie 后重试"
        } else if text.contains("429")
            || text.contains("403")
            || text.contains("private video")
            || text.contains("unavailable")
        {
            "源站限制：内容受限或请求过于频繁"
        } else if text.contains("format") {
            "格式不存在：重新解析并选择可用格式"
        } else {
            "网络错误：检查连接、代理和链接后重试"
        };
    category.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_urls_and_parameters_reject_command_injection() {
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://www.bilibili.com/video/BV123",
            "https://example.com/video.mp4",
        ] {
            assert!(validate_url(url).is_ok());
        }
        for url in [
            "file:///c:/secret",
            "https://user:password@example.com",
            "https://example.com\n--exec calc",
            "--exec calc",
            "http://127.0.0.1/private",
            "http://192.168.1.10/a",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        let mut request = DownloadRequest {
            url: "https://example.com/a".into(),
            format: "best".into(),
            quality: "1080p".into(),
            audio_only: false,
            subtitles: true,
            output_mode: "library".into(),
        };
        assert!(validate_request(&request).is_ok());
        request.format = "best --exec calc".into();
        assert!(validate_request(&request).is_err());
    }
    #[test]
    fn download_errors_are_classified_without_secrets() {
        for (text, category) in [
            ("HTTP Error 429 token=private", "源站限制"),
            ("No space left on device", "磁盘空间不足"),
            ("Permission denied", "权限错误"),
            ("Requested format is not available", "格式不存在"),
            ("Sign in to confirm cookie=private", "Cookie错误"),
            ("Unable to download timeout", "网络错误"),
        ] {
            let message = classify_error(text);
            assert!(message.contains(category), "{message}");
            assert!(!message.contains("private"));
        }
    }
}
