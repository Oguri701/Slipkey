#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod dll_provisioning;
mod hook_thread;
mod startup;
mod tray;
mod ui;

use std::sync::{Arc, Mutex};

use eframe::egui;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    // Provision the bundled helper DLL into %LOCALAPPDATA%\Slipkey\ before
    // wiring TSF dispatch. Without this, IME switching falls back to "HKL only"
    // silently (which is degraded behavior for Japanese alphanumeric mode).
    match dll_provisioning::ensure_helper_dll() {
        Ok(path) => {
            imeswitch_windows::ime::tsf_dispatch::set_helper_dll_path(path);
        }
        Err(e) => {
            log::error!(
                "helper DLL provisioning failed: {} \
                 (Japanese alphanumeric mode will be degraded)",
                e
            );
        }
    }

    let state: app::SharedState = Arc::new(Mutex::new(app::AppState::load()));
    let hook = hook_thread::spawn(state.clone());

    let (icon_rgba, icon_w, icon_h) = load_icon();
    let tray = tray::Tray::new(icon_rgba.clone(), icon_w, icon_h);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Slipkey")
            .with_inner_size([480.0, 380.0])
            .with_min_inner_size([480.0, 380.0])
            .with_max_inner_size([480.0, 380.0])
            .with_resizable(false)
            .with_decorations(true)
            .with_visible(false)
            .with_icon(egui::IconData {
                rgba: icon_rgba.clone(),
                width: icon_w,
                height: icon_h,
            }),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let hook_for_app = hook.clone();
    let icon_for_app = icon_rgba.clone();

    eframe::run_native(
        "Slipkey",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::SettingsWindow::new(
                cc,
                state_for_app,
                hook_for_app,
                tray,
                &icon_for_app,
                icon_w,
                icon_h,
            )))
        }),
    )
    .expect("eframe failed");
}

fn load_icon() -> (Vec<u8>, u32, u32) {
    let bytes = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(bytes)
        .expect("icon decode failed")
        .into_rgba8();
    let (width, height) = img.dimensions();
    (img.into_raw(), width, height)
}
