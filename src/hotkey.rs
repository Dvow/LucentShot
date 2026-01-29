use std::thread;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};
use windows_sys::Win32::UI::Input::KeyboardAndMouse as vk;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyBinding {
    pub key: Option<u32>,
    pub modifiers: egui::Modifiers,
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            key: Some(vk::VK_SNAPSHOT as u32),
            modifiers: egui::Modifiers::NONE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyConfig {
    pub general_enabled: bool,
    pub general_binding: HotkeyBinding,
    pub instant_save_enabled: bool,
    pub instant_save_binding: HotkeyBinding,
    pub instant_upload_enabled: bool,
    pub instant_upload_binding: HotkeyBinding,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            general_enabled: true,
            general_binding: HotkeyBinding::default(),
            instant_save_enabled: false,
            instant_save_binding: HotkeyBinding::default(),
            instant_upload_enabled: false,
            instant_upload_binding: HotkeyBinding::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HotkeyEvent {
    InstantSave,
    InstantUpload,
}

#[derive(Clone)]
pub struct HotkeyHandle {
    thread_id: u32,
    config: Arc<Mutex<HotkeyConfig>>,
}

impl HotkeyHandle {
    pub fn update(&self, config: HotkeyConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_APP + 1, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn start_low_level_hotkey_loop(
    ctx: egui::Context,
    trigger_flag: Arc<AtomicBool>,
    initial_config: HotkeyConfig,
    event_tx: mpsc::Sender<HotkeyEvent>,
) -> HotkeyHandle {
    let config = Arc::new(Mutex::new(initial_config));
    let (tx, rx) = mpsc::channel();
    let config_thread = Arc::clone(&config);

    thread::spawn(move || {
        unsafe {
            let thread_id = GetCurrentThreadId();
            let _ = tx.send(thread_id);
            apply_hotkey_config(&config_thread);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                if msg.message == WM_HOTKEY {
                    let id = msg.wParam.0 as u32;
                    match id {
                        1 => {
                            trigger_flag.store(true, Ordering::SeqCst);
                            ctx.request_repaint();
                        }
                        2 => {
                            let _ = event_tx.send(HotkeyEvent::InstantSave);
                        }
                        3 => {
                            let _ = event_tx.send(HotkeyEvent::InstantUpload);
                        }
                        _ => {}
                    }
                } else if msg.message == WM_APP + 1 {
                    apply_hotkey_config(&config_thread);
                }
            }
        }
    });

    let thread_id = rx.recv().unwrap_or(0);
    HotkeyHandle { thread_id, config }
}

fn apply_hotkey_config(config: &Arc<Mutex<HotkeyConfig>>) {
    let Ok(cfg) = config.lock() else { return };
    unsafe {
        let _ = UnregisterHotKey(None, 1);
        let _ = UnregisterHotKey(None, 2);
        let _ = UnregisterHotKey(None, 3);

        if cfg.general_enabled {
            register_hotkey(1, cfg.general_binding);
        }
        if cfg.instant_save_enabled {
            register_hotkey(2, cfg.instant_save_binding);
        }
        if cfg.instant_upload_enabled {
            register_hotkey(3, cfg.instant_upload_binding);
        }
    }
}

fn register_hotkey(id: u32, binding: HotkeyBinding) {
    let Some(vk) = binding.key else { return };
    let modifiers = egui_modifiers_to_win(binding.modifiers);
    unsafe {
        if RegisterHotKey(None, id as i32, modifiers, vk).is_err() {
            eprintln!("Failed to register hotkey {}", id);
        }
    }
}

fn egui_modifiers_to_win(mods: egui::Modifiers) -> HOT_KEY_MODIFIERS {
    let mut flags = HOT_KEY_MODIFIERS(MOD_NOREPEAT.0);
    if mods.ctrl {
        flags.0 |= MOD_CONTROL.0;
    }
    if mods.shift {
        flags.0 |= MOD_SHIFT.0;
    }
    if mods.alt {
        flags.0 |= MOD_ALT.0;
    }
    if mods.command {
        flags.0 |= MOD_WIN.0;
    }
    flags
}

pub fn egui_key_to_vk(key: egui::Key) -> Option<u32> {
    use egui::Key;
    let vk = match key {
        Key::A => vk::VK_A as u32,
        Key::B => vk::VK_B as u32,
        Key::C => vk::VK_C as u32,
        Key::D => vk::VK_D as u32,
        Key::E => vk::VK_E as u32,
        Key::F => vk::VK_F as u32,
        Key::G => vk::VK_G as u32,
        Key::H => vk::VK_H as u32,
        Key::I => vk::VK_I as u32,
        Key::J => vk::VK_J as u32,
        Key::K => vk::VK_K as u32,
        Key::L => vk::VK_L as u32,
        Key::M => vk::VK_M as u32,
        Key::N => vk::VK_N as u32,
        Key::O => vk::VK_O as u32,
        Key::P => vk::VK_P as u32,
        Key::Q => vk::VK_Q as u32,
        Key::R => vk::VK_R as u32,
        Key::S => vk::VK_S as u32,
        Key::T => vk::VK_T as u32,
        Key::U => vk::VK_U as u32,
        Key::V => vk::VK_V as u32,
        Key::W => vk::VK_W as u32,
        Key::X => vk::VK_X as u32,
        Key::Y => vk::VK_Y as u32,
        Key::Z => vk::VK_Z as u32,
        Key::Num0 => vk::VK_0 as u32,
        Key::Num1 => vk::VK_1 as u32,
        Key::Num2 => vk::VK_2 as u32,
        Key::Num3 => vk::VK_3 as u32,
        Key::Num4 => vk::VK_4 as u32,
        Key::Num5 => vk::VK_5 as u32,
        Key::Num6 => vk::VK_6 as u32,
        Key::Num7 => vk::VK_7 as u32,
        Key::Num8 => vk::VK_8 as u32,
        Key::Num9 => vk::VK_9 as u32,
        Key::F1 => vk::VK_F1 as u32,
        Key::F2 => vk::VK_F2 as u32,
        Key::F3 => vk::VK_F3 as u32,
        Key::F4 => vk::VK_F4 as u32,
        Key::F5 => vk::VK_F5 as u32,
        Key::F6 => vk::VK_F6 as u32,
        Key::F7 => vk::VK_F7 as u32,
        Key::F8 => vk::VK_F8 as u32,
        Key::F9 => vk::VK_F9 as u32,
        Key::F10 => vk::VK_F10 as u32,
        Key::F11 => vk::VK_F11 as u32,
        Key::F12 => vk::VK_F12 as u32,
        Key::Escape => vk::VK_ESCAPE as u32,
        Key::Tab => vk::VK_TAB as u32,
        Key::Backspace => vk::VK_BACK as u32,
        Key::Enter => vk::VK_RETURN as u32,
        Key::Space => vk::VK_SPACE as u32,
        Key::Insert => vk::VK_INSERT as u32,
        Key::Delete => vk::VK_DELETE as u32,
        Key::Home => vk::VK_HOME as u32,
        Key::End => vk::VK_END as u32,
        Key::PageUp => vk::VK_PRIOR as u32,
        Key::PageDown => vk::VK_NEXT as u32,
        Key::ArrowUp => vk::VK_UP as u32,
        Key::ArrowDown => vk::VK_DOWN as u32,
        Key::ArrowLeft => vk::VK_LEFT as u32,
        Key::ArrowRight => vk::VK_RIGHT as u32,
        _ => return None,
    };
    Some(vk)
}

pub fn vk_label(vk_code: u32) -> Option<&'static str> {
    Some(match vk_code {
        x if x == vk::VK_SNAPSHOT as u32 => "Prnt Scrn",
        x if x == vk::VK_ESCAPE as u32 => "Esc",
        x if x == vk::VK_BACK as u32 => "Backspace",
        x if x == vk::VK_RETURN as u32 => "Enter",
        x if x == vk::VK_SPACE as u32 => "Space",
        x if x == vk::VK_TAB as u32 => "Tab",
        x if x == vk::VK_UP as u32 => "Up",
        x if x == vk::VK_DOWN as u32 => "Down",
        x if x == vk::VK_LEFT as u32 => "Left",
        x if x == vk::VK_RIGHT as u32 => "Right",
        x if x == vk::VK_PRIOR as u32 => "Page Up",
        x if x == vk::VK_NEXT as u32 => "Page Down",
        x if x == vk::VK_0 as u32 => "0",
        x if x == vk::VK_1 as u32 => "1",
        x if x == vk::VK_2 as u32 => "2",
        x if x == vk::VK_3 as u32 => "3",
        x if x == vk::VK_4 as u32 => "4",
        x if x == vk::VK_5 as u32 => "5",
        x if x == vk::VK_6 as u32 => "6",
        x if x == vk::VK_7 as u32 => "7",
        x if x == vk::VK_8 as u32 => "8",
        x if x == vk::VK_9 as u32 => "9",
        x if x == vk::VK_F1 as u32 => "F1",
        x if x == vk::VK_F2 as u32 => "F2",
        x if x == vk::VK_F3 as u32 => "F3",
        x if x == vk::VK_F4 as u32 => "F4",
        x if x == vk::VK_F5 as u32 => "F5",
        x if x == vk::VK_F6 as u32 => "F6",
        x if x == vk::VK_F7 as u32 => "F7",
        x if x == vk::VK_F8 as u32 => "F8",
        x if x == vk::VK_F9 as u32 => "F9",
        x if x == vk::VK_F10 as u32 => "F10",
        x if x == vk::VK_F11 as u32 => "F11",
        x if x == vk::VK_F12 as u32 => "F12",
        x if x == vk::VK_A as u32 => "A",
        x if x == vk::VK_B as u32 => "B",
        x if x == vk::VK_C as u32 => "C",
        x if x == vk::VK_D as u32 => "D",
        x if x == vk::VK_E as u32 => "E",
        x if x == vk::VK_F as u32 => "F",
        x if x == vk::VK_G as u32 => "G",
        x if x == vk::VK_H as u32 => "H",
        x if x == vk::VK_I as u32 => "I",
        x if x == vk::VK_J as u32 => "J",
        x if x == vk::VK_K as u32 => "K",
        x if x == vk::VK_L as u32 => "L",
        x if x == vk::VK_M as u32 => "M",
        x if x == vk::VK_N as u32 => "N",
        x if x == vk::VK_O as u32 => "O",
        x if x == vk::VK_P as u32 => "P",
        x if x == vk::VK_Q as u32 => "Q",
        x if x == vk::VK_R as u32 => "R",
        x if x == vk::VK_S as u32 => "S",
        x if x == vk::VK_T as u32 => "T",
        x if x == vk::VK_U as u32 => "U",
        x if x == vk::VK_V as u32 => "V",
        x if x == vk::VK_W as u32 => "W",
        x if x == vk::VK_X as u32 => "X",
        x if x == vk::VK_Y as u32 => "Y",
        x if x == vk::VK_Z as u32 => "Z",
        _ => return None,
    })
}

pub fn parse_hotkey_combo(combo: &str) -> Option<HotkeyBinding> {
    let mut mods = egui::Modifiers::NONE;
    let mut key: Option<u32> = None;
    for raw in combo.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let normalized = token.replace(' ', "").to_ascii_uppercase();
        match normalized.as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "SHIFT" => mods.shift = true,
            "ALT" => mods.alt = true,
            "WIN" | "WINDOWS" => mods.command = true,
            "PRNTSCRN" | "PRNTSCR" | "PRINTSCREEN" | "PRINTSCR" | "PRINTSCRN" => {
                key = Some(vk::VK_SNAPSHOT as u32);
            }
            _ => {
                if let Some(vk_code) = token_to_vk(&normalized) {
                    key = Some(vk_code);
                }
            }
        }
    }
    key.map(|key| HotkeyBinding { key: Some(key), modifiers: mods })
}

fn token_to_vk(token: &str) -> Option<u32> {
    if token.len() == 1 {
        let c = token.chars().next()?;
        if ('A'..='Z').contains(&c) {
            return Some((vk::VK_A as u32) + (c as u32 - 'A' as u32));
        }
        if ('0'..='9').contains(&c) {
            return Some((vk::VK_0 as u32) + (c as u32 - '0' as u32));
        }
    }
    if token.starts_with('F') {
        if let Ok(num) = token[1..].parse::<u32>() {
            if (1..=12).contains(&num) {
                return Some((vk::VK_F1 as u32) + (num - 1));
            }
        }
    }
    match token {
        "ESC" | "ESCAPE" => Some(vk::VK_ESCAPE as u32),
        "TAB" => Some(vk::VK_TAB as u32),
        "BACKSPACE" => Some(vk::VK_BACK as u32),
        "ENTER" | "RETURN" => Some(vk::VK_RETURN as u32),
        "SPACE" => Some(vk::VK_SPACE as u32),
        "INSERT" => Some(vk::VK_INSERT as u32),
        "DELETE" => Some(vk::VK_DELETE as u32),
        "HOME" => Some(vk::VK_HOME as u32),
        "END" => Some(vk::VK_END as u32),
        "PAGEUP" => Some(vk::VK_PRIOR as u32),
        "PAGEDOWN" => Some(vk::VK_NEXT as u32),
        "UP" => Some(vk::VK_UP as u32),
        "DOWN" => Some(vk::VK_DOWN as u32),
        "LEFT" => Some(vk::VK_LEFT as u32),
        "RIGHT" => Some(vk::VK_RIGHT as u32),
        _ => None,
    }
}
