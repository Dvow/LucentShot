use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("LucentShot");

pub fn apply(enabled: bool) {
    if enabled {
        enable();
    } else {
        disable();
    }
}

fn enable() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let command = format!("\"{}\"", exe.display());
    let Some(key) = open_run_key() else {
        return;
    };
    set_sz(key, VALUE_NAME, &command);
    unsafe {
        let _ = RegCloseKey(key);
    }
}

fn disable() {
    let Some(key) = open_run_key() else {
        return;
    };
    unsafe {
        let _ = RegDeleteValueW(key, VALUE_NAME);
        let _ = RegCloseKey(key);
    }
}

fn open_run_key() -> Option<HKEY> {
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    status.is_ok().then_some(key)
}

fn set_sz(key: HKEY, name: PCWSTR, value: &str) {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    let _ = unsafe { RegSetValueExW(key, name, 0, REG_SZ, Some(bytes)) };
}
