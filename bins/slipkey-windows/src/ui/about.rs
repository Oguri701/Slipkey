use eframe::egui;

use super::{
    tr, FONT_BODY, FONT_CAPTION, FONT_SUBTITLE, FONT_TITLE, WIN11_ACCENT, WIN11_BG, WIN11_BORDER,
    WIN11_SURFACE, WIN11_TEXT, WIN11_TEXT_SEC,
};

const SUPPORT_QR_BYTES: &[u8] = include_bytes!("../../assets/wechat-support.jpeg");

pub fn show(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, lang: &str) {
    let mut support_open = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(support_panel_id()).unwrap_or(false));

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
                        egui::RichText::new(format!(
                            "{} {version}  -  (c) 2026 oguri701",
                            tr(lang, "Version")
                        ))
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

            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr(lang, "View on GitHub"))
                                .size(FONT_BODY)
                                .color(WIN11_ACCENT),
                        )
                        .min_size(egui::vec2(110.0, 30.0))
                        .frame(false),
                    )
                    .clicked()
                {
                    let _ = open::that("https://github.com/Oguri701/Slipkey");
                }

                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr(lang, "Support author"))
                                .size(FONT_BODY)
                                .color(WIN11_ACCENT),
                        )
                        .min_size(egui::vec2(104.0, 30.0))
                        .frame(false),
                    )
                    .clicked()
                {
                    support_open = !support_open;
                }
            });

            if support_open {
                ui.add_space(10.0);
                support_author_panel(ui, lang);
            }
        });
    });

    ui.ctx()
        .data_mut(|data| data.insert_temp(support_panel_id(), support_open));
}

fn support_author_panel(ui: &mut egui::Ui, lang: &str) {
    egui::Frame::new()
        .fill(WIN11_BG)
        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(tr(lang, "WeChat"))
                        .size(FONT_SUBTITLE)
                        .color(WIN11_TEXT),
                );
                ui.add_space(8.0);

                if let Some(texture) = support_qr_texture(ui.ctx()) {
                    egui::Frame::new()
                        .fill(WIN11_SURFACE)
                        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.image((texture.id(), egui::vec2(188.0, 188.0)));
                        });
                } else {
                    egui::Frame::new()
                        .fill(WIN11_SURFACE)
                        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.set_min_size(egui::vec2(200.0, 200.0));
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new(tr(lang, "QR code unavailable"))
                                        .size(FONT_BODY)
                                        .color(WIN11_TEXT_SEC),
                                );
                            });
                        });
                }

                ui.add_space(10.0);
                let text_width = (ui.available_width() - 12.0).max(180.0);
                ui.add_sized(
                    [text_width, 36.0],
                    egui::Label::new(
                        egui::RichText::new(tr(
                            lang,
                            "If Slipkey helped you, welcome to buy the author a coffee.",
                        ))
                        .size(FONT_BODY)
                        .color(WIN11_TEXT_SEC),
                    )
                    .wrap()
                    .halign(egui::Align::Center),
                );
            });
        });
}

fn support_qr_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    if let Some(texture) =
        ctx.data_mut(|data| data.get_temp::<egui::TextureHandle>(support_texture_id()))
    {
        return Some(texture);
    }

    let image = image::load_from_memory(SUPPORT_QR_BYTES).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let texture = ctx.load_texture(
        "support_author_wechat_qr",
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        egui::TextureOptions::NEAREST,
    );
    ctx.data_mut(|data| data.insert_temp(support_texture_id(), texture.clone()));
    Some(texture)
}

fn support_panel_id() -> egui::Id {
    egui::Id::new("support_author_panel")
}

fn support_texture_id() -> egui::Id {
    egui::Id::new("support_author_texture")
}
