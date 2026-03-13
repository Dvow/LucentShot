use eframe::egui;
use crate::config::{ConfigImpl, ImageFormat};

#[derive(Clone, Copy, PartialEq)]
pub enum SettingsTab {
    General,
    Hotkeys,
    Formats,
    Tts,
}

impl SettingsTab {
    fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Hotkeys => "Hotkeys",
            SettingsTab::Formats => "Formats",
            SettingsTab::Tts => "TTS",
        }
    }
}

pub struct SettingsWindowState {
    pub active_tab: SettingsTab,
    pub last_snapshot_down: bool,
    /// Cached TTS voices; populated lazily when TTS tab is shown (get_tts_voices spawns PowerShell).
    pub tts_voices_cache: Option<Vec<String>>,
}

impl Default for SettingsWindowState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            last_snapshot_down: false,
            tts_voices_cache: None,
        }
    }
}

pub fn show_settings_window(
    ctx: &egui::Context,
    state: &mut SettingsWindowState,
    config: &mut ConfigImpl,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.set_width(500.0);
        ui.heading("Settings");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            for tab in [
                SettingsTab::General,
                SettingsTab::Hotkeys,
                SettingsTab::Formats,
                SettingsTab::Tts,
            ] {
                let selected = state.active_tab == tab;
                if ui.selectable_label(selected, tab.label()).clicked() {
                    state.active_tab = tab;
                }
            }
        });

        ui.separator();

        match state.active_tab {
            SettingsTab::General => render_general(ui, config),
            SettingsTab::Hotkeys => render_hotkeys(ctx, ui, state, config),
            SettingsTab::Formats => render_formats(ui, config),
            SettingsTab::Tts => render_tts(ui, state, config),
        }
    });
}

fn render_general(ui: &mut egui::Ui, config: &mut ConfigImpl) {
    ui.add_space(6.0);
    ui.checkbox(
        &mut config.general_auto_copy_link,
        "Copy link after upload",
    );
    ui.checkbox(
        &mut config.general_auto_close_upload,
        "Do not open upload page",
    );
    ui.checkbox(
        &mut config.general_keep_selected_area,
        "Keep selection",
    );
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
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("config", &["json"])
                        .set_file_name("config.json")
                        .save_file()
                    {
                        config.general_config_path = path.to_string_lossy().to_string();
                    }
                }
            });
            ui.end_row();

            ui.label("Language");
            egui::ComboBox::from_id_source("language_select")
                .selected_text(&config.general_language)
                .width(180.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.general_language,
                        "English".to_string(),
                        "English",
                    );
                });
            ui.end_row();
        });
}

fn render_hotkeys(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    state: &mut SettingsWindowState,
    config: &mut ConfigImpl,
) {
    ui.add_space(6.0);
    let mut binding_display = format_binding(
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
                egui::TextEdit::singleline(&mut binding_display)
                    .desired_width(160.0),
            );
            if config.hotkey_general_enabled {
                capture_hotkey_input(
                    ctx,
                    &response,
                    config,
                    &mut state.last_snapshot_down,
                );
            }
            ui.end_row();

            ui.checkbox(
                &mut config.hotkey_instant_save_fullscreen,
                "Instant save of the fullscreen",
            );
            ui.add_enabled(
                config.hotkey_instant_save_fullscreen,
                egui::TextEdit::singleline(&mut config.hotkey_instant_save_combo)
                    .desired_width(160.0),
            );
            ui.end_row();

            ui.checkbox(
                &mut config.hotkey_instant_upload_fullscreen,
                "Instant upload of the fullscreen",
            );
            ui.add_enabled(
                config.hotkey_instant_upload_fullscreen,
                egui::TextEdit::singleline(&mut config.hotkey_instant_upload_combo)
                    .desired_width(160.0),
            );
            ui.end_row();

            ui.checkbox(
                &mut config.hotkey_copy_focused_window,
                "Copy focused window (e.g. Alt + Prnt Scrn)",
            );
            ui.add_enabled(
                config.hotkey_copy_focused_window,
                egui::TextEdit::singleline(&mut config.hotkey_copy_focused_window_combo)
                    .desired_width(160.0),
            );
            ui.end_row();
        });
}

fn capture_hotkey_input(
    ctx: &egui::Context,
    response: &egui::Response,
    config: &mut ConfigImpl,
    last_snapshot_down: &mut bool,
) {
    if !response.has_focus() {
        return;
    }

    let mut next_key: Option<u32> = None;
    let mut next_ctrl = None;
    let mut next_shift = None;
    let mut next_alt = None;
    let mut next_win = None;
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if *key == egui::Key::Backspace || *key == egui::Key::Delete {
                    next_key = Some(0);
                    next_ctrl = Some(false);
                    next_shift = Some(false);
                    next_alt = Some(false);
                    next_win = Some(false);
                } else if *key != egui::Key::Escape {
                    if let Some(vk) = crate::hotkey::egui_key_to_vk(*key) {
                        next_key = Some(vk);
                        next_ctrl = Some(modifiers.ctrl);
                        next_shift = Some(modifiers.shift);
                        next_alt = Some(modifiers.alt);
                        next_win = Some(modifiers.command);
                    }
                }
                break;
            }
        }
    });

    if next_key.is_none() {
        let snapshot_down = unsafe {
            (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
                windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as i32,
            ) as u16) & 0x8000 != 0
        };
        if snapshot_down && !*last_snapshot_down {
            next_key = Some(
                windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT.0 as u32,
            );
            next_ctrl = Some(false);
            next_shift = Some(false);
            next_alt = Some(false);
            next_win = Some(false);
        }
        *last_snapshot_down = snapshot_down;
    }

    if let Some(key) = next_key {
        if key == 0 {
            config.hotkey_general_key = None;
        } else {
            config.hotkey_general_key = Some(key);
        }
    }
    if let Some(value) = next_ctrl {
        config.hotkey_general_ctrl = value;
    }
    if let Some(value) = next_shift {
        config.hotkey_general_shift = value;
    }
    if let Some(value) = next_alt {
        config.hotkey_general_alt = value;
    }
    if let Some(value) = next_win {
        config.hotkey_general_win = value;
    }
}

fn format_binding(
    key: Option<u32>,
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
) -> String {
    let Some(key) = key else {
        return "Unassigned".to_string();
    };

    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    if win {
        parts.push("Win");
    }
    if let Some(label) = crate::hotkey::vk_label(key) {
        parts.push(label);
    } else {
        return format!("VK_{}", key);
    }
    parts.join(" + ")
}

fn render_formats(ui: &mut egui::Ui, config: &mut ConfigImpl) {
    ui.add_space(6.0);
    egui::Grid::new("format_grid")
        .num_columns(2)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.label("Upload using the format");
            egui::ComboBox::from_id_source("format_select")
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
            let slider = egui::Slider::new(&mut config.jpeg_quality, 1..=100)
                .show_value(true)
                .trailing_fill(true);
            ui.add_enabled(config.format == ImageFormat::Jpeg, slider);
            ui.end_row();
        });
}

fn render_tts(ui: &mut egui::Ui, state: &mut SettingsWindowState, config: &mut ConfigImpl) {
    ui.add_space(6.0);
    ui.label("Voice");
    let voices = state.tts_voices_cache.get_or_insert_with(crate::actions::get_tts_voices);
    let selected_display = if config.tts_voice.is_empty() {
        "(Default)".to_string()
    } else {
        config.tts_voice.clone()
    };
    egui::ComboBox::from_id_source("tts_voice")
        .selected_text(selected_display)
        .show_ui(ui, |ui| {
            if ui.selectable_label(config.tts_voice.is_empty(), "(Default)").clicked() {
                config.tts_voice = String::new();
                crate::config::save();
            }
            for name in voices.iter() {
                if ui.selectable_label(config.tts_voice == *name, name.as_str()).clicked() {
                    config.tts_voice = name.clone();
                    crate::config::save();
                }
            }
        });
    ui.add_space(10.0);
    ui.label("Speech rate");
    ui.add(egui::Slider::new(&mut config.tts_rate, -10..=10));
    ui.add_space(10.0);
    ui.label("Volume");
    ui.add(egui::Slider::new(&mut config.tts_volume, 0..=100));
}
