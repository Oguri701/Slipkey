use std::sync::mpsc;

use eframe::egui;
use imeswitch_windows::{
    config::{save, Config},
    ime::{detect_default_sources, SourceInfo},
};

use crate::app::AppState;
use crate::hook_thread::HookCmd;

pub fn show(ui: &mut egui::Ui, state: &mut AppState, hook_tx: &mpsc::Sender<HookCmd>) {
    ui.horizontal(|ui| {
        ui.label("Leader key:");
        let mut leader = state
            .config
            .leader
            .clone()
            .unwrap_or_else(|| ";".to_string());
        let text_edit = ui.add(
            egui::TextEdit::singleline(&mut leader)
                .desired_width(32.0)
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
        ui.label(
            egui::RichText::new("Type this before a prefix like ;en")
                .small()
                .weak(),
        );
    });

    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.add_sized(
            [90.0, 16.0],
            egui::Label::new(egui::RichText::new("Language").small().strong()),
        );
        ui.add_sized(
            [58.0, 16.0],
            egui::Label::new(egui::RichText::new("Prefix").small().strong()),
        );
        ui.label(egui::RichText::new("Input source").small().strong());
    });
    ui.separator();

    let mappings = state
        .config
        .mappings
        .get_or_insert_with(|| Config::default().mappings.unwrap_or_default());

    for mapping in mappings.iter_mut() {
        ui.horizontal(|ui| {
            let lang_label = match mapping.language.as_str() {
                "en" => "English",
                "ja" => "Japanese",
                "zh" => "Chinese",
                other => other,
            };
            ui.add_sized([90.0, 20.0], egui::Label::new(lang_label));

            ui.add(egui::TextEdit::singleline(&mut mapping.prefix).desired_width(50.0));

            let current_label = source_display(&mapping.source, &state.detected_sources);
            egui::ComboBox::from_id_salt(&mapping.language)
                .width(200.0)
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for source in state
                        .detected_sources
                        .iter()
                        .filter(|source| source.language == mapping.language)
                    {
                        let label = format!("{} ({})", source.name, source.id);
                        ui.selectable_value(&mut mapping.source, source.id.clone(), label);
                    }
                    if !state
                        .detected_sources
                        .iter()
                        .any(|source| source.id == mapping.source)
                    {
                        let current_source = mapping.source.clone();
                        ui.selectable_value(
                            &mut mapping.source,
                            current_source.clone(),
                            current_source,
                        );
                    }
                });
        });
        ui.add_space(2.0);
    }

    ui.separator();
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if !state.status_message.is_empty() {
            ui.label(egui::RichText::new(&state.status_message).small().weak());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Save").clicked() {
                let result = save(&state.config).and_then(|()| {
                    hook_tx
                        .send(HookCmd::Restart(state.config.clone()))
                        .map_err(|error| anyhow::anyhow!("{error}"))
                });
                match result {
                    Ok(()) => {
                        state.status_message = "Saved. Shortcuts are active now.".to_string();
                    }
                    Err(error) => state.status_message = format!("Save failed: {error}"),
                }
            }
            if ui.button("Detect").clicked() {
                state.detected_sources = detect_default_sources();
                state.status_message = String::new();
            }
            if ui.button("Reset").clicked() {
                state.config = Config::default();
                state.status_message = "Defaults restored. Click Save to apply.".to_string();
            }
        });
    });
}

fn source_display(id: &str, sources: &[SourceInfo]) -> String {
    sources
        .iter()
        .find(|source| source.id == id)
        .map(|source| format!("{} ({})", source.name, source.id))
        .unwrap_or_else(|| id.to_string())
}
