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
mod startup;
#[cfg(feature = "ocr")]
mod tesseract;

use eframe::egui;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};

pub(crate) fn app_icon() -> image::RgbaImage {
    image::load_from_memory(include_bytes!("../assets/icons/icon.ico"))
        .expect("icon.ico")
        .to_rgba8()
}

fn load_tray_icon() -> Icon {
    let rgba = app_icon();
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
    startup::apply(config::get().general_start_with_windows);
    notification::init();
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
    let icon = app_icon();
    let (width, height) = icon.dimensions();
    viewport = viewport.with_icon(Arc::new(egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    }));

    let result = eframe::run_native(
        crate::paths::APP_NAME,
        eframe::NativeOptions {
            viewport,
            vsync: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());

            let trigger_flag = Arc::new(AtomicBool::new(false));
            let settings_flag = Arc::new(AtomicBool::new(false));
            bind_tray_events(
                cc.egui_ctx.clone(),
                Arc::clone(&trigger_flag),
                Arc::clone(&settings_flag),
            );
            let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
            let hotkey_handle = hotkey::start_listener(
                cc.egui_ctx.clone(),
                Arc::clone(&trigger_flag),
                hotkey::HotkeyConfig::from_settings(&config::get()),
                hotkey_tx,
            );
            Ok(Box::new(overlay::OverlayApp::new(
                trigger_flag,
                settings_flag,
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

fn bind_tray_events(
    ctx: egui::Context,
    trigger_flag: Arc<AtomicBool>,
    settings_flag: Arc<AtomicBool>,
) {
    MenuEvent::set_event_handler(Some({
        let ctx = ctx.clone();
        move |event: MenuEvent| {
            if event.id == overlay::MENU_ID_QUIT {
                process::exit(0);
            }
            if event.id == overlay::MENU_ID_SETTINGS {
                settings_flag.store(true, Ordering::SeqCst);
                ctx.request_repaint();
            }
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
        ) {
            trigger_flag.store(true, Ordering::SeqCst);
            ctx.request_repaint();
        }
    }));
}
