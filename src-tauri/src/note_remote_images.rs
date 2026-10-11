//! Public HTTP(S) images only. DNS is validated and pinned for every redirect hop.
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;

pub struct StagedImages {
    pub directory: tempfile::TempDir,
    pub files: HashMap<String, PathBuf>,
    pub failures: HashMap<String, String>,
}
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && !v.is_broadcast()
                && !v.is_unspecified()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0)
                && !(a == 198 && (b == 18 || b == 19))
                && !(a == 198 && b == 51 && c == 100)
                && !(a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(v) => {
            if let Some(mapped) = v.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(mapped));
            }
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] <= 0x1ff || s[1] == 0xdb8))
        }
    }
}
pub fn validate_url(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|_| "图片地址无效")?;
    if raw.len() > 8192
        || !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.port(), None | Some(80) | Some(443))
    {
        return Err("图片仅允许无凭据的标准 HTTP/HTTPS 地址".into());
    }
    let host = url
        .host_str()
        .ok_or("图片缺少主机名")?
        .trim_matches(['[', ']'])
        .to_lowercase();
    if host == "localhost"
        || [".localhost", ".local", ".internal", ".home", ".lan"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
        || host.parse::<IpAddr>().is_ok_and(|ip| !public_ip(ip))
    {
        return Err("不能下载本机或内网地址的图片".into());
    }
    url.set_fragment(None);
    Ok(url)
}
pub fn urls(document: &Value) -> Result<Vec<String>, String> {
    let mut found = HashSet::new();
    let mut nodes = vec![(document, 0)];
    while let Some((node, depth)) = nodes.pop() {
        if depth > 64 {
            return Err("Markdown 层级过深".into());
        }
        if node["type"] == "image" || node["type"] == "remoteImage" {
            if let Some(src) = node["attrs"]["src"].as_str() {
                if src.starts_with("http://") || src.starts_with("https://") {
                    found.insert(src.to_owned());
                }
            }
        }
        if let Some(children) = node["content"].as_array() {
            nodes.extend(children.iter().map(|v| (v, depth + 1)));
        }
    }
    if found.len() > 100 {
        return Err("单次最多下载 100 张网络图片，请拆分笔记".into());
    }
    let mut urls: Vec<_> = found.into_iter().collect();
    urls.sort();
    Ok(urls)
}
fn write_image(bytes: &[u8], directory: &Path) -> Result<PathBuf, String> {
    if bytes.len() > 25 * 1024 * 1024 {
        return Err("网络图片超过 25 MB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "响应不是支持的图片")?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Gif => "gif",
        _ => return Err("网络图片仅支持 PNG/JPEG/WebP/GIF".into()),
    };
    let dimensions = image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "网络图片损坏")?;
    if dimensions.0 == 0
        || dimensions.1 == 0
        || u64::from(dimensions.0) * u64::from(dimensions.1) > 25_000_000
    {
        return Err("网络图片超过 2500 万像素".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| "网络图片损坏或解码资源超限")?;
    let path = directory.join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
    std::fs::write(&path, bytes).map_err(|_| "无法暂存下载图片")?;
    Ok(path)
}
pub(crate) async fn fetch(raw: &str, directory: &Path) -> Result<PathBuf, String> {
    let mut url = validate_url(raw)?;
    for _ in 0..=3 {
        let host = url
            .host_str()
            .ok_or("图片主机无效")?
            .trim_matches(['[', ']'])
            .to_owned();
        let port = url.port_or_known_default().ok_or("图片端口无效")?;
        let resolve_host = host.clone();
        let addresses: Vec<SocketAddr> = tokio::time::timeout(
            Duration::from_secs(5),
            tauri::async_runtime::spawn_blocking(move || {
                (resolve_host.as_str(), port)
                    .to_socket_addrs()
                    .map(|v| v.collect::<Vec<_>>())
            }),
        )
        .await
        .map_err(|_| "图片域名解析超时")?
        .map_err(|_| "图片域名解析失败")?
        .map_err(|_| "图片域名解析失败")?;
        if addresses.is_empty() || addresses.iter().any(|v| !public_ip(v.ip())) {
            return Err("图片域名解析到内网或保留地址，已阻止".into());
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .resolve_to_addrs(&host, &addresses)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(25))
            .user_agent("StackDCC/0.17 note-image-import")
            .build()
            .map_err(|_| "无法建立图片连接")?;
        let mut response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|_| "图片请求失败或超时")?;
        if response.status().is_redirection() {
            let target = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("图片跳转缺少地址")?;
            let next = validate_url(url.join(target).map_err(|_| "图片跳转无效")?.as_str())?;
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err("已阻止图片跳转到不安全 HTTP".into());
            }
            url = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("图片服务器返回 {}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|v| v > 25 * 1024 * 1024)
        {
            return Err("网络图片超过 25 MB".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "读取网络图片失败")? {
            if bytes.len() + chunk.len() > 25 * 1024 * 1024 {
                return Err("网络图片超过 25 MB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        return write_image(&bytes, directory);
    }
    Err("图片重定向次数超过 3 次".into())
}
pub async fn stage(document: &Value) -> Result<StagedImages, String> {
    let directory = tempfile::tempdir().map_err(|_| "无法建立图片暂存目录")?;
    let mut files = HashMap::new();
    let mut failures = HashMap::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let mut total = 0u64;
    for raw in urls(document)? {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let result = if total >= 300 * 1024 * 1024 {
            Err("网络图片总量达到 300 MB，请拆分笔记".into())
        } else if remaining.is_zero() {
            Err("本次图片下载已超时，请导入后重试".into())
        } else {
            tokio::time::timeout(
                remaining.min(Duration::from_secs(35)),
                fetch(&raw, directory.path()),
            )
            .await
            .unwrap_or_else(|_| Err("图片下载超时，请重试".into()))
        };
        match result {
            Ok(path) => {
                total += std::fs::metadata(&path)
                    .map_err(|_| "图片暂存读取失败")?
                    .len();
                if total > 300 * 1024 * 1024 {
                    failures.insert(raw, "网络图片总量超过 300 MB".into());
                } else {
                    files.insert(raw, path);
                }
            }
            Err(error) => {
                failures.insert(raw, error);
            }
        }
    }
    Ok(StagedImages {
        directory,
        files,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "manual network acceptance; requires DCC_NOTE_ACCEPTANCE_MD"]
    fn real_markdown_images_are_managed_offline() {
        let source = PathBuf::from(std::env::var("DCC_NOTE_ACCEPTANCE_MD").unwrap());
        let info = crate::note_markdown::read(&source).unwrap();
        let mut content = Vec::new();
        for line in info.text.lines() {
            if let Some(start) = line.find("https://") {
                let raw = line[start..].split(')').next().unwrap();
                content.push(serde_json::json!({"type":"image","attrs":{"src":raw}}));
            }
        }
        assert_eq!(content.len(), 10);
        let document = serde_json::json!({"type":"doc","content":content});
        let staged = tauri::async_runtime::block_on(stage(&document)).unwrap();
        assert!(staged.failures.is_empty(), "{:?}", staged.failures);
        assert_eq!(staged.files.len(), 10);
        let library = tempfile::tempdir().unwrap();
        let mut db = crate::db::open_database(&library.path().join("library.db")).unwrap();
        let result = crate::note_markdown::import_staged(
            &mut db,
            library.path(),
            &source,
            &info.sha256,
            document,
            None,
            &staged.files,
            &staged.failures,
        )
        .unwrap();
        drop(staged);
        for image in result.note.document["content"].as_array().unwrap() {
            let id = image["attrs"]["src"]
                .as_str()
                .unwrap()
                .strip_prefix("http://dcc-media.localhost/note/")
                .unwrap();
            let (path, _) = crate::knowledge_files::path(&db, library.path(), id).unwrap();
            assert!(std::fs::metadata(path).unwrap().len() > 0);
        }
        println!("10/10 remote images copied and readable after staging removal");
    }
    #[test]
    fn blocks_private_reserved_credentials_and_protocols() {
        for raw in [
            "http://127.0.0.1/a.png",
            "http://10.0.0.1/a",
            "http://[::1]/a",
            "http://[::ffff:127.0.0.1]/a",
            "http://100.64.0.1/a",
            "http://169.254.169.254/a",
            "https://localhost/a",
            "ftp://example.com/a",
            "file:///C:/a.png",
            "https://user:secret@example.com/a",
            "https://example.com:8080/a",
        ] {
            assert!(validate_url(raw).is_err(), "{raw}");
        }
        assert!(validate_url("https://api2.mubu.com/v3/document_image/image.png").is_ok());
        for ip in [
            "0.1.2.3",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.2",
            "203.0.113.2",
            "224.1.1.1",
            "2001:db8::1",
            "2002:7f00:1::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()));
        }
    }
    #[test]
    fn sniffs_valid_images_and_rejects_html_and_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        assert!(
            write_image(&std::fs::read(source).unwrap(), temp.path())
                .unwrap()
                .extension()
                .unwrap()
                == "png"
        );
        assert!(write_image(b"<html>login</html>", temp.path()).is_err());
        assert!(write_image(b"\x89PNG\r\n\x1a\n", temp.path()).is_err());
    }
}
