//! Cookie bytes are DPAPI-encrypted outside libraries; Credential Manager holds
//! only the random blob identifier. No plaintext cookie is persisted in settings.
use keyring::Entry;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

static TEMP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct CookieFile {
    // Drop the no-delete guard before NamedTempFile removes its path.
    _guard: std::fs::File,
    file: tempfile::NamedTempFile,
}
impl CookieFile {
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}
fn create_plaintext(directory: &Path, bytes: &[u8]) -> Result<CookieFile, String> {
    let _lock = TEMP_LOCK.lock().map_err(|_| "临时Cookie状态异常")?;
    let mut file = tempfile::Builder::new()
        .prefix("dcc-cookie-")
        .suffix(".txt")
        .tempfile_in(directory)
        .map_err(|_| "无法创建临时Cookie")?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let guard = options
        .open(file.path())
        .map_err(|_| "无法保护临时Cookie")?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "无法写入临时Cookie")?;
    Ok(CookieFile {
        _guard: guard,
        file,
    })
}
fn clean_stale_in(directory: &Path) -> Result<(), String> {
    let _lock = TEMP_LOCK.lock().map_err(|_| "临时Cookie状态异常")?;
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("无法检查临时Cookie目录".into()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| "无法检查临时Cookie")?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("dcc-cookie-")
            || !name.ends_with(".txt")
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|_| "无法检查临时Cookie")?;
        if !metadata.is_file() || metadata.is_symlink() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        // An active download's guard refuses this exclusive open.
        match options.open(entry.path()) {
            Ok(guard) => {
                drop(guard);
                std::fs::remove_file(entry.path()).map_err(|_| "无法清理遗留Cookie")?;
            }
            Err(e) if e.raw_os_error() == Some(32) || e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法安全清理遗留Cookie".into()),
        }
    }
    Ok(())
}
pub fn clean_stale() -> Result<(), String> {
    clean_stale_in(&root()?)
}

fn entry() -> Result<Entry, String> {
    Entry::new("DCCAssetLibrary.DownloadCookies", "default")
        .map_err(|_| "无法访问 Windows 凭据管理器".into())
}
fn root() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("未找到本机应用数据目录")?)
            .join("DCCAssetLibrary/download-auth"),
    )
}
fn blob_path(id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Cookie凭据标识损坏")?;
    Ok(root()?.join(format!("{id}.bin")))
}

#[cfg(windows)]
fn crypt(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
    if bytes.len() > 1024 * 1024 {
        return Err("Cookie文件过大".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output: CRYPT_INTEGER_BLOB = unsafe { std::mem::zeroed() };
    let success = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err("Cookie加密或解密失败，请重新导入".into());
    }
    unsafe {
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}
#[cfg(not(windows))]
fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("Cookie安全存储仅支持Windows".into())
}

pub fn import(path: &std::path::Path) -> Result<(), String> {
    let size = std::fs::metadata(path)
        .map_err(|_| "无法读取所选Cookie文件")?
        .len();
    if size == 0 || size > 900_000 {
        return Err("Cookie文件大小无效".into());
    }
    let mut clear = std::fs::read(path).map_err(|_| "无法读取Cookie文件")?;
    if !String::from_utf8_lossy(&clear)
        .lines()
        .take(3)
        .any(|s| s.contains("Netscape") || s.contains("HTTP Cookie File"))
    {
        clear.fill(0);
        return Err("请导入Netscape格式cookies.txt".into());
    }
    let sealed = crypt(&clear, false);
    clear.fill(0);
    let sealed = sealed?;
    let root = root()?;
    std::fs::create_dir_all(&root).map_err(|_| "无法创建安全Cookie目录")?;
    let id = uuid::Uuid::new_v4().to_string();
    let path = blob_path(&id)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&root).map_err(|_| "无法保存加密Cookie")?;
    temporary
        .write_all(&sealed)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "无法保存加密Cookie")?;
    temporary
        .persist_noclobber(&path)
        .map_err(|_| "无法提交加密Cookie")?;
    let old = entry()?.get_password().ok();
    if entry()?.set_password(&id).is_err() {
        let _ = std::fs::remove_file(path);
        return Err("保存Cookie凭据失败".into());
    }
    if let Some(old) = old {
        if let Ok(path) = blob_path(&old) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}
pub fn configured() -> bool {
    entry()
        .ok()
        .and_then(|e| e.get_password().ok())
        .and_then(|id| blob_path(&id).ok())
        .is_some_and(|p| p.is_file())
}
pub fn delete() -> Result<(), String> {
    clean_stale()?;
    let credential = entry()?;
    if let Ok(id) = credential.get_password() {
        std::fs::remove_file(blob_path(&id)?)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            })
            .map_err(|_| "删除加密Cookie失败")?;
    }
    match credential.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("删除Cookie凭据失败".into()),
    }
}
pub fn temporary() -> Result<CookieFile, String> {
    clean_stale()?;
    let id = entry()?
        .get_password()
        .map_err(|_| "Cookie错误：尚未导入授权Cookie")?;
    let sealed = std::fs::read(blob_path(&id)?).map_err(|_| "Cookie错误：加密文件缺失")?;
    let mut clear = crypt(&sealed, true)?;
    let result = root().and_then(|directory| create_plaintext(&directory, &clear));
    clear.fill(0);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_cookie_cleanup_removes_stale_plaintext_but_not_active_or_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("dcc-cookie-recovered.txt");
        let encrypted = dir.path().join("saved.bin");
        std::fs::write(&stale, b"secret").unwrap();
        std::fs::write(&encrypted, b"encrypted").unwrap();
        let active = create_plaintext(dir.path(), b"active-cookie").unwrap();
        let path = active.path().to_path_buf();
        clean_stale_in(dir.path()).unwrap();
        assert!(!stale.exists());
        assert!(path.exists());
        assert!(encrypted.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"active-cookie");
        drop(active);
        assert!(!path.exists());
    }
    #[test]
    fn download_cookie_dpapi_and_identifiers_are_safe() {
        let sealed = crypt(b"cookie=secret", false).unwrap();
        assert_ne!(sealed, b"cookie=secret");
        assert_eq!(crypt(&sealed, true).unwrap(), b"cookie=secret");
        assert!(crypt(b"invalid", true).is_err());
        assert!(blob_path("../../secret").is_err());
    }
}
