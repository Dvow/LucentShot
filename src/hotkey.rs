use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_NOREPEAT};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
use eframe::egui;

pub fn start_low_level_hotkey_loop(ctx: egui::Context, trigger_flag: Arc<AtomicBool>) {
    thread::spawn(move || {
        unsafe {
            // Register PrtSc globally (Key code 0x2C)
            // No modifiers required
            if RegisterHotKey(None, 1, MOD_NOREPEAT, 0x2C).is_err() {
                eprintln!("Failed to register low-level hotkey");
                return;
            }

            let mut msg = MSG::default();
            // NATIVE MESSAGE PUMP - 0 LATENCY
            while GetMessageW(&mut msg, None, 0, 0).into() {
                if msg.message == WM_HOTKEY {
                    trigger_flag.store(true, Ordering::SeqCst);
                    ctx.request_repaint(); // Signal GUI instantly
                }
            }
        }
    });
}
