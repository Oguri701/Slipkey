//! Slipkey TSF helper DLL.
//!
//! Injected briefly into the foreground window's GUI thread via
//! SetWindowsHookEx(WH_CALLWNDPROC). On the first hook callback, this DLL
//! reads a shared-memory command, writes the target TSF conversion mode via
//! ITfCompartment, signals completion, and unloads on UnhookWindowsHookEx.

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
mod compartment;

#[cfg(target_os = "windows")]
pub use platform::call_wnd_hook;

/// First-call-only guard. Returns true on the first call, false thereafter.
/// Exposed at crate root so tests can verify the contract on any host.
pub fn first_call_only(flag: &std::sync::atomic::AtomicBool) -> bool {
    !flag.swap(true, std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::first_call_only;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn first_call_returns_true_subsequent_calls_return_false() {
        let flag = AtomicBool::new(false);
        assert!(first_call_only(&flag));
        assert!(!first_call_only(&flag));
        assert!(!first_call_only(&flag));
    }
}
