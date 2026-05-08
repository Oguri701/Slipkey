use eframe::egui;

use super::{
    FONT_BODY, FONT_BODY_STRONG, FONT_CAPTION, WIN11_ACCENT, WIN11_SUCCESS, WIN11_TEXT,
    WIN11_TEXT_SEC, WIN11_WARNING,
};
use crate::app::AppState;

/// Renders the General tab.
///
/// Kept intentionally short: two cards (Startup, Permissions). Earlier
/// iterations had a tray-icon visibility toggle that was wired to a no-op,
/// and a Language picker hard-coded to English; both were removed because a
/// dead control reads worse than no control at all.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    super::preference_content(ui, |ui| {
        super::win11_card(ui, |ui| {
            super::section_title(ui, "Startup");
            setting_toggle(
                ui,
                "Open at startup",
                "Start Slipkey automatically after sign-in.",
                state.launch_at_login,
                |launch| match crate::startup::set_enabled(launch) {
                    Ok(()) => state.launch_at_login = launch,
                    Err(error) => state.status_message = format!("Startup error: {error}"),
                },
            );
        });

        ui.add_space(12.0);

        super::win11_card(ui, |ui| {
            super::section_title(ui, "Permissions");
            permission_row(ui, state.hook_active);
        });

        if !state.status_message.is_empty() {
            ui.add_space(8.0);
            super::caption(ui, &state.status_message);
        }
    });
}

fn setting_toggle(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    value: bool,
    on_change: impl FnOnce(bool),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 60.0);
            ui.label(
                egui::RichText::new(title)
                    .size(FONT_BODY_STRONG)
                    .strong()
                    .color(WIN11_TEXT),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(description)
                    .size(FONT_CAPTION)
                    .color(WIN11_TEXT_SEC),
            );
        });

        let mut value = value;
        if switch(ui, &mut value).changed() {
            on_change(value);
        }
    });
}

/// Win11-style toggle switch. 40×20 with a 16px knob — same proportions
/// Settings.exe uses for its inline switches.
fn switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let fill = if *value {
        WIN11_ACCENT
    } else {
        egui::Color32::from_rgb(245, 245, 245)
    };
    let stroke = egui::Stroke::new(
        1.0,
        if *value {
            WIN11_ACCENT
        } else {
            egui::Color32::from_rgb(140, 140, 140)
        },
    );
    ui.painter().rect_filled(rect, 10.0, fill);
    ui.painter().rect_stroke(rect, 10.0, stroke, egui::epaint::StrokeKind::Outside);

    let knob_radius = if *value { 6.0 } else { 5.0 };
    let knob_x = if *value {
        rect.max.x - 10.0
    } else {
        rect.min.x + 10.0
    };
    let knob_color = if *value {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(80, 80, 80)
    };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), knob_radius, knob_color);

    response
}

/// Permission status block: a colored dot, the state label, and a one-line
/// caption explaining why we need it.
fn permission_row(ui: &mut egui::Ui, hook_active: bool) {
    let (color, label) = if hook_active {
        (WIN11_SUCCESS, "Ready")
    } else {
        (WIN11_WARNING, "Inactive")
    };

    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, color);

        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Accessibility")
                        .size(FONT_BODY_STRONG)
                        .strong()
                        .color(WIN11_TEXT),
                );
                ui.label(
                    egui::RichText::new(label)
                        .size(FONT_BODY)
                        .color(color),
                );
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Required to intercept the leader key before the active IME consumes it.")
                    .size(FONT_CAPTION)
                    .color(WIN11_TEXT_SEC),
            );
        });
    });
}
