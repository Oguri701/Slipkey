use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};

use imeswitch_windows::config::Config;

use crate::app::SharedState;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub enum HookCmd {
    Restart(Config),
}

/// Handle for talking to the hook thread.
///
/// `send` queues a command and wakes the hook thread out of `GetMessageW` via
/// `PostThreadMessageW(WM_USER)`. This replaces the old design where the
/// thread polled with `PeekMessageW` + `try_recv` every 10 ms and burned CPU
/// even with nothing to do.
#[derive(Clone)]
pub struct HookHandle {
    tx: mpsc::Sender<HookCmd>,
    /// Set by the hook thread the moment it starts, before `SetWindowsHookExW`
    /// is called. Reads use Acquire so callers see a non-zero id only after
    /// the corresponding store on the worker thread.
    thread_id: Arc<AtomicU32>,
}

impl HookHandle {
    pub fn send(&self, cmd: HookCmd) -> Result<(), mpsc::SendError<HookCmd>> {
        self.tx.send(cmd)?;
        let tid = self.thread_id.load(Ordering::Acquire);
        if tid != 0 {
            wake_thread(tid);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn wake_thread(tid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_USER};
    unsafe {
        PostThreadMessageW(tid, WM_USER, 0, 0);
    }
}

#[cfg(not(target_os = "windows"))]
fn wake_thread(_tid: u32) {}

/// Spawn the hook thread and block until it has reported its thread id.
///
/// Blocking is intentional: we want `HookHandle::send` to be able to wake the
/// thread immediately, which means we cannot return until `GetCurrentThreadId`
/// has been recorded.
pub fn spawn(state: SharedState) -> HookHandle {
    let (tx, rx) = mpsc::channel::<HookCmd>();
    let (ready_tx, ready_rx) = mpsc::sync_channel::<u32>(0);
    let thread_id = Arc::new(AtomicU32::new(0));
    let thread_id_for_worker = thread_id.clone();

    std::thread::Builder::new()
        .name("slipkey-hook".into())
        .spawn(move || run(state, rx, thread_id_for_worker, ready_tx))
        .expect("hook thread spawn failed");

    let _ = ready_rx.recv();
    HookHandle { tx, thread_id }
}

#[cfg(target_os = "windows")]
fn install(
    state: &SharedState,
) -> (
    Option<imeswitch_windows::hook::EventHook>,
    std::sync::Arc<std::sync::Mutex<imeswitch_windows::ime::WindowsImeSwitcher>>,
) {
    use imeswitch_windows::ime::WindowsImeSwitcher;
    use imeswitch_windows::keymap::{leader_scan_code_for, SC_SEMICOLON};

    let config = state.lock().unwrap().config.clone();
    let mapping = config.into_mapping();
    let leader_scan_code = leader_scan_code_for(mapping.leader()).unwrap_or(SC_SEMICOLON);
    let switcher = std::sync::Arc::new(std::sync::Mutex::new(WindowsImeSwitcher::with_mapping(
        mapping.clone(),
    )));
    let hook = imeswitch_windows::hook::EventHook::install_with_mappings(
        leader_scan_code,
        mapping.trigger_mappings(),
        switcher.clone(),
    )
    .map_err(|error| log::error!("hook install failed: {error}"))
    .ok();
    (hook, switcher)
}

#[cfg(target_os = "windows")]
fn run(
    state: SharedState,
    rx: mpsc::Receiver<HookCmd>,
    thread_id: Arc<AtomicU32>,
    ready: mpsc::SyncSender<u32>,
) {
    use imeswitch_windows::ime::WindowsImeSwitcher;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_QUIT, WM_USER,
    };

    let tid = unsafe { GetCurrentThreadId() };
    thread_id.store(tid, Ordering::Release);
    let _ = ready.send(tid);
    drop(ready);

    let (maybe_hook, switcher) = install(&state);
    state.lock().unwrap().hook_active = maybe_hook.is_some();

    let hook = match maybe_hook {
        Some(hook) => hook,
        None => {
            log::error!("hook not installed; hook thread exiting");
            return;
        }
    };

    // Block in GetMessageW until the OS hands us a key event (delivered as
    // a low-level keyboard hook callback inside DispatchMessageW) or
    // `HookHandle::send` posts a WM_USER wake-up. No periodic polling.
    loop {
        unsafe {
            let mut msg = std::mem::zeroed::<MSG>();
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret <= 0 {
                // 0 = WM_QUIT, -1 = error. Either way, we're done.
                return;
            }
            if msg.message == WM_QUIT {
                return;
            }
            if msg.message == WM_USER {
                // Drain every queued config update — Save can be clicked
                // multiple times before we wake.
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        HookCmd::Restart(config) => {
                            let mapping = config.into_mapping();
                            *switcher.lock().unwrap() =
                                WindowsImeSwitcher::with_mapping(mapping.clone());
                            hook.update_config(&mapping);
                            state.lock().unwrap().hook_active = true;
                            log::info!("hook config updated: {}", mapping.describe());
                        }
                    }
                }
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn run(
    state: SharedState,
    _rx: mpsc::Receiver<HookCmd>,
    _thread_id: Arc<AtomicU32>,
    ready: mpsc::SyncSender<u32>,
) {
    log::warn!("hook thread: non-Windows target, hook disabled");
    state.lock().unwrap().hook_active = false;
    let _ = ready.send(0);
}
