use eframe::egui;
use imeswitch_windows::{
    config::{save, Config},
    ime::detect_default_sources,
};

use super::{
    mapping_language_label, tr, FONT_BODY, FONT_CAPTION, WIN11_BORDER, WIN11_SURFACE, WIN11_TEXT,
    WIN11_TEXT_SEC,
};
use crate::app::AppState;
use crate::hook_thread::{HookCmd, HookHandle};

pub fn show(ui: &mut egui::Ui, state: &mut AppState, hook: &HookHandle) {
    let lang = state.ui_language.clone();
    super::preference_content(ui, |ui| {
        super::win11_card(ui, |ui| {
            super::section_title(ui, tr(&lang, "Leader key"));
            ui.horizontal(|ui| {
                leader_edit(ui, state);
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr(
                            &lang,
                            "Type this first, then a prefix such as ;en. Pick a rarely used key to avoid accidental triggers.",
                        ))
                        .size(FONT_CAPTION)
                        .color(WIN11_TEXT_SEC),
                    )
                    .wrap(),
                );
            });
        });

        ui.add_space(8.0);

        super::win11_card(ui, |ui| {
            super::section_title(ui, tr(&lang, "Input sources"));
            shortcut_table(ui, state, &lang);

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr(&lang, "Detect")).clicked() {
                    state.detected_sources = detect_default_sources();
                    state.config = crate::app::merge_detected_sources(
                        state.config.clone(),
                        &state.detected_sources,
                    );
                    state.status_message = String::new();
                }
                if ui.button(tr(&lang, "Reset to defaults")).clicked() {
                    state.config = crate::app::merge_detected_sources(
                        Config::default(),
                        &state.detected_sources,
                    );
                    state.config.ui_language = Some(state.ui_language.clone());
                    state.status_message =
                        tr(&lang, "Defaults restored. Click Save to apply.").to_string();
                }
                if ui.button(tr(&lang, "Save")).clicked() {
                    state.config.ui_language = Some(state.ui_language.clone());
                    if let Some(error) = validation_error(&state.config, &lang) {
                        state.status_message = error;
                    } else {
                        let result = save(&state.config).and_then(|()| {
                            hook.send(HookCmd::Restart(state.config.clone()))
                                .map_err(|error| anyhow::anyhow!("{error}"))
                        });
                        match result {
                            Ok(()) => {
                                state.status_message =
                                    tr(&lang, "Saved. Shortcuts are active now.").to_string();
                            }
                            Err(error) => {
                                state.status_message =
                                    format!("{}: {error}", tr(&lang, "Save failed"));
                            }
                        }
                    }
                }
            });

            if !state.status_message.is_empty() {
                ui.add_space(6.0);
                super::caption(ui, &state.status_message);
            }
        });
    });
}

fn leader_edit(ui: &mut egui::Ui, state: &mut AppState) {
    let mut leader = state
        .config
        .leader
        .clone()
        .unwrap_or_else(|| ";".to_string());

    egui::Frame::default()
        .fill(WIN11_SURFACE)
        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            let text_edit = ui.add(
                egui::TextEdit::singleline(&mut leader)
                    .desired_width(34.0)
                    .horizontal_align(egui::Align::Center)
                    .font(egui::TextStyle::Monospace)
                    .frame(false),
            );
            if text_edit.changed() {
                state.config.leader = Some(
                    leader
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| ";".to_string()),
                );
            }
        });
}

fn shortcut_table(ui: &mut egui::Ui, state: &mut AppState, lang: &str) {
    let mappings = state
        .config
        .mappings
        .get_or_insert_with(|| Config::default().mappings.unwrap_or_default());

    egui::Frame::default()
        .fill(super::WIN11_BG)
        .stroke(egui::Stroke::new(1.0, super::WIN11_BORDER))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());

            ui.horizontal(|ui| {
                table_header(ui, tr(lang, "LanguageHeader"), 112.0);
                table_header(ui, tr(lang, "Prefix"), 92.0);
            });
            ui.add_space(4.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().min.y,
                egui::Stroke::new(1.0, super::WIN11_SEPARATOR),
            );
            ui.add_space(4.0);

            for mapping in mappings.iter_mut() {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [112.0, 24.0],
                        egui::Label::new(
                            egui::RichText::new(mapping_language_label(&mapping.language))
                                .size(FONT_BODY)
                                .color(WIN11_TEXT),
                        ),
                    );

                    ui.add_sized(
                        [92.0, 24.0],
                        egui::TextEdit::singleline(&mut mapping.prefix)
                            .desired_width(72.0)
                            .horizontal_align(egui::Align::Center),
                    );
                });
                ui.add_space(2.0);
            }
        });
}

fn table_header(ui: &mut egui::Ui, label: &str, width: f32) {
    ui.add_sized(
        [width, 18.0],
        egui::Label::new(
            egui::RichText::new(label)
                .size(FONT_CAPTION)
                .strong()
                .color(WIN11_TEXT_SEC),
        ),
    );
}

fn validation_error(config: &Config, lang: &str) -> Option<String> {
    let mappings = config.mappings.as_ref()?;
    let prefixes = mappings
        .iter()
        .map(|mapping| {
            (
                mapping.language.as_str(),
                mapping.prefix.trim().to_ascii_lowercase(),
            )
        })
        .filter(|(_, prefix)| !prefix.is_empty())
        .collect::<Vec<_>>();

    if prefixes
        .iter()
        .any(|(_, prefix)| !prefix.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Some(tr(lang, "Prefixes can only contain letters and numbers.").to_string());
    }

    let mut seen = std::collections::HashSet::new();
    for (_, prefix) in &prefixes {
        if !seen.insert(prefix) {
            return Some(tr(lang, "Prefixes must be unique.").to_string());
        }
    }

    for (index, (_, prefix)) in prefixes.iter().enumerate() {
        for (_, other) in prefixes.iter().skip(index + 1) {
            if prefix != other && (prefix.starts_with(other) || other.starts_with(prefix)) {
                return Some(
                    tr(
                        lang,
                        "Prefixes cannot start with another configured prefix.",
                    )
                    .to_string(),
                );
            }
        }
    }

    None
}
