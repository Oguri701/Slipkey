use eframe::egui;

pub fn show(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if let Some(tex) = icon {
            ui.image((tex.id(), egui::vec2(64.0, 64.0)));
            ui.add_space(12.0);
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Slipkey").size(34.0).strong());
            ui.add_space(2.0);
            ui.label("Switch input methods by typing.");
            ui.add_space(4.0);
            let version = env!("CARGO_PKG_VERSION");
            ui.label(
                egui::RichText::new(format!("v{version}  (c) 2026 zlb"))
                    .small()
                    .weak(),
            );
        });
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    if ui.button("View on GitHub").clicked() {
        let _ = open::that("https://github.com/Oguri701/imeswitch");
    }
}
