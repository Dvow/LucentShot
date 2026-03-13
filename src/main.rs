#![windows_subsystem = "windows"]

mod capture;
mod hotkey;
mod notification;
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

fn load_tray_icon() -> Icon {
    let png_data = include_bytes!("icon/icon.png");
    let img = image::load_from_memory(png_data).expect("Failed to load tray icon");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    // Resize to 32x32 for tray if larger
    let (w, h, data) = if w > 32 || h > 32 {
        let scaled = image::imageops::resize(
            &rgba,
            32,
            32,
            image::imageops::FilterType::Lanczos3,
        );
        let (w, h) = scaled.dimensions();
        (w, h, scaled.into_raw())
    } else {
        (w, h, rgba.into_raw())
    };
    Icon::from_rgba(data, w, h).expect("Failed to create tray icon")
}

fn main() {
    config::init();
    actions::cleanup_old_copy_temp_files();
    actions::warm_ocr_engine(); // Background: extract tessdata, init Tesseract — first OCR will be faster
    let tray_icon = load_tray_icon();

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

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Lightshot Clone - Left-click to screenshot")
        .with_icon(tray_icon)
        .build()
        .unwrap();
    let icon_data = eframe::icon_data::from_png_bytes(include_bytes!("icon/icon.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_visible(true)
        .with_inner_size([0.0, 0.0])
        .with_taskbar(false)
        .with_transparent(true);
    if let Some(ref icon) = icon_data {
        viewport = viewport.with_icon(Arc::new(icon.clone()));
    }
    let options = eframe::NativeOptions {
        viewport,
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
