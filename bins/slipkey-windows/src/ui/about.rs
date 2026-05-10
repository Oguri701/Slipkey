use eframe::egui;

use super::{tr, FONT_BODY, FONT_CAPTION, FONT_TITLE, WIN11_ACCENT, WIN11_TEXT, WIN11_TEXT_SEC};

pub fn show(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, lang: &str) {
    super::preference_content(ui, |ui| {
        super::win11_card(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(tex) = icon {
                    ui.image((tex.id(), egui::vec2(56.0, 56.0)));
                    ui.add_space(14.0);
                }
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new("Slipkey")
                            .size(FONT_TITLE)
                            .color(WIN11_TEXT),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(tr(lang, "Switch input methods by typing."))
                                .size(FONT_BODY)
                                .color(WIN11_TEXT_SEC),
                        )
                        .wrap(),
                    );
                    ui.add_space(4.0);
                    let version = env!("CARGO_PKG_VERSION");
                    ui.label(
                        egui::RichText::new(format!("Version {version}  -  (c) 2026 oguri701"))
                            .size(FONT_CAPTION)
                            .color(WIN11_TEXT_SEC),
                    );
                });
            });

            ui.add_space(14.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().min.y,
                egui::Stroke::new(1.0, super::WIN11_SEPARATOR),
            );
            ui.add_space(10.0);

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(tr(lang, "View on GitHub"))
                            .size(FONT_BODY)
                            .color(WIN11_ACCENT),
                    )
                    .frame(false),
                )
                .clicked()
            {
                let _ = open::that("https://github.com/Oguri701/Slipkey");
            }
        });
    });
}
