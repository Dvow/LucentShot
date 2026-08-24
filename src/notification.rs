pub fn show(title: &str, body: &str) {
    use std::sync::OnceLock;
    use tauri_winrt_notification::{IconCrop, Toast};

    static ICON_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = ICON_PATH.get_or_init(|| {
        let next_to_exe = crate::paths::icon_file();
        if next_to_exe.exists() {
            return next_to_exe;
        }
        let fallback = crate::paths::data_dir().join("icon.png");
        let _ = std::fs::write(&fallback, include_bytes!("../assets/icons/icon.png"));
        fallback
    });

    let mut toast = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body);
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
