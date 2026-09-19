use crate::models::{TranslationPreview, TranslationRequest, TranslationTestResult};
use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

const SERVICE: &str = "DCCAssetLibrary.BaiduTranslate";
const ACCOUNT: &str = "default";
const ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";
const CHUNK_CHARS: usize = 1800;
// Baidu's standard plan allows one request per second. Leave a little margin
// for clock and network jitter, and share the queue across all translation jobs.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(1200);
static LAST_REQUEST_SLOT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Credential {
    app_id: String,
    secret_key: String,
}

#[derive(Debug, Deserialize)]
struct BaiduReply {
    #[serde(default)]
    trans_result: Vec<BaiduItem>,
    error_code: Option<String>,
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaiduItem {
    dst: String,
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("无法访问 Windows 凭据管理器：{e}"))
}

pub fn save_credentials(app_id: &str, secret_key: &str) -> Result<(), String> {
    if app_id.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("APP ID 和密钥不能为空".into());
    }
    let value = serde_json::to_string(&Credential {
        app_id: app_id.trim().into(),
        secret_key: secret_key.trim().into(),
    })
    .map_err(|e| e.to_string())?;
    entry()?
        .set_password(&value)
        .map_err(|e| format!("保存翻译凭据失败：{e}"))?;

    // Read through a newly-created entry so a non-persistent credential backend
    // cannot report a misleading successful save.
    let saved = entry()?
        .get_password()
        .map_err(|e| format!("验证翻译凭据保存结果失败：{e}"))?;
    if saved != value {
        return Err("Windows 凭据管理器未能保存翻译凭据，请重试".into());
    }
    Ok(())
}

pub fn delete_credentials() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("删除翻译凭据失败：{e}")),
    }
}

pub fn is_configured() -> bool {
    load_credentials().is_ok()
}

fn load_credentials() -> Result<Credential, String> {
    let raw = entry()?.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => "尚未配置百度翻译凭据".into(),
        _ => format!("读取翻译凭据失败：{e}"),
    })?;
    serde_json::from_str(&raw).map_err(|_| "保存的翻译凭据已损坏，请重新配置".into())
}

fn language_code(value: &str) -> Result<&'static str, String> {
    match value {
        "zh-CN" => Ok("zh"),
        "en" => Ok("en"),
        _ => Err("仅支持简体中文和英文互译".into()),
    }
}

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化翻译请求失败：{e}"))
}

fn chunks(text: &str) -> Vec<String> {
    if text.chars().count() <= CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let mut output = Vec::new();
    let mut current = String::new();
    for paragraph in text.split_inclusive('\n') {
        if current.chars().count() + paragraph.chars().count() <= CHUNK_CHARS {
            current.push_str(paragraph);
            continue;
        }
        if !current.is_empty() {
            output.push(std::mem::take(&mut current));
        }
        let chars = paragraph.chars().collect::<Vec<_>>();
        for part in chars.chunks(CHUNK_CHARS) {
            output.push(part.iter().collect());
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

async fn wait_for_request_slot() {
    let wait = {
        let queue = LAST_REQUEST_SLOT.get_or_init(|| Mutex::new(None));
        let mut last = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let wait = last
            .and_then(|previous| (previous + MIN_REQUEST_INTERVAL).checked_duration_since(now))
            .unwrap_or_default();
        *last = Some(now + wait);
        wait
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

async fn translate_piece(
    client: &Client,
    credential: &Credential,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    let salt = Uuid::new_v4().simple().to_string();
    let sign = format!(
        "{:x}",
        md5::compute(format!(
            "{}{}{}{}",
            credential.app_id, text, salt, credential.secret_key
        ))
    );
    let form = [
        ("q", text),
        ("from", from),
        ("to", to),
        ("appid", credential.app_id.as_str()),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
    ];
    let mut last = None;
    for attempt in 0..3 {
        wait_for_request_slot().await;
        match client.post(ENDPOINT).form(&form).send().await {
            Ok(response) if response.status().is_server_error() && attempt == 0 => {
                last = Some(format!("翻译服务暂时不可用：{}", response.status()));
                continue;
            }
            Ok(response) => {
                let status = response.status();
                let reply: BaiduReply = response
                    .json()
                    .await
                    .map_err(|_| format!("翻译服务返回了无法识别的数据（{status}）"))?;
                if let Some(code) = reply.error_code {
                    let error = provider_error(&code, reply.error_msg.as_deref());
                    if code == "54003" && attempt < 2 {
                        last = Some(error);
                        tokio::time::sleep(Duration::from_millis(1500 * (attempt + 1) as u64))
                            .await;
                        continue;
                    }
                    if attempt == 0 && matches!(code.as_str(), "52001" | "52002") {
                        last = Some(error);
                        continue;
                    }
                    return Err(error);
                }
                let value = reply
                    .trans_result
                    .into_iter()
                    .map(|item| item.dst)
                    .collect::<Vec<_>>()
                    .join("\n");
                return if value.is_empty() {
                    Err("翻译服务没有返回译文".into())
                } else {
                    Ok(value)
                };
            }
            Err(error) if (error.is_timeout() || error.is_connect()) && attempt == 0 => {
                last = Some(format!("连接翻译服务失败：{error}"));
                continue;
            }
            Err(error) => return Err(format!("连接翻译服务失败：{error}")),
        }
    }
    Err(last.unwrap_or_else(|| "翻译请求失败".into()))
}

fn provider_error(code: &str, message: Option<&str>) -> String {
    match code {
        "52001" => "百度翻译请求超时，请稍后重试".into(),
        "52002" => "百度翻译系统错误，请稍后重试".into(),
        "52003" => "百度翻译 APP ID 无效，请检查设置".into(),
        "54000" => "百度翻译请求参数无效".into(),
        "54001" => "百度翻译密钥或签名无效，请重新配置".into(),
        "54003" => "百度翻译请求过于频繁，请稍后重试".into(),
        "54004" => "百度翻译账户额度不足".into(),
        "58001" => "百度翻译不支持该语言方向".into(),
        _ => format!(
            "百度翻译错误 {code}{}",
            message.map(|v| format!("：{v}")).unwrap_or_default()
        ),
    }
}

async fn translate_text(
    client: &Client,
    credential: &Credential,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let mut translated = Vec::new();
    for chunk in chunks(text) {
        translated.push(translate_piece(client, credential, &chunk, from, to).await?);
    }
    Ok(translated.join(""))
}

fn parsed_tag_lines(value: &str, expected: usize) -> Option<Vec<String>> {
    let lines = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (lines.len() == expected).then_some(lines)
}

async fn translate_tags(
    client: &Client,
    credential: &Credential,
    tags: &[String],
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let sources = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    // Translate the normal case in one request instead of issuing one request
    // per tag. Baidu preserves newlines for short tag lists.
    let combined = sources.join("\n");
    let translated = translate_text(client, credential, &combined, from, to).await?;
    let values = if let Some(lines) = parsed_tag_lines(&translated, sources.len()) {
        lines
    } else {
        // If the provider merged line breaks, retain correctness with the same
        // globally rate-limited request queue.
        let mut values = Vec::with_capacity(sources.len());
        for source in sources {
            values.push(translate_text(client, credential, source, from, to).await?);
        }
        values
    };

    let mut seen = HashSet::new();
    Ok(values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.to_lowercase()))
        .collect())
}

pub async fn translate(request: TranslationRequest) -> Result<TranslationPreview, String> {
    if request.source_language == request.target_language {
        return Err("源语言和目标语言不能相同".into());
    }
    let from = language_code(&request.source_language)?;
    let to = language_code(&request.target_language)?;
    let credential = load_credentials()?;
    let client = client()?;
    let character_count = request.fields.name.chars().count()
        + request.fields.description.chars().count()
        + request.fields.license.chars().count()
        + request
            .fields
            .tags
            .iter()
            .map(|v| v.chars().count())
            .sum::<usize>();
    let mut preview = TranslationPreview {
        character_count,
        ..Default::default()
    };
    for (field, source) in [
        ("name", request.fields.name.as_str()),
        ("description", request.fields.description.as_str()),
        ("license", request.fields.license.as_str()),
    ] {
        match translate_text(&client, &credential, source, from, to).await {
            Ok(value) => match field {
                "name" => preview.fields.name = value,
                "description" => preview.fields.description = value,
                _ => preview.fields.license = value,
            },
            Err(error) if !source.trim().is_empty() => {
                preview.failed_fields.push(field.into());
                preview.warnings.push(format!("{field}：{error}"));
            }
            _ => {}
        }
    }
    match translate_tags(&client, &credential, &request.fields.tags, from, to).await {
        Ok(tags) => preview.fields.tags = tags,
        Err(error) => {
            preview.failed_fields.push("tags".into());
            preview.warnings.push(format!("tags：{error}"));
        }
    }
    Ok(preview)
}

pub async fn test_service() -> Result<TranslationTestResult, String> {
    let credential = load_credentials()?;
    let value = translate_piece(&client()?, &credential, "test", "en", "zh").await?;
    Ok(TranslationTestResult {
        success: true,
        message: format!("连接成功，测试译文：{value}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_long_unicode_text_without_loss() {
        let source = format!("{}\n{}", "素材".repeat(1000), "asset".repeat(500));
        let parts = chunks(&source);
        assert!(parts.iter().all(|part| part.chars().count() <= CHUNK_CHARS));
        assert_eq!(parts.concat(), source);
    }
    #[test]
    fn maps_provider_errors_without_leaking_credentials() {
        assert!(provider_error("54001", None).contains("密钥"));
        assert!(provider_error("54004", None).contains("额度"));
    }

    #[test]
    fn parses_batched_tag_translation_only_when_line_count_matches() {
        assert_eq!(
            parsed_tag_lines("建筑\n亚洲\n竹子", 3),
            Some(vec!["建筑".into(), "亚洲".into(), "竹子".into()])
        );
        assert_eq!(parsed_tag_lines("建筑、亚洲、竹子", 3), None);
    }
}
