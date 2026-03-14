use serde::{Deserialize, Serialize};
use eframe::egui::{Color32, Pos2};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConfigImpl {
    pub general_auto_copy_link: bool,
    pub general_show_notifications: bool,
    pub general_auto_close_upload: bool,
    pub general_keep_selected_area: bool,
    pub general_capture_cursor: bool,
    pub general_config_path: String,
    pub hotkey_general_enabled: bool,
    pub hotkey_general_key: Option<u32>,
    pub hotkey_general_ctrl: bool,
    pub hotkey_general_shift: bool,
    pub hotkey_general_alt: bool,
    pub hotkey_general_win: bool,
    pub hotkey_instant_save_fullscreen: bool,
    pub hotkey_instant_upload_fullscreen: bool,
    pub hotkey_copy_focused_window: bool,
    pub hotkey_instant_save_combo: String,
    pub hotkey_instant_upload_combo: String,
    pub hotkey_copy_focused_window_combo: String,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
    pub tts_rate: i32,
    pub tts_volume: i32,
    pub tts_voice: String,
    pub print_selected_printer: String,
    pub print_copies: i32,
    pub print_landscape: bool,
    pub print_grayscale: bool,
    pub print_fit: bool,
    pub print_paper: String,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
    pub color_a: u8,
    pub marker_opacity: f32,
}

impl Default for ConfigImpl {
    fn default() -> Self {
        Self {
            general_auto_copy_link: false,
            general_show_notifications: true,
            general_auto_close_upload: false,
            general_keep_selected_area: false,
            general_capture_cursor: false,
            general_config_path: String::new(),
            hotkey_general_enabled: true,
            hotkey_general_key: Some(0x2C),
            hotkey_general_ctrl: false,
            hotkey_general_shift: false,
            hotkey_general_alt: false,
            hotkey_general_win: false,
            hotkey_instant_save_fullscreen: true,
            hotkey_instant_upload_fullscreen: false,
            hotkey_copy_focused_window: true,
            hotkey_instant_save_combo: "Shift + Prnt Scrn".to_string(),
            hotkey_instant_upload_combo: "Ctrl + Prnt Scrn".to_string(),
            hotkey_copy_focused_window_combo: "Alt + Prnt Scrn".to_string(),
            format: ImageFormat::Png,
            jpeg_quality: 100,
            tts_rate: 0,
            tts_volume: 100,
            tts_voice: String::new(),
            print_selected_printer: String::new(),
            print_copies: 1,
            print_landscape: false,
            print_grayscale: false,
            print_fit: true,
            print_paper: "A4".to_string(),
            color_r: 255,
            color_g: 0,
            color_b: 0,
            color_a: 255,
            marker_opacity: 0.4,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Gif,
}

impl ImageFormat {
    pub fn label(self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Bmp => "BMP",
            ImageFormat::Gif => "GIF",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Pen,
    Line,
    Arrow,
    Rect,
    Marker,
    Text,
}

#[derive(Clone)]
pub struct Shape {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub stroke_width: f32,
    pub tool: Tool,
    pub text: String,
    pub is_marker: bool,
    pub opacity: f32,
}

pub enum PendingAction {
    Copy,
    Save,
    Upload,
    Ocr,
    Speak,
    Print {
        printer: String,
        copies: i32,
        landscape: bool,
        grayscale: bool,
        paper: String,
    },
    Google,
}

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
    let _ = save_to_disk(&config);
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
    const VK_SNAPSHOT: u32 = 0x2C;
    let instant_save_binding = crate::hotkey::parse_hotkey_combo(&config.hotkey_instant_save_combo)
        .unwrap_or(HotkeyBinding {
            key: Some(VK_SNAPSHOT),
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
            key: Some(VK_SNAPSHOT),
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
            key: Some(VK_SNAPSHOT),
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
    std::env::temp_dir().join("lightshotv2_config.json")
}

fn bootstrap_path() -> PathBuf {
    std::env::temp_dir().join("lightshotv2_config_path.txt")
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
    let data = serde_json::to_string(config).map_err(|e| e.to_string())?;
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
