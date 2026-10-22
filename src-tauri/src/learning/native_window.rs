// Adapted from Yingliu c866508 native_window.rs; retain main-thread ownership and HWND reuse guard.
use super::native_player::NativeViewport;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::Manager;
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
pub struct NativeWindow {
    app: tauri::AppHandle,
    hwnd: usize,
    alive: Arc<AtomicBool>,
}
impl NativeWindow {
    pub fn create(app: &tauri::AppHandle) -> Result<Self, String> {
        let app2 = app.clone();
        let (tx, rx) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let token = alive.clone();
        app.run_on_main_thread(move || {
            let result = (|| {
                let parent = app2
                    .get_webview_window("main")
                    .ok_or("主窗口不可用")?
                    .hwnd()
                    .map_err(|e| e.to_string())?
                    .0 as HWND;
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        std::ptr::null(),
                        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                        0,
                        0,
                        1,
                        1,
                        parent,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    )
                };
                if hwnd.is_null() {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let key: Vec<u16> = "StackDccNativeOwner\0".encode_utf16().collect();
                if unsafe { SetPropW(hwnd, key.as_ptr(), Arc::as_ptr(&token) as _) } == 0 {
                    unsafe { DestroyWindow(hwnd) };
                    return Err("无法绑定播放窗口".into());
                }
                Ok(hwnd as usize)
            })();
            if let Err(mpsc::SendError(Ok(hwnd))) = tx.send(result) {
                unsafe { DestroyWindow(hwnd as HWND) };
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(Self {
            app: app.clone(),
            hwnd: rx
                .recv_timeout(std::time::Duration::from_secs(3))
                .map_err(|_| "播放窗口创建超时")??,
            alive,
        })
    }
    pub fn id(&self) -> usize {
        self.hwnd
    }
    pub fn set_fullscreen(&self, enabled: bool) -> Result<(), String> {
        self.app
            .get_webview_window("main")
            .ok_or("主窗口不可用")?
            .set_fullscreen(enabled)
            .map_err(|e| e.to_string())
    }
    pub fn update(&self, viewport: NativeViewport) -> Result<(), String> {
        let main = self.app.get_webview_window("main").ok_or("主窗口不可用")?;
        let size = main.inner_size().map_err(|e| e.to_string())?;
        let rect = viewport.physical(
            main.scale_factor().map_err(|e| e.to_string())?,
            size.width as i32,
            size.height as i32,
        )?;
        let token = self.alive.clone();
        let hwnd = self.hwnd;
        self.app
            .run_on_main_thread(move || unsafe {
                let key: Vec<u16> = "StackDccNativeOwner\0".encode_utf16().collect();
                if !token.load(Ordering::Acquire)
                    || GetPropW(hwnd as HWND, key.as_ptr()) != Arc::as_ptr(&token) as _
                {
                    return;
                }
                SetWindowPos(
                    hwnd as HWND,
                    HWND_TOP,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    SWP_NOACTIVATE,
                );
                ShowWindow(
                    hwnd as HWND,
                    if rect.visible {
                        SW_SHOWNOACTIVATE
                    } else {
                        SW_HIDE
                    },
                );
            })
            .map_err(|e| e.to_string())
    }
}
impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        let token = self.alive.clone();
        let hwnd = self.hwnd;
        let _ = self.app.run_on_main_thread(move || unsafe {
            let key: Vec<u16> = "StackDccNativeOwner\0".encode_utf16().collect();
            if GetPropW(hwnd as HWND, key.as_ptr()) == Arc::as_ptr(&token) as _ {
                RemovePropW(hwnd as HWND, key.as_ptr());
                DestroyWindow(hwnd as HWND);
            }
        });
    }
}
