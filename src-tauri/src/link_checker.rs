use crate::{db, models::*, state::AppState};
use chrono::Utc;
use reqwest::{
    header::{ACCEPT, ACCEPT_LANGUAGE, COOKIE, LOCATION, SET_COOKIE},
    redirect::Policy,
    Client, StatusCode,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use url::Url;

const MAX_BODY_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone)]
struct ProbeOutcome {
    status: &'static str,
    message: String,
    risk_limited: bool,
}

struct ProbeAttempt {
    outcome: ProbeOutcome,
    retryable: bool,
}

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(Policy::none())
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36",
        )
        .build()
        .map_err(|e| format!("创建链接检查请求失败：{e}"))
}

pub fn is_baidu_share_url(raw: &str) -> bool {
    Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "pan.baidu.com" || host.ends_with(".pan.baidu.com"))
}

fn is_baidu_host(url: &Url) -> bool {
    url.host_str()
        .map(str::to_ascii_lowercase)
        .is_some_and(|host| host == "baidu.com" || host.ends_with(".baidu.com"))
}

async fn read_limited_body(response: &mut reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("读取百度网盘响应失败：{e}"))?
    {
        let remaining = MAX_BODY_BYTES.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() >= MAX_BODY_BYTES {
            break;
        }
    }
    Ok(body)
}

fn classify_response(status: StatusCode, final_url: &Url, body: &str) -> ProbeAttempt {
    let lower = body.to_lowercase();
    let path = final_url.path().to_lowercase();
    let invalid_markers = [
        "分享的文件已经被删除",
        "分享已取消",
        "链接不存在",
        "分享内容不存在",
        "此链接分享内容可能因为涉及",
        "来晚了，该分享文件已过期",
        "啊哦，你来晚了",
        "该分享链接已失效",
    ];
    let risk_markers = [
        "请输入验证码",
        "安全验证",
        "访问过于频繁",
        "请求过于频繁",
        "异常访问",
    ];

    if status == StatusCode::NOT_FOUND
        || status == StatusCode::GONE
        || path.starts_with("/error/404")
        || invalid_markers.iter().any(|marker| lower.contains(marker))
    {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "invalid",
                message: "百度网盘页面明确显示分享已失效或不存在".into(),
                risk_limited: false,
            },
            retryable: false,
        };
    }

    // 百度的正常提取码页面会在公共脚本中包含“安全验证”等文案。
    // 明确落在 /share/init 的 2xx 页面必须先判为有效，不能被这些脚本文案误伤。
    if status.is_success() && path.starts_with("/share/init") {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "valid",
                message: "百度网盘分享存在，需要提取码".into(),
                risk_limited: false,
            },
            retryable: false,
        };
    }

    let hard_risk = status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::FORBIDDEN
        || path.contains("wappass");
    if hard_risk {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "error",
                message: if status == StatusCode::TOO_MANY_REQUESTS {
                    "百度网盘限制了检查频率，请稍后重试".into()
                } else {
                    "百度网盘要求安全验证，无法判断链接状态".into()
                },
                risk_limited: true,
            },
            retryable: false,
        };
    }
    if status.is_server_error() {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "error",
                message: format!("百度网盘服务暂时异常（HTTP {}）", status.as_u16()),
                risk_limited: false,
            },
            retryable: true,
        };
    }

    let password_page = lower.contains("请输入提取码") || lower.contains("share/verify");
    let share_page = lower.contains("window.yundata")
        || lower.contains("\"shareid\"")
        || lower.contains("\"share_id\"")
        || (lower.contains("filename") && lower.contains("share"));
    if status.is_success() && (password_page || share_page) {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "valid",
                message: if password_page {
                    "百度网盘分享存在，需要提取码".into()
                } else {
                    "百度网盘分享页面可访问".into()
                },
                risk_limited: false,
            },
            retryable: false,
        };
    }

    if risk_markers.iter().any(|marker| lower.contains(marker)) {
        return ProbeAttempt {
            outcome: ProbeOutcome {
                status: "error",
                message: "百度网盘要求安全验证，无法判断链接状态".into(),
                risk_limited: true,
            },
            retryable: false,
        };
    }

    ProbeAttempt {
        outcome: ProbeOutcome {
            status: "error",
            message: format!(
                "百度网盘返回了无法识别的页面（HTTP {}），未判定为失效",
                status.as_u16()
            ),
            risk_limited: false,
        },
        retryable: false,
    }
}

fn remember_cookies(response: &reqwest::Response, cookies: &mut BTreeMap<String, String>) {
    for header in response.headers().get_all(SET_COOKIE) {
        let Ok(value) = header.to_str() else { continue };
        let Some(pair) = value.split(';').next() else {
            continue;
        };
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if !name.trim().is_empty() {
            cookies.insert(name.trim().into(), value.trim().into());
        }
    }
}

fn cookie_header(cookies: &BTreeMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

async fn probe_once(
    client: &Client,
    url: &str,
    cookies: &mut BTreeMap<String, String>,
) -> ProbeAttempt {
    let mut current_url = match Url::parse(url) {
        Ok(url) => url,
        Err(_) => {
            return ProbeAttempt {
                outcome: ProbeOutcome {
                    status: "error",
                    message: "百度网盘链接格式无效".into(),
                    risk_limited: false,
                },
                retryable: false,
            }
        }
    };
    let mut redirects = 0usize;
    let mut response = loop {
        let mut request = client
            .get(current_url.clone())
            .header(
                ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
            )
            .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.7");
        if !cookies.is_empty() {
            request = request.header(COOKIE, cookie_header(cookies));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                let retryable = error.is_timeout() || error.is_connect();
                return ProbeAttempt {
                    outcome: ProbeOutcome {
                        status: "error",
                        message: if error.is_timeout() {
                            "连接百度网盘超时".into()
                        } else if error.is_connect() {
                            "无法连接百度网盘，请检查网络".into()
                        } else {
                            format!("检查请求失败：{error}")
                        },
                        risk_limited: false,
                    },
                    retryable,
                };
            }
        };
        remember_cookies(&response, cookies);
        if !response.status().is_redirection() {
            break response;
        }
        let Some(location) = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
        else {
            break response;
        };
        let next_url = match current_url.join(location) {
            Ok(url) => url,
            Err(_) => break response,
        };
        if !matches!(next_url.scheme(), "http" | "https") || !is_baidu_host(&next_url) {
            return ProbeAttempt {
                outcome: ProbeOutcome {
                    status: "error",
                    message: "百度网盘将请求重定向到了非百度页面，已停止检查".into(),
                    risk_limited: false,
                },
                retryable: false,
            };
        }
        if response.url() == &next_url {
            break response;
        }
        redirects += 1;
        if redirects > 5 {
            return ProbeAttempt {
                outcome: ProbeOutcome {
                    status: "error",
                    message: "百度网盘页面重定向次数过多".into(),
                    risk_limited: false,
                },
                retryable: false,
            };
        }
        if cookies.len() > 64 {
            cookies.clear();
        }
        current_url = next_url;
    };
    let status = response.status();
    let final_url = response.url().clone();
    match read_limited_body(&mut response).await {
        Ok(bytes) => classify_response(status, &final_url, &String::from_utf8_lossy(&bytes)),
        Err(message) => ProbeAttempt {
            outcome: ProbeOutcome {
                status: "error",
                message,
                risk_limited: false,
            },
            retryable: true,
        },
    }
}

async fn probe_with_retry(
    client: &Client,
    url: &str,
    cancelled: &AtomicBool,
    retry_security_check: bool,
) -> ProbeOutcome {
    let mut cookies = BTreeMap::new();
    let first = probe_once(client, url, &mut cookies).await;
    if !should_retry(&first, retry_security_check) || cancelled.load(Ordering::SeqCst) {
        return first.outcome;
    }
    sleep(Duration::from_secs(2)).await;
    if cancelled.load(Ordering::SeqCst) {
        return first.outcome;
    }
    probe_once(client, url, &mut cookies).await.outcome
}

fn should_retry(attempt: &ProbeAttempt, retry_security_check: bool) -> bool {
    attempt.retryable || (retry_security_check && attempt.outcome.risk_limited)
}

pub async fn check_all(
    state: &AppState,
    app: &AppHandle,
    cancel: Arc<AtomicBool>,
) -> Result<LinkCheckReport, String> {
    let library_id = state.active_library_id()?;
    let targets = state
        .with_library_if_id(&library_id, |connection, _| {
            db::all_link_check_targets(connection)
        })?
        .ok_or("素材库已切换，链接检查已停止")?;
    let mut groups: BTreeMap<String, Vec<LinkCheckTarget>> = BTreeMap::new();
    let mut skipped = 0usize;
    for target in targets.iter().cloned() {
        if is_baidu_share_url(&target.normalized_share_url) {
            groups
                .entry(target.normalized_share_url.clone())
                .or_default()
                .push(target);
        } else {
            skipped += 1;
        }
    }
    let mut report = LinkCheckReport {
        total_assets: targets.len(),
        unique_links: groups.len(),
        skipped,
        ..Default::default()
    };
    let total = groups.values().map(Vec::len).sum();
    let mut progress = LinkCheckProgress {
        total,
        ..Default::default()
    };
    let http = client()?;
    let mut risk_streak = 0usize;
    for (index, (normalized, assets)) in groups.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            report.cancelled = true;
            report.stopped_reason = Some("用户已取消检查".into());
            break;
        }
        if index > 0 {
            sleep(Duration::from_millis(800)).await;
            if cancel.load(Ordering::SeqCst) {
                report.cancelled = true;
                report.stopped_reason = Some("用户已取消检查".into());
                break;
            }
        }
        progress.current_url = Some(normalized.clone());
        let _ = app.emit("share-link-check-progress", progress.clone());
        let outcome = probe_with_retry(&http, &assets[0].share_url, &cancel, false).await;
        if cancel.load(Ordering::SeqCst) {
            report.cancelled = true;
            report.stopped_reason = Some("用户已取消检查".into());
            break;
        }
        let checked_at = Utc::now().to_rfc3339();
        let ids: Vec<String> = assets.iter().map(|asset| asset.id.clone()).collect();
        let saved = state.with_library_if_id(&library_id, |connection, _| {
            db::save_link_check_result(
                connection,
                &ids,
                outcome.status,
                &checked_at,
                &outcome.message,
            )
        })?;
        if saved.is_none() {
            report.cancelled = true;
            report.stopped_reason = Some("素材库已切换，检查结果未写入新素材库".into());
            break;
        }
        let count = ids.len();
        progress.checked += count;
        report.checked_assets += count;
        match outcome.status {
            "valid" => {
                progress.valid += count;
                report.valid += count;
            }
            "invalid" => {
                progress.invalid += count;
                report.invalid += count;
            }
            _ => {
                progress.error += count;
                report.error += count;
            }
        }
        risk_streak = if outcome.risk_limited {
            risk_streak + 1
        } else {
            0
        };
        progress.current_url = None;
        let _ = app.emit("share-link-check-progress", progress.clone());
        if risk_streak >= 3 {
            report.stopped_reason =
                Some("百度网盘连续要求安全验证或限制访问，已提前停止，请稍后再试".into());
            break;
        }
    }
    Ok(report)
}

pub async fn check_one(
    state: &AppState,
    id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<LinkCheckResult, String> {
    let library_id = state.active_library_id()?;
    let target = state
        .with_library_if_id(&library_id, |connection, _| {
            db::link_check_target(connection, id)
        })?
        .ok_or("素材库已切换，链接检查已停止")?;
    if !is_baidu_share_url(&target.normalized_share_url) {
        return Err("仅支持检查百度网盘分享链接".into());
    }
    let outcome = probe_with_retry(&client()?, &target.share_url, &cancel, true).await;
    if cancel.load(Ordering::SeqCst) {
        return Err("链接检查已取消".into());
    }
    let checked_at = Utc::now().to_rfc3339();
    let saved = state.with_library_if_id(&library_id, |connection, _| {
        db::save_link_check_result(
            connection,
            std::slice::from_ref(&target.id),
            outcome.status,
            &checked_at,
            &outcome.message,
        )
    })?;
    if saved.is_none() {
        return Err("素材库已切换，检查结果未写入新素材库".into());
    }
    Ok(LinkCheckResult {
        asset_id: target.id,
        status: outcome.status.into(),
        checked_at: Some(checked_at),
        message: outcome.message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_baidu_pan_hosts() {
        assert!(is_baidu_share_url("https://pan.baidu.com/s/abc"));
        assert!(is_baidu_share_url("https://sub.pan.baidu.com/s/abc"));
        assert!(!is_baidu_share_url("https://pan.baidu.com.evil.test/s/abc"));
        assert!(!is_baidu_share_url("https://example.com/s/abc"));
    }

    #[test]
    fn classifies_definite_invalid_and_password_pages() {
        let url = Url::parse("https://pan.baidu.com/s/abc").unwrap();
        assert_eq!(
            classify_response(StatusCode::OK, &url, "啊哦，你来晚了，该分享文件已过期")
                .outcome
                .status,
            "invalid"
        );
        let init = Url::parse("https://pan.baidu.com/share/init?surl=abc").unwrap();
        assert_eq!(
            classify_response(StatusCode::OK, &init, "请输入提取码")
                .outcome
                .status,
            "valid"
        );
        assert_eq!(
            classify_response(
                StatusCode::OK,
                &init,
                "正常提取码页面的公共脚本包含安全验证字样"
            )
            .outcome
            .status,
            "valid"
        );
    }

    #[test]
    fn ambiguous_pages_are_errors_not_invalid() {
        let url = Url::parse("https://pan.baidu.com/s/abc").unwrap();
        assert_eq!(
            classify_response(StatusCode::OK, &url, "普通页面")
                .outcome
                .status,
            "error"
        );
    }

    #[test]
    fn single_recheck_retries_a_transient_security_page() {
        let url = Url::parse("https://pan.baidu.com/wappass/check").unwrap();
        let attempt = classify_response(StatusCode::FORBIDDEN, &url, "安全验证");
        assert!(should_retry(&attempt, true));
        assert!(!should_retry(&attempt, false));
    }
}
