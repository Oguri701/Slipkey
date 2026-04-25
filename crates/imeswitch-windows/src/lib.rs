pub mod config;
pub mod ime;
pub mod keymap;

#[cfg(target_os = "windows")]
pub mod composition;
#[cfg(target_os = "windows")]
pub mod hook;

pub use ime::{ImeSwitcher, Mapping, SourceInfo, SwitchError, DEFAULT_LEADER};

#[cfg(target_os = "windows")]
pub use hook::{EventHook, HookError};

#[cfg(target_os = "windows")]
pub fn run_loop() {
    hook::run_message_loop();
}
