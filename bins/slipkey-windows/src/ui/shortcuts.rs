use eframe::egui;
use imeswitch_windows::{
    config::{save, Config},
    ime::{detect_default_sources, SourceInfo},
};

use super::{FONT_BODY, FONT_CAPTION, WIN11_TEXT, WIN11_TEXT_SEC};
use crate::app::AppState;
use crate::hook_thread::{HookCmd, HookHandle};

pub fn show(ui: &mut egui::Ui, state: &mut AppState, hook: &HookHandle) {
    super::preference_content(ui, |ui| {
        super::win11_card(ui, |ui| {
            super::section_title(ui, "Leader key");
            ui.horizontal(|ui| {
                let mut leader = state
                    .config
                    .leader
                    .clone()
                    .unwrap_or_else(|| ";".to_string());
                let text_edit = ui.add(
                    egui::TextEdit::singleline(&mut leader)
                        .desired_width(36.0)
                        .font(egui::TextStyle::Monospace),
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
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Type this first, then a prefix such as ;en.")
                        .size(FONT_CAPTION)
                        .color(WIN11_TEXT_SEC),
                );
            });
        });

        ui.add_space(12.0);

        super::win11_card(ui, |ui| {
            super::section_title(ui, "Input sources");
            shortcut_table(ui, state);

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Detect").clicked() {
                    state.detected_sources = detect_default_sources();
                    state.config = crate::app::merge_detected_sources(
                        state.config.clone(),
                        &state.detected_sources,
                    );
                    state.status_message = String::new();
                }
                if ui.button("Reset to defaults").clicked() {
                    state.config = crate::app::merge_detected_sources(
                        Config::default(),
                        &state.detected_sources,
                    );
                    state.status_message =
                        "Defaults restored. Click Save to apply.".to_string();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Save").clicked() {
                        if let Some(error) = validation_error(&state.config) {
                            state.status_message = error;
                        } else {
                            let result = save(&state.config).and_then(|()| {
                                hook.send(HookCmd::Restart(state.config.clone()))
                                    .map_err(|error| anyhow::anyhow!("{error}"))
                            });
                            match result {
                                Ok(()) => {
                                    state.status_message =
                                        "Saved. Shortcuts are active now.".to_string();
                                }
                                Err(error) => {
                                    state.status_message = format!("Save failed: {error}");
                                }
                            }
                        }
                    }
                });
            });

            if !state.status_message.is_empty() {
                ui.add_space(6.0);
                super::caption(ui, &state.status_message);
            }
        });
    });
}

fn shortcut_table(ui: &mut egui::Ui, state: &mut AppState) {
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
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                table_header(ui, "Language", 84.0);
                table_header(ui, "Prefix", 60.0);
                table_header(ui, "Source", ui.available_width());
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
                    let lang_label = match mapping.language.as_str() {
                        "en" => "English",
                        "ja" => "Japanese",
                        "zh" => "Chinese",
                        other => other,
                    };
                    ui.add_sized(
                        [84.0, 24.0],
                        egui::Label::new(
                            egui::RichText::new(lang_label)
                                .size(FONT_BODY)
                                .color(WIN11_TEXT),
                        ),
                    );

                    ui.add_sized(
                        [60.0, 24.0],
                        egui::TextEdit::singleline(&mut mapping.prefix).desired_width(54.0),
                    );

                    if mapping.language == "en" {
                        // English uses alphanumeric mode of the active CJK IME.
                        // No keyboard layout is selected — show a quiet caption.
                        ui.label(
                            egui::RichText::new("alphanumeric mode")
                                .size(FONT_CAPTION)
                                .italics()
                                .color(WIN11_TEXT_SEC),
                        );
                    } else {
                        let mut source_id = mapping.source.clone().unwrap_or_default();
                        let before = source_id.clone();
                        let current_label = source_display(&source_id, &state.detected_sources);

                        egui::ComboBox::from_id_salt(&mapping.language)
                            .width(ui.available_width().min(160.0))
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                for source in state
                                    .detected_sources
                                    .iter()
                                    .filter(|s| s.language == mapping.language)
                                {
                                    let label = format!("{} ({})", source.name, source.id);
                                    ui.selectable_value(&mut source_id, source.id.clone(), label);
                                }
                                if !source_id.is_empty()
                                    && !state.detected_sources.iter().any(|s| s.id == source_id)
                                {
                                    let id = source_id.clone();
                                    ui.selectable_value(&mut source_id, id.clone(), id);
                                }
                            });

                        if source_id != before {
                            mapping.source = if source_id.is_empty() {
                                None
                            } else {
                                Some(source_id)
                            };
                        }
                    }
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

fn source_display(id: &str, sources: &[SourceInfo]) -> String {
    if id.is_empty() {
        return String::new();
    }
    sources
        .iter()
        .find(|source| source.id == id)
        .map(|source| format!("{} ({})", source.name, source.id))
        .unwrap_or_else(|| id.to_string())
}

fn validation_error(config: &Config) -> Option<String> {
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
        return Some("Prefixes can only contain letters and numbers.".to_string());
    }

    let mut seen = std::collections::HashSet::new();
    for (_, prefix) in &prefixes {
        if !seen.insert(prefix) {
            return Some("Prefixes must be unique.".to_string());
        }
    }

    for (index, (_, prefix)) in prefixes.iter().enumerate() {
        for (_, other) in prefixes.iter().skip(index + 1) {
            if prefix != other && (prefix.starts_with(other) || other.starts_with(prefix)) {
                return Some("Prefixes cannot start with another configured prefix.".to_string());
            }
        }
    }

    None
}

