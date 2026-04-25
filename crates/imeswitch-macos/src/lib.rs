#![cfg(target_os = "macos")]

pub mod composition;
pub mod config;
pub mod dispatch;
pub mod hook;
pub mod ime;
pub mod keymap;

pub use hook::{EventHook, HookError};
pub use ime::{
    discover_installed_imes, DetectedIME, ImeSwitcher, Mapping, SwitchError, DEFAULT_LEADER,
};

/// Enter the current thread's CFRunLoop. Blocks until the runloop stops
/// (e.g. process shutdown). Callers must install the `EventHook` first so
/// its runloop source is registered.
pub fn run_loop() {
    core_foundation::runloop::CFRunLoop::run_current();
}
