mod commands;

use std::sync::{Arc, Mutex};

use anyhow::Context;
use imeswitch_core::Language;
use imeswitch_macos::config;
use imeswitch_macos::keymap::{leader_keycode_for, KC_SEMICOLON};
use imeswitch_macos::{EventHook, ImeSwitcher, Mapping};
use tauri::tray::TrayIcon;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    hook: Mutex<Option<HookHolder>>,
    mapping: Mutex<Mapping>,
    tray_visible: Mutex<bool>,
    tray: Mutex<Option<TrayIcon>>,
}

struct HookHolder(#[allow(dead_code)] EventHook);

unsafe impl Send for HookHolder {}
unsafe impl Sync for HookHolder {}

impl AppState {
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
            let (mapping, _outcome) = config::load_or_default();
            let state = AppState {
                hook: Mutex::new(None),
                mapping: Mutex::new(mapping.clone()),
                tray_visible: Mutex::new(false),
                tray: Mutex::new(None),
            };
            state
                .install_hook(mapping)
                .context("failed to install keyboard hook")?;
            app.manage(state);
            let app_state = app.state::<AppState>();
            commands::set_tray_visible(app.handle(), &app_state, true)
                .map_err(|error| anyhow::anyhow!("failed to create menu bar icon: {error}"))?;

            let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Comma);
            app.global_shortcut().register(shortcut)?;

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
        ])
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running imeswitch app");
}

pub(crate) fn show_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
