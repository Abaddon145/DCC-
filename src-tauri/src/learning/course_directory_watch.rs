//! Parent-directory watch for the marked synthetic course gate.
//!
//! The deny-write lease cannot name a `-wal` or `-shm` that does not exist yet,
//! and the dual-read stamps miss an in-place rewrite whose mtime is put back
//! before the next comparison. This watch is armed after that lease and held
//! until the final recheck. A create, delete, rename, or content change of the
//! database, the marker, or the `-wal` / `-shm` / `-journal` names fails closed
//! even when the endpoint stamps match again.
//!
//! Linux uses `inotify`. Windows uses overlapped `ReadDirectoryChangesW`.
//! Access and last-access events are not part of the filter, so the gate's own
//! reads are not activity. A watch that cannot be armed fails closed. Other
//! operating systems keep the previous no-op posture and are not a proof.

use std::path::Path;

#[cfg(target_os = "linux")]
pub(super) struct DirectoryWatch {
    fd: std::os::fd::OwnedFd,
    names: Vec<Vec<u8>>,
}

#[cfg(windows)]
pub(super) struct DirectoryWatch {
    file: std::fs::File,
    event: windows_sys::Win32::Foundation::HANDLE,
    overlapped: Box<windows_sys::Win32::System::IO::OVERLAPPED>,
    buffer: Box<NotifyBuf>,
    names: Vec<Vec<u16>>,
    pending: bool,
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(super) struct DirectoryWatch;

#[cfg(windows)]
#[repr(C, align(8))]
struct NotifyBuf {
    bytes: [u8; 8192],
}

#[cfg(windows)]
const NOTIFY_FILTER: u32 = windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_FILE_NAME
    | windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_DIR_NAME
    | windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_SIZE
    | windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_LAST_WRITE
    | windows_sys::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE_CREATION;

impl DirectoryWatch {
    /// Watch `database`'s parent. `marker` is the sibling marker file name.
    pub(super) fn arm(database: &Path, marker: &str) -> Result<Self, ()> {
        #[cfg(target_os = "linux")]
        {
            arm_linux(database, marker)
        }
        #[cfg(windows)]
        {
            arm_windows(database, marker)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (database, marker);
            Ok(Self)
        }
    }

    /// Fail if a watched name changed since the watch was armed or last checked.
    pub(super) fn check_quiet(&mut self) -> Result<(), ()> {
        #[cfg(target_os = "linux")]
        {
            check_linux(self)
        }
        #[cfg(windows)]
        {
            check_windows(self)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn arm_linux(database: &Path, marker: &str) -> Result<DirectoryWatch, ()> {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::AsRawFd;

    let names = linux_names(database, marker)?;
    let parent = database
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(())?;
    let bytes = parent.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(());
    }
    let mut path = Vec::with_capacity(bytes.len() + 1);
    path.extend_from_slice(bytes);
    path.push(0);
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return Err(());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mask = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MODIFY
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_CLOSE_WRITE
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF
        | libc::IN_ONLYDIR;
    let wd = unsafe {
        libc::inotify_add_watch(fd.as_raw_fd(), path.as_ptr().cast::<libc::c_char>(), mask)
    };
    if wd < 0 {
        return Err(());
    }
    Ok(DirectoryWatch { fd, names })
}

#[cfg(target_os = "linux")]
fn linux_names(database: &Path, marker: &str) -> Result<Vec<Vec<u8>>, ()> {
    use std::os::unix::ffi::OsStrExt;
    let base = database.file_name().ok_or(())?.as_bytes();
    if base.is_empty() || base.contains(&0) {
        return Err(());
    }
    let mut names = Vec::with_capacity(5);
    names.push(base.to_vec());
    for suffix in [b"-wal".as_slice(), b"-shm", b"-journal"] {
        let mut name = base.to_vec();
        name.extend_from_slice(suffix);
        names.push(name);
    }
    let marker = marker.as_bytes();
    if marker.is_empty() || marker.contains(&0) {
        return Err(());
    }
    names.push(marker.to_vec());
    Ok(names)
}

#[cfg(target_os = "linux")]
fn check_linux(watch: &DirectoryWatch) -> Result<(), ()> {
    use std::os::unix::io::AsRawFd;
    let mut buf = [0u8; 4096];
    loop {
        let read = unsafe {
            libc::read(
                watch.fd.as_raw_fd(),
                buf.as_mut_ptr().cast::<libc::c_void>(),
                buf.len(),
            )
        };
        if read < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(());
            }
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(());
        }
        let read = usize::try_from(read).map_err(|_| ())?;
        if read == 0 || scan_inotify(&buf[..read], &watch.names)? {
            return Err(());
        }
    }
}

/// `true` when a watched name was reported. Overflow, a lost watch, or a short
/// event fails closed instead of being treated as quiet.
#[cfg(target_os = "linux")]
fn scan_inotify(buf: &[u8], names: &[Vec<u8>]) -> Result<bool, ()> {
    let header = std::mem::size_of::<libc::inotify_event>();
    let lost = libc::IN_Q_OVERFLOW
        | libc::IN_IGNORED
        | libc::IN_UNMOUNT
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF;
    let mut offset = 0usize;
    while offset < buf.len() {
        if offset + header > buf.len() {
            return Err(());
        }
        let mask = u32::from_ne_bytes(buf[offset + 4..offset + 8].try_into().map_err(|_| ())?);
        let len =
            u32::from_ne_bytes(buf[offset + 12..offset + 16].try_into().map_err(|_| ())?) as usize;
        let name_at = offset + header;
        let next = name_at.checked_add(len).ok_or(())?;
        if next > buf.len() {
            return Err(());
        }
        if (mask & lost) != 0 {
            return Err(());
        }
        let raw = &buf[name_at..next];
        let name = match raw.iter().position(|byte| *byte == 0) {
            Some(end) => &raw[..end],
            None => raw,
        };
        if name.is_empty() || names.iter().any(|expected| expected.as_slice() == name) {
            return Ok(true);
        }
        offset = next;
    }
    Ok(false)
}

#[cfg(windows)]
fn arm_windows(database: &Path, marker: &str) -> Result<DirectoryWatch, ()> {
    use windows_sys::Win32::System::Threading::CreateEventW;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let names = windows_names(database, marker)?;
    let parent = database
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(())?;
    let file = open_directory(parent)?;
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(());
    }
    let mut watch = DirectoryWatch {
        file,
        event,
        overlapped: Box::new(OVERLAPPED::default()),
        buffer: Box::new(NotifyBuf { bytes: [0u8; 8192] }),
        names,
        pending: false,
    };
    // A queued overlapped read returns nonzero. Treating that as failure
    // rejected every quiet snapshot on Windows CI run 41422611748.
    if queue_changes(&mut watch).is_err() {
        return Err(());
    }
    Ok(watch)
}

#[cfg(windows)]
fn open_directory(directory: &Path) -> Result<std::fs::File, ()> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
        FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .access_mode(FILE_LIST_DIRECTORY)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED);
    let file = options.open(directory).map_err(|_| ())?;
    let meta = file.metadata().map_err(|_| ())?;
    if !meta.is_dir() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(());
    }
    Ok(file)
}

#[cfg(windows)]
fn windows_names(database: &Path, marker: &str) -> Result<Vec<Vec<u16>>, ()> {
    use std::os::windows::ffi::OsStrExt;
    let base: Vec<u16> = database.file_name().ok_or(())?.encode_wide().collect();
    if base.is_empty() {
        return Err(());
    }
    let mut names = Vec::with_capacity(5);
    names.push(base.clone());
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = base.clone();
        name.extend(suffix.encode_utf16());
        names.push(name);
    }
    let marker: Vec<u16> = marker.encode_utf16().collect();
    if marker.is_empty() {
        return Err(());
    }
    names.push(marker);
    Ok(names)
}

#[cfg(windows)]
fn check_windows(watch: &mut DirectoryWatch) -> Result<(), ()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::System::IO::GetOverlappedResult;

    for _ in 0..64 {
        if watch.pending {
            let status = unsafe { WaitForSingleObject(watch.event, 0) };
            if status == WAIT_TIMEOUT {
                return Ok(());
            }
            if status != WAIT_OBJECT_0 {
                return Err(());
            }
            let mut transferred = 0u32;
            let ok = unsafe {
                GetOverlappedResult(
                    watch.file.as_raw_handle(),
                    &*watch.overlapped,
                    &mut transferred,
                    0,
                )
            };
            watch.pending = false;
            if ok == 0 || transferred == 0 {
                return Err(());
            }
            let end = usize::try_from(transferred).map_err(|_| ())?;
            if end > watch.buffer.bytes.len()
                || scan_notify(&watch.buffer.bytes[..end], &watch.names)?
            {
                return Err(());
            }
        }
        if queue_changes(watch).is_err() {
            return Err(());
        }
    }
    Err(())
}

/// Queue one overlapped directory read. Nonzero means the read was queued.
/// `lpBytesReturned` stays null because that out-parameter is undefined until
/// `GetOverlappedResult`.
#[cfg(windows)]
fn queue_changes(watch: &mut DirectoryWatch) -> Result<(), ()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_IO_PENDING};
    use windows_sys::Win32::Storage::FileSystem::ReadDirectoryChangesW;
    use windows_sys::Win32::System::Threading::ResetEvent;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    unsafe {
        if ResetEvent(watch.event) == 0 {
            return Err(());
        }
    }
    *watch.overlapped = OVERLAPPED::default();
    watch.overlapped.hEvent = watch.event;
    let handle = watch.file.as_raw_handle();
    let buffer_ptr = watch.buffer.bytes.as_mut_ptr();
    let buffer_len = watch.buffer.bytes.len() as u32;
    let queued = unsafe {
        ReadDirectoryChangesW(
            handle,
            buffer_ptr.cast(),
            buffer_len,
            0,
            NOTIFY_FILTER,
            std::ptr::null_mut(),
            &mut *watch.overlapped,
            None,
        )
    };
    if queued != 0 || unsafe { GetLastError() } == ERROR_IO_PENDING {
        watch.pending = true;
        return Ok(());
    }
    watch.pending = false;
    Err(())
}

/// `true` when a watched name is in the notification buffer.
#[cfg(windows)]
fn scan_notify(buf: &[u8], names: &[Vec<u16>]) -> Result<bool, ()> {
    let mut offset = 0usize;
    while offset < buf.len() {
        if offset + 12 > buf.len() {
            return Err(());
        }
        let next = u32::from_le_bytes(buf[offset..offset + 4].try_into().map_err(|_| ())?) as usize;
        let name_len =
            u32::from_le_bytes(buf[offset + 8..offset + 12].try_into().map_err(|_| ())?) as usize;
        if name_len % 2 != 0 {
            return Err(());
        }
        let name_at = offset + 12;
        let name_end = name_at.checked_add(name_len).ok_or(())?;
        if name_end > buf.len() {
            return Err(());
        }
        let wide = buf[name_at..name_end]
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
            .collect::<Vec<_>>();
        if wide.is_empty() || names.iter().any(|name| ascii_eq(name, &wide)) {
            return Ok(true);
        }
        if next == 0 {
            return Ok(false);
        }
        if next < 12 {
            return Err(());
        }
        offset = offset.checked_add(next).ok_or(())?;
    }
    Err(())
}

#[cfg(windows)]
fn ascii_eq(left: &[u16], right: &[u16]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let fold = |unit: u16| {
                if (b'A' as u16..=b'Z' as u16).contains(&unit) {
                    unit + 32
                } else {
                    unit
                }
            };
            fold(*left) == fold(*right)
        })
}

#[cfg(windows)]
impl Drop for DirectoryWatch {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        if self.pending {
            let handle = self.file.as_raw_handle();
            unsafe {
                CancelIoEx(handle, &*self.overlapped);
                let mut transferred = 0u32;
                GetOverlappedResult(handle, &*self.overlapped, &mut transferred, 1);
            }
            self.pending = false;
        }
        unsafe {
            CloseHandle(self.event);
        }
    }
}
