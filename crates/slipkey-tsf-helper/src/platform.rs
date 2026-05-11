//! Windows-only entry points.

use std::sync::atomic::AtomicBool;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::CallNextHookEx;

use crate::{compartment, first_call_only};

static EXECUTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub unsafe extern "system" fn call_wnd_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Windows convention: code < 0 means "must call CallNextHookEx and do nothing".
    if code >= 0 && first_call_only(&EXECUTED) {
        // Best-effort: log+swallow any panic so hook never propagates into the
        // target process. compartment::execute_once is implemented in Task 4.
        let result = std::panic::catch_unwind(|| compartment::execute_once());
        if let Err(panic) = result {
            log::error!("slipkey_tsf_helper panic suppressed: {:?}", panic);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
