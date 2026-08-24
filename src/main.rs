#![cfg(windows)]
#![windows_subsystem = "windows"]

mod actions;
mod capture;
mod config;
mod draw;
mod hotkey;
mod icons;
mod notification;
mod ocr;
mod overlay;
mod paths;
mod settings;
#[cfg(feature = "ocr")]
mod tesseract;

use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuId, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

fn load_tray_icon() -> Icon {
    let img = image::load_from_memory(include_bytes!("../assets/icons/icon.png"))
        .expect("Failed to load tray icon");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let (w, h, data) = if w > 32 || h > 32 {
        let scaled = image::imageops::resize(&rgba, 32, 32, image::imageops::FilterType::Lanczos3);
        let (w, h) = scaled.dimensions();
        (w, h, scaled.into_raw())
    } else {
        (w, h, rgba.into_raw())
    };
    Icon::from_rgba(data, w, h).expect("Failed to create tray icon")
}

fn main() {
    config::init();
    ocr::warm_engine();

    let menu = Menu::new();
    for (id, label) in [
        (overlay::MENU_ID_SETTINGS, "Settings"),
        (overlay::MENU_ID_QUIT, "Exit LucentShot"),
    ] {
        let _ = menu.append(&MenuItem::with_id(MenuId::new(id), label, true, None));
    }
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("LucentShot — Left-click to screenshot")
        .with_icon(load_tray_icon())
        .build()
        .expect("Tray icon build failed");

    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_visible(true)
        .with_inner_size([0.0, 0.0])
        .with_taskbar(false)
        .with_transparent(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/icon.png"))
    {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let result = eframe::run_native(
        crate::paths::APP_NAME,
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());

            let trigger_flag = Arc::new(AtomicBool::new(false));
            let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
            let hotkey_handle = hotkey::start_listener(
                cc.egui_ctx.clone(),
                Arc::clone(&trigger_flag),
                hotkey::HotkeyConfig::from_settings(&config::get()),
                hotkey_tx,
            );
            Ok(Box::new(overlay::OverlayApp::new(
                trigger_flag,
                hotkey_handle,
                hotkey_rx,
            )))
        }),
    );
    drop(tray);
    if let Err(err) = result {
        eprintln!("eframe failed: {err}");
    }
}
