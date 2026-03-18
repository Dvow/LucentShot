mod vk {
    pub const VK_SNAPSHOT: u32 = 0x2C;
    pub const VK_A: u32 = 0x41;
    pub const VK_B: u32 = 0x42;
    pub const VK_C: u32 = 0x43;
    pub const VK_D: u32 = 0x44;
    pub const VK_E: u32 = 0x45;
    pub const VK_F: u32 = 0x46;
    pub const VK_G: u32 = 0x47;
    pub const VK_H: u32 = 0x48;
    pub const VK_I: u32 = 0x49;
    pub const VK_J: u32 = 0x4A;
    pub const VK_K: u32 = 0x4B;
    pub const VK_L: u32 = 0x4C;
    pub const VK_M: u32 = 0x4D;
    pub const VK_N: u32 = 0x4E;
    pub const VK_O: u32 = 0x4F;
    pub const VK_P: u32 = 0x50;
    pub const VK_Q: u32 = 0x51;
    pub const VK_R: u32 = 0x52;
    pub const VK_S: u32 = 0x53;
    pub const VK_T: u32 = 0x54;
    pub const VK_U: u32 = 0x55;
    pub const VK_V: u32 = 0x56;
    pub const VK_W: u32 = 0x57;
    pub const VK_X: u32 = 0x58;
    pub const VK_Y: u32 = 0x59;
    pub const VK_Z: u32 = 0x5A;
    pub const VK_0: u32 = 0x30;
    pub const VK_1: u32 = 0x31;
    pub const VK_2: u32 = 0x32;
    pub const VK_3: u32 = 0x33;
    pub const VK_4: u32 = 0x34;
    pub const VK_5: u32 = 0x35;
    pub const VK_6: u32 = 0x36;
    pub const VK_7: u32 = 0x37;
    pub const VK_8: u32 = 0x38;
    pub const VK_9: u32 = 0x39;
    pub const VK_F1: u32 = 0x70;
    pub const VK_F2: u32 = 0x71;
    pub const VK_F3: u32 = 0x72;
    pub const VK_F4: u32 = 0x73;
    pub const VK_F5: u32 = 0x74;
    pub const VK_F6: u32 = 0x75;
    pub const VK_F7: u32 = 0x76;
    pub const VK_F8: u32 = 0x77;
    pub const VK_F9: u32 = 0x78;
    pub const VK_F10: u32 = 0x79;
    pub const VK_F11: u32 = 0x7A;
    pub const VK_F12: u32 = 0x7B;
    pub const VK_ESCAPE: u32 = 0x1B;
    pub const VK_TAB: u32 = 0x09;
    pub const VK_BACK: u32 = 0x08;
    pub const VK_RETURN: u32 = 0x0D;
    pub const VK_SPACE: u32 = 0x20;
    pub const VK_INSERT: u32 = 0x2D;
    pub const VK_DELETE: u32 = 0x2E;
    pub const VK_HOME: u32 = 0x24;
    pub const VK_END: u32 = 0x23;
    pub const VK_PRIOR: u32 = 0x21;
    pub const VK_NEXT: u32 = 0x22;
    pub const VK_UP: u32 = 0x26;
    pub const VK_DOWN: u32 = 0x28;
    pub const VK_LEFT: u32 = 0x25;
    pub const VK_RIGHT: u32 = 0x27;
}

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyBinding {
    pub key: Option<u32>,
    pub modifiers: egui::Modifiers,
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            key: Some(vk::VK_SNAPSHOT),
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
    pub copy_focused_window_enabled: bool,
    pub copy_focused_window_binding: HotkeyBinding,
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
            copy_focused_window_enabled: true,
            copy_focused_window_binding: HotkeyBinding {
                key: Some(vk::VK_SNAPSHOT),
                modifiers: egui::Modifiers {
                    ctrl: false,
                    shift: false,
                    alt: true,
                    command: false,
                    mac_cmd: false,
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HotkeyEvent {
    InstantSave,
    InstantUpload,
    CopyFocusedWindow,
}

#[derive(Clone)]
pub struct HotkeyHandle {
    thread_id: u32,
    config: std::sync::Arc<std::sync::Mutex<HotkeyConfig>>,
    listening: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl HotkeyHandle {
    pub fn update(&self, config: HotkeyConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        const MSG_UPDATE_CONFIG: u32 = 0x8001;
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, MSG_UPDATE_CONFIG, WPARAM(0), LPARAM(0));
        }
    }

    pub fn set_listening(&self, listen: bool) {
        self.listening.store(listen, std::sync::atomic::Ordering::SeqCst);
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        const MSG_SET_LISTENING: u32 = 0x8002;
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, MSG_SET_LISTENING, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn start_low_level_hotkey_loop(
    ctx: egui::Context,
    trigger_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    initial_config: HotkeyConfig,
    event_tx: std::sync::mpsc::Sender<HotkeyEvent>,
) -> HotkeyHandle {
    let config = std::sync::Arc::new(std::sync::Mutex::new(initial_config));
    let listening = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    use std::sync::atomic::Ordering;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    const MSG_UPDATE_CONFIG: u32 = 0x8001;
    const MSG_SET_LISTENING: u32 = 0x8002;

    let config_thread = config.clone();
    let listening_thread = listening.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        unsafe {
            let thread_id = GetCurrentThreadId();
            let _ = tx.send(thread_id);
            apply_hotkey_config(&config_thread, &listening_thread);

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
                        4 => {
                            let _ = event_tx.send(HotkeyEvent::CopyFocusedWindow);
                        }
                        _ => {}
                    }
                } else if msg.message == MSG_UPDATE_CONFIG || msg.message == MSG_SET_LISTENING {
                    apply_hotkey_config(&config_thread, &listening_thread);
                }
            }
        }
    });

    let thread_id = rx.recv().unwrap_or(0);
    HotkeyHandle {
        thread_id,
        config,
        listening,
    }
}

fn apply_hotkey_config(
    config: &std::sync::Arc<std::sync::Mutex<HotkeyConfig>>,
    listening: &std::sync::atomic::AtomicBool,
) {
    use std::sync::atomic::Ordering;
    use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;

    let Ok(cfg) = config.lock() else { return };
    let overlay_visible = listening.load(Ordering::SeqCst);

    unsafe {
        let _ = UnregisterHotKey(None, 1);
        let _ = UnregisterHotKey(None, 2);
        let _ = UnregisterHotKey(None, 3);
        let _ = UnregisterHotKey(None, 4);
    }

    if cfg.general_enabled {
        register_hotkey(1, cfg.general_binding);
    }
    if overlay_visible {
        if cfg.instant_save_enabled {
            register_hotkey(2, cfg.instant_save_binding);
        }
        if cfg.instant_upload_enabled {
            register_hotkey(3, cfg.instant_upload_binding);
        }
        if cfg.copy_focused_window_enabled {
            register_hotkey(4, cfg.copy_focused_window_binding);
        }
    }
}

fn register_hotkey(id: u32, binding: HotkeyBinding) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey,
    };

    let Some(vk) = binding.key else { return };
    let modifiers = egui_modifiers_to_win(binding.modifiers);
    unsafe {
        let _ = RegisterHotKey(None, id as i32, modifiers, vk);
    }
}

fn egui_modifiers_to_win(mods: egui::Modifiers) -> windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };

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

pub fn vk_label(vk_code: u32) -> Option<&'static str> {
    Some(match vk_code {
        x if x == vk::VK_SNAPSHOT => "Prnt Scrn",
        x if x == vk::VK_ESCAPE => "Esc",
        x if x == vk::VK_BACK => "Backspace",
        x if x == vk::VK_RETURN => "Enter",
        x if x == vk::VK_SPACE => "Space",
        x if x == vk::VK_TAB => "Tab",
        x if x == vk::VK_UP => "Up",
        x if x == vk::VK_DOWN => "Down",
        x if x == vk::VK_LEFT => "Left",
        x if x == vk::VK_RIGHT => "Right",
        x if x == vk::VK_PRIOR => "Page Up",
        x if x == vk::VK_NEXT => "Page Down",
        x if x == vk::VK_0 => "0",
        x if x == vk::VK_1 => "1",
        x if x == vk::VK_2 => "2",
        x if x == vk::VK_3 => "3",
        x if x == vk::VK_4 => "4",
        x if x == vk::VK_5 => "5",
        x if x == vk::VK_6 => "6",
        x if x == vk::VK_7 => "7",
        x if x == vk::VK_8 => "8",
        x if x == vk::VK_9 => "9",
        x if x == vk::VK_F1 => "F1",
        x if x == vk::VK_F2 => "F2",
        x if x == vk::VK_F3 => "F3",
        x if x == vk::VK_F4 => "F4",
        x if x == vk::VK_F5 => "F5",
        x if x == vk::VK_F6 => "F6",
        x if x == vk::VK_F7 => "F7",
        x if x == vk::VK_F8 => "F8",
        x if x == vk::VK_F9 => "F9",
        x if x == vk::VK_F10 => "F10",
        x if x == vk::VK_F11 => "F11",
        x if x == vk::VK_F12 => "F12",
        x if x == vk::VK_A => "A",
        x if x == vk::VK_B => "B",
        x if x == vk::VK_C => "C",
        x if x == vk::VK_D => "D",
        x if x == vk::VK_E => "E",
        x if x == vk::VK_F => "F",
        x if x == vk::VK_G => "G",
        x if x == vk::VK_H => "H",
        x if x == vk::VK_I => "I",
        x if x == vk::VK_J => "J",
        x if x == vk::VK_K => "K",
        x if x == vk::VK_L => "L",
        x if x == vk::VK_M => "M",
        x if x == vk::VK_N => "N",
        x if x == vk::VK_O => "O",
        x if x == vk::VK_P => "P",
        x if x == vk::VK_Q => "Q",
        x if x == vk::VK_R => "R",
        x if x == vk::VK_S => "S",
        x if x == vk::VK_T => "T",
        x if x == vk::VK_U => "U",
        x if x == vk::VK_V => "V",
        x if x == vk::VK_W => "W",
        x if x == vk::VK_X => "X",
        x if x == vk::VK_Y => "Y",
        x if x == vk::VK_Z => "Z",
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
        let normalized = token.replace(' ', "").to_uppercase();
        match normalized.as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "SHIFT" => mods.shift = true,
            "ALT" => mods.alt = true,
            "WIN" | "WINDOWS" => mods.command = true,
            "PRNTSCRN" | "PRNTSCR" | "PRINTSCREEN" | "PRINTSCR" | "PRINTSCRN" => {
                key = Some(vk::VK_SNAPSHOT);
            }
            _ => {
                if let Some(vk_code) = token_to_vk(&normalized) {
                    key = Some(vk_code);
                }
            }
        }
    }
    key.map(|key| HotkeyBinding {
        key: Some(key),
        modifiers: mods,
    })
}

fn token_to_vk(token: &str) -> Option<u32> {
    if token.len() == 1 {
        let c = token.chars().next()?;
        if c.is_ascii_uppercase() {
            return Some(vk::VK_A + (c as u32 - 'A' as u32));
        }
        if c.is_ascii_digit() {
            return Some(vk::VK_0 + (c as u32 - '0' as u32));
        }
    }
    if let Some(rest) = token.strip_prefix('F') {
        if let Ok(num) = rest.parse::<u32>() {
            if (1..=12).contains(&num) {
                return Some(vk::VK_F1 + (num - 1));
            }
        }
    }
    match token {
        "ESC" | "ESCAPE" => Some(vk::VK_ESCAPE),
        "TAB" => Some(vk::VK_TAB),
        "BACKSPACE" => Some(vk::VK_BACK),
        "ENTER" | "RETURN" => Some(vk::VK_RETURN),
        "SPACE" => Some(vk::VK_SPACE),
        "INSERT" => Some(vk::VK_INSERT),
        "DELETE" => Some(vk::VK_DELETE),
        "HOME" => Some(vk::VK_HOME),
        "END" => Some(vk::VK_END),
        "PAGEUP" => Some(vk::VK_PRIOR),
        "PAGEDOWN" => Some(vk::VK_NEXT),
        "UP" => Some(vk::VK_UP),
        "DOWN" => Some(vk::VK_DOWN),
        "LEFT" => Some(vk::VK_LEFT),
        "RIGHT" => Some(vk::VK_RIGHT),
        _ => None,
    }
}

