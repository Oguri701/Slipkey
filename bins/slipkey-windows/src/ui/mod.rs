use std::sync::mpsc;

use eframe::egui;

use crate::app::SharedState;
use crate::hook_thread::HookCmd;
use crate::tray::Tray;

pub mod about;
pub mod general;
pub mod shortcuts;

#[derive(PartialEq)]
enum Tab {
    General,
    Shortcuts,
    About,
}

pub struct SettingsWindow {
    state: SharedState,
    hook_tx: mpsc::Sender<HookCmd>,
    tray: Tray,
    tab: Tab,
    icon_texture: Option<egui::TextureHandle>,
}

impl SettingsWindow {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: SharedState,
        hook_tx: mpsc::Sender<HookCmd>,
        tray: Tray,
        icon_rgba: &[u8],
        icon_w: u32,
        icon_h: u32,
    ) -> Self {
        let icon_texture = cc.egui_ctx.load_texture(
            "app_icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon_w as usize, icon_h as usize],
                icon_rgba,
            ),
            egui::TextureOptions::default(),
        );
        Self {
            state,
            hook_tx,
            tray,
            tab: Tab::General,
            icon_texture: Some(icon_texture),
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { .. } = event {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.tray.open_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if event.id == self.tray.quit_id {
                std::process::exit(0);
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::General, "General");
                ui.add_space(4.0);
                ui.selectable_value(&mut self.tab, Tab::Shortcuts, "Shortcuts");
                ui.add_space(4.0);
                ui.selectable_value(&mut self.tab, Tab::About, "About");
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::General => {
                let mut state = self.state.lock().unwrap();
                general::show(ui, &mut state);
            }
            Tab::Shortcuts => {
                let mut state = self.state.lock().unwrap();
                shortcuts::show(ui, &mut state, &self.hook_tx);
            }
            Tab::About => {
                about::show(ui, self.icon_texture.as_ref());
            }
        });
    }
}
