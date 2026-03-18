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
    pub last_hotkey_vk_down: Option<u32>,
    #[cfg(feature = "ocr")]
    pub tts_voices_cache: Option<Vec<String>>,
}

impl Default for SettingsWindowState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            last_hotkey_vk_down: None,
            #[cfg(feature = "ocr")]
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
            let tabs: &[SettingsTab] = if cfg!(feature = "ocr") {
                &[SettingsTab::General, SettingsTab::Hotkeys, SettingsTab::Formats, SettingsTab::Tts]
            } else {
                &[SettingsTab::General, SettingsTab::Hotkeys, SettingsTab::Formats]
            };
            for tab in tabs {
                let selected = state.active_tab == *tab;
                if ui.selectable_label(selected, tab.label()).clicked() {
                    state.active_tab = *tab;
                }
            }
        });

        ui.separator();

        match state.active_tab {
            SettingsTab::General => render_general(ui, config),
            SettingsTab::Hotkeys => render_hotkeys(ui, state, config),
            SettingsTab::Formats => render_formats(ui, config),
            SettingsTab::Tts => {
                #[cfg(feature = "ocr")]
                render_tts(ui, state, config);
                #[cfg(not(feature = "ocr"))]
                ui.label("OCR/TTS not available in this build.");
            }
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
        &mut config.general_show_notifications,
        "Show notifications (toast popups for actions)",
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
                    if let Some(path) = crate::actions::pick_save_path("config.json", "JSON config", "json") {
                        config.general_config_path = path.to_string_lossy().to_string();
                    }
                }
            });
            ui.end_row();
        });
}

fn render_hotkeys(ui: &mut egui::Ui, state: &mut SettingsWindowState, config: &mut ConfigImpl) {
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
                    &response,
                    config,
                    &mut state.last_hotkey_vk_down,
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
    response: &egui::Response,
    config: &mut ConfigImpl,
    last_hotkey_vk_down: &mut Option<u32>,
) {
    if !response.has_focus() {
        *last_hotkey_vk_down = None;
        return;
    }

    let current_vk_down = current_hotkey_vk_down();
    if current_vk_down.is_none() {
        *last_hotkey_vk_down = None;
        return;
    }

    if current_vk_down == *last_hotkey_vk_down {
        return;
    }

    *last_hotkey_vk_down = current_vk_down;

    if let Some(vk) = current_vk_down {
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_BACK.0 as u32
            || vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_DELETE.0 as u32
        {
            config.hotkey_general_key = None;
            config.hotkey_general_ctrl = false;
            config.hotkey_general_shift = false;
            config.hotkey_general_alt = false;
            config.hotkey_general_win = false;
        } else if vk != windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0 as u32 {
            config.hotkey_general_key = Some(vk);
            config.hotkey_general_ctrl = is_ctrl_down();
            config.hotkey_general_shift = is_shift_down();
            config.hotkey_general_alt = is_alt_down();
            config.hotkey_general_win = is_windows_key_down();
        }
    }
}

fn current_hotkey_vk_down() -> Option<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT,
        VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP,
    };

    for vk in [
        VK_SNAPSHOT.0 as u32,
        VK_BACK.0 as u32,
        VK_DELETE.0 as u32,
        VK_ESCAPE.0 as u32,
        VK_TAB.0 as u32,
        VK_RETURN.0 as u32,
        VK_SPACE.0 as u32,
        VK_INSERT.0 as u32,
        VK_HOME.0 as u32,
        VK_END.0 as u32,
        VK_PRIOR.0 as u32,
        VK_NEXT.0 as u32,
        VK_UP.0 as u32,
        VK_DOWN.0 as u32,
        VK_LEFT.0 as u32,
        VK_RIGHT.0 as u32,
    ] {
        if is_vk_down(vk) {
            return Some(vk);
        }
    }

    for vk in 0x30..=0x39 {
        if is_vk_down(vk) {
            return Some(vk);
        }
    }

    for vk in 0x41..=0x5A {
        if is_vk_down(vk) {
            return Some(vk);
        }
    }

    for vk in 0x70..=0x7B {
        if is_vk_down(vk) {
            return Some(vk);
        }
    }

    None
}

fn is_vk_down(vk: u32) -> bool {
    unsafe {
        (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16) & 0x8000
            != 0
    }
}

fn is_ctrl_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_LCONTROL, VK_RCONTROL};

    is_vk_down(VK_CONTROL.0 as u32)
        || is_vk_down(VK_LCONTROL.0 as u32)
        || is_vk_down(VK_RCONTROL.0 as u32)
}

fn is_shift_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LSHIFT, VK_RSHIFT, VK_SHIFT};

    is_vk_down(VK_SHIFT.0 as u32)
        || is_vk_down(VK_LSHIFT.0 as u32)
        || is_vk_down(VK_RSHIFT.0 as u32)
}

fn is_alt_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LMENU, VK_MENU, VK_RMENU};

    is_vk_down(VK_MENU.0 as u32)
        || is_vk_down(VK_LMENU.0 as u32)
        || is_vk_down(VK_RMENU.0 as u32)
}

fn is_windows_key_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN};

    unsafe {
        ((GetAsyncKeyState(VK_LWIN.0 as i32) as u16) & 0x8000 != 0)
            || ((GetAsyncKeyState(VK_RWIN.0 as i32) as u16) & 0x8000 != 0)
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
        return format!("VK_{key}");
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

#[cfg(feature = "ocr")]
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
