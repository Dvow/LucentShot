use std::path::PathBuf;
use std::sync::OnceLock;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

pub fn init() {
    let icon = icon_path();
    register_aumid(&icon);
    let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(crate::paths::AUMID)) };
}

pub fn show(title: &str, body: &str) {
    use tauri_winrt_notification::{IconCrop, Toast};

    static READY: OnceLock<()> = OnceLock::new();
    READY.get_or_init(init);

    let path = icon_path();
    let mut toast = Toast::new(crate::paths::AUMID).title(title).text1(body);
    if path.exists() {
        toast = toast.icon(path.as_path(), IconCrop::Circular, crate::paths::APP_NAME);
    }
    if let Err(err) = toast.show() {
        eprintln!("Notification failed: {err}");
    }
}

pub fn maybe(enabled: bool, title: &str, body: &str) {
    if enabled {
        show(title, body);
    }
}

fn icon_path() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = crate::paths::data_dir().join("icon.png");
        let _ = crate::app_icon().save(&path);
        path
    })
    .clone()
}

fn register_aumid(icon: &std::path::Path) {
    let subkey: Vec<u16> = format!(
        "Software\\Classes\\AppUserModelId\\{}",
        crate::paths::AUMID
    )
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    if status.is_err() {
        return;
    }
    set_sz(key, w!("DisplayName"), crate::paths::APP_NAME);
    if let Some(icon) = icon.to_str() {
        set_sz(key, w!("IconUri"), icon);
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
}

fn set_sz(key: HKEY, name: PCWSTR, value: &str) {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    let _ = unsafe { RegSetValueExW(key, name, 0, REG_SZ, Some(bytes)) };
}
