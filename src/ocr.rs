use anyhow::{anyhow, Result};
use image::DynamicImage;

#[cfg(feature = "ocr")]
use image::imageops::FilterType;
#[cfg(feature = "ocr")]
use std::ffi::CString;
#[cfg(feature = "ocr")]
use std::sync::{LazyLock, Mutex};
#[cfg(feature = "ocr")]
use tesseract_static::tesseract_plumbing::TessBaseApi;

pub fn show_error(msg: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("OCR Error")
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(not(feature = "ocr"))]
pub fn warm_engine() {}

#[cfg(not(feature = "ocr"))]
pub fn image_to_text(_img: &DynamicImage) -> Result<String> {
    Err(anyhow!("OCR not available. Rebuild with default features."))
}

#[cfg(not(feature = "ocr"))]
pub fn image_to_speech(_img: &DynamicImage) -> Result<()> {
    Err(anyhow!(
        "Image to Speech not available. Rebuild with default features."
    ))
}

#[cfg(feature = "ocr")]
const OCR_SCALE: u32 = 3;
#[cfg(feature = "ocr")]
const DESKEW_THRESHOLD_DEG: f32 = 0.8;
#[cfg(feature = "ocr")]
static ENG_TRAINEDDATA: &[u8] = include_bytes!("../assets/eng.traineddata");

#[cfg(feature = "ocr")]
static TESSDATA_DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
    let tess_dir = std::env::temp_dir().join("lightshotv2_tessdata");
    std::fs::create_dir_all(&tess_dir).expect("Failed to create tessdata dir");
    std::fs::write(tess_dir.join("eng.traineddata"), ENG_TRAINEDDATA)
        .expect("Failed to write eng.traineddata");
    tess_dir
});

#[cfg(feature = "ocr")]
static ENGINE: LazyLock<Mutex<TessBaseApi>> = LazyLock::new(|| {
    let mut api = TessBaseApi::create();
    let datapath = CString::new(TESSDATA_DIR.to_string_lossy().as_bytes()).expect("tessdata path");
    let lang = CString::new("eng").expect("language is valid");
    api.init_2(Some(datapath.as_c_str()), Some(lang.as_c_str()))
        .expect("Tesseract init failed");
    api.set_variable(
        CString::new("tessedit_pageseg_mode")
            .expect("variable name")
            .as_c_str(),
        CString::new("6").expect("variable value").as_c_str(),
    )
    .expect("Tesseract set_variable failed");
    Mutex::new(api)
});

#[cfg(feature = "ocr")]
static TTS: LazyLock<Mutex<Option<tts::Tts>>> =
    LazyLock::new(|| Mutex::new(tts::Tts::default().ok()));

#[cfg(feature = "ocr")]
pub fn warm_engine() {
    std::thread::spawn(|| {
        LazyLock::force(&ENGINE);
    });
}

#[cfg(feature = "ocr")]
pub fn image_to_text(img: &DynamicImage) -> Result<String> {
    let gray = preprocess(img);
    let (width, height) = gray.dimensions();
    let mut api = ENGINE
        .lock()
        .map_err(|e| anyhow!("Tesseract lock poisoned: {e}"))?;
    api.set_image(gray.as_raw(), width as i32, height as i32, 1, width as i32)
        .map_err(|e| anyhow!("Tesseract set_image failed: {e:?}"))?;
    api.set_source_resolution(300);
    api.recognize()
        .map_err(|e| anyhow!("Tesseract recognize failed: {e:?}"))?;
    let raw = api
        .get_utf8_text()
        .map_err(|e| anyhow!("Tesseract get_text failed: {e:?}"))?;
    let trimmed = raw.as_ref().to_string_lossy();
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        Err(anyhow!("No text detected"))
    } else {
        Ok(fix_slashed_zero(trimmed))
    }
}

#[cfg(feature = "ocr")]
pub fn image_to_speech(img: &DynamicImage) -> Result<()> {
    let text = image_to_text(img)?;
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("No text detected"));
    }

    let mut guard = TTS.lock().map_err(|e| anyhow!("TTS lock poisoned: {e}"))?;
    let Some(engine) = guard.as_mut() else {
        return Err(anyhow!("TTS not available on this system"));
    };

    let settings = crate::config::get();
    let feat = engine.supported_features();
    if feat.voice && !settings.tts_voice.trim().is_empty() {
        apply_tts_voice(engine, settings.tts_voice.trim());
    }
    if feat.rate {
        let (min_r, max_r, norm_r) = (engine.min_rate(), engine.max_rate(), engine.normal_rate());
        let rate = (norm_r + (settings.tts_rate as f32 / 10.0) * (max_r - min_r) * 0.2)
            .clamp(min_r, max_r);
        let _ = engine.set_rate(rate);
    }
    if feat.volume {
        let (min_v, max_v) = (engine.min_volume(), engine.max_volume());
        let vol =
            (min_v + (settings.tts_volume as f32 / 100.0) * (max_v - min_v)).clamp(min_v, max_v);
        let _ = engine.set_volume(vol);
    }

    engine
        .speak(trimmed, true)
        .map_err(|e| anyhow!("TTS speak: {e:?}"))?;
    drop(guard);

    while TTS
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|t| t.is_speaking().ok()))
        .unwrap_or(false)
    {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

#[cfg(feature = "ocr")]
fn apply_tts_voice(engine: &mut tts::Tts, wanted: &str) {
    let Ok(voices) = engine.voices() else {
        return;
    };
    let Some(voice) = voices.into_iter().find(|v| v.name() == wanted) else {
        return;
    };
    let _ = engine.set_voice(&voice);
}

#[cfg(feature = "ocr")]
pub fn voices() -> Vec<String> {
    let Ok(guard) = TTS.lock() else {
        return Vec::new();
    };
    let Some(engine) = guard.as_ref() else {
        return Vec::new();
    };
    engine
        .voices()
        .map(|v| v.into_iter().map(|voice| voice.name()).collect())
        .unwrap_or_default()
}

#[cfg(feature = "ocr")]
fn preprocess(img: &DynamicImage) -> image::GrayImage {
    use imageproc::contrast;

    let gray0 = img.to_luma8();
    let binary0 = contrast::threshold(&gray0, contrast::otsu_level(&gray0));
    let img = deskew(img, detect_skew_angle(&binary0));
    let scaled = DynamicImage::ImageRgba8(image::imageops::resize(
        &img.to_rgba8(),
        img.width() * OCR_SCALE,
        img.height() * OCR_SCALE,
        FilterType::Lanczos3,
    ));
    let gray = scaled.to_luma8();
    contrast::threshold(&gray, contrast::otsu_level(&gray))
}

#[cfg(feature = "ocr")]
fn detect_skew_angle(binary: &image::GrayImage) -> f32 {
    use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

    let (w, h) = binary.dimensions();
    if w < 20 || h < 20 {
        return 0.0;
    }

    let mut best_angle = 0.0f32;
    let mut best_variance = 0.0f64;
    for deg in (-45..=45).step_by(3) {
        let rotated = rotate_about_center(
            binary,
            (deg as f32).to_radians(),
            Interpolation::Bilinear,
            image::Luma([255u8]),
        );
        let variance = row_ink_variance(&rotated);
        if variance <= best_variance {
            continue;
        }
        best_variance = variance;
        best_angle = deg as f32;
    }
    best_angle
}

#[cfg(feature = "ocr")]
fn row_ink_variance(binary: &image::GrayImage) -> f64 {
    let mut row_sums = vec![0u64; binary.height() as usize];
    for (y, row) in binary.rows().enumerate() {
        row_sums[y] = row.map(|pixel| u64::from(pixel[0])).sum();
    }
    let mean = row_sums.iter().sum::<u64>() as f64 / row_sums.len() as f64;
    row_sums
        .iter()
        .map(|&sum| {
            let delta = sum as f64 - mean;
            delta * delta
        })
        .sum::<f64>()
        / row_sums.len() as f64
}

#[cfg(feature = "ocr")]
fn deskew(img: &DynamicImage, angle_deg: f32) -> DynamicImage {
    use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

    if angle_deg.abs() < DESKEW_THRESHOLD_DEG {
        return img.clone();
    }
    DynamicImage::ImageLuma8(rotate_about_center(
        &img.to_luma8(),
        angle_deg.to_radians(),
        Interpolation::Bilinear,
        image::Luma([255u8]),
    ))
}

#[cfg(feature = "ocr")]
fn fix_slashed_zero(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let zero_like = c == '€' || c == '@';
        let prev = out
            .chars()
            .last()
            .map(|c| c.is_ascii_digit() || c == '.')
            .unwrap_or(false);
        let next = chars
            .peek()
            .map(|&c| c.is_ascii_digit() || c == '.')
            .unwrap_or(false);
        out.push(if zero_like && (prev || next) { '0' } else { c });
    }
    out
}
