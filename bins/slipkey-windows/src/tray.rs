use tray_icon::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    #[allow(dead_code)]
    inner: TrayIcon,
    pub open_id: MenuId,
    pub quit_id: MenuId,
}

impl Tray {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        let icon = Icon::from_rgba(rgba, width, height).expect("tray icon RGBA");

        let open_item = MenuItem::new("Open Settings", true, None);
        let quit_item = MenuItem::new("Quit Slipkey", true, None);
        let open_id = open_item.id().clone();
        let quit_id = quit_item.id().clone();

        let menu = Menu::new();
        menu.append(&open_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        let inner = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("Slipkey - Switch input methods by typing")
            .build()
            .expect("tray icon build");

        Self {
            inner,
            open_id,
            quit_id,
        }
    }
}
