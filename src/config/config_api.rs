use crate::config::config_impl::ConfigImpl;
use crate::hotkey::{HotkeyBinding, HotkeyConfig};
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

static CONFIG: OnceLock<RwLock<ConfigImpl>> = OnceLock::new();

pub fn init() {
    let config = load_from_disk().unwrap_or_default();
    let _ = CONFIG.set(RwLock::new(config));
}

pub fn cfg() -> RwLockReadGuard<'static, ConfigImpl> {
    CONFIG
        .get()
        .expect("Config not initialized")
        .read()
        .expect("Config lock poisoned")
}

pub fn cfg_mut() -> RwLockWriteGuard<'static, ConfigImpl> {
    CONFIG
        .get()
        .expect("Config not initialized")
        .write()
        .expect("Config lock poisoned")
}

pub fn save() {
    let config = cfg();
    if let Err(err) = save_to_disk(&config) {
        eprintln!("Config save failed: {}", err);
    }
}

pub fn hotkey_config(config: &ConfigImpl) -> HotkeyConfig {
    let general_binding = HotkeyBinding {
        key: config.hotkey_general_key,
        modifiers: egui::Modifiers {
            ctrl: config.hotkey_general_ctrl,
            shift: config.hotkey_general_shift,
            alt: config.hotkey_general_alt,
            command: config.hotkey_general_win,
            mac_cmd: false,
        },
    };
    let instant_save_binding = crate::hotkey::parse_hotkey_combo(&config.hotkey_instant_save_combo)
        .unwrap_or(HotkeyBinding {
            key: Some(windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32),
            modifiers: egui::Modifiers {
                ctrl: false,
                shift: true,
                alt: false,
                command: false,
                mac_cmd: false,
            },
        });
    let instant_upload_binding = crate::hotkey::parse_hotkey_combo(&config.hotkey_instant_upload_combo)
        .unwrap_or(HotkeyBinding {
            key: Some(windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32),
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: false,
                alt: false,
                command: false,
                mac_cmd: false,
            },
        });
    let copy_focused_window_binding = crate::hotkey::parse_hotkey_combo(&config.hotkey_copy_focused_window_combo)
        .unwrap_or(HotkeyBinding {
            key: Some(windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32),
            modifiers: egui::Modifiers {
                ctrl: false,
                shift: false,
                alt: true,
                command: false,
                mac_cmd: false,
            },
        });
    HotkeyConfig {
        general_enabled: config.hotkey_general_enabled,
        general_binding,
        instant_save_enabled: config.hotkey_instant_save_fullscreen,
        instant_save_binding,
        instant_upload_enabled: config.hotkey_instant_upload_fullscreen,
        instant_upload_binding,
        copy_focused_window_enabled: config.hotkey_copy_focused_window,
        copy_focused_window_binding,
    }
}

fn default_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
            .join("lightshotv2")
            .join("config.json")
    } else {
        std::env::temp_dir().join("lightshotv2_config.json")
    }
}

fn bootstrap_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
            .join("lightshotv2")
            .join("config_path.txt")
    } else {
        std::env::temp_dir().join("lightshotv2_config_path.txt")
    }
}

fn config_path_from_bootstrap() -> PathBuf {
    let bootstrap = bootstrap_path();
    if let Ok(s) = fs::read_to_string(&bootstrap) {
        let s = s.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    default_config_path()
}

fn load_from_disk() -> Option<ConfigImpl> {
    let path = config_path_from_bootstrap();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_to_disk(config: &ConfigImpl) -> Result<(), String> {
    let path = if config.general_config_path.trim().is_empty() {
        default_config_path()
    } else {
        PathBuf::from(config.general_config_path.trim())
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())?;
    let bootstrap = bootstrap_path();
    if config.general_config_path.trim().is_empty() {
        let _ = fs::remove_file(bootstrap);
    } else {
        if let Some(parent) = bootstrap.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(bootstrap, path.to_string_lossy().as_bytes());
    }
    Ok(())
}
