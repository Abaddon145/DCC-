//! All downloader child processes are owned by a kill-on-close Windows job.
use std::{path::Path, time::Duration};
#[cfg(windows)]
pub struct OwnedProcess {
    process: std::os::windows::io::OwnedHandle,
    job: std::os::windows::io::OwnedHandle,
}
#[cfg(windows)]
impl OwnedProcess {
    pub fn id(&self) -> u32 {
        use std::os::windows::io::AsRawHandle;
        unsafe { windows_sys::Win32::System::Threading::GetProcessId(self.process.as_raw_handle()) }
    }
    pub fn terminate(&self) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        if unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job.as_raw_handle(), 1)
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.process.as_raw_handle(),
                5000,
            )
        };
        Ok(())
    }
    pub fn exited(&self) -> bool {
        use std::os::windows::io::AsRawHandle;
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.process.as_raw_handle(),
                0,
            ) == windows_sys::Win32::Foundation::WAIT_OBJECT_0
        }
    }
}
#[cfg(windows)]
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
pub fn quote_argument(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        quoted.push_str(&"\\".repeat(if ch == '"' { slashes * 2 + 1 } else { slashes }));
        quoted.push(ch);
        slashes = 0;
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
pub fn spawn_owned(
    exe: &Path,
    args: &[String],
) -> Result<(OwnedProcess, std::fs::File, std::fs::File), String> {
    spawn_owned_in(exe, args, None)
}
#[cfg(windows)]
pub fn spawn_owned_in(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<(OwnedProcess, std::fs::File, std::fs::File), String> {
    use std::{
        fs::File,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
    };
    use windows_sys::Win32::{
        Foundation::*,
        Security::SECURITY_ATTRIBUTES,
        System::{JobObjects::*, Pipes::CreatePipe, Threading::*},
    };
    // Std Command cannot expose the suspended thread handle. Create suspended, assign the
    // job, then resume: no extractor/FFmpeg child can escape between spawn and assignment.
    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }
    fn error() -> String {
        format!("启动下载进程失败：{}", std::io::Error::last_os_error())
    }
    unsafe fn pipe() -> Result<(File, OwnedHandle), String> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
        if CreatePipe(&mut read, &mut write, &sa, 0) == 0 {
            return Err(error());
        }
        let read = File::from_raw_handle(read);
        let write = OwnedHandle::from_raw_handle(write);
        if SetHandleInformation(read.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) == 0 {
            return Err(error());
        }
        Ok((read, write))
    }
    if args.iter().any(|a| a.contains('\0')) {
        return Err("参数包含无效字符".into());
    }
    if let Some(path) = cwd {
        if !path.is_absolute()
            || !path.is_dir()
            || path.as_os_str().to_string_lossy().contains('\0')
        {
            return Err("子进程工作目录无效".into());
        }
        crate::media_access::reject_links(path)?;
    }
    let directory = cwd.map(|path| wide(path.as_os_str()));
    let (process, job, stdout, stderr) = unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return Err(error());
        }
        let job = OwnedHandle::from_raw_handle(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(error());
        }
        let (stdout, out_write) = pipe()?;
        let (stderr, err_write) = pipe()?;
        let stdin = File::open("NUL").map_err(|_| "无法打开空输入")?;
        if SetHandleInformation(
            stdin.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        ) == 0
        {
            return Err(error());
        }
        let mut size = 0;
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let attrs = buffer.as_mut_ptr().cast();
        if InitializeProcThreadAttributeList(attrs, 1, 0, &mut size) == 0 {
            return Err(error());
        }
        let result = (|| {
            let handles = [
                stdin.as_raw_handle(),
                out_write.as_raw_handle(),
                err_write.as_raw_handle(),
            ];
            if UpdateProcThreadAttribute(
                attrs,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(&handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            ) == 0
            {
                return Err(error());
            }
            let mut startup: STARTUPINFOEXW = std::mem::zeroed();
            startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = handles[0];
            startup.StartupInfo.hStdOutput = handles[1];
            startup.StartupInfo.hStdError = handles[2];
            startup.lpAttributeList = attrs;
            let text = std::iter::once(exe.to_string_lossy().into_owned())
                .chain(args.iter().cloned())
                .map(|a| quote_argument(&a))
                .collect::<Vec<_>>()
                .join(" ");
            let mut command = wide(std::ffi::OsStr::new(&text));
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessW(
                wide(exe.as_os_str()).as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
                std::ptr::null(),
                directory
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                return Err(error());
            }
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            let thread = OwnedHandle::from_raw_handle(info.hThread);
            if AssignProcessToJobObject(job.as_raw_handle(), process.as_raw_handle()) == 0 {
                TerminateProcess(process.as_raw_handle(), 1);
                WaitForSingleObject(process.as_raw_handle(), 5000);
                return Err(error());
            }
            if ResumeThread(thread.as_raw_handle()) == u32::MAX {
                TerminateJobObject(job.as_raw_handle(), 1);
                return Err(error());
            }
            Ok(process)
        })();
        DeleteProcThreadAttributeList(attrs);
        (result?, job, stdout, stderr)
    };
    Ok((OwnedProcess { process, job }, stdout, stderr))
}

#[cfg(windows)]
pub fn capture(
    exe: &Path,
    args: &[String],
    timeout: Duration,
    stop: &dyn Fn() -> bool,
    line: &mut dyn FnMut(&str),
) -> Result<String, String> {
    capture_in(exe, args, None, timeout, stop, line)
}
#[cfg(windows)]
pub fn capture_in(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    stop: &dyn Fn() -> bool,
    line: &mut dyn FnMut(&str),
) -> Result<String, String> {
    use std::{fs::File, io::Read, os::windows::io::AsRawHandle};
    use windows_sys::Win32::System::{
        JobObjects::TerminateJobObject,
        Threading::{GetExitCodeProcess, WaitForSingleObject},
    };
    if stop() {
        return Err("子进程在启动前已取消".into());
    }
    let (owned, stdout, stderr) = spawn_owned_in(exe, args, cwd)?;
    let process = &owned.process;
    let job = &owned.job;
    // Drain both pipes continuously, with bounded retained data and no unbounded lines.
    let (sender, receiver) = std::sync::mpsc::sync_channel::<(bool, String)>(128);
    let read_lines =
        move |mut file: File, send: std::sync::mpsc::SyncSender<(bool, String)>, is_error: bool| {
            let mut pending = Vec::new();
            let mut buffer = [0u8; 8192];
            while let Ok(count) = file.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                for byte in &buffer[..count] {
                    if *byte == b'\n' {
                        if send
                            .send((is_error, String::from_utf8_lossy(&pending).into()))
                            .is_err()
                        {
                            return;
                        }
                        pending.clear();
                    } else if pending.len() < 32 * 1024 * 1024 {
                        pending.push(*byte);
                    }
                }
            }
            if !pending.is_empty() {
                let _ = send.send((is_error, String::from_utf8_lossy(&pending).into()));
            }
        };
    let out_sender = sender.clone();
    let output_thread = std::thread::spawn(move || read_lines(stdout, out_sender, false));
    let error_thread = std::thread::spawn(move || read_lines(stderr, sender, true));
    let started = std::time::Instant::now();
    let mut output = String::new();
    let mut errors = String::new();
    let mut stopped = false;
    let mut too_large = false;
    loop {
        if !stopped && (stop() || started.elapsed() > timeout) {
            unsafe {
                TerminateJobObject(job.as_raw_handle(), 1);
            }
            stopped = true;
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok((is_error, text)) => {
                if is_error {
                    if errors.len() < 65536 {
                        errors.extend(text.chars().take(4096));
                    }
                    continue;
                }
                line(&text);
                // Streamed progress has no value as retained output and can span hours.
                if text.starts_with("DCC_PROGRESS") || text.starts_with("DCC_PROCESS") {
                    continue;
                }
                if output.len() + text.len() <= 32 * 1024 * 1024 {
                    output.push_str(&text);
                    output.push('\n');
                } else {
                    too_large = true;
                    unsafe {
                        TerminateJobObject(job.as_raw_handle(), 1);
                    }
                    stopped = true;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
    }
    let _ = output_thread.join();
    let _ = error_thread.join();
    unsafe {
        WaitForSingleObject(process.as_raw_handle(), 5000);
    }
    let mut code = 1;
    unsafe {
        GetExitCodeProcess(process.as_raw_handle(), &mut code);
    }
    if too_large {
        return Err("源站输出过大，请缩小播放列表".into());
    }
    if stopped {
        return Err("下载进程已停止或超时".into());
    }
    if code != 0 {
        return Err(crate::downloader::domain::classify_error(&errors));
    }
    Ok(output)
}
#[cfg(not(windows))]
pub fn capture_in(
    _: &Path,
    _: &[String],
    _: Option<&Path>,
    _: Duration,
    _: &dyn Fn() -> bool,
    _: &mut dyn FnMut(&str),
) -> Result<String, String> {
    Err("本地处理暂仅支持 Windows".into())
}
#[cfg(not(windows))]
pub fn capture(
    _: &Path,
    _: &[String],
    _: Duration,
    _: &dyn Fn() -> bool,
    _: &mut dyn FnMut(&str),
) -> Result<String, String> {
    Err("下载进程暂仅支持 Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learning_transcription_process_preserves_unicode_cwd_and_precancel() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("转写目录");
        std::fs::create_dir(&cwd).unwrap();
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let marker = format!("marker-{}.txt", uuid::Uuid::new_v4());
        std::fs::write(cwd.join(&marker), b"scoped").unwrap();
        let out = capture_in(
            exe,
            &[
                "-NoProfile".into(),
                "-Command".into(),
                format!("if (!(Test-Path -LiteralPath '{marker}')) {{ exit 71 }}"),
            ],
            Some(&cwd),
            Duration::from_secs(5),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        assert!(out.trim().is_empty());
        let cancelled = cwd.join("cancelled.txt");
        assert!(capture_in(
            exe,
            &[
                "-NoProfile".into(),
                "-Command".into(),
                format!(
                    "Set-Content -LiteralPath '{}' -Value wrong",
                    cancelled.display().to_string().replace('\'', "''")
                )
            ],
            Some(&cwd),
            Duration::from_secs(5),
            &|| true,
            &mut |_| {}
        )
        .is_err());
        assert!(!cwd.join("cancelled.txt").exists());
    }
    #[test]
    fn learning_transcription_process_timeout_does_not_wait_for_sleeping_child() {
        let root = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let result = capture_in(
            exe,
            &[
                "-NoProfile".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 30".into(),
            ],
            Some(root.path()),
            Duration::from_millis(400),
            &|| false,
            &mut |_| {},
        );
        assert!(result.unwrap_err().contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    #[cfg(windows)]
    #[test]
    fn learning_native_process_drop_reaps_owned_tree() {
        use std::io::{BufRead, BufReader};
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let args=["-NoProfile".into(),"-Command".into(),"$child=Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30' -WindowStyle Hidden -PassThru; Write-Output $child.Id; Start-Sleep -Seconds 30".into()];
        let (owned, stdout, _stderr) = spawn_owned(exe, &args).unwrap();
        let parent = owned.id();
        assert!(parent > 0);
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let child = line.trim().parse::<u32>().unwrap();
        drop(owned);
        unsafe {
            for pid in [parent, child] {
                let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                if !h.is_null() {
                    assert_eq!(WaitForSingleObject(h, 5000), WAIT_OBJECT_0);
                    CloseHandle(h);
                }
            }
        }
    }
    #[test]
    fn download_progress_is_streamed_without_retained_history() {
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let args = ["-NoProfile".into(), "-Command".into(), "1..500 | ForEach-Object {Write-Output 'DCC_PROGRESS100'}; Write-Output 'DCC_FILE\"done.mp4\"'".into()];
        let mut count = 0;
        let output = capture(
            exe,
            &args,
            Duration::from_secs(10),
            &|| false,
            &mut |line| {
                if line.starts_with("DCC_PROGRESS") {
                    count += 1;
                }
            },
        )
        .unwrap();
        assert_eq!(count, 500);
        assert!(!output.contains("DCC_PROGRESS"));
        assert!(output.contains("DCC_FILE"));
    }
    #[cfg(windows)]
    #[test]
    fn download_cancel_terminates_grandchildren() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let args=["-NoProfile".into(),"-Command".into(),"$child=Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30' -WindowStyle Hidden -PassThru; Write-Output $child.Id; Start-Sleep -Seconds 30".into()];
        let stop = AtomicBool::new(false);
        let mut pid = 0;
        assert!(capture(
            exe,
            &args,
            Duration::from_secs(10),
            &|| stop.load(Ordering::Relaxed),
            &mut |line| {
                if let Ok(value) = line.trim().parse::<u32>() {
                    pid = value;
                    stop.store(true, Ordering::Relaxed);
                }
            }
        )
        .is_err());
        assert!(pid > 0);
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if !handle.is_null() {
                assert_eq!(WaitForSingleObject(handle, 5000), WAIT_OBJECT_0);
                CloseHandle(handle);
            }
        }
    }
    #[test]
    fn download_windows_argument_quoting_preserves_paths() {
        assert_eq!(quote_argument("C:\\我的下载\\"), "\"C:\\我的下载\\\\\"");
        assert_eq!(quote_argument("say \"yes\""), "\"say \\\"yes\\\"\"");
    }
    #[test]
    fn download_process_reads_utf8_and_cancels() {
        let exe = Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let args = [
            "-NoProfile".into(),
            "-Command".into(),
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Write-Output '下载'".into(),
        ];
        assert!(
            capture(exe, &args, Duration::from_secs(10), &|| false, &mut |_| {})
                .unwrap()
                .contains("下载")
        );
        let started = std::time::Instant::now();
        let args = [
            "-NoProfile".into(),
            "-Command".into(),
            "Start-Sleep -Seconds 30".into(),
        ];
        assert!(capture(
            exe,
            &args,
            Duration::from_secs(10),
            &|| started.elapsed() > Duration::from_millis(100),
            &mut |_| {}
        )
        .is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
