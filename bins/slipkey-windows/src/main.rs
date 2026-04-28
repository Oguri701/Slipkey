#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hook_thread;
mod startup;
mod tray;
mod ui;

use std::sync::{mpsc, Arc, Mutex};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let state: app::SharedState = Arc::new(Mutex::new(app::AppState::load()));
    let (hook_tx, hook_rx) = mpsc::channel::<hook_thread::HookCmd>();
    hook_thread::spawn(state.clone(), hook_rx);

    let (icon_rgba, icon_w, icon_h) = load_icon();
    let tray = tray::Tray::new(icon_rgba.clone(), icon_w, icon_h);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Slipkey")
            .with_inner_size([500.0, 360.0])
            .with_resizable(false)
            .with_visible(false)
            .with_icon(egui::IconData {
                rgba: icon_rgba.clone(),
                width: icon_w,
                height: icon_h,
            }),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let hook_tx_for_app = hook_tx.clone();
    let icon_for_app = icon_rgba.clone();

    eframe::run_native(
        "Slipkey",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::SettingsWindow::new(
                cc,
                state_for_app,
                hook_tx_for_app,
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
