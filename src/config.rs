use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

static STORE: OnceLock<RwLock<Config>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub general_auto_copy_link: bool,
    pub general_show_notifications: bool,
    pub general_auto_close_upload: bool,
    pub general_capture_cursor: bool,
    pub general_start_with_windows: bool,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            general_auto_copy_link: false,
            general_show_notifications: true,
            general_auto_close_upload: false,
            general_capture_cursor: false,
            general_start_with_windows: false,
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

impl Config {
    pub fn drawing_color(&self) -> eframe::egui::Color32 {
        eframe::egui::Color32::from_rgba_unmultiplied(
            self.color_r,
            self.color_g,
            self.color_b,
            self.color_a,
        )
    }

    pub fn set_drawing_color(&mut self, color: eframe::egui::Color32) {
        self.color_r = color.r();
        self.color_g = color.g();
        self.color_b = color.b();
        self.color_a = color.a();
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
    fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Png => ("PNG", "png", "image/png"),
            Self::Jpeg => ("JPEG", "jpg", "image/jpeg"),
            Self::Bmp => ("BMP", "bmp", "image/bmp"),
            Self::Gif => ("GIF", "gif", "image/gif"),
        }
    }

    pub fn label(self) -> &'static str {
        self.info().0
    }

    pub fn extension(self) -> &'static str {
        self.info().1
    }

    pub fn mime(self) -> &'static str {
        self.info().2
    }
}

pub fn init() {
    let config = load_from_disk().unwrap_or_default();
    let _ = STORE.set(RwLock::new(config));
}

pub fn get() -> RwLockReadGuard<'static, Config> {
    STORE
        .get()
        .expect("Config not initialized")
        .read()
        .expect("Config lock poisoned")
}

pub fn get_mut() -> RwLockWriteGuard<'static, Config> {
    STORE
        .get()
        .expect("Config not initialized")
        .write()
        .expect("Config lock poisoned")
}

pub fn save() {
    let config = get();
    if let Err(err) = save_to_disk(&config) {
        eprintln!("Failed to save config: {err}");
    }
}

pub fn persist(config: &Config) {
    *get_mut() = config.clone();
    crate::startup::apply(config.general_start_with_windows);
    save();
}

fn default_config_path() -> PathBuf {
    crate::paths::config_file()
}

fn bootstrap_path() -> PathBuf {
    crate::paths::config_bootstrap()
}

fn config_path_from_bootstrap() -> PathBuf {
    let Ok(s) = fs::read_to_string(bootstrap_path()) else {
        return default_config_path();
    };
    let s = s.trim();
    if s.is_empty() {
        default_config_path()
    } else {
        PathBuf::from(s)
    }
}

fn load_from_disk() -> Option<Config> {
    let data = fs::read_to_string(config_path_from_bootstrap()).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_to_disk(config: &Config) -> Result<(), String> {
    let custom = config.general_config_path.trim();
    let path = if custom.is_empty() {
        default_config_path()
    } else {
        PathBuf::from(custom)
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        &path,
        serde_json::to_string(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let bootstrap = bootstrap_path();
    if custom.is_empty() {
        let _ = fs::remove_file(bootstrap);
        return Ok(());
    }
    if let Some(parent) = bootstrap.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(bootstrap, path.to_string_lossy().as_bytes());
    Ok(())
}
