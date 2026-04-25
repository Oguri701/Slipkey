use imeswitch_core::Language;
use imeswitch_macos::config::{self, Config};
use imeswitch_macos::ime::{discover_installed_imes, Mapping, MappingEntry, DEFAULT_LEADER};
use imeswitch_macos::keymap::leader_keycode_for;
use serde::{Deserialize, Serialize};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{show_settings, AppState};

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDto {
    pub path: String,
    pub leader: String,
    pub mappings: Vec<MappingDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MappingDto {
    pub language: String,
    pub prefix: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedImeDto {
    pub language: String,
    pub source_id: String,
    pub name: String,
    pub is_selectable: bool,
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> ConfigDto {
    let mapping = state.mapping.lock().unwrap().clone();
    dto_from_mapping(&mapping)
}

#[tauri::command]
pub fn set_config(
    state: State<'_, AppState>,
    leader: Option<String>,
    mappings: Vec<MappingDto>,
) -> Result<ConfigDto, String> {
    let leader_char = parse_leader(leader.as_deref())?;
    let mapping = mapping_from_dto(leader_char, mappings)?;
    let config = Config::from_mapping(&mapping);
    let path = config::default_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = toml::to_string_pretty(&config).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    state
        .install_hook(mapping.clone())
        .map_err(|error| error.to_string())?;
    Ok(dto_from_mapping(&mapping))
}

#[tauri::command]
pub fn discover_imes() -> Vec<DetectedImeDto> {
    discover_installed_imes()
        .into_iter()
        .map(|item| DetectedImeDto {
            language: item.language.to_string(),
            source_id: item.source_id,
            name: item.name,
            is_selectable: item.is_selectable,
        })
        .collect()
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|error| error.to_string())
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_menubar_visible(state: State<'_, AppState>) -> bool {
    *state.tray_visible.lock().unwrap()
}

#[tauri::command]
pub fn set_menubar_visible(
    app: AppHandle,
    state: State<'_, AppState>,
    visible: bool,
) -> Result<(), String> {
    set_tray_visible(&app, &state, visible)
}

pub fn set_tray_visible(app: &AppHandle, state: &AppState, visible: bool) -> Result<(), String> {
    let mut tray = state.tray.lock().unwrap();
    if visible && tray.is_none() {
        let handle = app.clone();
        let built = TrayIconBuilder::new()
            .tooltip("Slipkey")
            .on_tray_icon_event(move |_tray, _event| {
                show_settings(&handle);
            })
            .build(app)
            .map_err(|error| error.to_string())?;
        *tray = Some(built);
    } else if !visible {
        *tray = None;
    }
    *state.tray_visible.lock().unwrap() = visible;
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings(&app);
}

fn dto_from_mapping(mapping: &Mapping) -> ConfigDto {
    ConfigDto {
        path: config::default_path().display().to_string(),
        leader: mapping.leader().to_string(),
        mappings: mapping
            .entries()
            .iter()
            .map(|entry| MappingDto {
                language: entry.language.to_string(),
                prefix: entry.prefix.clone(),
                source: entry.source.clone(),
            })
            .collect(),
    }
}

fn parse_leader(input: Option<&str>) -> Result<char, String> {
    let trimmed = input.map(str::trim).filter(|s| !s.is_empty());
    let Some(value) = trimmed else {
        return Ok(DEFAULT_LEADER);
    };
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Ok(DEFAULT_LEADER);
    };
    if chars.next().is_some() {
        return Err(format!("leader must be a single character, got '{value}'"));
    }
    if leader_keycode_for(first).is_none() {
        return Err(format!(
            "leader '{first}' has no unmodified key on a US-QWERTY layout"
        ));
    }
    Ok(first)
}

fn mapping_from_dto(leader: char, mappings: Vec<MappingDto>) -> Result<Mapping, String> {
    let mut entries = Vec::new();
    for mapping in mappings {
        if mapping.language.len() != 2 {
            return Err(format!("invalid language code '{}'", mapping.language));
        }
        if !mapping.prefix.is_empty() && !mapping.prefix.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(format!("invalid prefix '{}'", mapping.prefix));
        }
        if mapping.source.trim().is_empty() {
            return Err(format!("missing source for {}", mapping.language));
        }
        entries.push(MappingEntry {
            language: Language::from(mapping.language),
            prefix: mapping.prefix.to_ascii_lowercase(),
            source: mapping.source,
        });
    }

    Ok(Mapping::with_leader(leader, entries))
}
