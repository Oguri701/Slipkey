use std::sync::{
    atomic::{AtomicIsize, Ordering},
    Arc,
};

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{menu::MenuEvent, MouseButton, MouseButtonState, TrayIconEvent};

use crate::app::SharedState;
use crate::hook_thread::HookHandle;
use crate::tray::Tray;

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOWNORMAL},
};

pub mod about;
pub mod general;
pub mod shortcuts;

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

pub const FONT_CAPTION: f32 = 11.0;
pub const FONT_BODY: f32 = 12.5;
pub const FONT_BODY_STRONG: f32 = 13.0;
pub const FONT_SUBTITLE: f32 = 17.0;
pub const FONT_TITLE: f32 = 24.0;

const CONTENT_MARGIN: f32 = 14.0;

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
    /// Main window HWND cached on first `update`. Tray callbacks read this and
    /// call Win32 ShowWindow directly, bypassing eframe's update loop entirely
    /// (which would otherwise be stuck when the window is hidden).
    hwnd: Arc<AtomicIsize>,
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

        let hwnd = Arc::new(AtomicIsize::new(0));

        let tray_hwnd = hwnd.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray_hwnd.load(Ordering::SeqCst));
            }
        }));

        let menu_hwnd = hwnd.clone();
        let open_id = tray.open_id.clone();
        let quit_id = tray.quit_id.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == open_id {
                show_main_window(menu_hwnd.load(Ordering::SeqCst));
            } else if event.id == quit_id {
                std::process::exit(0);
            }
        }));

        Self {
            state,
            hook,
            tray,
            tab: Tab::General,
            icon_texture: Some(icon_texture),
            hwnd,
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Cache the HWND once eframe has actually created the window so tray
        // callbacks can act on it from any thread without going through the
        // update loop (which stalls while the window is hidden).
        if self.hwnd.load(Ordering::SeqCst) == 0 {
            if let Ok(handle) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = handle.as_raw() {
                    self.hwnd.store(w.hwnd.get() as isize, Ordering::SeqCst);
                }
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            // Keep the window alive (just hidden) so it can be re-shown later.
            // The close event itself triggers this update tick, so calling
            // Win32 ShowWindow here is safe and synchronous.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            hide_main_window(self.hwnd.load(Ordering::SeqCst));
        }

        let ui_language = {
            let state = self.state.lock().unwrap();
            state.ui_language.clone()
        };
        self.tray.set_language(&ui_language);

        egui::SidePanel::left("navigation_view")
            .exact_width(160.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(WIN11_NAV))
            .show(ctx, |ui| {
                navigation_view(ui, &mut self.tab, &ui_language);
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
                    about::show(ui, self.icon_texture.as_ref(), &ui_language);
                }
            });
    }
}

impl Tab {
    fn title(self, language: &str) -> &'static str {
        match self {
            Tab::General => tr(language, "General"),
            Tab::Shortcuts => tr(language, "Shortcuts"),
            Tab::About => tr(language, "About"),
        }
    }
}

fn navigation_view(ui: &mut egui::Ui, current_tab: &mut Tab, language: &str) {
    ui.set_width(ui.available_width());
    ui.add_space(16.0);

    for tab in [Tab::General, Tab::Shortcuts, Tab::About] {
        nav_item(ui, current_tab, tab, language);
        ui.add_space(4.0);
    }

    let r = ui.max_rect();
    ui.painter().vline(
        r.max.x,
        r.min.y..=r.max.y,
        egui::Stroke::new(1.0, WIN11_SEPARATOR),
    );
}

fn nav_item(ui: &mut egui::Ui, current_tab: &mut Tab, tab: Tab, language: &str) {
    const NAV_H: f32 = 36.0;
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
    nav_icon(ui, tab, rect, text_color);
    ui.painter().text(
        egui::pos2(rect.min.x + 34.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tab.title(language),
        egui::FontId::proportional(FONT_BODY),
        text_color,
    );
}

fn nav_icon(ui: &mut egui::Ui, tab: Tab, rect: egui::Rect, color: egui::Color32) {
    let center = egui::pos2(rect.min.x + 15.0, rect.center().y);
    match tab {
        Tab::General => {
            ui.painter().circle_filled(center, 3.5, color);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let dir = egui::vec2(angle.cos(), angle.sin());
                ui.painter().line_segment(
                    [center + dir * 6.5, center + dir * 9.0],
                    egui::Stroke::new(1.25, color),
                );
            }
        }
        Tab::Shortcuts => {
            let body = egui::Rect::from_center_size(center, egui::vec2(18.0, 11.0));
            ui.painter().rect_stroke(
                body,
                2.0,
                egui::Stroke::new(1.25, color),
                egui::epaint::StrokeKind::Inside,
            );
            for row in 0..2 {
                for col in 0..4 {
                    let x = body.min.x + 4.0 + col as f32 * 4.0;
                    let y = body.min.y + 4.0 + row as f32 * 4.0;
                    ui.painter().circle_filled(egui::pos2(x, y), 0.8, color);
                }
            }
        }
        Tab::About => {
            ui.painter()
                .circle_stroke(center, 7.5, egui::Stroke::new(1.25, color));
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(12.0),
                color,
            );
        }
    }
}

pub fn win11_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(WIN11_SURFACE)
        .stroke(egui::Stroke::new(1.0, WIN11_BORDER))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            add_contents(ui);
        });
}

pub fn preference_content(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(CONTENT_MARGIN);

            egui::Frame::default()
                .inner_margin(egui::Margin::symmetric(CONTENT_MARGIN as i8, 0))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.set_max_width(ui.available_width());
                        add_contents(ui);
                    });
                });

            ui.add_space(CONTENT_MARGIN);
        });
}

pub fn section_title(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(FONT_SUBTITLE)
            .color(WIN11_TEXT),
    );
    ui.add_space(6.0);
}

pub fn caption(ui: &mut egui::Ui, label: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(label)
                .size(FONT_CAPTION)
                .color(WIN11_TEXT_SEC),
        )
        .wrap(),
    );
}

pub fn tr(language: &str, key: &'static str) -> &'static str {
    match language {
        "zh" => match key {
            "General" => "通用",
            "Shortcuts" => "快捷键",
            "About" => "关于",
            "Startup" => "启动",
            "Open at startup" => "开机启动",
            "Start Slipkey automatically after sign-in." => "登录后自动启动 Slipkey。",
            "Startup error" => "启动项错误",
            "Language" => "语言",
            "Display language" => "显示语言",
            "Permissions" => "权限",
            "Accessibility" => "辅助功能",
            "Ready" => "已就绪",
            "Inactive" => "未启用",
            "Required to intercept the leader key before the active IME consumes it." => "用于在当前输入法处理前拦截引导键。",
            "Leader key" => "引导键",
            "Type this first, then a prefix such as ;en. Pick a rarely used key to avoid accidental triggers." => "先输入它，再输入如 ;en 的前缀。建议选择少用键以避免误触发。",
            "Input sources" => "输入源",
            "LanguageHeader" => "语言",
            "Prefix" => "前缀",
            "Detect" => "检测",
            "Reset to defaults" => "恢复默认",
            "Save" => "保存",
            "Defaults restored. Click Save to apply." => "已恢复默认值。点击保存后生效。",
            "Saved. Shortcuts are active now." => "已保存。快捷键已生效。",
            "Save failed" => "保存失败",
            "Prefixes can only contain letters and numbers." => "前缀只能包含字母和数字。",
            "Prefixes must be unique." => "前缀必须唯一。",
            "Prefixes cannot start with another configured prefix." => "前缀不能以另一个已配置的前缀开头。",
            "Switch input methods by typing." => "通过键入来切换输入法。",
            "Slipkey - Switch input methods by typing" => "Slipkey - 通过键入来切换输入法",
            "Open Settings" => "打开设置",
            "Quit Slipkey" => "退出 Slipkey",
            "Version" => "版本",
            "View on GitHub" => "在 GitHub 上查看",
            _ => key,
        },
        "ja" => match key {
            "General" => "一般",
            "Shortcuts" => "ショートカット",
            "About" => "情報",
            "Startup" => "起動",
            "Open at startup" => "起動時に開く",
            "Start Slipkey automatically after sign-in." => "サインイン後に Slipkey を自動起動します。",
            "Startup error" => "起動項目エラー",
            "Language" => "言語",
            "Display language" => "表示言語",
            "Permissions" => "権限",
            "Accessibility" => "アクセシビリティ",
            "Ready" => "準備完了",
            "Inactive" => "無効",
            "Required to intercept the leader key before the active IME consumes it." => "現在の IME が処理する前にリーダーキーを捕捉するために必要です。",
            "Leader key" => "リーダーキー",
            "Type this first, then a prefix such as ;en. Pick a rarely used key to avoid accidental triggers." => "先にこれを入力し、次に ;en のようなプレフィックスを入力します。",
            "Input sources" => "入力ソース",
            "LanguageHeader" => "言語",
            "Prefix" => "プレフィックス",
            "Detect" => "検出",
            "Reset to defaults" => "既定値に戻す",
            "Save" => "保存",
            "Defaults restored. Click Save to apply." => "既定値に戻しました。保存すると適用されます。",
            "Saved. Shortcuts are active now." => "保存しました。ショートカットは有効です。",
            "Save failed" => "保存に失敗しました",
            "Prefixes can only contain letters and numbers." => "プレフィックスに使えるのは英数字だけです。",
            "Prefixes must be unique." => "プレフィックスは重複できません。",
            "Prefixes cannot start with another configured prefix." => "プレフィックスは他の設定済みプレフィックスで始められません。",
            "Switch input methods by typing." => "入力して入力方式を切り替えます。",
            "Slipkey - Switch input methods by typing" => "Slipkey - 入力して入力方式を切り替え",
            "Open Settings" => "設定を開く",
            "Quit Slipkey" => "Slipkey を終了",
            "Version" => "バージョン",
            "View on GitHub" => "GitHub で見る",
            _ => key,
        },
        _ => key,
    }
}

pub fn language_label(language: &str) -> &'static str {
    match language {
        "zh" => "中文",
        "ja" => "日本語",
        _ => "English",
    }
}

pub fn mapping_language_label(language: &str) -> String {
    match language {
        "zh" => "中文".to_string(),
        "ja" => "日本語".to_string(),
        "en" => "English".to_string(),
        other => other.to_string(),
    }
}

/// Show the settings window from any thread.
///
/// `SW_SHOWNORMAL` activates the window and also restores it if minimized,
/// so the same call covers "hidden -> visible" and "minimized -> normal".
/// `SetForegroundWindow` is allowed here because the call originates from a
/// user click on the tray icon / menu, which Windows treats as user-initiated
/// foreground activation (no anti-stealing block).
#[cfg(target_os = "windows")]
fn show_main_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    let hwnd = hwnd as HWND;
    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
    }
}

#[cfg(target_os = "windows")]
fn hide_main_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}

#[cfg(not(target_os = "windows"))]
fn show_main_window(_hwnd: isize) {}

#[cfg(not(target_os = "windows"))]
fn hide_main_window(_hwnd: isize) {}

fn apply_win11_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    register_font(
        &mut fonts,
        "segoe_ui",
        &[
            "C:\\Windows\\Fonts\\SegoeUIVariable.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
        ],
        true,
    );
    register_font(
        &mut fonts,
        "microsoft_yahei",
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
        ],
        false,
    );
    register_font(
        &mut fonts,
        "yu_gothic",
        &[
            "C:\\Windows\\Fonts\\YuGothM.ttc",
            "C:\\Windows\\Fonts\\YuGothR.ttc",
        ],
        false,
    );
    register_font(
        &mut fonts,
        "meiryo",
        &["C:\\Windows\\Fonts\\meiryo.ttc"],
        false,
    );
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

    use egui::TextStyle;
    style
        .text_styles
        .insert(TextStyle::Body, egui::FontId::proportional(FONT_BODY));
    style
        .text_styles
        .insert(TextStyle::Button, egui::FontId::proportional(FONT_BODY));
    style
        .text_styles
        .insert(TextStyle::Small, egui::FontId::proportional(FONT_CAPTION));

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(4);

    ctx.set_style(style);
}

fn register_font(fonts: &mut egui::FontDefinitions, name: &str, paths: &[&str], primary: bool) {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), egui::FontData::from_owned(bytes).into());

            let proportional = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if primary {
                proportional.insert(0, name.to_owned());
            } else if !proportional.iter().any(|font| font == name) {
                proportional.push(name.to_owned());
            }

            let monospace = fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default();
            if !monospace.iter().any(|font| font == name) {
                monospace.push(name.to_owned());
            }
            break;
        }
    }
}
