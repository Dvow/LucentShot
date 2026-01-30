use serde::{Deserialize, Serialize};
use eframe::egui::{Color32, Pos2};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConfigImpl {
    pub general_auto_copy_link: bool,
    pub general_auto_close_upload: bool,
    pub general_keep_selected_area: bool,
    pub general_capture_cursor: bool,
    pub general_config_path: String,
    pub general_language: String,
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
            general_auto_close_upload: false,
            general_keep_selected_area: false,
            general_capture_cursor: false,
            general_config_path: String::new(),
            general_language: "English".to_string(),
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
    OCR,
    Speak,
    Print {
        printer: String,
        copies: i32,
        landscape: bool,
        grayscale: bool,
        fit: bool,
        paper: String,
    },
    Google,
}
