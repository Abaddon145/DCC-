//! Internal Markdown preflight; not a filesystem authorization or import API.
use super::{
    note_origins::{self, NoteOriginKey, NoteOriginState},
    source_profiles::{self, LibraryScope},
};
use crate::state::AppState;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, GetFinalPathNameByHandleW, BY_HANDLE_FILE_INFORMATION,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

const MAX_MARKDOWN: u64 = 5 * 1024 * 1024;

pub(super) struct Inspection {
    pub state: NoteOriginState,
    pub sha256: String,
    pub conversion: super::note_conversion::ConversionPreview,
}

pub(super) struct MarkdownLease {
    file: File,
    _directories: Vec<File>,
}

pub(super) struct VaultSelection {
    scope: LibraryScope,
    profile: String,
    epoch: String,
    vault: String,
    root: PathBuf,
    _directories: Vec<File>,
}

/// In-memory selection only: the caller must obtain separate permission before invoking this.
pub(super) fn select_vault(
    state: &AppState,
    scope: &LibraryScope,
    profile: &str,
    epoch: &str,
    vault: &str,
    root: &Path,
) -> Result<VaultSelection, String> {
    NoteOriginKey::new(profile, vault, "selection.md")?;
    if source_profiles::get(state, scope, profile)?.access_epoch != epoch {
        return Err("来源状态已变化，请重新检查".into());
    }
    if !root.is_absolute() {
        return Err("来源目录无效".into());
    }
    crate::media_access::reject_links(root)?;
    let canonical = root.canonicalize().map_err(|_| "来源目录不可用")?;
    if canonical
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with(r"\\?\unc\")
    {
        return Err("不支持网络来源目录".into());
    }
    crate::media_access::reject_links(&canonical)?;
    let mut parents: Vec<_> = canonical.ancestors().collect();
    parents.reverse();
    let mut directories = Vec::with_capacity(parents.len());
    for parent in parents {
        directories.push(open_checked(parent, true)?);
    }
    if root.canonicalize().map_err(|_| "来源目录已变化")? != canonical
        || source_profiles::get(state, scope, profile)?.access_epoch != epoch
    {
        return Err("来源状态已变化，请重新检查".into());
    }
    Ok(VaultSelection {
        scope: scope.clone(),
        profile: profile.into(),
        epoch: epoch.into(),
        vault: vault.into(),
        root: canonical,
        _directories: directories,
    })
}

impl VaultSelection {
    fn root_identity(&self) -> Result<String, String> {
        let root = self._directories.last().ok_or("来源目录不可用")?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(root.as_raw_handle(), &mut info) } == 0
            || (info.nFileIndexHigh == 0 && info.nFileIndexLow == 0)
        {
            return Err("无法确认来源目录身份".into());
        }
        Ok(format!(
            "win1:{:08x}:{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }

    fn check_identity(&self, db: &rusqlite::Connection, identity: &str) -> Result<(), String> {
        if source_profiles::get_in(db, &self.profile)?.access_epoch != self.epoch {
            return Err("来源状态已变化，请重新检查".into());
        }
        let stored: Option<String> = rusqlite::OptionalExtension::optional(db.query_row(
            "SELECT root_identity FROM learning_source_vault_bindings WHERE source_profile_id=?1 AND vault_id=?2",
            rusqlite::params![self.profile, self.vault], |row| row.get(0),
        )).map_err(|_| "无法检查来源目录身份")?;
        if stored.as_deref() != Some(identity) {
            return Err("来源目录尚未登记或与原来源不同".into());
        }
        Ok(())
    }

    /// Record identity only after an explicit source choice; this is not an access grant.
    pub(super) fn register_identity(&self, state: &AppState) -> Result<(), String> {
        let identity = self.root_identity()?;
        source_profiles::scoped(state, &self.scope, |db| {
            let tx =
                rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
                    .map_err(|_| "无法登记来源目录")?;
            if source_profiles::get_in(&tx, &self.profile)?.access_epoch != self.epoch {
                return Err("来源状态已变化，请重新检查".into());
            }
            let existing: Option<String> = rusqlite::OptionalExtension::optional(tx.query_row(
                "SELECT root_identity FROM learning_source_vault_bindings WHERE source_profile_id=?1 AND vault_id=?2",
                rusqlite::params![self.profile, self.vault], |row| row.get(0),
            )).map_err(|_| "无法检查来源目录身份")?;
            match existing {
                Some(value) if value == identity => {}
                Some(_) => return Err("所选目录与原来源不同，请重新核对".into()),
                None => {
                    let prior: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM learning_origin_map WHERE source_profile_id=?1)", [&self.profile], |row| row.get(0)).map_err(|_| "无法检查原有来源映射")?;
                    if prior {
                        return Err("原有来源尚无目录身份，请先人工核对".into());
                    }
                    tx.execute("INSERT INTO learning_source_vault_bindings(source_profile_id,vault_id,root_identity) VALUES(?1,?2,?3)", rusqlite::params![self.profile, self.vault, identity]).map_err(|_| "无法登记来源目录")?;
                }
            }
            tx.commit().map_err(|_| "无法保存来源目录身份")?;
            Ok(())
        })
    }

    pub(super) fn inspect(&self, state: &AppState, relative: &str) -> Result<Inspection, String> {
        inspect_markdown(state, self, relative)
    }
}

fn open_checked(path: &Path, directory: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).share_mode(if directory {
        FILE_SHARE_READ | FILE_SHARE_WRITE
    } else {
        FILE_SHARE_READ
    });
    if directory {
        options.access_mode(FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY);
    }
    options.custom_flags(
        FILE_FLAG_OPEN_REPARSE_POINT
            | if directory {
                FILE_FLAG_BACKUP_SEMANTICS
            } else {
                0
            },
    );
    let file = options.open(path).map_err(|_| "来源文件不可用或正在写入")?;
    let meta = file.metadata().map_err(|_| "无法检查来源文件")?;
    if meta.file_attributes() & 0x400 != 0
        || meta.is_dir() != directory
        || (!directory && !meta.is_file())
    {
        return Err("来源路径包含链接或不是普通文件".into());
    }
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
        || (!directory && info.nNumberOfLinks != 1)
    {
        return Err("来源文件身份不可验证".into());
    }
    let mut buffer = vec![0u16; 32768];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    } as usize;
    let actual = PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..count.min(buffer.len())],
    ));
    if count == 0 || count >= buffer.len() || actual != path {
        return Err("来源路径已变化".into());
    }
    Ok(file)
}

pub(super) fn open_markdown(root: &Path, relative: &str) -> Result<MarkdownLease, String> {
    if !root.is_absolute()
        || relative.is_empty()
        || relative.contains(['\\', ':'])
        || relative.chars().any(char::is_control)
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !relative.to_ascii_lowercase().ends_with(".md")
    {
        return Err("来源笔记路径无效".into());
    }
    crate::media_access::reject_links(root)?;
    let expected_root = root;
    let root = root.canonicalize().map_err(|_| "来源目录不可用")?;
    if root != expected_root {
        return Err("来源目录已变化，请重新选择".into());
    }
    if root
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with(r"\\?\unc\")
    {
        return Err("不支持网络来源目录".into());
    }
    crate::media_access::reject_links(&root)?;
    let path = root.join(relative);
    crate::media_access::reject_links(&path)?;
    let canonical = path.canonicalize().map_err(|_| "来源笔记不可用")?;
    if !canonical.starts_with(&root) {
        return Err("来源笔记不在所选目录中".into());
    }
    if canonical
        .strip_prefix(&root)
        .map_err(|_| "来源笔记路径无效")?
        != Path::new(relative)
    {
        return Err("来源笔记路径与实际文件名不一致".into());
    }
    let mut parents: Vec<_> = canonical
        .parent()
        .ok_or("来源目录不可用")?
        .ancestors()
        .collect();
    parents.reverse();
    let mut directories = Vec::with_capacity(parents.len());
    for parent in parents {
        directories.push(open_checked(parent, true)?);
    }
    let file = open_checked(&canonical, false)?;
    if file.metadata().map_err(|_| "无法检查来源笔记")?.len() > MAX_MARKDOWN {
        return Err("来源笔记超过 5 MiB".into());
    }
    Ok(MarkdownLease {
        file,
        _directories: directories,
    })
}

fn inspect_markdown(
    state: &AppState,
    selection: &VaultSelection,
    relative: &str,
) -> Result<Inspection, String> {
    let key = NoteOriginKey::new(&selection.profile, &selection.vault, relative)?;
    let identity = selection.root_identity()?;
    source_profiles::scoped(state, &selection.scope, |db| {
        selection.check_identity(db, &identity)
    })?;
    let mut lease = open_markdown(&selection.root, relative)?;
    let expected_size = lease.file.metadata().map_err(|_| "无法检查来源笔记")?.len();
    let mut bytes = Vec::new();
    (&mut lease.file)
        .take(MAX_MARKDOWN + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取来源笔记")?;
    if bytes.len() as u64 > MAX_MARKDOWN
        || bytes.len() as u64 != expected_size
        || lease.file.metadata().map_err(|_| "无法检查来源笔记")?.len() != expected_size
    {
        return Err("来源笔记超过 5 MiB".into());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let conversion = super::note_conversion::convert_source_note(
        &bytes,
        &super::note_conversion::AttachmentMap::new(),
        &super::note_conversion::MediaMap::new(),
    )?;
    let state = source_profiles::scoped(state, &selection.scope, |db| {
        let tx = db.unchecked_transaction().map_err(|_| "无法检查笔记来源")?;
        selection.check_identity(&tx, &identity)?;
        let result = note_origins::inspect(&tx, &key, &sha256)?;
        tx.commit().map_err(|_| "无法检查笔记来源")?;
        Ok(result)
    })?;
    Ok(Inspection {
        state,
        sha256,
        conversion,
    })
}
