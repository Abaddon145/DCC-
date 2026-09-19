use crate::models::{
    BaiduDeviceAuthorization, BaiduNetdiskFolder, BaiduNetdiskSettings, BaiduSaveTask,
    BaiduTransferResult,
};
use chrono::Utc;
use keyring::Entry;
use reqwest::{multipart, Client, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use url::Url;

const SERVICE: &str = "DCCAssetLibrary.BaiduNetdisk";
const ACCOUNT: &str = "default";
const OAUTH_BASE: &str = "https://openapi.baidu.com";
const PAN_BASE: &str = "https://pan.baidu.com";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CredentialBundle {
    #[serde(default)]
    app_id: String,
    #[serde(default)]
    app_key: String,
    #[serde(default)]
    secret_key: String,
    token: Option<TokenCredential>,
    pending: Option<PendingAuthorization>,
    account_name: Option<String>,
    #[serde(default = "root_path")]
    default_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenCredential {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingAuthorization {
    device_code: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    verification_url: String,
    qrcode_url: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

#[derive(Debug, Deserialize)]
struct PanReply<T> {
    #[serde(default)]
    errno: i64,
    data: Option<T>,
    #[serde(default)]
    show_msg: String,
}

#[derive(Debug, Deserialize)]
struct FolderListReply {
    #[serde(default)]
    errno: i64,
    #[serde(default)]
    list: Vec<FolderItem>,
}

#[derive(Debug, Deserialize)]
struct FolderItem {
    #[serde(default)]
    path: String,
    #[serde(default)]
    server_filename: String,
    #[serde(default)]
    isdir: i64,
}

#[derive(Debug, Deserialize)]
struct ShareListData {
    #[serde(default)]
    list: Vec<ShareItem>,
}

#[derive(Debug, Deserialize)]
struct ShareItem {
    fs_id: Value,
}

#[derive(Debug, Deserialize)]
struct VerifyData {
    spwd: String,
}

#[derive(Debug, Deserialize)]
struct TransferData {
    task_id: Value,
}

#[derive(Debug, Deserialize)]
struct TaskData {
    #[serde(default)]
    status: Value,
    #[serde(default)]
    progress: i64,
    #[serde(default)]
    desc: TaskDescription,
}

#[derive(Debug, Default, Deserialize)]
struct TaskDescription {
    #[serde(default, rename = "succNum")]
    success_count: usize,
}

fn root_path() -> String {
    "/".into()
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|_| "无法访问 Windows 凭据管理器".into())
}

fn load_bundle() -> Result<CredentialBundle, String> {
    match entry()?.get_password() {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|_| "保存的百度网盘授权信息已损坏，请重新配置".into()),
        Err(keyring::Error::NoEntry) => Ok(CredentialBundle::default()),
        Err(_) => Err("读取百度网盘授权信息失败".into()),
    }
}

fn save_bundle(bundle: &CredentialBundle) -> Result<(), String> {
    let value = serde_json::to_string(bundle).map_err(|_| "序列化网盘授权信息失败".to_string())?;
    entry()?
        .set_password(&value)
        .map_err(|_| "保存百度网盘授权信息失败".to_string())
}

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("StackDCCAssetLibrary/0.8")
        .build()
        .map_err(|_| "初始化百度网盘连接失败".into())
}

fn network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "连接百度网盘超时".into()
    } else {
        "无法连接百度网盘，请检查网络后重试".into()
    }
}

async fn json_response<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| network_error(&error))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        if status.is_success() {
            "百度网盘返回了无法识别的数据".into()
        } else {
            format!("百度网盘请求失败（HTTP {}）", status.as_u16())
        }
    })?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let description = value
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or(error);
        return Err(match error {
            "authorization_pending" => "尚未完成百度账号授权，请授权后重试".into(),
            "slow_down" => "授权查询过于频繁，请稍后重试".into(),
            "expired_token" | "invalid_grant" => "授权码已失效，请重新发起授权".into(),
            _ => format!("百度网盘授权失败：{description}"),
        });
    }
    serde_json::from_value(value).map_err(|_| "百度网盘响应字段不完整".into())
}

fn ensure_configured(bundle: &CredentialBundle) -> Result<(), String> {
    if bundle.app_id.trim().is_empty()
        || bundle.app_key.trim().is_empty()
        || bundle.secret_key.trim().is_empty()
    {
        Err("请先在设置中心配置百度网盘 App ID、App Key 和 Secret Key".into())
    } else {
        Ok(())
    }
}

fn settings(bundle: &CredentialBundle) -> BaiduNetdiskSettings {
    BaiduNetdiskSettings {
        configured: ensure_configured(bundle).is_ok(),
        connected: bundle.token.is_some(),
        account_name: bundle.account_name.clone(),
        default_path: if bundle.default_path.is_empty() {
            root_path()
        } else {
            bundle.default_path.clone()
        },
    }
}

pub fn get_settings() -> Result<BaiduNetdiskSettings, String> {
    Ok(settings(&load_bundle()?))
}

pub fn save_credentials(app_id: &str, app_key: &str, secret_key: &str) -> Result<(), String> {
    if app_id.trim().is_empty() || app_key.trim().is_empty() || secret_key.trim().is_empty() {
        return Err("App ID、App Key 和 Secret Key 不能为空".into());
    }
    let mut bundle = load_bundle()?;
    let changed = bundle.app_id != app_id.trim()
        || bundle.app_key != app_key.trim()
        || bundle.secret_key != secret_key.trim();
    bundle.app_id = app_id.trim().into();
    bundle.app_key = app_key.trim().into();
    bundle.secret_key = secret_key.trim().into();
    if changed {
        bundle.token = None;
        bundle.pending = None;
        bundle.account_name = None;
    }
    save_bundle(&bundle)
}

pub fn delete_credentials() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("删除百度网盘授权信息失败".into()),
    }
}

pub fn disconnect() -> Result<BaiduNetdiskSettings, String> {
    let mut bundle = load_bundle()?;
    bundle.token = None;
    bundle.pending = None;
    bundle.account_name = None;
    save_bundle(&bundle)?;
    Ok(settings(&bundle))
}

pub async fn start_authorization() -> Result<BaiduDeviceAuthorization, String> {
    let mut bundle = load_bundle()?;
    ensure_configured(&bundle)?;
    let response = client()?
        .get(format!("{OAUTH_BASE}/oauth/2.0/device/code"))
        .query(&[
            ("response_type", "device_code"),
            ("client_id", bundle.app_key.as_str()),
            ("scope", "basic,netdisk"),
        ])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let reply: DeviceCodeReply = json_response(response).await?;
    bundle.pending = Some(PendingAuthorization {
        device_code: reply.device_code,
        expires_at: Utc::now().timestamp() + reply.expires_in,
    });
    save_bundle(&bundle)?;
    Ok(BaiduDeviceAuthorization {
        user_code: reply.user_code,
        verification_url: reply.verification_url,
        qrcode_url: reply.qrcode_url,
        expires_in: reply.expires_in,
        interval: reply.interval,
    })
}

pub async fn complete_authorization() -> Result<BaiduNetdiskSettings, String> {
    let mut bundle = load_bundle()?;
    ensure_configured(&bundle)?;
    let pending = bundle
        .pending
        .clone()
        .ok_or_else(|| "请先发起百度网盘账号授权".to_string())?;
    if pending.expires_at <= Utc::now().timestamp() {
        bundle.pending = None;
        save_bundle(&bundle)?;
        return Err("授权码已失效，请重新发起授权".into());
    }
    let response = client()?
        .get(format!("{OAUTH_BASE}/oauth/2.0/token"))
        .query(&[
            ("grant_type", "device_token"),
            ("code", pending.device_code.as_str()),
            ("client_id", bundle.app_key.as_str()),
            ("client_secret", bundle.secret_key.as_str()),
        ])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let reply: TokenReply = json_response(response).await?;
    bundle.token = Some(TokenCredential {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        expires_at: Utc::now().timestamp() + reply.expires_in - 60,
        scope: reply.scope,
    });
    bundle.pending = None;
    save_bundle(&bundle)?;
    let access_token = bundle.token.as_ref().unwrap().access_token.clone();
    bundle.account_name = fetch_account_name(&access_token).await.ok();
    save_bundle(&bundle)?;
    Ok(settings(&bundle))
}

async fn fetch_account_name(access_token: &str) -> Result<String, String> {
    let response = client()?
        .get(format!("{PAN_BASE}/rest/2.0/xpan/nas"))
        .query(&[("method", "uinfo"), ("access_token", access_token)])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let value: Value = json_response(response).await?;
    let errno = value.get("errno").and_then(Value::as_i64).unwrap_or(0);
    if errno != 0 {
        return Err(format!("读取百度网盘账号失败（错误码 {errno}）"));
    }
    value
        .get("netdisk_name")
        .or_else(|| value.get("baidu_name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "百度网盘未返回账号名称".into())
}

async fn access_context() -> Result<(CredentialBundle, String), String> {
    let mut bundle = load_bundle()?;
    ensure_configured(&bundle)?;
    let token = bundle
        .token
        .clone()
        .ok_or_else(|| "尚未登录百度网盘，请先在设置中心完成账号授权".to_string())?;
    if token.expires_at > Utc::now().timestamp() {
        return Ok((bundle, token.access_token));
    }
    if token.refresh_token.is_empty() {
        return Err("百度网盘登录已过期，请重新授权".into());
    }
    let response = client()?
        .get(format!("{OAUTH_BASE}/oauth/2.0/token"))
        .query(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token.refresh_token.as_str()),
            ("client_id", bundle.app_key.as_str()),
            ("client_secret", bundle.secret_key.as_str()),
            ("scope", "basic,netdisk"),
        ])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let reply: TokenReply = json_response(response).await?;
    let access_token = reply.access_token.clone();
    bundle.token = Some(TokenCredential {
        access_token: reply.access_token,
        refresh_token: if reply.refresh_token.is_empty() {
            token.refresh_token
        } else {
            reply.refresh_token
        },
        expires_at: Utc::now().timestamp() + reply.expires_in - 60,
        scope: reply.scope,
    });
    save_bundle(&bundle)?;
    Ok((bundle, access_token))
}

fn validate_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() || !path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err("网盘目录必须是以 / 开头的绝对路径".into());
    }
    if path.split('/').any(|part| part == ".." || part == ".") {
        return Err("网盘目录不能包含相对路径片段".into());
    }
    Ok(if path.len() > 1 {
        path.trim_end_matches('/').to_string()
    } else {
        root_path()
    })
}

pub async fn list_folders(path: &str) -> Result<Vec<BaiduNetdiskFolder>, String> {
    let path = validate_path(path)?;
    let (_, access_token) = access_context().await?;
    let response = client()?
        .get(format!("{PAN_BASE}/rest/2.0/xpan/file"))
        .query(&[
            ("method", "list"),
            ("dir", path.as_str()),
            ("folder", "1"),
            ("order", "name"),
            ("desc", "0"),
            ("start", "0"),
            ("limit", "1000"),
            ("access_token", access_token.as_str()),
        ])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let reply: FolderListReply = json_response(response).await?;
    if reply.errno != 0 {
        return Err(map_pan_error(reply.errno, "读取网盘目录失败"));
    }
    let mut folders = reply
        .list
        .into_iter()
        .filter(|item| item.isdir == 1)
        .map(|item| {
            let name = if item.server_filename.is_empty() {
                item.path.rsplit('/').next().unwrap_or("/").to_string()
            } else {
                item.server_filename
            };
            BaiduNetdiskFolder {
                name,
                path: item.path,
            }
        })
        .collect::<Vec<_>>();
    folders.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(folders)
}

pub fn set_default_path(path: &str) -> Result<BaiduNetdiskSettings, String> {
    let mut bundle = load_bundle()?;
    bundle.default_path = validate_path(path)?;
    save_bundle(&bundle)?;
    Ok(settings(&bundle))
}

fn short_url(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw).map_err(|_| "百度网盘分享链接格式无效")?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "pan.baidu.com" && !host.ends_with(".pan.baidu.com") {
        return Err("仅支持百度网盘分享链接".into());
    }
    if let Some(value) = url
        .path_segments()
        .and_then(|mut parts| {
            while let Some(part) = parts.next() {
                if part == "s" {
                    return parts.next().map(ToString::to_string);
                }
            }
            None
        })
        .filter(|value| !value.is_empty())
    {
        return Ok(value);
    }
    url.query_pairs()
        .find(|(key, _)| key == "surl")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "无法从分享链接识别 22 位短链标识".into())
}

async fn checked_pan_reply<T: DeserializeOwned>(
    response: Response,
    action: &str,
) -> Result<T, String> {
    let reply: PanReply<T> = json_response(response).await?;
    if reply.errno != 0 {
        let message = if reply.show_msg.trim().is_empty() {
            map_pan_error(reply.errno, action)
        } else {
            format!("{}（错误码 {}）", reply.show_msg, reply.errno)
        };
        return Err(message);
    }
    reply
        .data
        .ok_or_else(|| format!("{action}：百度网盘未返回结果"))
}

fn map_pan_error(errno: i64, action: &str) -> String {
    match errno {
        -6 => "百度网盘登录已失效或权限不足，请重新授权".into(),
        2 => format!("{action}：请求参数错误"),
        13998 => "当前应用未开通文件分享服务权限".into(),
        13000 | 13001 | 13004 => "分享链接已失效、取消或不存在".into(),
        13003 => "分享链接需要提取码或提取码错误".into(),
        13070 => "转存任务暂时无法查询，请稍后在百度网盘中确认".into(),
        13071 => "已有其他转存任务正在进行，请稍后再试".into(),
        13072 | 13073 => "分享内容超过当前账号单次转存数量上限".into(),
        _ => format!("{action}（错误码 {errno}）"),
    }
}

async fn verify_share(
    bundle: &CredentialBundle,
    access_token: &str,
    short_url: &str,
    password: &str,
) -> Result<String, String> {
    if password.trim().is_empty() {
        return Ok(String::new());
    }
    let response = client()?
        .post(format!("{PAN_BASE}/apaas/1.0/share/verify"))
        .query(&[
            ("product", "netdisk"),
            ("appid", bundle.app_id.as_str()),
            ("access_token", access_token),
            ("short_url", short_url),
        ])
        .multipart(multipart::Form::new().text("pwd", password.trim().to_string()))
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    Ok(
        checked_pan_reply::<VerifyData>(response, "验证分享提取码失败")
            .await?
            .spwd,
    )
}

async fn share_file_ids(
    bundle: &CredentialBundle,
    access_token: &str,
    short_url: &str,
    spwd: &str,
) -> Result<Vec<String>, String> {
    let response = client()?
        .post(format!("{PAN_BASE}/apaas/1.0/share/list"))
        .query(&[
            ("product", "netdisk"),
            ("appid", bundle.app_id.as_str()),
            ("access_token", access_token),
            ("short_url", short_url),
        ])
        .multipart(
            multipart::Form::new()
                .text("spwd", spwd.to_string())
                .text("page", "1")
                .text("page_size", "100")
                .text("dir", "")
                .text("order_by", "name")
                .text("desc_order", "0"),
        )
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let data: ShareListData = checked_pan_reply(response, "读取分享内容失败").await?;
    let ids = data
        .list
        .into_iter()
        .filter_map(|item| match item.fs_id {
            Value::String(value) => Some(value),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if ids.is_empty() {
        Err("分享链接中没有可转存的文件".into())
    } else {
        Ok(ids)
    }
}

pub async fn transfer(
    task: &BaiduSaveTask,
    destination: &str,
) -> Result<BaiduTransferResult, String> {
    let destination = validate_path(destination)?;
    let (bundle, access_token) = access_context().await?;
    let short_url = short_url(&task.share_url)?;
    let spwd = verify_share(&bundle, &access_token, &short_url, &task.extraction_code).await?;
    let ids = share_file_ids(&bundle, &access_token, &short_url, &spwd).await?;
    let response = client()?
        .post(format!("{PAN_BASE}/apaas/1.0/share/transfer"))
        .query(&[
            ("product", "netdisk"),
            ("appid", bundle.app_id.as_str()),
            ("access_token", access_token.as_str()),
            ("short_url", short_url.as_str()),
        ])
        .multipart(
            multipart::Form::new()
                .text("spwd", spwd)
                .text("fsid_list", serde_json::to_string(&ids).unwrap_or_default())
                .text("to_path", destination)
                .text("async", "2")
                .text("ondup", "fail"),
        )
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let data: TransferData = checked_pan_reply(response, "提交转存任务失败").await?;
    let task_id = match data.task_id {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        _ => return Err("百度网盘未返回有效的转存任务 ID".into()),
    };
    Ok(BaiduTransferResult {
        asset_id: task.id.clone(),
        task_id,
        status: "submitted".into(),
        message: format!("已提交 {} 项内容，等待百度网盘完成转存", ids.len()),
        saved_count: 0,
    })
}

pub async fn query_transfer(asset_id: &str, task_id: &str) -> Result<BaiduTransferResult, String> {
    if task_id.trim().is_empty() {
        return Err("转存任务 ID 不能为空".into());
    }
    let (bundle, access_token) = access_context().await?;
    let response = client()?
        .get(format!("{PAN_BASE}/apaas/1.0/share/taskquery"))
        .query(&[
            ("appid", bundle.app_id.as_str()),
            ("access_token", access_token.as_str()),
            ("task_id", task_id.trim()),
        ])
        .send()
        .await
        .map_err(|error| network_error(&error))?;
    let data: TaskData = checked_pan_reply(response, "查询转存任务失败").await?;
    let raw_status = match data.status {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    };
    let status = match raw_status.as_str() {
        "success" | "0" => "success",
        "failed" | "fail" | "-1" => "failed",
        _ => "submitted",
    };
    let message = match status {
        "success" => format!("转存完成，共保存 {} 项", data.desc.success_count),
        "failed" => "百度网盘转存任务失败".into(),
        _ => format!("百度网盘正在转存（{}%）", data.progress.clamp(0, 100)),
    };
    Ok(BaiduTransferResult {
        asset_id: asset_id.into(),
        task_id: task_id.into(),
        status: status.into(),
        message,
        saved_count: data.desc.success_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_supported_share_identifiers() {
        assert_eq!(
            short_url("https://pan.baidu.com/s/1abcdefghijklmnopqrstu?pwd=a1b2").unwrap(),
            "1abcdefghijklmnopqrstu"
        );
        assert_eq!(
            short_url("https://pan.baidu.com/share/init?surl=abcdefghijklmnopqrstuv").unwrap(),
            "abcdefghijklmnopqrstuv"
        );
        assert!(short_url("https://example.com/s/not-baidu").is_err());
    }

    #[test]
    fn validates_cloud_paths_without_allowing_traversal() {
        assert_eq!(validate_path("/apps/栈藏/").unwrap(), "/apps/栈藏");
        assert!(validate_path("apps/栈藏").is_err());
        assert!(validate_path("/apps/../private").is_err());
    }
}
