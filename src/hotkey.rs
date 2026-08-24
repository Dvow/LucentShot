use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VIRTUAL_KEY, VK_0, VK_9, VK_A, VK_BACK, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F12, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY};

const MSG_UPDATE_CONFIG: u32 = 0x8001;
const MSG_SET_LISTENING: u32 = 0x8002;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyBinding {
    pub key: Option<u32>,
    pub modifiers: egui::Modifiers,
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

impl HotkeyConfig {
    pub fn from_settings(config: &crate::config::Config) -> Self {
        fn snap(ctrl: bool, shift: bool, alt: bool) -> HotkeyBinding {
            HotkeyBinding {
                key: Some(VK_SNAPSHOT.0 as u32),
                modifiers: egui::Modifiers {
                    ctrl,
                    shift,
                    alt,
                    command: false,
                    mac_cmd: false,
                },
            }
        }

        Self {
            general_enabled: config.hotkey_general_enabled,
            general_binding: HotkeyBinding {
                key: config.hotkey_general_key,
                modifiers: egui::Modifiers {
                    ctrl: config.hotkey_general_ctrl,
                    shift: config.hotkey_general_shift,
                    alt: config.hotkey_general_alt,
                    command: config.hotkey_general_win,
                    mac_cmd: false,
                },
            },
            instant_save_enabled: config.hotkey_instant_save_fullscreen,
            instant_save_binding: parse_combo(&config.hotkey_instant_save_combo)
                .unwrap_or(snap(false, true, false)),
            instant_upload_enabled: config.hotkey_instant_upload_fullscreen,
            instant_upload_binding: parse_combo(&config.hotkey_instant_upload_combo)
                .unwrap_or(snap(true, false, false)),
            copy_focused_window_enabled: config.hotkey_copy_focused_window,
            copy_focused_window_binding: parse_combo(&config.hotkey_copy_focused_window_combo)
                .unwrap_or(snap(false, false, true)),
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
    config: Arc<Mutex<HotkeyConfig>>,
    listening: Arc<AtomicBool>,
}

impl HotkeyHandle {
    pub fn update(&self, config: HotkeyConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
        self.post(MSG_UPDATE_CONFIG);
    }

    pub fn set_listening(&self, listen: bool) {
        self.listening.store(listen, Ordering::SeqCst);
        self.post(MSG_SET_LISTENING);
    }

    fn post(&self, message: u32) {
        if self.thread_id == 0 {
            return;
        }
        // SAFETY: thread_id is the hotkey loop thread; posting a custom message is fire-and-forget.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, message, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn start_listener(
    ctx: egui::Context,
    trigger_flag: Arc<AtomicBool>,
    initial_config: HotkeyConfig,
    event_tx: mpsc::Sender<HotkeyEvent>,
) -> HotkeyHandle {
    use windows::Win32::System::Threading::GetCurrentThreadId;

    let config = Arc::new(Mutex::new(initial_config));
    let listening = Arc::new(AtomicBool::new(false));
    let config_thread = Arc::clone(&config);
    let listening_thread = Arc::clone(&listening);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        // SAFETY: GetCurrentThreadId and GetMessageW run on this dedicated thread.
        unsafe {
            let _ = tx.send(GetCurrentThreadId());
            apply_hotkey_config(&config_thread, &listening_thread);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                if msg.message == WM_HOTKEY {
                    match msg.wParam.0 as u32 {
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

    HotkeyHandle {
        thread_id: rx.recv().unwrap_or(0),
        config,
        listening,
    }
}

fn apply_hotkey_config(config: &Mutex<HotkeyConfig>, listening: &AtomicBool) {
    let Ok(cfg) = config.lock() else {
        return;
    };
    let overlay_visible = listening.load(Ordering::SeqCst);

    // SAFETY: UnregisterHotKey/RegisterHotKey on this thread's previous registrations.
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

fn register_hotkey(id: i32, binding: HotkeyBinding) {
    let Some(vk) = binding.key else {
        return;
    };
    // SAFETY: RegisterHotKey accepts any virtual-key and modifier combination.
    unsafe {
        let _ = RegisterHotKey(None, id, modifiers_to_win(binding.modifiers), vk);
    }
}

fn modifiers_to_win(mods: egui::Modifiers) -> HOT_KEY_MODIFIERS {
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

pub fn vk_label(vk_code: u32) -> Option<String> {
    Some(match vk_code {
        x if x == VK_SNAPSHOT.0 as u32 => "Prnt Scrn".into(),
        x if x == VK_ESCAPE.0 as u32 => "Esc".into(),
        x if x == VK_BACK.0 as u32 => "Backspace".into(),
        x if x == VK_RETURN.0 as u32 => "Enter".into(),
        x if x == VK_SPACE.0 as u32 => "Space".into(),
        x if x == VK_TAB.0 as u32 => "Tab".into(),
        x if x == VK_UP.0 as u32 => "Up".into(),
        x if x == VK_DOWN.0 as u32 => "Down".into(),
        x if x == VK_LEFT.0 as u32 => "Left".into(),
        x if x == VK_RIGHT.0 as u32 => "Right".into(),
        x if x == VK_PRIOR.0 as u32 => "Page Up".into(),
        x if x == VK_NEXT.0 as u32 => "Page Down".into(),
        x if x == VK_INSERT.0 as u32 => "Insert".into(),
        x if x == VK_DELETE.0 as u32 => "Delete".into(),
        x if x == VK_HOME.0 as u32 => "Home".into(),
        x if x == VK_END.0 as u32 => "End".into(),
        x if (VK_0.0 as u32..=VK_9.0 as u32).contains(&x) => {
            ((b'0' + (x - VK_0.0 as u32) as u8) as char).to_string()
        }
        x if (VK_A.0 as u32..=VK_Z.0 as u32).contains(&x) => {
            ((b'A' + (x - VK_A.0 as u32) as u8) as char).to_string()
        }
        x if (VK_F1.0 as u32..=VK_F12.0 as u32).contains(&x) => {
            format!("F{}", x - VK_F1.0 as u32 + 1)
        }
        _ => return None,
    })
}

pub fn parse_combo(combo: &str) -> Option<HotkeyBinding> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
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
                key = Some(VK_SNAPSHOT.0 as u32);
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
            return Some(VK_A.0 as u32 + (c as u32 - u32::from(b'A')));
        }
        if c.is_ascii_digit() {
            return Some(VK_0.0 as u32 + (c as u32 - u32::from(b'0')));
        }
    }
    if let Some(rest) = token.strip_prefix('F') {
        let Ok(num) = rest.parse::<u32>() else {
            return None;
        };
        if (1..=12).contains(&num) {
            return Some(VK_F1.0 as u32 + (num - 1));
        }
    }
    Some(match token {
        "ESC" | "ESCAPE" => VK_ESCAPE.0 as u32,
        "TAB" => VK_TAB.0 as u32,
        "BACKSPACE" => VK_BACK.0 as u32,
        "ENTER" | "RETURN" => VK_RETURN.0 as u32,
        "SPACE" => VK_SPACE.0 as u32,
        "INSERT" => VK_INSERT.0 as u32,
        "DELETE" => VK_DELETE.0 as u32,
        "HOME" => VK_HOME.0 as u32,
        "END" => VK_END.0 as u32,
        "PAGEUP" => VK_PRIOR.0 as u32,
        "PAGEDOWN" => VK_NEXT.0 as u32,
        "UP" => VK_UP.0 as u32,
        "DOWN" => VK_DOWN.0 as u32,
        "LEFT" => VK_LEFT.0 as u32,
        "RIGHT" => VK_RIGHT.0 as u32,
        _ => return None,
    })
}

pub fn format_binding(key: Option<u32>, ctrl: bool, shift: bool, alt: bool, win: bool) -> String {
    let Some(key) = key else {
        return "Unassigned".to_string();
    };
    let Some(label) = vk_label(key) else {
        return format!("VK_{key}");
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
    parts.push(label.as_str());
    parts.join(" + ")
}

pub fn assignment_key_down() -> Option<u32> {
    const SPECIAL: [VIRTUAL_KEY; 16] = [
        VK_SNAPSHOT,
        VK_BACK,
        VK_DELETE,
        VK_ESCAPE,
        VK_TAB,
        VK_RETURN,
        VK_SPACE,
        VK_INSERT,
        VK_HOME,
        VK_END,
        VK_PRIOR,
        VK_NEXT,
        VK_UP,
        VK_DOWN,
        VK_LEFT,
        VK_RIGHT,
    ];
    SPECIAL
        .into_iter()
        .map(|vk| vk.0 as u32)
        .chain(VK_0.0 as u32..=VK_9.0 as u32)
        .chain(VK_A.0 as u32..=VK_Z.0 as u32)
        .chain(VK_F1.0 as u32..=VK_F12.0 as u32)
        .find(|&vk| key_is_down(vk))
}

pub fn modifiers_down() -> (bool, bool, bool, bool) {
    (
        key_is_down(VK_CONTROL.0 as u32)
            || key_is_down(VK_LCONTROL.0 as u32)
            || key_is_down(VK_RCONTROL.0 as u32),
        key_is_down(VK_SHIFT.0 as u32)
            || key_is_down(VK_LSHIFT.0 as u32)
            || key_is_down(VK_RSHIFT.0 as u32),
        key_is_down(VK_MENU.0 as u32)
            || key_is_down(VK_LMENU.0 as u32)
            || key_is_down(VK_RMENU.0 as u32),
        key_is_down(VK_LWIN.0 as u32) || key_is_down(VK_RWIN.0 as u32),
    )
}

pub fn is_clear_key(vk: u32) -> bool {
    vk == VK_BACK.0 as u32 || vk == VK_DELETE.0 as u32
}

pub fn is_escape_key(vk: u32) -> bool {
    vk == VK_ESCAPE.0 as u32
}

fn key_is_down(vk: u32) -> bool {
    // SAFETY: GetAsyncKeyState is valid for any virtual-key code.
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_combo_reads_modifiers_and_print_screen() {
        let binding = parse_combo("Ctrl + Shift + Prnt Scrn").expect("combo");
        assert_eq!(binding.key, Some(VK_SNAPSHOT.0 as u32));
        assert!(binding.modifiers.ctrl);
        assert!(binding.modifiers.shift);
        assert!(!binding.modifiers.alt);
    }

    #[test]
    fn parse_combo_rejects_empty_and_unknown_tokens() {
        assert!(parse_combo("").is_none());
        assert!(parse_combo("Ctrl +").is_none());
        assert!(parse_combo("Ctrl + NotAKey").is_none());
    }

    #[test]
    fn format_binding_shows_unassigned_without_a_key() {
        assert_eq!(
            format_binding(None, true, false, false, false),
            "Unassigned"
        );
        assert_eq!(
            format_binding(Some(VK_SNAPSHOT.0 as u32), false, true, false, false),
            "Shift + Prnt Scrn"
        );
    }
}
