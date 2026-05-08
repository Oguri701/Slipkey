use std::sync::{Arc, Mutex};

use eframe::egui;
use tray_icon::{menu::MenuEvent, TrayIconEvent};

use crate::app::SharedState;
use crate::hook_thread::HookHandle;
use crate::tray::Tray;

pub mod about;
pub mod general;
pub mod shortcuts;

// ---- Win11 palette ---------------------------------------------------------

pub const WIN11_BG: egui::Color32 = egui::Color32::from_rgb(243, 243, 243);
pub const WIN11_SURFACE: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
pub const WIN11_NAV: egui::Color32 = egui::Color32::from_rgb(242, 242, 242);
pub const WIN11_NAV_SELECTED: egui::Color32 = egui::Color32::from_rgb(213, 226, 242);
pub const WIN11_NAV_HOVER: egui::Color32 = egui::Color32::from_rgb(236, 236, 236);
pub const WIN11_ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 120, 212);
pub const WIN11_TEXT: egui::Color32 = egui::Color32::from_rgb(26, 26, 26);
pub const WIN11_TEXT_SEC: egui::Color32 = egui::Color32::from_rgb(96, 94, 92);
pub const WIN11_BORDER: egui::Color32 = egui::Color32::from_rgb(218, 218, 218);
pub const WIN11_SEPARATOR: egui::Color32 = egui::Color32::from_rgb(224, 224, 224);
pub const WIN11_SUCCESS: egui::Color32 = egui::Color32::from_rgb(15, 123, 15);
pub const WIN11_WARNING: egui::Color32 = egui::Color32::from_rgb(202, 80, 16);
pub const WIN11_BTN: egui::Color32 = egui::Color32::from_rgb(251, 251, 251);
pub const WIN11_BTN_HOVER: egui::Color32 = egui::Color32::from_rgb(239, 239, 239);
pub const WIN11_BTN_ACTIVE: egui::Color32 = egui::Color32::from_rgb(225, 225, 225);
pub const WIN11_CLOSE_HOVER: egui::Color32 = egui::Color32::from_rgb(196, 43, 28);

// ---- Win11 type ramp -------------------------------------------------------
//
// Mirrors the Fluent / Win11 Settings type scale so every label in the app
// lands on one of these five sizes. Anything that needs a different size is a
// signal that the layout, not the type, should be reworked.

/// Caption — secondary metadata, table headers, supporting text.
pub const FONT_CAPTION: f32 = 12.0;
/// Body — default UI text, control labels, table rows.
pub const FONT_BODY: f32 = 14.0;
/// BodyStrong — emphasized body (toggle titles, current section).
pub const FONT_BODY_STRONG: f32 = 14.0;
/// Subtitle — section titles inside a card.
pub const FONT_SUBTITLE: f32 = 20.0;
/// Title — page-level headlines (About).
pub const FONT_TITLE: f32 = 28.0;

const TITLE_BAR_HEIGHT: f32 = 40.0;
const TITLE_BTN_W: f32 = 44.0;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    General,
    Shortcuts,
    About,
}

pub struct SettingsWindow {
    state: SharedState,
    hook: HookHandle,
    tray: Tray,
    tab: Tab,
    icon_texture: Option<egui::TextureHandle>,
    visible: bool,
    /// Tray icon clicks queued by the global event handler. Drained on each
    /// `update`. The handler also calls `ctx.request_repaint()` so we do not
    /// need to poll for these events when the window is hidden.
    tray_events: Arc<Mutex<Vec<TrayIconEvent>>>,
    menu_events: Arc<Mutex<Vec<MenuEvent>>>,
}

impl SettingsWindow {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: SharedState,
        hook: HookHandle,
        tray: Tray,
        icon_rgba: &[u8],
        icon_w: u32,
        icon_h: u32,
    ) -> Self {
        apply_win11_style(&cc.egui_ctx);
        let icon_texture = cc.egui_ctx.load_texture(
            "app_icon",
            egui::ColorImage::from_rgba_unmultiplied([icon_w as usize, icon_h as usize], icon_rgba),
            egui::TextureOptions::default(),
        );

        // Take over the tray-icon and menu event handlers so we can wake egui
        // exactly when an event arrives, instead of polling every frame.
        let tray_events: Arc<Mutex<Vec<TrayIconEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let menu_events: Arc<Mutex<Vec<MenuEvent>>> = Arc::new(Mutex::new(Vec::new()));

        let tray_sink = tray_events.clone();
        let ctx_for_tray = cc.egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            tray_sink.lock().unwrap().push(event);
            ctx_for_tray.request_repaint();
        }));

        let menu_sink = menu_events.clone();
        let ctx_for_menu = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            menu_sink.lock().unwrap().push(event);
            ctx_for_menu.request_repaint();
        }));

        Self {
            state,
            hook,
            tray,
            tab: Tab::General,
            icon_texture: Some(icon_texture),
            visible: false,
            tray_events,
            menu_events,
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.visible = false;
        }

        let tray_events: Vec<_> = std::mem::take(&mut *self.tray_events.lock().unwrap());
        for event in tray_events {
            if let TrayIconEvent::Click { .. } = event {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.visible = true;
            }
        }

        let menu_events: Vec<_> = std::mem::take(&mut *self.menu_events.lock().unwrap());
        for event in menu_events {
            if event.id == self.tray.open_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.visible = true;
            } else if event.id == self.tray.quit_id {
                std::process::exit(0);
            }
        }

        // No periodic repaint: the tray/menu handlers above call
        // `request_repaint` exactly when they have work for us. When the
        // window is hidden egui parks the main thread until the OS sends
        // input or one of those handlers fires.

        egui::TopBottomPanel::top("title_bar")
            .exact_height(TITLE_BAR_HEIGHT)
            .frame(egui::Frame::new().fill(WIN11_BG))
            .show(ctx, |ui| {
                title_bar(ui, ctx, &mut self.visible);
            });

        egui::SidePanel::left("navigation_view")
            .exact_width(230.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(WIN11_NAV))
            .show(ctx, |ui| {
                navigation_view(ui, &mut self.tab);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(WIN11_BG))
            .show(ctx, |ui| match self.tab {
                Tab::General => {
                    let mut state = self.state.lock().unwrap();
                    general::show(ui, &mut state);
                }
                Tab::Shortcuts => {
                    let mut state = self.state.lock().unwrap();
                    shortcuts::show(ui, &mut state, &self.hook);
                }
                Tab::About => {
                    about::show(ui, self.icon_texture.as_ref());
                }
            });
    }
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Shortcuts => "Shortcuts",
            Tab::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::General => "☀",
            Tab::Shortcuts => "⌨",
            Tab::About => "ⓘ",
        }
    }
}

fn title_bar(ui: &mut egui::Ui, ctx: &egui::Context, visible: &mut bool) {
    let full = ui.max_rect();
    // Drag region stops at the start of the window controls (min + close).
    let controls_w = TITLE_BTN_W * 2.0;
    let drag_rect = egui::Rect::from_min_max(
        full.min,
        egui::pos2(full.max.x - controls_w, full.max.y),
    );
    let drag_response = ui.interact(drag_rect, ui.id().with("title_drag"), egui::Sense::drag());
    if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.allocate_ui_with_layout(
            egui::vec2(full.width() - controls_w - 16.0, TITLE_BAR_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new("Slipkey")
                        .size(FONT_BODY_STRONG)
                        .strong()
                        .color(WIN11_TEXT),
                );
            },
        );

        title_button(ui, "\u{2013}", TitleHover::neutral(), || {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        });
        title_button(ui, "\u{2715}", TitleHover::close(), || {
            *visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        });
    });

    ui.painter().hline(
        full.min.x..=full.max.x,
        full.max.y,
        egui::Stroke::new(1.0, WIN11_SEPARATOR),
    );
}

#[derive(Clone, Copy)]
struct TitleHover {
    bg: egui::Color32,
    fg: egui::Color32,
}

impl TitleHover {
    fn neutral() -> Self {
        Self { bg: WIN11_BTN_HOVER, fg: WIN11_TEXT }
    }
    fn close() -> Self {
        Self { bg: WIN11_CLOSE_HOVER, fg: egui::Color32::WHITE }
    }
}

fn title_button(ui: &mut egui::Ui, glyph: &str, hover: TitleHover, action: impl FnOnce()) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(TITLE_BTN_W, TITLE_BAR_HEIGHT), egui::Sense::click());

    let (bg, fg) = if response.hovered() {
        (hover.bg, hover.fg)
    } else {
        (egui::Color32::TRANSPARENT, WIN11_TEXT_SEC)
    };

    ui.painter().rect_filled(rect, 0.0, bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(12.0),
        fg,
    );

    if response.clicked() {
        action();
    }
}

fn navigation_view(ui: &mut egui::Ui, current_tab: &mut Tab) {
    ui.set_width(ui.available_width());
    ui.add_space(20.0);

    for tab in [Tab::General, Tab::Shortcuts, Tab::About] {
        nav_item(ui, current_tab, tab);
        ui.add_space(4.0);
    }

    let r = ui.max_rect();
    ui.painter().vline(
        r.max.x,
        r.min.y..=r.max.y,
        egui::Stroke::new(1.0, WIN11_SEPARATOR),
    );
}

fn nav_item(ui: &mut egui::Ui, current_tab: &mut Tab, tab: Tab) {
    const NAV_H: f32 = 40.0;
    const NAV_MARGIN_X: f32 = 8.0;
    const RAIL_W: f32 = 3.0;

    let width = ui.available_width() - NAV_MARGIN_X * 2.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, NAV_H), egui::Sense::click());
    let rect = rect.translate(egui::vec2(NAV_MARGIN_X, 0.0));

    if response.clicked() {
        *current_tab = tab;
    }

    let is_active = *current_tab == tab;
    let fill = if is_active {
        WIN11_NAV_SELECTED
    } else if response.hovered() {
        WIN11_NAV_HOVER
    } else {
        egui::Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 6.0, fill);

    if is_active {
        let rail = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y + 10.0),
            egui::pos2(rect.min.x + RAIL_W, rect.max.y - 10.0),
        );
        ui.painter().rect_filled(rail, 1.5, WIN11_ACCENT);
    }

    let text_color = if is_active { WIN11_ACCENT } else { WIN11_TEXT };
    ui.painter().text(
        egui::pos2(rect.min.x + 16.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        tab.icon(),
        egui::FontId::proportional(16.0),
        text_color,
    );
    ui.painter().text(
        egui::pos2(rect.min.x + 36.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tab.title(),
        egui::FontId::proportional(FONT_BODY),
        text_color,
    );
}

pub fn win11_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(WIN11_SURFACE)
        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

pub fn preference_content(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 40.0);
                add_contents(ui);
            });
            ui.add_space(20.0);
        });
        ui.add_space(20.0);
    });
}

pub fn section_title(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(FONT_SUBTITLE)
            .color(WIN11_TEXT),
    );
    ui.add_space(8.0);
}

pub fn caption(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(FONT_CAPTION)
            .color(WIN11_TEXT_SEC),
    );
}

fn apply_win11_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in &[
        "C:\\Windows\\Fonts\\SegoeUIVariable.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "segoe_ui".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();

    style.visuals.window_fill = WIN11_SURFACE;
    style.visuals.panel_fill = WIN11_BG;
    style.visuals.window_stroke = egui::Stroke::NONE;
    style.visuals.override_text_color = Some(WIN11_TEXT);
    style.visuals.selection.bg_fill = WIN11_ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    style.visuals.hyperlink_color = WIN11_ACCENT;

    style.visuals.widgets.noninteractive.bg_fill = WIN11_SURFACE;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, WIN11_BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, WIN11_TEXT);

    style.visuals.widgets.inactive.bg_fill = WIN11_BTN;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, WIN11_BORDER);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, WIN11_TEXT);

    style.visuals.widgets.hovered.bg_fill = WIN11_BTN_HOVER;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, WIN11_ACCENT);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, WIN11_TEXT);

    style.visuals.widgets.active.bg_fill = WIN11_BTN_ACTIVE;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, WIN11_ACCENT);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, WIN11_TEXT);

    style.visuals.widgets.open.bg_fill = WIN11_BTN_HOVER;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, WIN11_ACCENT);
    style.visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, WIN11_TEXT);

    // Default text style: body 14pt. Sections / captions opt in via RichText.
    use egui::TextStyle;
    style.text_styles.insert(
        TextStyle::Body,
        egui::FontId::proportional(FONT_BODY),
    );
    style.text_styles.insert(
        TextStyle::Button,
        egui::FontId::proportional(FONT_BODY),
    );
    style.text_styles.insert(
        TextStyle::Small,
        egui::FontId::proportional(FONT_CAPTION),
    );

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(4);

    ctx.set_style(style);
}
