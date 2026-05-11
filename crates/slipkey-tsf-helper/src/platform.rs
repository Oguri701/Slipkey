//! Windows-only entry points.

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::CallNextHookEx;

/// CallWndProc hook procedure. Exported for `SetWindowsHookEx` to discover via
/// GetProcAddress when the host passes our HMODULE.
#[no_mangle]
pub unsafe extern "system" fn call_wnd_hook(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Step 1 only: pass through. Real logic added in Task 3.
    CallNextHookEx(None, code, wparam, lparam)
}
