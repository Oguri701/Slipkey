use eframe::egui;

use crate::app::AppState;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);

    let mut launch = state.launch_at_login;
    if ui.checkbox(&mut launch, "Launch at login").changed() {
        match crate::startup::set_enabled(launch) {
            Ok(()) => state.launch_at_login = launch,
            Err(error) => state.status_message = format!("Startup error: {error}"),
        }
    }
    ui.label(
        egui::RichText::new("Start Slipkey automatically after login.")
            .small()
            .weak(),
    );

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        let (color, label) = if state.hook_active {
            (egui::Color32::from_rgb(50, 200, 80), "Active")
        } else {
            (egui::Color32::from_rgb(200, 80, 50), "Inactive")
        };
        ui.colored_label(color, "Status");
        ui.label(label);
        ui.label(egui::RichText::new("- keyboard hook").small().weak());
    });
}
