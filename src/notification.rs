pub fn show(title: &str, body: &str) {
    use std::path::Path;
    use std::sync::OnceLock;
    use tauri_winrt_notification::{IconCrop, Toast};

    static ICON_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = ICON_PATH.get_or_init(|| {
        let path = std::env::temp_dir().join("lightshot_clone_icon.png");
        let _ = std::fs::write(&path, include_bytes!("../assets/icons/icon.png"));
        path
    });

    let mut toast = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body);
    if path.exists() {
        toast = toast.icon(Path::new(path), IconCrop::Circular, "Lightshot Clone");
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
