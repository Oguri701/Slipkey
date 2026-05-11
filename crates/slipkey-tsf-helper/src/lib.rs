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
