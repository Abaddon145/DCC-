use crate::{db, fab, models::FabDuplicateMatch, state::AppState};
use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;

const SERVICE: &str = "DCCAssetLibrary.FabAccount";
const ACCOUNT: &str = "default";
const LOGIN_WINDOW: &str = "fab-epic-login";
const OAUTH_HOST: &str = "account-public-service-prod03.ol.epicgames.com";
// Epic Games Launcher 的公开客户端标识，只用来换取当前用户自己的令牌。
const LAUNCHER_CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const LAUNCHER_CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
const LAUNCHER_USER_AGENT: &str =
    "UELauncher/11.0.1-14937197+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";
const MAX_LIBRARY_PAGES: usize = 400;
const MAX_LIBRARY_ITEMS: usize = 20_000;
const LIBRARY_DONE_HOST: &str = "fab-login.invalid";

const LOGIN_WATCH_SCRIPT: &str = r#"
(() => {
  if (window.__dccFabLoginWatch) return;
  window.__dccFabLoginWatch = true;
  const read = () => {
    let text = "";
    try { text = (document.body && document.body.innerText) || ""; } catch (e) { return; }
    const start = text.indexOf("{");
    const end = text.lastIndexOf("}");
    if (start < 0 || end <= start) return;
    let data;
    try { data = JSON.parse(text.slice(start, end + 1)); } catch (e) { return; }
    const code = data && (data.authorizationCode || data.code);
    if (typeof code !== "string" || code.length < 8 || code.length > 512) return;
    location.replace("https://fab-login.invalid/done?code=" + encodeURIComponent(code));
  };
  setInterval(read, 500);
})();
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabAccountStatus {
    pub signed_in: bool,
    pub display_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedFabRow {
    pub title: String,
    pub url: String,
    pub listing_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedFabPreview {
    pub display_name: String,
    pub fetched: usize,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub skipped_count: usize,
    pub rows: Vec<OwnedFabRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySyncProgress {
    phase: String,
    fetched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSession {
    refresh_token: String,
    account_id: String,
    display_name: String,
}

#[derive(Debug, Clone)]
struct LiveSession {
    access_token: String,
    account_id: String,
    display_name: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    account_id: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedLibraryItem {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedLibraryPage {
    pub items: Vec<ParsedLibraryItem>,
    pub next_cursor: Option<String>,
}

fn sessions() -> &'static Mutex<Option<LiveSession>> {
    static SESSIONS: OnceLock<Mutex<Option<LiveSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(None))
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|error| format!("无法访问 Windows 凭据管理器：{error}"))
}

pub(crate) fn login_page_url() -> String {
    let redirect = format!(
        "https://www.epicgames.com/id/api/redirect?clientId={LAUNCHER_CLIENT_ID}&responseType=code"
    );
    format!(
        "https://www.epicgames.com/id/login?redirectUrl={}",
        urlencoding_query(&redirect)
    )
}

fn urlencoding_query(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub(crate) fn accept_authorization_code(code: &str) -> Option<String> {
    let code = code.trim();
    if (8..=512).contains(&code.len())
        && code
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~'))
    {
        Some(code.to_string())
    } else {
        None
    }
}

pub(crate) fn authorization_code_from_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str().unwrap_or_default();
    let allowed = host == LIBRARY_DONE_HOST
        || (host.ends_with("epicgames.com") && url.path().contains("/id/api/redirect"));
    if !allowed {
        return None;
    }
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code" || key == "authorizationCode")
        .map(|(_, value)| value.into_owned())?;
    accept_authorization_code(&code)
}

pub(crate) fn authorization_code_from_document(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let code = value
        .get("authorizationCode")
        .or_else(|| value.get("code"))
        .and_then(Value::as_str)?;
    accept_authorization_code(code)
}

pub(crate) fn canonical_listing_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.starts_with('/') {
        format!("https://www.fab.com{trimmed}")
    } else {
        trimmed.to_string()
    };
    fab::listing_id_from_url(&candidate)
        .ok()
        .map(|id| format!("https://www.fab.com/listings/{id}"))
}

pub(crate) fn parse_library_page(body: &str) -> Result<ParsedLibraryPage, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| "Fab 已购库返回的不是可识别的列表".to_string())?;
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or("Fab 已购库没有返回商品列表")?;
    let mut items = Vec::new();
    for item in results {
        let title = item
            .get("title")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("未命名商品")
            .trim();
        let url = item
            .get("url")
            .or_else(|| item.get("listingUrl"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        items.push(ParsedLibraryItem {
            title: if title.is_empty() {
                "未命名商品".into()
            } else {
                title.to_string()
            },
            url,
        });
    }
    let next_cursor = value
        .pointer("/cursors/next")
        .or_else(|| value.get("cursor"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string);
    Ok(ParsedLibraryPage { items, next_cursor })
}

pub(crate) fn organize_owned_items(items: Vec<ParsedLibraryItem>) -> Vec<OwnedFabRow> {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for item in items {
        match canonical_listing_url(&item.url) {
            Some(url) => {
                let listing_id = url.rsplit('/').next().unwrap_or_default().to_string();
                if !seen.insert(listing_id.clone()) {
                    continue;
                }
                rows.push(OwnedFabRow {
                    title: item.title,
                    listing_id,
                    url,
                    status: "new".into(),
                    message: String::new(),
                });
            }
            None => rows.push(OwnedFabRow {
                title: item.title,
                url: item.url,
                listing_id: String::new(),
                status: "skipped".into(),
                message: "没有独立商品页，未导入".into(),
            }),
        }
    }
    rows
}

pub(crate) fn write_listing_csv(urls: &[String]) -> Result<PathBuf, String> {
    if urls.is_empty() {
        return Err("没有选择新的商品".into());
    }
    if urls.len() > MAX_LIBRARY_ITEMS {
        return Err(format!("一次最多导入 {MAX_LIBRARY_ITEMS} 条"));
    }
    let mut seen = HashSet::new();
    let mut clean = Vec::new();
    for url in urls {
        let Some(canonical) = canonical_listing_url(url) else {
            return Err("选择里包含无法导入的链接".into());
        };
        if seen.insert(canonical.clone()) {
            clean.push(canonical);
        }
    }
    if clean.is_empty() {
        return Err("没有选择新的商品".into());
    }
    let path = std::env::temp_dir().join(format!("dcc-fab-library-{}.csv", uuid::Uuid::new_v4()));
    let mut writer =
        csv::Writer::from_path(&path).map_err(|error| format!("无法准备导入清单：{error}"))?;
    writer
        .write_record(["fab_url"])
        .map_err(|error| format!("无法准备导入清单：{error}"))?;
    for url in &clean {
        writer
            .write_record([url])
            .map_err(|error| format!("无法准备导入清单：{error}"))?;
    }
    writer
        .flush()
        .map_err(|error| format!("无法准备导入清单：{error}"))?;
    Ok(path)
}

fn account_id_ok(account_id: &str) -> bool {
    account_id.len() == 32 && account_id.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(LAUNCHER_USER_AGENT)
        .build()
        .map_err(|error| format!("无法初始化网络请求：{error}"))
}

async fn request_token(
    form: &[(&str, &str)],
    clear_on_reject: bool,
) -> Result<TokenResponse, String> {
    let response = http_client()?
        .post(format!("https://{OAUTH_HOST}/account/api/oauth/token"))
        .basic_auth(LAUNCHER_CLIENT_ID, Some(LAUNCHER_CLIENT_SECRET))
        .form(form)
        .send()
        .await
        .map_err(|_| "无法连接 Epic 登录服务".to_string())?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|_| "Epic 登录服务没有返回结果".to_string())?;
    if !status.is_success() {
        let rejected = body.to_ascii_lowercase().contains("invalid_grant")
            || body.contains("invalid_refresh_token")
            || body.contains("invalid_token");
        if clear_on_reject && rejected {
            clear_session();
            return Err("登录已失效，请重新登录".into());
        }
        return Err("Epic 没有接受这次登录，请重试".into());
    }
    serde_json::from_str(&body).map_err(|_| "Epic 登录回执无法识别".to_string())
}

fn remember(token: TokenResponse) -> Result<LiveSession, String> {
    if !account_id_ok(&token.account_id) || token.refresh_token.trim().is_empty() {
        return Err("Epic 登录回执不完整，请重试".into());
    }
    let stored = StoredSession {
        refresh_token: token.refresh_token,
        account_id: token.account_id.clone(),
        display_name: if token.display_name.trim().is_empty() {
            "Epic 账号".into()
        } else {
            token.display_name.trim().to_string()
        },
    };
    let raw = serde_json::to_string(&stored).map_err(|error| error.to_string())?;
    entry()?
        .set_password(&raw)
        .map_err(|error| format!("无法把登录凭证保存到 Windows 凭据管理器：{error}"))?;
    let saved = entry()?
        .get_password()
        .map_err(|error| format!("无法确认登录凭证已保存：{error}"))?;
    if saved != raw {
        return Err("Windows 凭据管理器未能保存登录凭证，请重试".into());
    }
    let live = LiveSession {
        access_token: token.access_token,
        account_id: stored.account_id,
        display_name: stored.display_name,
        expires_at: chrono::Utc::now()
            + chrono::Duration::seconds(token.expires_in.clamp(60, 60 * 60 * 48)),
    };
    *sessions().lock().unwrap_or_else(|error| error.into_inner()) = Some(live.clone());
    Ok(live)
}

fn clear_session() {
    *sessions().lock().unwrap_or_else(|error| error.into_inner()) = None;
    if let Ok(entry) = entry() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {}
        }
    }
}

fn load_stored() -> Result<Option<StoredSession>, String> {
    match entry()?.get_password() {
        Ok(raw) => match serde_json::from_str::<StoredSession>(&raw) {
            Ok(session)
                if account_id_ok(&session.account_id)
                    && !session.refresh_token.trim().is_empty() =>
            {
                Ok(Some(session))
            }
            _ => {
                clear_session();
                Err("保存的登录已损坏，请重新登录".into())
            }
        },
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取登录凭证：{error}")),
    }
}

async fn ensure_live() -> Result<LiveSession, String> {
    {
        let guard = sessions().lock().unwrap_or_else(|error| error.into_inner());
        if let Some(live) = guard.as_ref() {
            if live.expires_at - chrono::Utc::now() > chrono::Duration::minutes(5) {
                return Ok(live.clone());
            }
        }
    }
    let stored = load_stored()?.ok_or("尚未登录 Fab")?;
    let token = request_token(
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &stored.refresh_token),
            ("token_type", "eg1"),
        ],
        true,
    )
    .await?;
    remember(token)
}

async fn exchange_code(code: &str) -> Result<LiveSession, String> {
    let token = request_token(
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("token_type", "eg1"),
        ],
        false,
    )
    .await?;
    remember(token)
}

fn emit_sync(app: &AppHandle, phase: &str, fetched: usize) {
    let _ = app.emit(
        "fab-library-sync",
        LibrarySyncProgress {
            phase: phase.into(),
            fetched,
        },
    );
}

async fn fetch_owned_pages(
    app: &AppHandle,
    live: &LiveSession,
) -> Result<Vec<ParsedLibraryItem>, String> {
    let client = http_client()?;
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    let mut items = Vec::new();
    for _ in 0..MAX_LIBRARY_PAGES {
        let mut url = format!(
            "https://www.fab.com/e/accounts/{}/ue/library?count=100",
            live.account_id
        );
        if let Some(cursor) = cursor.as_deref() {
            url.push_str("&cursor=");
            url.push_str(&urlencoding_query(cursor));
        }
        let response = client
            .get(&url)
            .bearer_auth(&live.access_token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| "无法连接 Fab".to_string())?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|_| "Fab 已购库没有返回结果".to_string())?;
        if status.as_u16() == 401 || status.as_u16() == 403 {
            *sessions().lock().unwrap_or_else(|error| error.into_inner()) = None;
            return Err("登录已失效，请重新登录".into());
        }
        if !status.is_success() {
            return Err("Fab 没有返回已购库，请稍后重试".into());
        }
        let page = parse_library_page(&body)?;
        items.extend(page.items);
        emit_sync(app, "正在读取已购库", items.len());
        if items.len() > MAX_LIBRARY_ITEMS {
            return Err(format!(
                "已购库超过 {MAX_LIBRARY_ITEMS} 条，已停止以免一次写入过多"
            ));
        }
        match page.next_cursor {
            Some(next) if seen_cursors.insert(next.clone()) => cursor = Some(next),
            _ => return Ok(items),
        }
    }
    Err("已购库分页没有结束，已停止".into())
}

fn apply_duplicates(
    connection: &rusqlite::Connection,
    rows: &mut [OwnedFabRow],
) -> Result<(), String> {
    for row in rows.iter_mut().filter(|row| row.status == "new") {
        if let Some(existing) = db::fab_duplicate_match(connection, &row.listing_id, None)? {
            row.status = "duplicate".into();
            row.message = duplicate_message(&existing);
        }
    }
    Ok(())
}

fn duplicate_message(existing: &FabDuplicateMatch) -> String {
    if existing.location == "trash" {
        format!("已在回收站：{}", existing.asset_name)
    } else if existing.category_path.is_empty() {
        format!("已在库中：{}", existing.asset_name)
    } else {
        format!(
            "已在库中：{} · {}",
            existing.asset_name, existing.category_path
        )
    }
}

fn summarize(display_name: String, fetched: usize, rows: Vec<OwnedFabRow>) -> OwnedFabPreview {
    let new_count = rows.iter().filter(|row| row.status == "new").count();
    let duplicate_count = rows.iter().filter(|row| row.status == "duplicate").count();
    let skipped_count = rows.iter().filter(|row| row.status == "skipped").count();
    OwnedFabPreview {
        display_name,
        fetched,
        new_count,
        duplicate_count,
        skipped_count,
        rows,
    }
}

#[tauri::command]
pub fn fab_account_status() -> Result<FabAccountStatus, String> {
    match load_stored() {
        Ok(Some(session)) => Ok(FabAccountStatus {
            signed_in: true,
            display_name: session.display_name,
            message: String::new(),
        }),
        Ok(None) => Ok(FabAccountStatus {
            signed_in: false,
            display_name: String::new(),
            message: "尚未登录".into(),
        }),
        Err(error) => Ok(FabAccountStatus {
            signed_in: false,
            display_name: String::new(),
            message: error,
        }),
    }
}

#[tauri::command]
pub async fn begin_fab_login(app: AppHandle) -> Result<FabAccountStatus, String> {
    if app.get_webview_window(LOGIN_WINDOW).is_some() {
        return Err("登录窗口已经打开".into());
    }
    let code = capture_authorization_code(&app).await?;
    let live = exchange_code(&code).await?;
    Ok(FabAccountStatus {
        signed_in: true,
        display_name: live.display_name,
        message: String::new(),
    })
}

async fn capture_authorization_code(app: &AppHandle) -> Result<String, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let slot = std::sync::Arc::new(Mutex::new(Some(sender)));
    let deliver = {
        let slot = std::sync::Arc::clone(&slot);
        move |result: Result<String, String>| {
            if let Some(sender) = slot
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
            {
                let _ = sender.send(result);
            }
        }
    };
    let deliver_navigation = deliver.clone();
    let window = tauri::WebviewWindowBuilder::new(
        app,
        LOGIN_WINDOW,
        tauri::WebviewUrl::External(
            login_page_url()
                .parse()
                .map_err(|_| "无法打开 Epic 登录页".to_string())?,
        ),
    )
    .title("登录 Epic 账号")
    .inner_size(480.0, 760.0)
    .min_inner_size(420.0, 640.0)
    .resizable(true)
    .center()
    .incognito(true)
    .initialization_script(LOGIN_WATCH_SCRIPT)
    .on_navigation(move |url| {
        if let Some(code) = authorization_code_from_url(url.as_str()) {
            deliver_navigation(Ok(code));
            false
        } else {
            true
        }
    })
    .build()
    .map_err(|error| format!("无法打开登录窗口：{error}"))?;
    let deliver_close = deliver.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
            deliver_close(Err("已取消登录".into()));
        }
    });
    let _ = window.set_focus();
    let result = receiver.await.map_err(|_| "登录窗口已关闭".to_string())?;
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = window.close();
    }
    result
}

#[tauri::command]
pub fn logout_fab_account() -> Result<(), String> {
    clear_session();
    Ok(())
}

#[tauri::command]
pub async fn preview_owned_fab_library(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OwnedFabPreview, String> {
    let live = ensure_live().await?;
    let items = fetch_owned_pages(&app, &live).await?;
    let fetched = items.len();
    let mut rows = organize_owned_items(items);
    emit_sync(&app, "正在和栈藏比对", fetched);
    state.with_library(|connection, _| apply_duplicates(connection, &mut rows))?;
    Ok(summarize(live.display_name, fetched, rows))
}

#[tauri::command]
pub fn stage_owned_fab_import(urls: Vec<String>) -> Result<String, String> {
    write_listing_csv(&urls).map(|path| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_authorization_code_only_from_the_login_handoff() {
        let code = "abcDEF123456";
        assert_eq!(
            authorization_code_from_url(&format!("https://fab-login.invalid/done?code={code}"))
                .as_deref(),
            Some(code)
        );
        assert_eq!(
            authorization_code_from_url(&format!(
                "https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&authorizationCode={code}"
            ))
            .as_deref(),
            Some(code)
        );
        assert!(authorization_code_from_url(
            "https://www.epicgames.com/id/login?code=abcDEF123456"
        )
        .is_none());
        assert!(authorization_code_from_url("https://fab-login.invalid/done?code=short").is_none());
        assert!(
            authorization_code_from_document("{\"authorizationCode\":\"abcDEF123456\"}").is_some()
        );
        assert!(authorization_code_from_document("login form").is_none());
        assert!(LOGIN_WATCH_SCRIPT.contains("authorizationCode"));
        assert!(LOGIN_WATCH_SCRIPT.contains(LIBRARY_DONE_HOST));
    }

    #[test]
    fn organizes_listing_urls_and_skips_collections() {
        let id = "11111111-1111-4111-8111-111111111111";
        let rows = organize_owned_items(vec![
            ParsedLibraryItem {
                title: "岩石".into(),
                url: format!("https://www.fab.com/zh-cn/listings/{id}/"),
            },
            ParsedLibraryItem {
                title: "岩石重复".into(),
                url: format!("https://fab.com/listings/{id}?lang=en"),
            },
            ParsedLibraryItem {
                title: "合集".into(),
                url: "https://www.fab.com/library/assets/megascans".into(),
            },
            ParsedLibraryItem {
                title: "空链接".into(),
                url: String::new(),
            },
        ]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].status, "new");
        assert_eq!(rows[0].listing_id, id);
        assert_eq!(rows[0].url, format!("https://www.fab.com/listings/{id}"));
        assert!(rows.iter().skip(1).all(|row| row.status == "skipped"));
    }

    #[test]
    fn parses_a_library_page_and_writes_only_listing_urls() {
        let body = r#"{"cursors":{"next":"page-2"},"results":[{"title":"City Kit","url":"https://www.fab.com/listings/22222222-2222-4222-8222-222222222222","assetId":"not-the-listing"},{"title":"","url":"/listings/33333333-3333-4333-8333-333333333333"}]}"#;
        let page = parse_library_page(body).unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("page-2"));
        assert_eq!(page.items[1].title, "未命名商品");
        let rows = organize_owned_items(page.items);
        let path =
            write_listing_csv(&rows.iter().map(|row| row.url.clone()).collect::<Vec<_>>()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(text.lines().next().unwrap().contains("fab_url"));
        assert_eq!(text.lines().filter(|line| !line.is_empty()).count(), 3);
        assert!(write_listing_csv(&["https://www.fab.com/library/assets/nope".into()]).is_err());
    }
}
