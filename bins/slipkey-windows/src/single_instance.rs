#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOWNORMAL},
};

const WINDOW_TITLE: &str = "Slipkey";

#[cfg(target_os = "windows")]
const MUTEX_NAME: &str = r"Local\Slipkey-singleton";

pub enum AcquireResult {
    Acquired(SingleInstanceGuard),
    AlreadyRunning,
}

pub struct SingleInstanceGuard {
    #[cfg(target_os = "windows")]
    handle: HANDLE,
}

pub fn acquire() -> AcquireResult {
    acquire_platform()
}

#[cfg(target_os = "windows")]
fn acquire_platform() -> AcquireResult {
    let mutex_name = wide_null(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if handle.is_null() {
        log::warn!("single-instance mutex creation failed");
        return AcquireResult::Acquired(SingleInstanceGuard { handle });
    }

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        show_existing_instance();
        unsafe {
            CloseHandle(handle);
        }
        return AcquireResult::AlreadyRunning;
    }

    AcquireResult::Acquired(SingleInstanceGuard { handle })
}

#[cfg(not(target_os = "windows"))]
fn acquire_platform() -> AcquireResult {
    AcquireResult::Acquired(SingleInstanceGuard {})
}

#[cfg(target_os = "windows")]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn show_existing_instance() {
    let title = wide_null(WINDOW_TITLE);
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        log::info!("Slipkey is already running; existing settings window not found yet");
        return;
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
    }
}

#[cfg(target_os = "windows")]
fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
