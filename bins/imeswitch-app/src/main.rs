mod commands;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use imeswitch_core::Language;
use imeswitch_macos::config;
use imeswitch_macos::keymap::{leader_keycode_for, KC_SEMICOLON};
use imeswitch_macos::{
    is_accessibility_trusted, request_accessibility_permission, EventHook, ImeSwitcher, Mapping,
};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    hook: Mutex<Option<HookHolder>>,
    mapping: Mutex<Mapping>,
    tray_visible: Mutex<bool>,
    tray: Mutex<Option<TrayIcon>>,
    // Set while a hook (re)install is queued on the main thread. The watcher
    // thread observes hook_installed=false from a worker thread; without this
    // guard, two consecutive 2 s ticks could enqueue two install closures
    // before the first runs, causing redundant tap creation.
    install_in_flight: AtomicBool,
}

struct HookHolder(#[allow(dead_code)] EventHook);

// EventHook owns CF objects whose threading rules require creation and drop on
// the AppKit main thread. The Send/Sync impl is sound because:
// - install_hook always runs on the main thread (setup() at startup, and
//   run_on_main_thread() from the watcher's reinstall path).
// - The watcher thread only reads `hook_installed()` (Mutex lock + bool); it
//   never drops the EventHook itself.
// Future contributors: do not drop a HookHolder from a non-main thread.
unsafe impl Send for HookHolder {}
unsafe impl Sync for HookHolder {}

impl AppState {
    pub fn hook_installed(&self) -> bool {
        self.hook.lock().unwrap().is_some()
    }

    pub fn current_mapping(&self) -> Mapping {
        self.mapping.lock().unwrap().clone()
    }

    fn install_hook(&self, mapping: Mapping) -> anyhow::Result<()> {
        // Drop the previous hook BEFORE installing the new one so the
        // CGEventTap is uninstalled and we don't double-hook the keyboard.
        self.hook.lock().unwrap().take();

        let trigger_mappings = mapping.trigger_mappings();
        let leader_keycode = leader_keycode_for(mapping.leader()).unwrap_or(KC_SEMICOLON);
        let switcher = Arc::new(ImeSwitcher::with_mapping(mapping.clone()));
        let hook = EventHook::install_with_mappings(
            leader_keycode,
            trigger_mappings,
            move |lang: Language| {
                if let Err(error) = switcher.switch_to(&lang) {
                    log::error!("switch {} failed: {}", lang, error);
                }
            },
        )
        .map_err(|error| anyhow::anyhow!("hook install failed: {error}"))?;

        *self.hook.lock().unwrap() = Some(HookHolder(hook));
        *self.mapping.lock().unwrap() = mapping;
        Ok(())
    }
}

fn main() {
    tauri::Builder::default()
        // Single-instance plugin must be the first plugin registered. When a
        // second launch happens (Spotlight, Finder, login item), the new
        // process detects the lock, fires this callback in the original
        // process, and exits. Prevents tray-icon stacking and duplicate
        // event taps.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        show_settings(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Belt-and-braces with Info.plist's LSUIElement=true. macOS
            // Launch Services caches activation policy per bundle id; if the
            // app was ever launched without LSUIElement, the dock-icon entry
            // sticks until we explicitly request Accessory at runtime.
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let (mapping, _outcome) = config::load_or_default();
            let state = AppState {
                hook: Mutex::new(None),
                mapping: Mutex::new(mapping.clone()),
                tray_visible: Mutex::new(false),
                tray: Mutex::new(None),
                install_in_flight: AtomicBool::new(false),
            };
            // If Accessibility is not granted, CGEventTap will fail.
            // Request permission (shows the system dialog) and continue —
            // the polling thread below picks up the grant without a restart.
            let hook_failed = state.install_hook(mapping).is_err();
            if hook_failed {
                request_accessibility_permission();
            }
            app.manage(state);
            let app_state = app.state::<AppState>();
            commands::set_tray_visible(app.handle(), &app_state, true)
                .map_err(|error| anyhow::anyhow!("failed to create menu bar icon: {error}"))?;

            let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Comma);
            app.global_shortcut().register(shortcut)?;

            // Background watcher: when the user grants Accessibility in System
            // Settings, retry installing the hook on the main thread. Emits
            // `hook-status` to the frontend on every state change.
            spawn_hook_watcher(app.handle().clone(), hook_failed);

            // Open settings if the hook couldn't start (typically first run
            // before Accessibility is granted).
            if hook_failed {
                show_settings(app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::discover_imes,
            commands::set_autostart,
            commands::get_autostart,
            commands::get_menubar_visible,
            commands::set_menubar_visible,
            commands::open_settings,
            commands::get_status,
            commands::request_accessibility,
        ])
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building imeswitch app")
        .run(|app, event| {
            // The app keeps running with LSUIElement + a tray icon after the
            // settings window is closed. macOS fires Reopen when the user
            // double-clicks the bundle again or clicks the dock icon (which
            // we don't have, but Finder/Spotlight launches still go through
            // here). Without this handler the second launch silently no-ops
            // and the user thinks the app is broken.
            if let tauri::RunEvent::Reopen { .. } = event {
                show_settings(app);
            }
        });
}

pub(crate) fn show_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn spawn_hook_watcher(app: AppHandle, hook_initially_failed: bool) {
    // No initial emit here. The frontend's load() always seeds the UI from
    // invoke("get_status"), which is the canonical source. The watcher's diff
    // below is the only place that emits hook-status thereafter.
    thread::spawn(move || {
        let mut last_status = (
            !hook_initially_failed,
            is_accessibility_trusted(),
        );
        loop {
            thread::sleep(Duration::from_secs(2));
            let state = app.state::<AppState>();
            let hook_installed = state.hook_installed();
            let trusted = is_accessibility_trusted();

            // Try to (re)install the hook on the main thread when Accessibility
            // becomes available. CGEventTap must be created on a thread that
            // pumps a CFRunLoop, which is the Tauri/AppKit main thread.
            // The AtomicBool gate prevents queuing a second install closure
            // while the previous one is still pending on the main thread.
            if !hook_installed
                && trusted
                && !state
                    .install_in_flight
                    .swap(true, Ordering::AcqRel)
            {
                let mapping = state.current_mapping();
                let app_for_main = app.clone();
                let dispatch = app.run_on_main_thread(move || {
                    let state = app_for_main.state::<AppState>();
                    if let Err(e) = state.install_hook(mapping) {
                        log::error!("hook re-install failed: {e}");
                    } else {
                        log::info!("hook installed after Accessibility was granted");
                    }
                    state.install_in_flight.store(false, Ordering::Release);
                });
                if dispatch.is_err() {
                    // Closure won't run; release the gate ourselves.
                    state.install_in_flight.store(false, Ordering::Release);
                }
            }

            let new_status = (state.hook_installed(), trusted);
            if new_status != last_status {
                let payload = StatusPayload {
                    hook_installed: new_status.0,
                    accessibility_granted: new_status.1,
                };
                let _ = app.emit("hook-status", payload);
                last_status = new_status;
            }
        }
    });
}

#[derive(Clone, serde::Serialize)]
struct StatusPayload {
    hook_installed: bool,
    accessibility_granted: bool,
}
