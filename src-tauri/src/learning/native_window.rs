// Adapted from Yingliu c866508 native_window.rs; retain main-thread ownership and HWND reuse guard.
use super::native_player::NativeViewport;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use tauri::Manager;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::{
    core::w,
    Win32::{
        Foundation::HWND, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
    },
};
#[derive(Default, Clone, Copy)]
struct MouseGesture {
    ignore_up: bool,
}
impl MouseGesture {
    fn next(&mut self, message: u32, delta: i16) -> Option<&'static str> {
        match message {
            WM_LBUTTONDBLCLK => {
                self.ignore_up = true;
                Some("doubleClick")
            }
            WM_LBUTTONUP if self.ignore_up => {
                self.ignore_up = false;
                None
            }
            WM_LBUTTONUP => Some("click"),
            WM_RBUTTONUP => Some("context"),
            WM_MOUSEWHEEL if delta > 0 => Some("wheelUp"),
            WM_MOUSEWHEEL if delta < 0 => Some("wheelDown"),
            _ => None,
        }
    }
}
struct WindowInput {
    app: tauri::AppHandle,
    library: String,
    session: String,
    course: String,
    mouse: std::cell::Cell<MouseGesture>,
}
// mpv deliberately disables input on its --wid child. Receive mouse input on our
// own parent class instead of assuming mpv keybinds or DOM overlays can see it.
unsafe extern "system" fn surface_proc(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let pointer = GetPropW(hwnd, w!("StackDccNativeInput")) as *mut WindowInput;
    if !pointer.is_null() {
        if message == WM_NCDESTROY {
            RemovePropW(hwnd, w!("StackDccNativeInput"));
            drop(Box::from_raw(pointer));
        } else {
            let input = &*pointer;
            let mut mouse = input.mouse.get();
            let kind = mouse.next(message, (wparam >> 16) as u16 as i16);
            input.mouse.set(mouse);
            if let Some(kind) = kind {
                use tauri::Emitter;
                let _=input.app.emit("learning-playback-pointer",serde_json::json!({"libraryId":input.library,"sessionId":input.session,"courseId":input.course,"kind":kind,"doubleClickMs":GetDoubleClickTime()}));
                return 0;
            }
        }
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

#[cfg(test)]
mod pointer_tests {
    use super::*;
    #[test]
    fn learning_window_double_click_does_not_schedule_another_single_click() {
        let mut mouse = MouseGesture::default();
        assert_eq!(mouse.next(WM_LBUTTONUP, 0), Some("click"));
        assert_eq!(mouse.next(WM_LBUTTONDBLCLK, 0), Some("doubleClick"));
        assert_eq!(mouse.next(WM_LBUTTONUP, 0), None);
        assert_eq!(mouse.next(WM_LBUTTONUP, 0), Some("click"));
        assert_eq!(mouse.next(WM_MOUSEWHEEL, 120), Some("wheelUp"));
        assert_eq!(mouse.next(WM_MOUSEWHEEL, -120), Some("wheelDown"));
        assert_eq!(mouse.next(WM_MOUSEWHEEL, 0), None);
        assert_eq!(mouse.next(WM_RBUTTONUP, 0), Some("context"));
    }
}
pub struct NativeWindow {
    app: tauri::AppHandle,
    hwnd: usize,
    alive: Arc<AtomicBool>,
}
impl NativeWindow {
    pub fn create(
        app: &tauri::AppHandle,
        library: &str,
        session: &str,
        course: &str,
    ) -> Result<Self, String> {
        let app2 = app.clone();
        let input = WindowInput {
            app: app.clone(),
            library: library.into(),
            session: session.into(),
            course: course.into(),
            mouse: std::cell::Cell::new(MouseGesture::default()),
        };
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
                static REGISTERED: std::sync::OnceLock<Result<(), String>> =
                    std::sync::OnceLock::new();
                REGISTERED
                    .get_or_init(|| unsafe {
                        let class = WNDCLASSW {
                            style: CS_DBLCLKS,
                            lpfnWndProc: Some(surface_proc),
                            hInstance: GetModuleHandleW(std::ptr::null()),
                            lpszClassName: w!("StackDccLearningSurface"),
                            ..Default::default()
                        };
                        if RegisterClassW(&class) == 0 {
                            Err(std::io::Error::last_os_error().to_string())
                        } else {
                            Ok(())
                        }
                    })
                    .clone()?;
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        w!("StackDccLearningSurface"),
                        std::ptr::null(),
                        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                        0,
                        0,
                        1,
                        1,
                        parent,
                        std::ptr::null_mut(),
                        GetModuleHandleW(std::ptr::null()),
                        std::ptr::null(),
                    )
                };
                if hwnd.is_null() {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let input = Box::into_raw(Box::new(input));
                if unsafe { SetPropW(hwnd, w!("StackDccNativeInput"), input as _) } == 0 {
                    unsafe {
                        drop(Box::from_raw(input));
                        DestroyWindow(hwnd);
                    }
                    return Err("无法绑定播放鼠标事件".into());
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
