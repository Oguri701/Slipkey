use std::sync::mpsc;

use imeswitch_windows::config::Config;

use crate::app::SharedState;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub enum HookCmd {
    Restart(Config),
}

pub fn spawn(state: SharedState, rx: mpsc::Receiver<HookCmd>) {
    std::thread::Builder::new()
        .name("slipkey-hook".into())
        .spawn(move || run(state, rx))
        .expect("hook thread spawn failed");
}

#[cfg(target_os = "windows")]
fn install(
    state: &SharedState,
) -> (
    Option<imeswitch_windows::hook::EventHook>,
    std::sync::Arc<std::sync::Mutex<imeswitch_windows::ime::ImeSwitcher>>,
) {
    use imeswitch_windows::ime::ImeSwitcher;
    use imeswitch_windows::keymap::{leader_vk_for, VK_SEMICOLON};

    let config = state.lock().unwrap().config.clone();
    let mapping = config.into_mapping();
    let leader_vk = leader_vk_for(mapping.leader()).unwrap_or(VK_SEMICOLON);
    let switcher = std::sync::Arc::new(std::sync::Mutex::new(ImeSwitcher::with_mapping(
        mapping.clone(),
    )));
    let hook = imeswitch_windows::hook::EventHook::install_with_mappings(
        leader_vk,
        mapping.trigger_mappings(),
        switcher.clone(),
    )
    .map_err(|error| log::error!("hook install failed: {error}"))
    .ok();
    (hook, switcher)
}

#[cfg(target_os = "windows")]
fn run(state: SharedState, rx: mpsc::Receiver<HookCmd>) {
    use imeswitch_windows::ime::ImeSwitcher;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };

    let (maybe_hook, switcher) = install(&state);
    state.lock().unwrap().hook_active = maybe_hook.is_some();

    let hook = match maybe_hook {
        Some(hook) => hook,
        None => {
            log::error!("hook not installed; hook thread exiting");
            return;
        }
    };

    loop {
        unsafe {
            let mut msg = std::mem::zeroed::<MSG>();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        match rx.try_recv() {
            Ok(HookCmd::Restart(config)) => {
                let mapping = config.into_mapping();
                *switcher.lock().unwrap() = ImeSwitcher::with_mapping(mapping.clone());
                hook.update_config(&mapping);
                state.lock().unwrap().hook_active = true;
                log::info!("hook config updated: {}", mapping.describe());
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => return,
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(not(target_os = "windows"))]
fn run(state: SharedState, _rx: mpsc::Receiver<HookCmd>) {
    log::warn!("hook thread: non-Windows target, hook disabled");
    state.lock().unwrap().hook_active = false;
}
