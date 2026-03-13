#![windows_subsystem = "windows"]

mod capture;
mod hotkey;
mod actions;
mod overlay;
mod render;
mod ui;
mod config;

use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tray_icon::{
    menu::{Menu, MenuItem, MenuId},
    TrayIconBuilder, Icon,
};

fn main() {
    config::init();
    actions::cleanup_old_copy_temp_files();
    let icon_data = vec![150u8; 32 * 32 * 4]; 
    let tray_icon = Icon::from_rgba(icon_data, 32, 32).expect("Failed to create tray icon");

    let tray_menu = Menu::new();
    let settings_item = MenuItem::with_id(
        MenuId::new(overlay::MENU_ID_SETTINGS),
        "Settings",
        true,
        None,
    );
    let quit_item = MenuItem::with_id(
        MenuId::new(overlay::MENU_ID_QUIT),
        "Exit Lightshot Clone",
        true,
        None,
    );
    let _ = tray_menu.append(&settings_item);
    let _ = tray_menu.append(&quit_item);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Lightshotv2 - Ready")
        .with_icon(tray_icon)
        .build()
        .unwrap();
    println!("DEBUG: Tray initialized: {:?}", tray.id());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_visible(true) // Must stay "visible" to keep the loop alive
            .with_inner_size([0.0, 0.0]) // But 0x0 size
            .with_taskbar(false) // No taskbar icon
            .with_transparent(true),
        follow_system_theme: false,
        default_theme: eframe::Theme::Dark,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Lightshot Clone",
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_nerdfonts::add_to_fonts(&mut fonts, egui_nerdfonts::Variant::Regular);
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            
            let trigger_flag = Arc::new(AtomicBool::new(false));
            let config_snapshot = config::cfg().clone();
            let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
            let hotkey_handle = hotkey::start_low_level_hotkey_loop(
                cc.egui_ctx.clone(),
                Arc::clone(&trigger_flag),
                config::hotkey_config(&config_snapshot),
                hotkey_tx,
            );
            
            Box::new(overlay::OverlayApp::new_background(
                trigger_flag,
                hotkey_handle,
                hotkey_rx,
            ))
        }),
    ) {
        eprintln!("Fatal Error: {}", e);
    }
}
