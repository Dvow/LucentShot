use std::path::PathBuf;
use std::sync::OnceLock;

pub const APP_NAME: &str = "LucentShot";
pub const AUMID: &str = "Dvow.LucentShot";

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = dirs::data_local_dir()
            .unwrap_or_else(exe_dir)
            .join(APP_NAME);
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

pub fn config_file() -> PathBuf {
    data_dir().join("config.json")
}

pub fn config_bootstrap() -> PathBuf {
    data_dir().join("config_path.txt")
}

pub fn cache_dir() -> PathBuf {
    let dir = data_dir().join("cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(feature = "ocr")]
pub fn tessdata_dir() -> PathBuf {
    exe_dir().join("tessdata")
}

#[cfg(feature = "ocr")]
pub fn tesseract_dll() -> PathBuf {
    exe_dir().join("tesseract.dll")
}

#[cfg(feature = "ocr")]
pub fn leptonica_dll() -> PathBuf {
    exe_dir().join("leptonica-1.85.0.dll")
}
