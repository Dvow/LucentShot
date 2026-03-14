use arboard::{Clipboard, ImageData};
use image::{DynamicImage, imageops::FilterType};
use image::codecs::jpeg::JpegEncoder;
use anyhow::{Result, anyhow};
use std::borrow::Cow;
use std::io::Cursor;
use std::fs;
use serde_json::json;

pub enum CopyResult {
    Image,
}

pub fn copy_to_clipboard(
    img: &DynamicImage,
    _format: crate::config::ImageFormat,
    _jpeg_quality: u8,
) -> Result<CopyResult> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut clipboard = Clipboard::new().map_err(|e| anyhow!("Clipboard: {}", e))?;
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(rgba.into_raw()),
        })
        .map_err(|e| anyhow!("Clipboard set_image: {}", e))?;
    Ok(CopyResult::Image)
}

pub fn set_clipboard_text(text: &str) -> Result<()> {
    Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_owned()))
        .map_err(|e| anyhow!("Clipboard: {}", e))
}

pub fn save_to_file(img: &DynamicImage) -> Result<bool> {
    let config = crate::config::cfg();
    let (ext, filter_label) = format_extension_and_label(config.format);
    let start_dir = dirs::picture_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut i = 1;
    let mut file_name = format!("screenshot_{}.{}", i, ext);
    while start_dir.join(&file_name).exists() {
        i += 1;
        file_name = format!("screenshot_{}.{}", i, ext);
    }

    if let Some(path) = rfd::FileDialog::new()
        .add_filter(filter_label, &[ext])
        .set_directory(&start_dir)
        .set_file_name(&file_name)
        .save_file()
    {
        let path = if path.extension().is_some() { path } else { path.with_extension(ext) };
        if let Err(e) = save_image_with_config(img, &path, config.format, config.jpeg_quality) {
            eprintln!("Failed to save image: {}", e);
        }
        return Ok(true);
    }
    Ok(false)
}

/// Pick a path to save a file (for config, etc). Returns None if user cancels.
pub fn pick_save_path(default_name: &str, filter_label: &str, ext: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter(filter_label, &[ext])
        .set_file_name(default_name)
        .save_file()
}

pub fn open_url(url: &str) -> Result<()> {
    webbrowser::open(url).map_err(|e| anyhow!("Failed to open URL: {}", e))
}

static REQWEST_CLIENT: once_cell::sync::Lazy<reqwest::blocking::Client> =
    once_cell::sync::Lazy::new(|| {
        reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .expect("reqwest client")
    });

fn upload_to_anonymous_host(img: &DynamicImage) -> Result<String> {
    let config = crate::config::cfg();
    let (bytes, ext, mime) = encode_image_for_upload(img, config.format, config.jpeg_quality)?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part("fileToUpload", reqwest::blocking::multipart::Part::bytes(bytes)
            .file_name(format!("screenshot.{}", ext))
            .mime_str(mime)?);

    let resp = REQWEST_CLIENT.post("https://catbox.moe/user/api.php")
        .multipart(form)
        .send()?;

    if !resp.status().is_success() {
        let err_body = resp.text().unwrap_or_else(|e| format!("Failed to read error body: {}", e));
        return Err(anyhow!("Host failed: {}", err_body));
    }

    Ok(resp.text()?)
}

pub fn google_search(img: &DynamicImage) -> Result<()> {
    println!("Uploading for Google Search...");
    let direct_url = upload_to_anonymous_host(img)?;
    println!("Image hosted at: {}", direct_url);

    let search_url = format!("https://lens.google.com/uploadbyurl?url={}", direct_url);
    open_url(&search_url)?;
    Ok(())
}

/// Show error to user via native message dialog.
pub fn show_ocr_error(msg: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("OCR Error")
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

pub fn image_to_text(img: &DynamicImage) -> Result<String> {
    image_to_text_tesseract(img)
}

const OCR_SCALE: u32 = 3;

/// Minimum skew angle (degrees) to trigger deskew — avoids rotating for nearly-straight text.
const DESKEW_THRESHOLD_DEG: f32 = 0.8;

/// Detect skew angle using horizontal projection variance — when text lines are horizontal,
/// row sums have high variance; when skewed, variance drops. Returns angle in degrees
/// (positive = clockwise correction needed).
fn detect_skew_angle(binary: &image::GrayImage) -> f32 {
    use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

    let (w, h) = binary.dimensions();
    if w < 20 || h < 20 {
        return 0.0;
    }

    // Sample angles from -45° to +45° in 3° steps (handles significantly tilted text)
    let mut best_angle = 0.0f32;
    let mut best_variance = 0.0f64;

    for deg in (-45..=45).step_by(3) {
        let theta = (deg as f32).to_radians();
        let rotated = rotate_about_center(
            binary,
            theta,
            Interpolation::Bilinear,
            image::Luma([255u8]), // white fill for background
        );

        // Horizontal projection: sum of pixels per row (0 = black, 255 = white; we want low where text)
        let mut row_sums: Vec<u64> = vec![0; rotated.height() as usize];
        for y in 0..rotated.height() {
            for x in 0..rotated.width() {
                let p = rotated.get_pixel(x, y)[0];
                row_sums[y as usize] += p as u64;
            }
        }

        // Variance of row sums — maximized when text lines are horizontal
        let mean = row_sums.iter().sum::<u64>() as f64 / row_sums.len() as f64;
        let variance = row_sums
            .iter()
            .map(|&s| {
                let d = s as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / row_sums.len() as f64;

        if variance > best_variance {
            best_variance = variance;
            best_angle = deg as f32;
        }
    }

    best_angle
}

/// Deskew image by rotating to correct for detected skew.
fn deskew_image(img: &DynamicImage, angle_deg: f32) -> DynamicImage {
    use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

    if angle_deg.abs() < DESKEW_THRESHOLD_DEG {
        return img.clone();
    }

    let gray = img.to_luma8();
    let theta = angle_deg.to_radians();
    let rotated = rotate_about_center(
        &gray,
        theta,
        Interpolation::Bilinear,
        image::Luma([255u8]),
    );
    DynamicImage::ImageLuma8(rotated)
}

/// Preprocess image for OCR: deskew, 3x upscale, grayscale, binarize (Otsu).
fn preprocess_for_ocr(img: &DynamicImage) -> image::GrayImage {
    use imageproc::contrast;

    // 0. Deskew: detect and correct rotation so Tesseract sees horizontal text
    let gray0 = img.to_luma8();
    let level0 = contrast::otsu_level(&gray0);
    let binary0 = contrast::threshold(&gray0, level0);
    let skew_angle = detect_skew_angle(&binary0);
    let img = deskew_image(img, skew_angle);

    // 1. Always 3x upscale
    let w = img.width();
    let h = img.height();
    let new_w = w * OCR_SCALE;
    let new_h = h * OCR_SCALE;
    let img = DynamicImage::ImageRgba8(image::imageops::resize(
        &img.to_rgba8(),
        new_w,
        new_h,
        FilterType::Lanczos3,
    ));

    // 2. Convert to grayscale
    let gray = img.to_luma8();

    // 3. Binarize with Otsu threshold (black text on white background)
    let level = contrast::otsu_level(&gray);
    contrast::threshold(&gray, level)
}

/// Tessdata embedded at compile time — zero disk reads for the model file at build.
static ENG_TRAINEDDATA: &[u8] = include_bytes!("../tessdata/eng.traineddata");

/// Tessdata path — extracted once to temp at first use.
static TESSDATA_DIR: once_cell::sync::Lazy<std::path::PathBuf> =
    once_cell::sync::Lazy::new(|| {
        let tess_dir = std::env::temp_dir().join("lightshotv2_tessdata");
        std::fs::create_dir_all(&tess_dir).expect("Failed to create tessdata dir");
        std::fs::write(tess_dir.join("eng.traineddata"), ENG_TRAINEDDATA)
            .expect("Failed to write eng.traineddata");
        tess_dir
    });

/// Reusable Tesseract instance — initialized once, reused for all OCR calls (30–80ms per image).
use tesseract_static::tesseract_plumbing::TessBaseApi;
use std::ffi::CString;

static TESS_ENGINE: once_cell::sync::Lazy<std::sync::Mutex<TessBaseApi>> =
    once_cell::sync::Lazy::new(|| {
        let mut api = TessBaseApi::create();
        let datapath = CString::new(TESSDATA_DIR.to_string_lossy().as_bytes()).unwrap();
        let lang = CString::new("eng").unwrap();
        api.init_2(Some(datapath.as_c_str()), Some(lang.as_c_str()))
            .expect("Tesseract init failed");
        let ps_mode = CString::new("tessedit_pageseg_mode").unwrap();
        let ps_val = CString::new("6").unwrap(); // single uniform block
        api.set_variable(ps_mode.as_c_str(), ps_val.as_c_str())
            .expect("Tesseract set_variable failed");
        std::sync::Mutex::new(api)
    });

/// Pre-warms the engine at startup — forces TESS_ENGINE init in background.
pub fn warm_ocr_engine() {
    std::thread::spawn(|| {
        once_cell::sync::Lazy::force(&TESS_ENGINE);
    });
}

/// OCR using the shared TessBaseApi — ~30–80ms per image (model stays in memory).
fn image_to_text_tesseract(img: &DynamicImage) -> Result<String> {
    let gray = preprocess_for_ocr(img);
    let (width, height) = gray.dimensions();
    let frame_data = gray.as_raw();

    let mut api = TESS_ENGINE
        .lock()
        .map_err(|e| anyhow!("Tesseract lock poisoned: {}", e))?;

    api.set_image(
        frame_data,
        width as i32,
        height as i32,
        1,            // bytes_per_pixel (grayscale)
        width as i32, // bytes_per_line
    )
    .map_err(|e| anyhow!("Tesseract set_image failed: {:?}", e))?;
    api.set_source_resolution(300); // 300 DPI for printed text
    api.recognize().map_err(|e| anyhow!("Tesseract recognize failed: {:?}", e))?;
    let raw = api
        .get_utf8_text()
        .map_err(|e| anyhow!("Tesseract get_text failed: {:?}", e))?;
    let text = raw.as_ref().to_string_lossy().into_owned();

    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err(anyhow!("No text detected"))
    } else {
        Ok(fix_ocr_slashed_zero(&trimmed))
    }
}

/// Fix slashed zero misreads: replace € or @ with 0 when they appear in a numeric context.
fn fix_ocr_slashed_zero(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let is_zero_like = c == '€' || c == '@';
        let prev_digit_or_dot = out.chars().last().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false);
        let next_digit_or_dot = chars.peek().map(|&c| c.is_ascii_digit() || c == '.').unwrap_or(false);
        if is_zero_like && (prev_digit_or_dot || next_digit_or_dot) {
            out.push('0');
        } else {
            out.push(c);
        }
    }
    out
}

/// Shared TTS engine — reused for all speak calls. New speak(interrupt: true) cancels previous.
static TTS_ENGINE: once_cell::sync::Lazy<std::sync::Mutex<Option<tts::Tts>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(tts::Tts::default().ok()));

pub fn image_to_speech(img: &DynamicImage) -> Result<()> {
    let text = match image_to_text(img) {
        Ok(text) => text,
        Err(err) => return Err(err),
    };
    let normalized = normalize_tts_text(&text);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("No text detected"));
    }

    let mut guard = TTS_ENGINE.lock().map_err(|e| anyhow!("TTS lock poisoned: {}", e))?;
    let Some(ref mut tts) = *guard else {
        return Err(anyhow!("TTS not available on this system"));
    };

    let settings = crate::config::cfg();
    let feat = tts.supported_features();

    if feat.voice {
        let voice_name = settings.tts_voice.trim();
        if !voice_name.is_empty() {
            if let Ok(voices) = tts.voices() {
                if let Some(voice) = voices.into_iter().find(|v| v.name() == voice_name) {
                    let _ = tts.set_voice(&voice);
                }
            }
        }
    }
    if feat.rate {
        let min_r = tts.min_rate();
        let max_r = tts.max_rate();
        let norm_r = tts.normal_rate();
        let rate = norm_r + (settings.tts_rate as f32 / 10.0) * (max_r - min_r) * 0.2;
        let rate = rate.clamp(min_r, max_r);
        let _ = tts.set_rate(rate);
    }
    if feat.volume {
        let min_v = tts.min_volume();
        let max_v = tts.max_volume();
        let vol = min_v + (settings.tts_volume as f32 / 100.0) * (max_v - min_v);
        let vol = vol.clamp(min_v, max_v);
        let _ = tts.set_volume(vol);
    }

    tts.speak(trimmed, true).map_err(|e| anyhow!("TTS speak: {:?}", e))?;
    drop(guard);

    while {
        let g = TTS_ENGINE.lock().unwrap();
        g.as_ref().and_then(|t| t.is_speaking().ok()).unwrap_or(false)
    } {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

/// Returns installed TTS voice names for the settings dropdown.
pub fn get_tts_voices() -> Vec<String> {
    let guard = match TTS_ENGINE.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    let Some(ref tts) = *guard else {
        return Vec::new();
    };
    tts.voices()
        .map(|v| v.into_iter().map(|voice| voice.name()).collect())
        .unwrap_or_default()
}

fn normalize_tts_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn prntsc_upload(img: &DynamicImage) -> Result<String> {
    println!("Uploading image...");
    let img_url = upload_to_anonymous_host(img)?;
    println!("Image host link: {}", img_url);

    println!("Registering with Lightshot API...");
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "save",
        "id": 1,
        "params": {
            "img_url": img_url,
            "app_id": "{F1F88C8C-9A9B-45E2-913F-489DF108D86F}",
            "width": img.width(),
            "height": img.height()
        }
    });

    let api_resp = REQWEST_CLIENT.post("https://api.prntscr.com/v1/")
        .json(&payload)
        .send()?;

    let api_json: serde_json::Value = api_resp.json()?;
    let prnt_url = api_json["result"]["url"].as_str()
        .ok_or_else(|| anyhow!("Lightshot API failed: {:?}", api_json))?;

    Ok(prnt_url.to_string())
}

pub fn get_printers() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        use winprint::printer::PrinterDevice;
        match PrinterDevice::all() {
            Ok(devices) => {
                let names: Vec<String> = devices.into_iter().map(|d| d.name().to_string()).collect();
                if names.is_empty() {
                    vec!["Microsoft Print to PDF".to_string()]
                } else {
                    names
                }
            }
            Err(_) => vec!["Microsoft Print to PDF".to_string()],
        }
    }
    #[cfg(not(target_os = "windows"))]
    Vec::new()
}

#[cfg(target_os = "windows")]
fn paper_size_to_predefined(paper: &str) -> Option<winprint::ticket::PredefinedMediaName> {
    match paper.trim() {
        "A4" => Some(winprint::ticket::PredefinedMediaName::ISOA4),
        "Letter" => Some(winprint::ticket::PredefinedMediaName::NorthAmericaLetter),
        "Legal" => Some(winprint::ticket::PredefinedMediaName::NorthAmericaLegal),
        _ => winprint::ticket::PredefinedMediaName::try_from(paper).ok(),
    }
}

pub fn print_image_to(
    img: &DynamicImage,
    printer_name: &str,
    copies: i32,
    landscape: bool,
    grayscale: bool,
    _fit_to_page: bool,
    paper_size: &str,
) -> Result<()> {
    let temp_path = std::env::temp_dir().join("lightshot_print.png");
    img.save(&temp_path)?;

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (printer_name, copies, landscape, grayscale, paper_size);
        return Err(anyhow!("Printing is only supported on Windows"));
    }

    #[cfg(target_os = "windows")]
    {
        use winprint::printer::{FilePrinter, ImagePrinter, PrinterDevice};
        use winprint::ticket::{Copies, PrintCapabilities, PrintTicketBuilder};
        use winprint::ticket::{FeatureOptionPack, FeatureOptionPackWithPredefined};
        use winprint::ticket::{PredefinedPageOrientation, PredefinedPageOutputColor};

        let devices = PrinterDevice::all().map_err(|e| anyhow!("List printers: {:?}", e))?;
        let device = devices
            .into_iter()
            .find(|d| d.name() == printer_name)
            .ok_or_else(|| anyhow!("Printer not found: {}", printer_name))?;

        let capabilities = PrintCapabilities::fetch(&device)
            .map_err(|e| anyhow!("Fetch capabilities: {:?}", e))?;

        let mut builder = PrintTicketBuilder::new(&device).map_err(|e| anyhow!("Print ticket: {:?}", e))?;

        builder.merge(Copies(copies.clamp(1, 9999) as u16))?;

        let orient = if landscape {
            PredefinedPageOrientation::Landscape
        } else {
            PredefinedPageOrientation::Portrait
        };
        if let Some(opt) = winprint::ticket::PageOrientation::list(&capabilities)
            .find(|o| o.as_predefined_name() == Some(orient))
        {
            let _ = builder.merge(opt);
        }

        let color = if grayscale {
            PredefinedPageOutputColor::Grayscale
        } else {
            PredefinedPageOutputColor::Color
        };
        if let Some(opt) = winprint::ticket::PageOutputColor::list(&capabilities)
            .find(|o| o.as_predefined_name() == Some(color))
        {
            let _ = builder.merge(opt);
        }

        if let Some(predef) = paper_size_to_predefined(paper_size) {
            if let Some(media) = capabilities
                .page_media_sizes()
                .find(|m| m.as_predefined_name() == Some(predef))
            {
                let _ = builder.merge(media);
            }
        }

        let ticket = builder.build().map_err(|e| anyhow!("Build ticket: {:?}", e))?;
        let printer = ImagePrinter::new(device);
        printer
            .print(&temp_path, ticket)
            .map_err(|e| anyhow!("Print: {:?}", e))?;
        Ok(())
    }
}

fn format_extension_and_label(format: crate::config::ImageFormat) -> (&'static str, &'static str) {
    match format {
        crate::config::ImageFormat::Png => ("png", "PNG"),
        crate::config::ImageFormat::Jpeg => ("jpg", "JPEG"),
        crate::config::ImageFormat::Bmp => ("bmp", "BMP"),
        crate::config::ImageFormat::Gif => ("gif", "GIF"),
    }
}

fn encode_image_for_upload(
    img: &DynamicImage,
    format: crate::config::ImageFormat,
    jpeg_quality: u8,
) -> Result<(Vec<u8>, &'static str, &'static str)> {
    let (ext, mime) = match format {
        crate::config::ImageFormat::Png => ("png", "image/png"),
        crate::config::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        crate::config::ImageFormat::Bmp => ("bmp", "image/bmp"),
        crate::config::ImageFormat::Gif => ("gif", "image/gif"),
    };
    let bytes = encode_image_bytes(img, format, jpeg_quality)?;
    Ok((bytes, ext, mime))
}

fn encode_image_bytes(
    img: &DynamicImage,
    format: crate::config::ImageFormat,
    jpeg_quality: u8,
) -> Result<Vec<u8>> {
    match format {
        crate::config::ImageFormat::Jpeg => {
            let mut buf = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut buf, jpeg_quality.clamp(1, 100));
            encoder.encode_image(img)?;
            Ok(buf)
        }
        crate::config::ImageFormat::Png => encode_with_format(img, image::ImageFormat::Png),
        crate::config::ImageFormat::Bmp => encode_with_format(img, image::ImageFormat::Bmp),
        crate::config::ImageFormat::Gif => encode_with_format(img, image::ImageFormat::Gif),
    }
}

fn encode_with_format(img: &DynamicImage, format: image::ImageFormat) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), format)?;
    Ok(buf)
}

fn save_image_with_config(
    img: &DynamicImage,
    path: &std::path::Path,
    format: crate::config::ImageFormat,
    jpeg_quality: u8,
) -> Result<()> {
    let bytes = encode_image_bytes(img, format, jpeg_quality)?;
    fs::write(path, bytes)?;
    Ok(())
}

