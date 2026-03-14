//! Windows toast notifications via tauri-winrt-notification.
//! Uses PowerShell AppUserModelID for unpackaged apps (toast header shows "PowerShell").

pub fn show(title: &str, body: &str) {
    use std::path::Path;
    use std::sync::OnceLock;
    use tauri_winrt_notification::{Toast, IconCrop};

    static ICON_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = ICON_PATH.get_or_init(|| {
        let png = include_bytes!("icon/icon.png");
        let path = std::env::temp_dir().join("lightshot_clone_icon.png");
        let _ = std::fs::write(&path, png);
        path
    });

    let mut toast = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body);
    if path.exists() {
        toast = toast.icon(Path::new(path), IconCrop::Circular, "Lightshot Clone");
    }
    if let Err(e) = toast.show() {
        eprintln!("Notification failed: {e}");
    }
}
