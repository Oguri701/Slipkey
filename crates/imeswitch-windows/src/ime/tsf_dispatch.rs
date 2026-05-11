//! Host-side TSF dispatch: inject helper DLL, signal it, wait for completion.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock as StdOnceLock;

use crate::ime::WinImeMode;

/// TSF conversion mode bits, mirror values from `<msctf.h>`.
pub const TF_CONVERSIONMODE_ALPHANUMERIC: u32 = 0x0000;
pub const TF_CONVERSIONMODE_NATIVE: u32 = 0x0001;
pub const TF_CONVERSIONMODE_FULLSHAPE: u32 = 0x0008;
pub const TF_CONVERSIONMODE_ROMAN: u32 = 0x0010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsfTarget {
    pub conversion_mode: u32,
    pub open_status: bool,
}

impl TsfTarget {
    /// Translate a (mode, language) pair into the TSF Compartment values.
    pub fn for_mode(mode: WinImeMode, language: &str) -> Option<Self> {
        match mode {
            WinImeMode::Alphanumeric => Some(Self {
                conversion_mode: TF_CONVERSIONMODE_ALPHANUMERIC,
                // Keep the IME active; only switch its internal mode.
                // This is the decision D1 from the design doc.
                open_status: true,
            }),
            WinImeMode::Native => Some(Self {
                conversion_mode: match language {
                    // Japanese needs full-shape + Roman input style for "ja kana via romaji".
                    "ja" => {
                        TF_CONVERSIONMODE_NATIVE
                            | TF_CONVERSIONMODE_FULLSHAPE
                            | TF_CONVERSIONMODE_ROMAN
                    }
                    // Chinese/Korean: just native. No full-shape forcing.
                    _ => TF_CONVERSIONMODE_NATIVE,
                },
                open_status: true,
            }),
            // LayoutOnly bypasses TSF entirely (e.g. French AZERTY).
            WinImeMode::LayoutOnly => None,
        }
    }
}

#[derive(Debug)]
pub enum DispatchError {
    DllNotFound(PathBuf),
    NoFocusWindow,
    InjectionRefused(u32),
    Timeout,
    HelperFailed { hresult: u32 },
    AbiMismatch,
    System(u32),
}

pub struct TsfDispatcher {
    next_sequence: AtomicU32,
    helper_dll_path: PathBuf,
}

/// Static slot for the resolved helper DLL path. Must be `set()` by the host
/// app before the first call to `tsf_dispatch::global()`.
static DLL_PATH: StdOnceLock<PathBuf> = StdOnceLock::new();

/// Inject the path that `TsfDispatcher` should use when injecting the helper
/// DLL via `SetWindowsHookEx`. Call this from `main()` after provisioning.
pub fn set_helper_dll_path(path: PathBuf) {
    let _ = DLL_PATH.set(path);
}

impl TsfDispatcher {
    pub fn new_with_path(helper_dll_path: PathBuf) -> Result<Self, DispatchError> {
        if !helper_dll_path.exists() {
            return Err(DispatchError::DllNotFound(helper_dll_path));
        }
        Ok(Self {
            next_sequence: AtomicU32::new(1),
            helper_dll_path,
        })
    }

    pub fn dispatch(&self, target: TsfTarget) -> Result<(), DispatchError> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = target;
            Err(DispatchError::System(0))
        }

        #[cfg(target_os = "windows")]
        {
            platform::dispatch_impl(
                &self.helper_dll_path,
                self.next_sequence.fetch_add(1, Ordering::SeqCst),
                target,
            )
        }
    }
}

/// Lazily-initialized global dispatcher. Returns None if the helper DLL is
/// missing from the install directory; caller logs and skips the TSF step.
pub fn global() -> Option<&'static TsfDispatcher> {
    static INSTANCE: StdOnceLock<Option<TsfDispatcher>> = StdOnceLock::new();
    INSTANCE
        .get_or_init(|| {
            let path = match DLL_PATH.get().cloned() {
                Some(p) => p,
                None => {
                    log::warn!(
                        "TsfDispatcher disabled: helper DLL path not set \
                         (call set_helper_dll_path() from main before first dispatch)"
                    );
                    return None;
                }
            };
            match TsfDispatcher::new_with_path(path) {
                Ok(d) => Some(d),
                Err(e) => {
                    log::warn!("TsfDispatcher disabled: {:?}", e);
                    None
                }
            }
        })
        .as_ref()
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    use imeswitch_tsf_protocol::{
        completion_event_name, shared_memory_name, TsfCommand, TsfResult, ABI_VERSION,
        DISPATCH_TIMEOUT_MS,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
        MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, SendMessageTimeoutW,
        SetWindowsHookExW, UnhookWindowsHookEx, GUITHREADINFO, SMTO_ABORTIFHUNG, WH_CALLWNDPROC,
        WM_NULL,
    };

    pub(super) fn dispatch_impl(
        helper_dll_path: &std::path::Path,
        sequence: u32,
        target: TsfTarget,
    ) -> Result<(), DispatchError> {
        let focus = focused_target().ok_or(DispatchError::NoFocusWindow)?;
        let target_thread_id = focus.thread_id;

        let shm_name = wide(&shared_memory_name(target_thread_id));
        let cmd_size = std::mem::size_of::<TsfCommand>();
        let shm_handle = unsafe {
            CreateFileMappingW(
                std::ptr::null_mut(),
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                cmd_size as u32,
                shm_name.as_ptr(),
            )
        };
        if shm_handle.is_null() {
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }
        let view = unsafe { MapViewOfFile(shm_handle, FILE_MAP_ALL_ACCESS, 0, 0, cmd_size) };
        if view.Value.is_null() {
            unsafe {
                let _ = CloseHandle(shm_handle);
            };
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }
        let cmd_ptr = view.Value as *mut TsfCommand;

        unsafe {
            std::ptr::write(
                cmd_ptr,
                TsfCommand {
                    abi_version: ABI_VERSION,
                    sequence,
                    target_conversion_mode: target.conversion_mode,
                    target_open_status: if target.open_status { 1 } else { 0 },
                    result: std::sync::atomic::AtomicU32::new(TsfResult::Pending as u32),
                    error_hresult: 0,
                },
            );
        }

        let event_name = wide(&completion_event_name(target_thread_id, sequence));
        let event_handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
        if event_handle.is_null() {
            unsafe {
                let _ = UnmapViewOfFile(view);
                let _ = CloseHandle(shm_handle);
            };
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        let helper_hmod =
            unsafe { LoadLibraryW(wide(&helper_dll_path.to_string_lossy()).as_ptr()) };
        if helper_hmod.is_null() {
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        let hook_proc = unsafe { GetProcAddress(helper_hmod, b"call_wnd_hook\0".as_ptr()) };
        if hook_proc.is_none() {
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::System(unsafe { GetLastError() }));
        }

        let hook = unsafe {
            SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(std::mem::transmute(hook_proc)),
                helper_hmod,
                focus.thread_id,
            )
        };
        if hook.is_null() {
            let err = unsafe { GetLastError() };
            cleanup(view, shm_handle, event_handle);
            return Err(DispatchError::InjectionRefused(err));
        }

        let mut send_result = 0usize;
        unsafe {
            let _ = SendMessageTimeoutW(
                focus.hwnd,
                WM_NULL,
                0,
                0,
                SMTO_ABORTIFHUNG,
                50,
                &mut send_result,
            );
        }

        let wait_rc = unsafe { WaitForSingleObject(event_handle, DISPATCH_TIMEOUT_MS) };

        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        };

        let outcome = match wait_rc {
            WAIT_OBJECT_0 => {
                let cmd_ref = unsafe { &*cmd_ptr };
                match cmd_ref.result.load(Ordering::SeqCst) {
                    v if v == TsfResult::Ok as u32 => Ok(()),
                    v if v == TsfResult::AbiMismatch as u32 => Err(DispatchError::AbiMismatch),
                    v if v == TsfResult::Failed as u32 => Err(DispatchError::HelperFailed {
                        hresult: cmd_ref.error_hresult,
                    }),
                    _ => Err(DispatchError::Timeout),
                }
            }
            WAIT_TIMEOUT => Err(DispatchError::Timeout),
            _ => Err(DispatchError::System(unsafe { GetLastError() })),
        };

        cleanup(view, shm_handle, event_handle);
        outcome
    }

    struct FocusTarget {
        hwnd: HWND,
        thread_id: u32,
    }

    fn focused_target() -> Option<FocusTarget> {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.is_null() {
                return None;
            }
            let mut info: GUITHREADINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            let fg_tid = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
            if fg_tid == 0 {
                return None;
            }
            if GetGUIThreadInfo(fg_tid, &mut info) != 0 && !info.hwndFocus.is_null() {
                let focused_tid = GetWindowThreadProcessId(info.hwndFocus, std::ptr::null_mut());
                if focused_tid != 0 {
                    return Some(FocusTarget {
                        hwnd: info.hwndFocus,
                        thread_id: focused_tid,
                    });
                }
            }
            Some(FocusTarget {
                hwnd: foreground,
                thread_id: fg_tid,
            })
        }
    }

    fn cleanup(view: MEMORY_MAPPED_VIEW_ADDRESS, shm: HANDLE, event: HANDLE) {
        unsafe {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(shm);
            let _ = CloseHandle(event);
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphanumeric_keeps_ime_open_and_uses_zero_mode() {
        let t = TsfTarget::for_mode(WinImeMode::Alphanumeric, "ja").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_ALPHANUMERIC);
        assert!(t.open_status, "must keep IME open (design D1)");
    }

    #[test]
    fn native_japanese_uses_native_fullshape_roman() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ja").unwrap();
        assert_eq!(
            t.conversion_mode,
            TF_CONVERSIONMODE_NATIVE | TF_CONVERSIONMODE_FULLSHAPE | TF_CONVERSIONMODE_ROMAN
        );
        assert!(t.open_status);
    }

    #[test]
    fn native_chinese_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "zh").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
        assert!(t.open_status);
    }

    #[test]
    fn native_korean_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ko").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
    }

    #[test]
    fn layout_only_returns_none() {
        assert!(TsfTarget::for_mode(WinImeMode::LayoutOnly, "fr").is_none());
    }

    #[test]
    fn dispatch_error_is_debug() {
        let e = DispatchError::NoFocusWindow;
        let _ = format!("{:?}", e);
    }
}
