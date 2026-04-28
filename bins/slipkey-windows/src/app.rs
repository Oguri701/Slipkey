use std::sync::{Arc, Mutex};

use imeswitch_windows::config::{load_or_default, Config};
use imeswitch_windows::ime::{detect_default_sources, SourceInfo};

pub struct AppState {
    pub config: Config,
    pub detected_sources: Vec<SourceInfo>,
    pub status_message: String,
    pub hook_active: bool,
    pub launch_at_login: bool,
}

impl AppState {
    pub fn load() -> Self {
        let (mapping, _outcome) = load_or_default();
        AppState {
            config: Config::from_mapping(&mapping),
            detected_sources: detect_default_sources(),
            status_message: String::new(),
            hook_active: false,
            launch_at_login: crate::startup::is_enabled(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
