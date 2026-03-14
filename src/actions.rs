use arboard::{Clipboard, ImageData};
use image::{DynamicImage, imageops::FilterType};
use image::codecs::jpeg::JpegEncoder;
use anyhow::{Result, anyhow};
use std::borrow::Cow;
use std::io::Cursor;
use std::fs;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
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
    let start_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join("Pictures"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
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

fn upload_to_anonymous_host(img: &DynamicImage) -> Result<String> {
    let config = crate::config::cfg();
    let (bytes, ext, mime) = encode_image_for_upload(img, config.format, config.jpeg_quality)?;

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part("fileToUpload", reqwest::blocking::multipart::Part::bytes(bytes)
            .file_name(format!("screenshot.{}", ext))
            .mime_str(mime)?);

    let resp = client.post("https://catbox.moe/user/api.php")
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

/// Show error to user via Windows message box (call from UI/action handler).
#[cfg(target_os = "windows")]
pub fn show_ocr_error(msg: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONWARNING};
    use std::iter;
    let text: Vec<u16> = msg.encode_utf16().chain(iter::once(0)).collect();
    let title: Vec<u16> = "OCR Error".encode_utf16().chain(iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(None, windows::core::PCWSTR(text.as_ptr()), windows::core::PCWSTR(title.as_ptr()), MB_OK | MB_ICONWARNING);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_ocr_error(_msg: &str) {
    eprintln!("OCR Error: {}", _msg);
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

pub fn image_to_speech(img: &DynamicImage) -> Result<()> {
    let text = match image_to_text(img) {
        Ok(text) => text,
        Err(err) => {
            let log_path = std::env::temp_dir().join("lightshotv2_tts_error.txt");
            let _ = fs::write(&log_path, format!("{}", err));
            return Err(err);
        }
    };
    let normalized = normalize_tts_text(&text);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("No text detected"));
    }
    let path = std::env::temp_dir().join("lightshotv2_tts.txt");
    fs::write(&path, trimmed)?;
    let settings = crate::config::cfg();

    #[cfg(target_os = "windows")]
    {
        let voice_part = if settings.tts_voice.trim().is_empty() {
            String::new()
        } else {
            let escaped = settings.tts_voice.trim().replace("'", "''");
            format!("$s.SelectVoice('{}'); ", escaped)
        };
        let script = format!(
            "Add-Type -AssemblyName System.Speech; \
             $t = Get-Content -Raw -Path '{}'; \
             $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             {}; \
             $s.Rate = {}; \
             $s.Volume = {}; \
             $s.Speak($t);",
            path.display().to_string().replace("'", "''")
            ,voice_part
            ,settings.tts_rate
            ,settings.tts_volume
        );
        run_tts_powershell_silent(&script)?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Err(anyhow!("Image to speech is only supported on Windows"));
    }

    Ok(())
}

/// Returns installed Windows TTS voice names. Empty on non-Windows or if listing fails.
pub fn get_tts_voices() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let script = "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.GetInstalledVoices() | ForEach-Object { if ($_.Enabled) { $_.VoiceInfo.Name } }";
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
            }
            _ => Vec::new(),
        }
    }
    #[cfg(not(target_os = "windows"))]
    Vec::new()
}

#[cfg(target_os = "windows")]
static TTS_PROCESS: std::sync::OnceLock<std::sync::RwLock<Option<windows::Win32::Foundation::HANDLE>>> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
fn run_tts_powershell_silent(script: &str) -> Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
        PROCESS_INFORMATION, STARTUPINFOW, INFINITE,
    };
    let tts_lock = TTS_PROCESS.get_or_init(|| std::sync::RwLock::new(None));

    // Terminate any currently playing TTS before starting new one
    {
        let mut guard = tts_lock.write().unwrap();
        if let Some(prev) = guard.take() {
            unsafe {
                let _ = TerminateProcess(prev, 1);
                let _ = CloseHandle(prev);
            }
        }
    }

    let cmd = format!(
        "powershell.exe -NoProfile -WindowStyle Hidden -Command \"{}\"",
        script.replace('"', "\\\"")
    );
    let mut wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();

    let mut si = STARTUPINFOW::default();
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = windows::Win32::System::Threading::STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // SW_HIDE

    let mut pi = PROCESS_INFORMATION::default();

    let ok = unsafe {
        CreateProcessW(
            None,
            windows::core::PWSTR(wide.as_mut_ptr()),
            None,
            None,
            false,
            windows::Win32::System::Threading::PROCESS_CREATION_FLAGS::default(),
            None,
            None,
            &si,
            &mut pi,
        )
    };

    if ok.is_err() {
        return Err(anyhow!("Failed to start TTS process"));
    }

    // Store our process handle so a future TTS can terminate us
    {
        let mut guard = tts_lock.write().unwrap();
        *guard = Some(pi.hProcess);
    }

    unsafe {
        let _ = WaitForSingleObject(pi.hProcess, INFINITE);
        let mut code: u32 = 0;
        let _ = GetExitCodeProcess(pi.hProcess, &mut code);
        let mut guard = tts_lock.write().unwrap();
        let was_replaced = guard.as_ref() != Some(&pi.hProcess);
        if !was_replaced {
            *guard = None;
            let _ = CloseHandle(pi.hProcess);
        }
        let _ = CloseHandle(pi.hThread);
        if !was_replaced && code != 0 {
            return Err(anyhow!("Speech failed (exit code {})", code));
        }
    }
    Ok(())
}

fn normalize_tts_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn prntsc_upload(img: &DynamicImage) -> Result<String> {
    println!("Uploading image...");
    let img_url = upload_to_anonymous_host(img)?;
    println!("Image host link: {}", img_url);

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

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

    let api_resp = client.post("https://api.prntscr.com/v1/")
        .json(&payload)
        .send()?;

    let api_json: serde_json::Value = api_resp.json()?;
    let prnt_url = api_json["result"]["url"].as_str()
        .ok_or_else(|| anyhow!("Lightshot API failed: {:?}", api_json))?;

    Ok(prnt_url.to_string())
}

pub fn get_printers() -> Vec<String> {
    let mut printers = Vec::new();
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Printing::{EnumPrintersW, PRINTER_ENUM_LOCAL, PRINTER_ENUM_CONNECTIONS, PRINTER_INFO_4W};

        unsafe {
            let mut needed: u32 = 0;
            let mut returned: u32 = 0;
            let _ = EnumPrintersW(PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS, None, 4, None, &mut needed, &mut returned);
            
            if needed > 0 {
                let mut buffer = vec![0u8; needed as usize];
                if EnumPrintersW(PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS, None, 4, Some(&mut buffer), &mut needed, &mut returned).is_ok() {
                    let info = buffer.as_ptr() as *const PRINTER_INFO_4W;
                    for i in 0..returned {
                        let printer = &*info.add(i as usize);
                        if let Ok(name) = printer.pPrinterName.to_string() {
                            printers.push(name);
                        }
                    }
                }
            }
        }
    }
    
    if printers.is_empty() {
        printers.push("Microsoft Print to PDF".to_string());
    }
    printers
}

pub fn print_image_to(
    img: &DynamicImage, 
    printer_name: &str, 
    copies: i32, 
    landscape: bool, 
    grayscale: bool,
    fit_to_page: bool,
    paper_size: &str
) -> Result<()> {
    let temp_path = std::env::temp_dir().join("lightshot_print.png");
    img.save(&temp_path)?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let path_str = temp_path.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Drawing; \
             $doc = New-Object System.Drawing.Printing.PrintDocument; \
             $doc.DocumentName = 'Lightshot Capture'; \
             $doc.PrinterSettings.PrinterName = '{}'; \
             $doc.PrinterSettings.Copies = {}; \
             $doc.DefaultPageSettings.Landscape = {}; \
             $doc.DefaultPageSettings.Color = {}; \
             foreach ($ps in $doc.PrinterSettings.PaperSizes) {{ \
                if ($ps.PaperName -eq '{}') {{ $doc.DefaultPageSettings.PaperSize = $ps; break; }} \
             }} \
             $img = [System.Drawing.Image]::FromFile('{}'); \
             $doc.add_PrintPage({{ \
                $arg = $_; \
                $rect = if ({}) {{ $arg.MarginBounds }} else {{ New-Object System.Drawing.Rectangle(0, 0, $img.Width, $img.Height) }}; \
                if ({}) {{ \
                    if ($img.Width / $img.Height -gt $rect.Width / $rect.Height) {{ \
                        $rect.Height = $img.Height * $rect.Width / $img.Width; \
                    }} else {{ \
                        $rect.Width = $img.Width * $rect.Height / $img.Height; \
                    }} \
                }} \
                $arg.Graphics.DrawImage($img, $rect); \
             }}); \
             $doc.Print(); \
             $img.Dispose();",
            printer_name.replace("'", "''"),
            copies,
            if landscape { "$true" } else { "$false" },
            if grayscale { "$false" } else { "$true" },
            paper_size,
            path_str,
            if fit_to_page { "$true" } else { "$false" },
            if fit_to_page { "$true" } else { "$false" }
        );

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000) 
            .output()?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Advanced Print Error: {}", err));
        }
    }

    Ok(())
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

