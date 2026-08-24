pub fn show(title: &str, body: &str) {
    use std::sync::OnceLock;
    use tauri_winrt_notification::{IconCrop, Toast};

    static ICON_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = ICON_PATH.get_or_init(|| {
        let path = crate::paths::data_dir().join("icon.png");
        let _ = crate::app_icon().save(&path);
        path
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
