use tray_icon::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    inner: TrayIcon,
    open_item: MenuItem,
    quit_item: MenuItem,
    pub open_id: MenuId,
    pub quit_id: MenuId,
    language: String,
}

impl Tray {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32, language: &str) -> Self {
        let icon = Icon::from_rgba(rgba, width, height).expect("tray icon RGBA");

        let open_item = MenuItem::new(crate::ui::tr(language, "Open Settings"), true, None);
        let quit_item = MenuItem::new(crate::ui::tr(language, "Quit Slipkey"), true, None);
        let open_id = open_item.id().clone();
        let quit_id = quit_item.id().clone();

        let menu = Menu::new();
        menu.append(&open_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        let inner = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip(crate::ui::tr(
                language,
                "Slipkey - Switch input methods by typing",
            ))
            .build()
            .expect("tray icon build");

        Self {
            inner,
            open_item,
            quit_item,
            open_id,
            quit_id,
            language: language.to_string(),
        }
    }

    pub fn set_language(&mut self, language: &str) {
        if self.language == language {
            return;
        }
        self.open_item
            .set_text(crate::ui::tr(language, "Open Settings"));
        self.quit_item
            .set_text(crate::ui::tr(language, "Quit Slipkey"));
        let _ = self.inner.set_tooltip(Some(crate::ui::tr(
            language,
            "Slipkey - Switch input methods by typing",
        )));
        self.language = language.to_string();
    }
}
