//! Synthetic fixture checks only: this is not a production file-access grant.
use super::import_manifest::validate_synthetic_manifest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAX_SYNTHETIC_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct SyntheticFilesReport {
    pub verified: usize,
    pub missing_required: usize,
    pub missing_optional: usize,
}

pub(super) fn digest(file: &mut File, expected_size: u64) -> Result<String, String> {
    let failed = || "合成来源文件不可用或已变化".to_string();
    file.seek(SeekFrom::Start(0)).map_err(|_| failed())?;
    let limit = expected_size.checked_add(1).ok_or_else(failed)?;
    let mut input = file.take(limit);
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 32768];
    let mut total = 0u64;
    loop {
        let count = input.read(&mut buffer).map_err(|_| failed())?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or_else(failed)?;
        if total > expected_size {
            return Err(failed());
        }
        hash.update(&buffer[..count]);
    }
    if total != expected_size {
        return Err(failed());
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn verify_synthetic_files(
    root: &Path,
    bytes: &[u8],
) -> Result<SyntheticFilesReport, String> {
    let failed = || "合成来源文件不可用或已变化".to_string();
    validate_synthetic_manifest(bytes).map_err(|_| failed())?;
    let manifest: Value = serde_json::from_slice(bytes).map_err(|_| failed())?;
    crate::media_access::reject_links(root).map_err(|_| failed())?;
    let root = fs::canonicalize(root).map_err(|_| failed())?;
    if !root.is_dir() {
        return Err(failed());
    }
    let files = manifest["files"].as_array().ok_or_else(failed)?;
    let items = std::iter::once((&manifest["snapshot"], true)).chain(
        files
            .iter()
            .map(|item| (item, item["required"].as_bool().unwrap_or(true))),
    );
    let mut report = SyntheticFilesReport {
        verified: 0,
        missing_required: 0,
        missing_optional: 0,
    };
    let mut total = 0u64;
    for (item, required) in items {
        let relative = item["relativePath"].as_str().ok_or_else(failed)?;
        let size = item["bytes"].as_u64().ok_or_else(failed)?;
        let expected = item["sha256"].as_str().ok_or_else(failed)?;
        total = total
            .checked_add(size)
            .filter(|v| *v <= MAX_SYNTHETIC_BYTES)
            .ok_or_else(failed)?;
        let path = root.join(relative);
        crate::media_access::reject_links(&path).map_err(|_| failed())?;
        let resolved = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if required {
                    report.missing_required += 1;
                } else {
                    report.missing_optional += 1;
                }
                continue;
            }
            Err(_) => return Err(failed()),
        };
        if !resolved.starts_with(&root) {
            return Err(failed());
        }
        crate::media_access::reject_links(&resolved).map_err(|_| failed())?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        }
        let mut file = options.open(&resolved).map_err(|_| failed())?;
        let before = file.metadata().map_err(|_| failed())?;
        if !before.is_file() || before.len() != size {
            return Err(failed());
        }
        let first = digest(&mut file, size)?;
        let second = digest(&mut file, size)?;
        let after = file.metadata().map_err(|_| failed())?;
        if before.len() != after.len()
            || before.modified().map_err(|_| failed())? != after.modified().map_err(|_| failed())?
            || first != expected
            || second != first
            || fs::canonicalize(&path).map_err(|_| failed())? != resolved
        {
            return Err(failed());
        }
        crate::media_access::reject_links(&path).map_err(|_| failed())?;
        report.verified += 1;
    }
    Ok(report)
}
