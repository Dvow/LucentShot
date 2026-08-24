use crate::config::{Config, ImageFormat};
use crate::hotkey;
use eframe::egui;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Hotkeys,
    Formats,
    Tts,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Hotkeys => "Hotkeys",
            Self::Formats => "Formats",
            Self::Tts => "TTS",
        }
    }
}

pub struct SettingsState {
    tab: Tab,
    last_hotkey_vk_down: Option<u32>,
    #[cfg(feature = "ocr")]
    tts_voices: Option<Vec<String>>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            tab: Tab::General,
            last_hotkey_vk_down: None,
            #[cfg(feature = "ocr")]
            tts_voices: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SettingsState, config: &mut Config) {
    ui.set_width(500.0);
    ui.heading("Settings");
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        let tabs: &[Tab] = if cfg!(feature = "ocr") {
            &[Tab::General, Tab::Hotkeys, Tab::Formats, Tab::Tts]
        } else {
            &[Tab::General, Tab::Hotkeys, Tab::Formats]
        };
        for tab in tabs {
            if ui
                .selectable_label(state.tab == *tab, tab.label())
                .clicked()
            {
                state.tab = *tab;
            }
        }
    });
    ui.separator();

    match state.tab {
        Tab::General => general(ui, config),
        Tab::Hotkeys => hotkeys(ui, state, config),
        Tab::Formats => formats(ui, config),
        Tab::Tts => {
            #[cfg(feature = "ocr")]
            tts(ui, state, config);
            #[cfg(not(feature = "ocr"))]
            ui.label("OCR/TTS not available in this build.");
        }
    }
}

fn general(ui: &mut egui::Ui, config: &mut Config) {
    ui.add_space(6.0);
    ui.checkbox(&mut config.general_auto_copy_link, "Copy link after upload");
    ui.checkbox(
        &mut config.general_show_notifications,
        "Show notifications (toast popups for actions)",
    );
    ui.checkbox(
        &mut config.general_auto_close_upload,
        "Do not open upload page",
    );
    ui.checkbox(&mut config.general_keep_selected_area, "Keep selection");
    ui.checkbox(
        &mut config.general_capture_cursor,
        "Include cursor in capture",
    );
    ui.add_space(8.0);

    egui::Grid::new("general_grid")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label("Config file path");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut config.general_config_path)
                        .desired_width(260.0)
                        .hint_text("Default"),
                );
                if ui.button("Browse…").clicked() {
                    let picked =
                        crate::actions::pick_save_path("config.json", "JSON config", "json");
                    if let Some(path) = picked {
                        config.general_config_path = path.to_string_lossy().to_string();
                    }
                }
            });
            ui.end_row();
        });
}

fn hotkeys(ui: &mut egui::Ui, state: &mut SettingsState, config: &mut Config) {
    ui.add_space(6.0);
    let mut binding_display = hotkey::format_binding(
        config.hotkey_general_key,
        config.hotkey_general_ctrl,
        config.hotkey_general_shift,
        config.hotkey_general_alt,
        config.hotkey_general_win,
    );
    egui::Grid::new("hotkey_grid")
        .num_columns(2)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.checkbox(&mut config.hotkey_general_enabled, "General hotkey");
            let response = ui.add_enabled(
                config.hotkey_general_enabled,
                egui::TextEdit::singleline(&mut binding_display).desired_width(160.0),
            );
            if config.hotkey_general_enabled {
                capture_hotkey(&response, config, &mut state.last_hotkey_vk_down);
            }
            ui.end_row();

            combo_row(
                ui,
                &mut config.hotkey_instant_save_fullscreen,
                "Instant save of the fullscreen",
                &mut config.hotkey_instant_save_combo,
            );
            combo_row(
                ui,
                &mut config.hotkey_instant_upload_fullscreen,
                "Instant upload of the fullscreen",
                &mut config.hotkey_instant_upload_combo,
            );
            combo_row(
                ui,
                &mut config.hotkey_copy_focused_window,
                "Copy focused window (e.g. Alt + Prnt Scrn)",
                &mut config.hotkey_copy_focused_window_combo,
            );
        });
}

fn combo_row(ui: &mut egui::Ui, enabled: &mut bool, label: &str, combo: &mut String) {
    ui.checkbox(enabled, label);
    ui.add_enabled(
        *enabled,
        egui::TextEdit::singleline(combo).desired_width(160.0),
    );
    ui.end_row();
}

fn capture_hotkey(response: &egui::Response, config: &mut Config, last_vk: &mut Option<u32>) {
    if !response.has_focus() {
        *last_vk = None;
        return;
    }
    let current = hotkey::assignment_key_down();
    if current.is_none() || current == *last_vk {
        if current.is_none() {
            *last_vk = None;
        }
        return;
    }
    let Some(vk) = current else {
        return;
    };
    *last_vk = current;
    if hotkey::is_clear_key(vk) {
        config.hotkey_general_key = None;
        config.hotkey_general_ctrl = false;
        config.hotkey_general_shift = false;
        config.hotkey_general_alt = false;
        config.hotkey_general_win = false;
    } else if !hotkey::is_escape_key(vk) {
        let (ctrl, shift, alt, win) = hotkey::modifiers_down();
        config.hotkey_general_key = Some(vk);
        config.hotkey_general_ctrl = ctrl;
        config.hotkey_general_shift = shift;
        config.hotkey_general_alt = alt;
        config.hotkey_general_win = win;
    }
}

fn formats(ui: &mut egui::Ui, config: &mut Config) {
    ui.add_space(6.0);
    egui::Grid::new("format_grid")
        .num_columns(2)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.label("Upload using the format");
            egui::ComboBox::from_id_salt("format_select")
                .selected_text(config.format.label())
                .width(140.0)
                .show_ui(ui, |ui| {
                    for fmt in [
                        ImageFormat::Png,
                        ImageFormat::Jpeg,
                        ImageFormat::Bmp,
                        ImageFormat::Gif,
                    ] {
                        ui.selectable_value(&mut config.format, fmt, fmt.label());
                    }
                });
            ui.end_row();

            ui.label("JPEG quality");
            ui.add_enabled(
                config.format == ImageFormat::Jpeg,
                egui::Slider::new(&mut config.jpeg_quality, 1..=100)
                    .show_value(true)
                    .trailing_fill(true),
            );
            ui.end_row();
        });
}

#[cfg(feature = "ocr")]
fn tts(ui: &mut egui::Ui, state: &mut SettingsState, config: &mut Config) {
    ui.add_space(6.0);
    ui.label("Voice");
    let voices = state.tts_voices.get_or_insert_with(crate::ocr::voices);
    let selected = if config.tts_voice.is_empty() {
        "(Default)".to_string()
    } else {
        config.tts_voice.clone()
    };
    egui::ComboBox::from_id_salt("tts_voice")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(config.tts_voice.is_empty(), "(Default)")
                .clicked()
            {
                config.tts_voice.clear();
                crate::config::save();
            }
            for name in voices.iter() {
                let selected = config.tts_voice == *name;
                if !ui.selectable_label(selected, name.as_str()).clicked() {
                    continue;
                }
                config.tts_voice = name.clone();
                crate::config::save();
            }
        });
    ui.add_space(10.0);
    ui.label("Speech rate");
    ui.add(egui::Slider::new(&mut config.tts_rate, -10..=10));
    ui.add_space(10.0);
    ui.label("Volume");
    ui.add(egui::Slider::new(&mut config.tts_volume, 0..=100));
}
