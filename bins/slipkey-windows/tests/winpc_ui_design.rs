//! Pin the Settings window's high-level visual contract: a left NavigationView
//! sidebar, native WinPC window chrome, and the Win11 Fluent type ramp. Tests
//! guard against silent regressions like the old top tab bar or hard-coded
//! one-off font sizes creeping back in.

use std::fs;
use std::path::Path;

fn read_source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .expect("source file should be readable")
}

#[test]
fn settings_window_uses_sample_navigation_layout() {
    let main_rs = read_source("src/main.rs");
    let ui_rs = read_source("src/ui/mod.rs");

    assert!(
        main_rs.contains(".with_inner_size([480.0, 380.0])"),
        "WinPC settings window should keep compact settings-panel proportions"
    );
    assert!(
        main_rs.contains(".with_decorations(true)"),
        "WinPC settings should use native window chrome for rounded corners and frame"
    );
    assert!(
        ui_rs.contains("SidePanel::left(\"navigation_view\")"),
        "WinPC settings should use a left NavigationView-style sidebar"
    );
    assert!(
        ui_rs.contains(".exact_width(160.0)"),
        "WinPC settings should keep the navigation rail narrow enough for a compact panel"
    );
    assert!(
        !ui_rs.contains("TopBottomPanel::top(\"tab_bar\")"),
        "WinPC settings should not keep the old top tab bar"
    );
}

#[test]
fn settings_window_exposes_full_type_ramp() {
    let ui_rs = read_source("src/ui/mod.rs");

    for token in [
        "FONT_CAPTION",
        "FONT_BODY",
        "FONT_BODY_STRONG",
        "FONT_SUBTITLE",
        "FONT_TITLE",
    ] {
        assert!(
            ui_rs.contains(token),
            "ui/mod.rs should publish the `{token}` constant so other panels can stay on the ramp"
        );
    }
}

#[test]
fn general_tab_does_not_carry_dead_controls() {
    let general_rs = read_source("src/ui/general.rs");

    assert!(
        !general_rs.contains("Show menu bar"),
        "the no-op `Show menu bar icon` toggle should stay removed"
    );
    assert!(
        general_rs.contains("general_language"),
        "the Language picker should stay present and backed by real UI state"
    );
}

#[test]
fn settings_window_uses_native_title_bar() {
    let ui_rs = read_source("src/ui/mod.rs");

    assert!(
        !ui_rs.contains("ViewportCommand::Maximized"),
        "the Settings window is fixed-size, so the maximize button should stay removed"
    );
    assert!(
        !ui_rs.contains("TopBottomPanel::top(\"title_bar\")"),
        "WinPC settings should not duplicate the native title bar"
    );
}

#[test]
fn shortcuts_table_keeps_sources_internal() {
    let shortcuts_rs = read_source("src/ui/shortcuts.rs");

    assert!(
        !shortcuts_rs.contains("table_header(ui, \"Source\""),
        "Shortcuts should not expose a Source column in the compact WinPC table"
    );
    assert!(
        shortcuts_rs.contains("mapping_language_label"),
        "Shortcuts should mirror the macOS language labels through the shared label helper"
    );
}
