use crate::models::{
    TranslationPreview, TranslationRequest, TranslationTerm, TranslationTermApplication,
    TranslationTestResult,
};
use keyring::Entry;
use regex::{Regex, RegexBuilder};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
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
        let mut remaining = paragraph;
        while remaining.chars().count() > CHUNK_CHARS {
            let hard_cut = remaining
                .char_indices()
                .nth(CHUNK_CHARS)
                .map(|(index, _)| index)
                .unwrap_or(remaining.len());
            let search_start = remaining
                .char_indices()
                .nth(CHUNK_CHARS.saturating_sub(220))
                .map(|(index, _)| index)
                .unwrap_or(0);
            let mut cut = remaining[search_start..hard_cut]
                .char_indices()
                .filter(|(_, ch)| {
                    ch.is_whitespace()
                        || matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | ';' | '；')
                })
                .map(|(index, ch)| search_start + index + ch.len_utf8())
                .last()
                .unwrap_or(hard_cut);
            if let Some(token_start) = remaining[..cut].rfind("ZXQ") {
                let token_finished = remaining[token_start..cut].contains("QXZ");
                if !token_finished {
                    cut = token_start;
                }
            }
            if cut == 0 {
                cut = hard_cut;
            }
            output.push(remaining[..cut].to_string());
            remaining = &remaining[cut..];
        }
        if !remaining.is_empty() {
            output.push(remaining.to_string());
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

async fn translate_piece_items(
    client: &Client,
    credential: &Credential,
    text: &str,
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
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
                    .collect::<Vec<_>>();
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

async fn translate_piece(
    client: &Client,
    credential: &Credential,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, String> {
    Ok(translate_piece_items(client, credential, text, from, to)
        .await?
        .join("\n"))
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

#[derive(Debug, Clone)]
struct ProtectedToken {
    marker: String,
    replacement: String,
}

#[derive(Debug, Default)]
struct TranslationOutcome {
    value: String,
    applications: Vec<TranslationTermApplication>,
    protected_token_count: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct Candidate {
    start: usize,
    end: usize,
    replacement: String,
    term: Option<TranslationTerm>,
}

fn is_word_character(value: Option<char>) -> bool {
    value.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn term_spans(text: &str, term: &TranslationTerm) -> Vec<(usize, usize)> {
    if term.source.is_empty() {
        return Vec::new();
    }
    let pattern = match RegexBuilder::new(&regex::escape(&term.source))
        .case_insensitive(!term.case_sensitive)
        .build()
    {
        Ok(pattern) => pattern,
        Err(_) => return Vec::new(),
    };
    let boundary_sensitive = term
        .source
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && term
            .source
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    pattern
        .find_iter(text)
        .filter_map(|matched| {
            if boundary_sensitive
                && (is_word_character(text[..matched.start()].chars().next_back())
                    || is_word_character(text[matched.end()..].chars().next()))
            {
                None
            } else {
                Some((matched.start(), matched.end()))
            }
        })
        .collect()
}

fn generic_candidates(text: &str) -> Vec<Candidate> {
    let patterns = [
        r#"(?i)https?://[^\s<>()]+"#,
        r#"(?i)\b[A-Z]:\\[^\r\n\t<>|?\"]+"#,
        r#"(?i)\.(?:uasset|umap|fbx|obj|usd|usda|usdc|gltf|glb|abc|blend|ma|mb|max|hip|sbsar|exr|hdr|png|jpe?g|webp|tiff?)\b"#,
        r#"(?i)\b(?:UE\s*)?\d+(?:\.\d+){1,3}\b"#,
        r#"\b(?:[A-Za-z_][A-Za-z0-9_]*[_.:]){1,}[A-Za-z0-9_]+\b|\b[A-Za-z]+[A-Z][A-Za-z0-9]*\b"#,
    ];
    let mut output = Vec::new();
    for source in patterns {
        if let Ok(pattern) = Regex::new(source) {
            output.extend(pattern.find_iter(text).map(|matched| Candidate {
                start: matched.start(),
                end: matched.end(),
                replacement: matched.as_str().to_string(),
                term: None,
            }));
        }
    }
    output
}

fn protect_text(
    text: &str,
    terms: &[TranslationTerm],
    field: &str,
) -> (String, Vec<ProtectedToken>, Vec<TranslationTermApplication>) {
    let mut candidates = generic_candidates(text);
    for term in terms {
        for (start, end) in term_spans(text, term) {
            let matched = &text[start..end];
            candidates.push(Candidate {
                start,
                end,
                replacement: if term.mode == "preserve" {
                    matched.to_string()
                } else {
                    term.target.clone()
                },
                term: Some(term.clone()),
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
            .then_with(|| {
                b.term
                    .as_ref()
                    .map(|term| term.origin == "custom")
                    .cmp(&a.term.as_ref().map(|term| term.origin == "custom"))
            })
    });

    let mut selected = Vec::new();
    let mut cursor = 0;
    for candidate in candidates {
        if candidate.start >= cursor {
            cursor = candidate.end;
            selected.push(candidate);
        }
    }
    let mut masked = String::with_capacity(text.len());
    let mut source_cursor = 0;
    let mut tokens = Vec::new();
    let mut applications = Vec::new();
    for candidate in selected {
        masked.push_str(&text[source_cursor..candidate.start]);
        // Keep markers free of natural-language words. Translation providers may
        // translate words such as "GLOSSARY", which makes exact restoration fail.
        let marker = format!("ZXQ{:05}QXZ", tokens.len());
        masked.push_str(&marker);
        if let Some(term) = candidate.term {
            applications.push(TranslationTermApplication {
                field: field.into(),
                source: term.source,
                target: candidate.replacement.clone(),
                mode: term.mode,
                origin: term.origin,
                count: 1,
            });
        }
        tokens.push(ProtectedToken {
            marker,
            replacement: candidate.replacement,
        });
        source_cursor = candidate.end;
    }
    masked.push_str(&text[source_cursor..]);
    (masked, tokens, applications)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TranslationPart {
    Literal(String),
    Translatable(usize),
}

fn push_plain_text_parts(text: &str, parts: &mut Vec<TranslationPart>, sources: &mut Vec<String>) {
    let mut line_start = 0;
    for (index, value) in text.char_indices() {
        if value == '\n' {
            push_plain_line(&text[line_start..index], parts, sources);
            parts.push(TranslationPart::Literal("\n".into()));
            line_start = index + value.len_utf8();
        }
    }
    push_plain_line(&text[line_start..], parts, sources);
}

fn push_plain_line(line: &str, parts: &mut Vec<TranslationPart>, sources: &mut Vec<String>) {
    if line.is_empty() {
        return;
    }
    let Some(content_start) = line.find(|value: char| !value.is_whitespace()) else {
        parts.push(TranslationPart::Literal(line.into()));
        return;
    };
    let content_end = line
        .char_indices()
        .rev()
        .find(|(_, value)| !value.is_whitespace())
        .map(|(index, value)| index + value.len_utf8())
        .unwrap_or(content_start);
    if content_start > 0 {
        parts.push(TranslationPart::Literal(line[..content_start].into()));
    }
    for chunk in chunks(&line[content_start..content_end]) {
        let slot = sources.len();
        sources.push(chunk);
        parts.push(TranslationPart::Translatable(slot));
    }
    if content_end < line.len() {
        parts.push(TranslationPart::Literal(line[content_end..].into()));
    }
}

fn isolated_translation_parts(
    masked: &str,
    tokens: &[ProtectedToken],
) -> Result<(Vec<TranslationPart>, Vec<String>), String> {
    let mut parts = Vec::new();
    let mut sources = Vec::new();
    let mut cursor = 0;
    for token in tokens {
        let relative = masked[cursor..]
            .find(&token.marker)
            .ok_or_else(|| format!("内部术语占位符丢失：{}", token.marker))?;
        let marker_start = cursor + relative;
        push_plain_text_parts(&masked[cursor..marker_start], &mut parts, &mut sources);
        parts.push(TranslationPart::Literal(token.replacement.clone()));
        cursor = marker_start + token.marker.len();
    }
    push_plain_text_parts(&masked[cursor..], &mut parts, &mut sources);
    Ok((parts, sources))
}

fn normalized_provider_items(items: Vec<String>, expected: usize) -> Vec<String> {
    if items.len() == expected {
        return items;
    }
    let expanded = items
        .iter()
        .flat_map(|item| item.split('\n'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if expanded.len() == expected {
        expanded
    } else {
        items
    }
}

async fn translate_isolated_sources(
    client: &Client,
    credential: &Credential,
    sources: &[String],
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    let mut initial_batches = Vec::<Vec<usize>>::new();
    let mut current = Vec::new();
    let mut current_chars = 0;
    for (index, source) in sources.iter().enumerate() {
        let extra = source.chars().count() + usize::from(!current.is_empty());
        if !current.is_empty() && current_chars + extra > CHUNK_CHARS {
            initial_batches.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current_chars += source.chars().count() + usize::from(!current.is_empty());
        current.push(index);
    }
    if !current.is_empty() {
        initial_batches.push(current);
    }

    let mut pending = initial_batches;
    let mut translated = vec![None; sources.len()];
    while let Some(mut batch) = pending.pop() {
        let query = batch
            .iter()
            .map(|index| sources[*index].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let values = normalized_provider_items(
            translate_piece_items(client, credential, &query, from, to).await?,
            batch.len(),
        );
        if values.len() == batch.len() {
            for (index, value) in batch.into_iter().zip(values) {
                translated[index] = Some(value);
            }
        } else if batch.len() > 1 {
            let right = batch.split_off(batch.len() / 2);
            pending.push(right);
            pending.push(batch);
        } else {
            translated[batch[0]] = Some(values.join("\n"));
        }
    }

    translated
        .into_iter()
        .enumerate()
        .map(|(index, value)| value.ok_or_else(|| format!("第 {} 段译文缺失", index + 1)))
        .collect()
}

fn assemble_isolated_translation(parts: &[TranslationPart], translated: &[String]) -> String {
    let mut output = String::new();
    for part in parts {
        match part {
            TranslationPart::Literal(value) => output.push_str(value),
            TranslationPart::Translatable(index) => output.push_str(&translated[*index]),
        }
    }
    output
}

#[cfg(test)]
fn restore_tokens(value: &str, tokens: &[ProtectedToken]) -> Result<String, String> {
    let mut output = value.to_string();
    for token in tokens {
        let exact = RegexBuilder::new(&regex::escape(&token.marker))
            .case_insensitive(true)
            .build()
            .map_err(|error| error.to_string())?;
        let digits = token
            .marker
            .strip_prefix("ZXQ")
            .and_then(|value| value.strip_suffix("QXZ"))
            .ok_or("术语占位符格式无效")?;
        let separator = r"[\s\p{P}]*";
        let fuzzy_digits = digits
            .chars()
            .map(|value| regex::escape(&value.to_string()))
            .collect::<Vec<_>>()
            .join(separator);
        let fuzzy = RegexBuilder::new(&format!(
            r"Z{separator}X{separator}Q{separator}{fuzzy_digits}{separator}Q{separator}X{separator}Z"
        ))
        .case_insensitive(true)
        .build()
        .map_err(|error| error.to_string())?;
        let pattern = if exact.is_match(&output) {
            &exact
        } else {
            &fuzzy
        };
        if !pattern.is_match(&output) {
            return Err(format!("翻译服务未完整保留术语占位符 {}", token.marker));
        }
        output = pattern
            .replace_all(&output, regex::NoExpand(&token.replacement))
            .into_owned();
    }
    if Regex::new(r"(?i)ZXQ\d+QXZ").is_ok_and(|pattern| pattern.is_match(&output)) {
        return Err("译文中仍存在未恢复的术语占位符".into());
    }
    Ok(output)
}

async fn translate_text(
    client: &Client,
    credential: &Credential,
    text: &str,
    from: &str,
    to: &str,
    terms: &[TranslationTerm],
    field: &str,
) -> Result<TranslationOutcome, String> {
    if text.trim().is_empty() {
        return Ok(TranslationOutcome::default());
    }
    let (masked, tokens, applications) = protect_text(text, terms, field);
    // Never send glossary terms, URLs, paths or identifiers to the provider.
    // Translate only the ordinary text between them, then rebuild the field
    // locally. This avoids a provider dropping or rewriting a placeholder and
    // forcing the entire long description to fall back to its source text.
    let (parts, sources) = isolated_translation_parts(&masked, &tokens)?;
    let translated = translate_isolated_sources(client, credential, &sources, from, to).await?;
    Ok(TranslationOutcome {
        value: assemble_isolated_translation(&parts, &translated),
        applications,
        protected_token_count: tokens.len(),
        warnings: Vec::new(),
    })
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
    terms: &[TranslationTerm],
) -> Result<
    (
        Vec<String>,
        Vec<TranslationTermApplication>,
        usize,
        Vec<String>,
    ),
    String,
> {
    let sources = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Ok((Vec::new(), Vec::new(), 0, Vec::new()));
    }

    let mut values = vec![None; sources.len()];
    let mut unresolved = Vec::new();
    let mut applications = Vec::new();
    let mut protected_token_count = 0;
    let mut warnings = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if let Some(term) = terms.iter().find(|term| {
            if term.case_sensitive {
                term.source == *source
            } else {
                term.source.eq_ignore_ascii_case(source)
            }
        }) {
            let target = if term.mode == "preserve" {
                (*source).to_string()
            } else {
                term.target.clone()
            };
            values[index] = Some(target.clone());
            protected_token_count += 1;
            applications.push(TranslationTermApplication {
                field: "tags".into(),
                source: term.source.clone(),
                target,
                mode: term.mode.clone(),
                origin: term.origin.clone(),
                count: 1,
            });
        } else {
            unresolved.push((index, *source));
        }
    }
    if !unresolved.is_empty() {
        let combined = unresolved
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>()
            .join("\n");
        let outcome =
            translate_text(client, credential, &combined, from, to, terms, "tags").await?;
        let translated_values =
            if let Some(lines) = parsed_tag_lines(&outcome.value, unresolved.len()) {
                applications.extend(outcome.applications);
                protected_token_count += outcome.protected_token_count;
                warnings.extend(outcome.warnings);
                lines
            } else {
                let mut lines = Vec::with_capacity(unresolved.len());
                for (_, source) in &unresolved {
                    let item =
                        translate_text(client, credential, source, from, to, terms, "tags").await?;
                    applications.extend(item.applications);
                    protected_token_count += item.protected_token_count;
                    warnings.extend(item.warnings);
                    lines.push(item.value);
                }
                lines
            };
        for ((index, _), translated) in unresolved.into_iter().zip(translated_values) {
            values[index] = Some(translated);
        }
    }

    let mut seen = HashSet::new();
    let values = values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.to_lowercase()))
        .collect();
    Ok((values, applications, protected_token_count, warnings))
}

fn merge_applications(values: Vec<TranslationTermApplication>) -> Vec<TranslationTermApplication> {
    let mut merged: HashMap<(String, String, String, String, String), TranslationTermApplication> =
        HashMap::new();
    for value in values {
        let key = (
            value.field.clone(),
            value.source.to_lowercase(),
            value.target.clone(),
            value.mode.clone(),
            value.origin.clone(),
        );
        merged
            .entry(key)
            .and_modify(|existing| existing.count += value.count)
            .or_insert(value);
    }
    let mut values = merged.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| a.field.cmp(&b.field).then_with(|| a.source.cmp(&b.source)));
    values
}

pub async fn translate(
    request: TranslationRequest,
    terms: &[TranslationTerm],
) -> Result<TranslationPreview, String> {
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
        match translate_text(&client, &credential, source, from, to, terms, field).await {
            Ok(outcome) => {
                match field {
                    "name" => preview.fields.name = outcome.value,
                    "description" => preview.fields.description = outcome.value,
                    _ => preview.fields.license = outcome.value,
                }
                preview.applied_terms.extend(outcome.applications);
                preview.protected_token_count += outcome.protected_token_count;
                preview.warnings.extend(
                    outcome
                        .warnings
                        .into_iter()
                        .map(|warning| format!("{field}：{warning}")),
                );
            }
            Err(error) if !source.trim().is_empty() => {
                preview.failed_fields.push(field.into());
                preview.warnings.push(format!("{field}：{error}"));
            }
            _ => {}
        }
    }
    match translate_tags(&client, &credential, &request.fields.tags, from, to, terms).await {
        Ok((tags, applications, protected, warnings)) => {
            preview.fields.tags = tags;
            preview.applied_terms.extend(applications);
            preview.protected_token_count += protected;
            preview.warnings.extend(
                warnings
                    .into_iter()
                    .map(|warning| format!("tags：{warning}")),
            );
        }
        Err(error) => {
            preview.failed_fields.push("tags".into());
            preview.warnings.push(format!("tags：{error}"));
        }
    }
    preview.applied_terms = merge_applications(preview.applied_terms);
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
    fn term(source: &str, target: &str, mode: &str, origin: &str) -> TranslationTerm {
        TranslationTerm {
            id: format!("{origin}-{source}"),
            source_language: "en".into(),
            target_language: "zh-CN".into(),
            source: source.into(),
            target: target.into(),
            mode: mode.into(),
            case_sensitive: false,
            enabled: true,
            origin: origin.into(),
        }
    }
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

    #[test]
    fn protects_longest_term_and_restores_preferred_translation() {
        let terms = vec![
            term("Material Instance", "材质实例", "translate", "builtin"),
            term("Material", "材质", "translate", "builtin"),
        ];
        let (masked, tokens, applications) =
            protect_text("A Material Instance asset", &terms, "description");
        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].source, "Material Instance");
        assert_eq!(
            restore_tokens(&masked, &tokens).unwrap(),
            "A 材质实例 asset"
        );
    }

    #[test]
    fn english_term_matching_respects_word_boundaries() {
        let terms = vec![term("Asset", "资产", "translate", "builtin")];
        let (_, _, applications) = protect_text("Assets and Asset", &terms, "name");
        assert_eq!(applications.len(), 1);
    }

    #[test]
    fn protects_brands_urls_versions_extensions_and_identifiers() {
        let terms = vec![term("Nanite", "Nanite", "preserve", "builtin")];
        let source = "Nanite UE 5.5 https://example.com/a.uasset r.Shadow.Virtual.Enable";
        let (masked, tokens, applications) = protect_text(source, &terms, "description");
        assert_eq!(applications.len(), 1);
        assert!(tokens.len() >= 4);
        assert_eq!(restore_tokens(&masked, &tokens).unwrap(), source);
    }

    #[test]
    fn provider_input_excludes_all_protected_values_and_placeholders() {
        let terms = vec![
            term("Material Instance", "材质实例", "translate", "builtin"),
            term("Fab", "Fab", "preserve", "builtin"),
        ];
        let (masked, tokens, _) = protect_text(
            "Use Material Instance from Fab at https://www.fab.com/listings/123.",
            &terms,
            "description",
        );
        let (parts, sources) = isolated_translation_parts(&masked, &tokens).unwrap();
        let provider_input = sources.join("\n");
        assert!(!provider_input.contains("ZXQ"));
        assert!(!provider_input.contains("Material Instance"));
        assert!(!provider_input.contains("https://"));
        let simulated = sources
            .iter()
            .map(|value| format!("译文({value})"))
            .collect::<Vec<_>>();
        let restored = assemble_isolated_translation(&parts, &simulated);
        assert!(restored.contains("材质实例"));
        assert!(restored.contains("Fab"));
        assert!(restored.contains("https://www.fab.com/listings/123."));
    }

    #[test]
    fn normalizes_provider_items_that_return_multiple_lines_as_one_item() {
        assert_eq!(
            normalized_provider_items(vec!["第一段\n第二段".into()], 2),
            vec!["第一段", "第二段"]
        );
    }

    #[test]
    fn rejects_missing_placeholders_instead_of_leaking_markers() {
        let (_, tokens, _) = protect_text(
            "Skeletal Mesh",
            &[term("Skeletal Mesh", "骨骼网格体", "translate", "builtin")],
            "name",
        );
        assert!(restore_tokens("占位符已经丢失", &tokens).is_err());
    }

    #[test]
    fn restores_markers_even_when_provider_inserts_spacing_and_punctuation() {
        let (masked, tokens, _) = protect_text(
            "Landscape and Fab",
            &[
                term("Landscape", "地形", "translate", "builtin"),
                term("Fab", "Fab", "preserve", "builtin"),
            ],
            "description",
        );
        let provider_value = masked
            .replace("ZXQ00000QXZ", "Z X Q-0 0 0 0 0-Q X Z")
            .replace("ZXQ00001QXZ", "zxq_00001_qxz");
        assert_eq!(
            restore_tokens(&provider_value, &tokens).unwrap(),
            "地形 and Fab"
        );
    }
}
