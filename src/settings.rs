use crate::config::{Config, ImageFormat};
use crate::hotkey;
use eframe::egui::{self, Align, Layout, RichText};

pub const WINDOW_SIZE: [f32; 2] = [560.0, 440.0];
pub const WINDOW_MIN: [f32; 2] = [480.0, 360.0];

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Hotkeys,
    Formats,
    Speech,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Hotkeys => "Hotkeys",
            Self::Formats => "Formats",
            Self::Speech => "Speech",
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
    let tabs: &[Tab] = if cfg!(feature = "ocr") {
        &[Tab::General, Tab::Hotkeys, Tab::Formats, Tab::Speech]
    } else {
        &[Tab::General, Tab::Hotkeys, Tab::Formats]
    };

    egui::Panel::left("settings_nav")
        .exact_size(128.0)
        .resizable(false)
        .frame(
            egui::Frame::side_top_panel(ui.style())
                .inner_margin(egui::Margin::same(10))
                .stroke(egui::Stroke::NONE),
        )
        .show_inside(ui, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for tab in tabs {
                    if ui
                        .selectable_label(state.tab == *tab, tab.label())
                        .clicked()
                    {
                        state.tab = *tab;
                    }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::same(16)))
        .show_inside(ui, |ui| {
            ui.heading(state.tab.label());
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match state.tab {
                    Tab::General => general(ui, config),
                    Tab::Hotkeys => hotkeys(ui, state, config),
                    Tab::Formats => formats(ui, config),
                    Tab::Speech => {
                        #[cfg(feature = "ocr")]
                        speech(ui, state, config);
                        #[cfg(not(feature = "ocr"))]
                        ui.label("Speech is not available in this build.");
                    }
                });
        });
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title).strong());
    ui.add_space(6.0);
    add_contents(ui);
    ui.add_space(16.0);
}

fn general(ui: &mut egui::Ui, config: &mut Config) {
    section(ui, "Startup", |ui| {
        ui.checkbox(
            &mut config.general_start_with_windows,
            "Start with Windows",
        );
    });

    section(ui, "Capture", |ui| {
        ui.checkbox(
            &mut config.general_capture_cursor,
            "Include cursor in screenshots",
        );
        ui.checkbox(
            &mut config.general_keep_selected_area,
            "Remember last selection",
        );
    });

    section(ui, "Sharing", |ui| {
        ui.checkbox(&mut config.general_auto_copy_link, "Copy link after upload");
        let mut open_page = !config.general_auto_close_upload;
        if ui
            .checkbox(&mut open_page, "Open upload page in the browser")
            .changed()
        {
            config.general_auto_close_upload = !open_page;
        }
        ui.checkbox(
            &mut config.general_show_notifications,
            "Show action notifications",
        );
    });

    section(ui, "Config file", |ui| {
        ui.horizontal(|ui| {
            let browse_w = 80.0;
            let edit_w = (ui.available_width() - browse_w - ui.spacing().item_spacing.x).max(80.0);
            ui.add(
                egui::TextEdit::singleline(&mut config.general_config_path)
                    .desired_width(edit_w)
                    .hint_text("Default location"),
            );
            if ui
                .add_sized([browse_w, 20.0], egui::Button::new("Browse…"))
                .clicked()
            {
                if let Some(path) =
                    crate::actions::pick_save_path("config.json", "JSON config", "json")
                {
                    config.general_config_path = path.to_string_lossy().to_string();
                }
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("Leave empty to use %LOCALAPPDATA%\\LucentShot.")
                .small()
                .weak(),
        );
    });
}

fn hotkeys(ui: &mut egui::Ui, state: &mut SettingsState, config: &mut Config) {
    ui.label(
        RichText::new("Enable a shortcut, then click its field and press a key. Backspace clears.")
            .small()
            .weak(),
    );
    ui.add_space(10.0);

    let mut binding_display = hotkey::format_binding(
        config.hotkey_general_key,
        config.hotkey_general_ctrl,
        config.hotkey_general_shift,
        config.hotkey_general_alt,
        config.hotkey_general_win,
    );

    egui::Grid::new("hotkey_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .min_col_width(180.0)
        .show(ui, |ui| {
            ui.checkbox(&mut config.hotkey_general_enabled, "Open overlay");
            let response = ui.add_enabled(
                config.hotkey_general_enabled,
                egui::TextEdit::singleline(&mut binding_display).desired_width(180.0),
            );
            if config.hotkey_general_enabled {
                capture_hotkey(&response, config, &mut state.last_hotkey_vk_down);
            }
            ui.end_row();

            combo_row(
                ui,
                &mut config.hotkey_instant_save_fullscreen,
                "Save fullscreen",
                &mut config.hotkey_instant_save_combo,
            );
            combo_row(
                ui,
                &mut config.hotkey_instant_upload_fullscreen,
                "Upload fullscreen",
                &mut config.hotkey_instant_upload_combo,
            );
            combo_row(
                ui,
                &mut config.hotkey_copy_focused_window,
                "Copy focused window",
                &mut config.hotkey_copy_focused_window_combo,
            );
        });
}

fn combo_row(ui: &mut egui::Ui, enabled: &mut bool, label: &str, combo: &mut String) {
    ui.checkbox(enabled, label);
    ui.add_enabled(
        *enabled,
        egui::TextEdit::singleline(combo).desired_width(180.0),
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
    egui::Grid::new("format_grid")
        .num_columns(2)
        .spacing([16.0, 12.0])
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label("Upload format");
            egui::ComboBox::from_id_salt("format_select")
                .selected_text(config.format.label())
                .width(160.0)
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
                    .suffix("%")
                    .trailing_fill(true),
            );
            ui.end_row();
        });
}

#[cfg(feature = "ocr")]
fn speech(ui: &mut egui::Ui, state: &mut SettingsState, config: &mut Config) {
    let voices = state.tts_voices.get_or_insert_with(crate::ocr::voices);
    let selected = if config.tts_voice.is_empty() {
        "Default".to_string()
    } else {
        config.tts_voice.clone()
    };

    egui::Grid::new("speech_grid")
        .num_columns(2)
        .spacing([16.0, 12.0])
        .min_col_width(80.0)
        .show(ui, |ui| {
            ui.label("Voice");
            egui::ComboBox::from_id_salt("tts_voice")
                .selected_text(selected)
                .width(260.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.tts_voice, String::new(), "Default");
                    for name in voices.iter() {
                        ui.selectable_value(&mut config.tts_voice, name.clone(), name.as_str());
                    }
                });
            ui.end_row();

            ui.label("Rate");
            ui.add(egui::Slider::new(&mut config.tts_rate, -10..=10).trailing_fill(true));
            ui.end_row();

            ui.label("Volume");
            ui.add(
                egui::Slider::new(&mut config.tts_volume, 0..=100)
                    .suffix("%")
                    .trailing_fill(true),
            );
            ui.end_row();
        });
}
