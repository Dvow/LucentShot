use arboard::{Clipboard, ImageData};
use image::{DynamicImage, imageops::FilterType};
use image::codecs::jpeg::JpegEncoder;
use rfd::FileDialog;
use anyhow::{Result, anyhow};
use std::borrow::Cow;
use std::thread;
use std::time::{Duration, SystemTime};
use std::io::Cursor;
use std::fs;
#[cfg(target_os = "windows")]
use std::os::windows::{ffi::OsStrExt, process::CommandExt};
use serde_json::json;

pub enum CopyResult {
    Image,
    File,
}

pub fn copy_to_clipboard(
    img: &DynamicImage,
    format: crate::config::ImageFormat,
    jpeg_quality: u8,
) -> Result<CopyResult> {
    if matches!(format, crate::config::ImageFormat::Png) {
        return copy_image_to_clipboard(img).map(|_| CopyResult::Image);
    }

    let path = save_temp_image(img, format, jpeg_quality)?;
    #[cfg(target_os = "windows")]
    {
        set_clipboard_file(&path)?;
        return Ok(CopyResult::File);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut last_err = anyhow!("Failed to initialize clipboard");
        for i in 0..5 {
            match Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(e) = clipboard.set_text(path.display().to_string()) {
                        last_err = anyhow!("arboard set_text failed: {}", e);
                        thread::sleep(Duration::from_millis(50 * (i + 1)));
                        continue;
                    }
                    return Ok(CopyResult::File);
                }
                Err(e) => {
                    last_err = anyhow!("arboard init failed: {}", e);
                    thread::sleep(Duration::from_millis(50 * (i + 1)));
                }
            }
        }
        Err(last_err)
    }
}

fn copy_image_to_clipboard(img: &DynamicImage) -> Result<()> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let bytes = rgba.into_raw();

    let mut last_err = anyhow!("Failed to initialize clipboard");

    for i in 0..5 {
        match Clipboard::new() {
            Ok(mut clipboard) => {
                let image_data = ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: Cow::Owned(bytes.clone()),
                };

                if let Err(e) = clipboard.set_image(image_data) {
                    last_err = anyhow!("arboard set_image failed: {}", e);
                    thread::sleep(Duration::from_millis(50 * (i + 1)));
                    continue;
                }
                return Ok(());
            }
            Err(e) => {
                last_err = anyhow!("arboard init failed: {}", e);
                thread::sleep(Duration::from_millis(50 * (i + 1)));
            }
        }
    }

    Err(last_err)
}

pub fn save_to_file(img: &DynamicImage) -> Result<bool> {
    let config = crate::config::cfg();
    let (ext, filter_label) = format_extension_and_label(config.format);
    let mut i = 1;
    let mut file_name = format!("screenshot_{}.{}", i, ext);
    
    // We'll use the user's Pictures folder as a starting point if available
    let start_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::Path::new(&p).join("Pictures"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Find the first available screenshot_N.ext name in the target directory
    while start_dir.join(&file_name).exists() {
        i += 1;
        file_name = format!("screenshot_{}.{}", i, ext);
    }

    if let Some(path) = FileDialog::new()
        .add_filter(filter_label, &[ext])
        .set_directory(&start_dir)
        .set_file_name(&file_name)
        .save_file() {
        let target_path = if path.extension().is_some() {
            path
        } else {
            path.with_extension(ext)
        };
        if let Err(e) = save_image_with_config(img, &target_path, config.format, config.jpeg_quality) {
            eprintln!("Failed to save image: {}", e);
        }
        return Ok(true);
    }
    Ok(false)
}

/// Opens a URL in the default browser (Windows: ShellExecuteW).
#[cfg(target_os = "windows")]
pub fn open_url(url: &str) -> Result<()> {
    use std::iter;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = url.encode_utf16().chain(iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            PCWSTR::from_raw(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOW,
        )
    };
    if result.0 as i32 <= 32 {
        return Err(anyhow!("Failed to open URL (ShellExecuteW returned {})", result.0));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn open_url(url: &str) -> Result<()> {
    Err(anyhow!("Opening URLs is only supported on Windows"))
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
    #[cfg(target_os = "windows")]
    {
        image_to_text_windows_ocr(img)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(anyhow!("OCR is only supported on Windows (uses Windows.Media.Ocr API)"))
    }
}

/// Minimum dimension (px) for reliable Windows OCR. Windows.Media.Ocr fails on small images
/// (e.g. <50px height, <40px width). Upscale small crops before passing to the engine.
const MIN_OCR_DIM: u32 = 100;

/// Upscale image if too small for Windows OCR. Uses Lanczos3 for quality.
fn upscale_for_ocr_if_needed(img: &DynamicImage) -> DynamicImage {
    let w = img.width();
    let h = img.height();
    let min_side = w.min(h);
    if min_side >= MIN_OCR_DIM {
        return img.clone();
    }
    let scale = ((MIN_OCR_DIM as f32 / min_side as f32).ceil() as u32).max(2);
    let new_w = (w * scale).max(MIN_OCR_DIM);
    let new_h = (h * scale).max(MIN_OCR_DIM);
    DynamicImage::ImageRgba8(image::imageops::resize(
        &img.to_rgba8(),
        new_w,
        new_h,
        FilterType::Lanczos3,
    ))
}

/// Uses Windows.Media.Ocr (built-in Windows API) - no Tesseract required.
#[cfg(target_os = "windows")]
fn image_to_text_windows_ocr(img: &DynamicImage) -> Result<String> {
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;

    let img = upscale_for_ocr_if_needed(img);
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Convert Rgba8 to Bgra8 (swap R and B for Windows format)
    let mut bgra: Vec<u8> = rgba.into_raw();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    let buffer = CryptographicBuffer::CreateFromByteArray(bgra.as_slice())?;
    let software_bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
        BitmapAlphaMode::Premultiplied,
    )?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| anyhow!("Windows OCR engine init failed: {:?}", e))?;

    let async_op = engine.RecognizeAsync(&software_bitmap)?;
    let result = async_op.get()?;
    let text = ocr_result_to_read_order(&result)?;
    let trimmed = text.trim().to_string();

    if trimmed.is_empty() {
        Err(anyhow!("No text detected"))
    } else {
        let refined = refine_ocr_text(&trimmed);
        Ok(fix_ocr_slashed_zero(&refined))
    }
}

/// Build text in left-to-right, top-to-bottom reading order.
/// Flattens all words, clusters by row (adaptive threshold from median height),
/// sorts each row left-to-right. Handles mixed column/line layouts and
/// mixed-case words on the same line.
#[cfg(target_os = "windows")]
fn ocr_result_to_read_order(result: &windows::Media::Ocr::OcrResult) -> Result<String> {
    use windows::Media::Ocr::{OcrLine, OcrWord};
    use windows::Foundation::Collections::IVectorView;

    let lines = result.Lines()?;
    let mut words: Vec<(String, f32, f32)> = Vec::new();
    let mut heights: Vec<f32> = Vec::new();
    for i in 0..lines.Size()? {
        let line: OcrLine = lines.GetAt(i)?;
        let line_words: IVectorView<OcrWord> = line.Words()?;
        for j in 0..line_words.Size()? {
            let word = line_words.GetAt(j)?;
            let text = word.Text()?.to_string();
            if text.is_empty() {
                continue;
            }
            let rect = word.BoundingRect()?;
            let center_y = rect.Y + rect.Height / 2.0;
            words.push((text, center_y, rect.X));
            heights.push(rect.Height);
        }
    }
    if words.is_empty() {
        return Ok(String::new());
    }

    // Adaptive row threshold: half median line height so words on same line cluster
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_h = heights[heights.len() / 2];
    let row_threshold = (median_h * 0.6).max(6.0);

    // Assign row index by clustering: sort by Y, new row when Y jump > threshold
    words.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut rowed: Vec<(String, u32, f32)> = Vec::with_capacity(words.len());
    let mut row_id = 0u32;
    let mut last_y = f32::NEG_INFINITY;
    for (text, y, x) in words {
        if y - last_y > row_threshold && last_y != f32::NEG_INFINITY {
            row_id += 1;
        }
        last_y = y;
        rowed.push((text, row_id, x));
    }

    // Sort by row, then X (left-to-right within each line)
    rowed.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)));

    let mut out = String::new();
    let mut last_row = u32::MAX;
    for (text, row, _x) in rowed {
        if row != last_row && last_row != u32::MAX {
            out.push('\n');
        } else if !out.is_empty() && !out.ends_with('\n') {
            out.push(' ');
        }
        last_row = row;
        out.push_str(&text);
    }
    Ok(out)
}

/// Refine OCR output for readability: collapse spaces around punctuation,
/// replace common misreads (bullet → arrow), normalize spacing.
fn refine_ocr_text(s: &str) -> String {
    let mut t = s.to_string();
    // Collapse spaces around punctuation (order: longer patterns first)
    let replacements: &[(&str, &str)] = &[
        (" : : ", "::"),
        (" : ", ":"),
        (" . ", "."),
        (" \" , \" ", "\", \""),
        (" \" , ", "\", "),
        (" \" ", "\""),
        (" ( ", "("),
        (" ) ", ")"),
        (" , ", ", "),
        (" • ", " => "),
        // Bullet misread as =>, often adjacent to quotes or colons
        (" •\"", "\""),
        ("\"• ", "\" "),
        ("•\"", "\""),
        (":•", "::"),
        ("•.", ""),
        ("•G", " G"),
    ];
    for (from, to) in replacements {
        while t.contains(from) {
            t = t.replace(from, to);
        }
    }
    // Common OCR misreads in format names (l↔i, letter order)
    let fixes: &[(&str, &str)] = &[("Glf", "Gif"), ("glf", "gif"), ("BmP", "BMP")];
    for (from, to) in fixes {
        t = t.replace(from, to);
    }
    // Collapse multiple spaces
    while t.contains("  ") {
        t = t.replace("  ", " ");
    }
    t
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
    let temp_path = std::path::Path::new("C:\\Users\\Public\\lightshot_print.png");
    img.save(temp_path)?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        
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
             $img = [System.Drawing.Image]::FromFile('C:\\Users\\Public\\lightshot_print.png'); \
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

pub fn cleanup_old_copy_temp_files() {
    let temp_dir = std::env::temp_dir().join("lightshotv2");
    let Ok(entries) = fs::read_dir(&temp_dir) else { return };
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(3600))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with("copy_") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        if modified < cutoff {
            let _ = fs::remove_file(path);
        }
    }
}

fn save_temp_image(
    img: &DynamicImage,
    format: crate::config::ImageFormat,
    jpeg_quality: u8,
) -> Result<std::path::PathBuf> {
    let (ext, _) = format_extension_and_label(format);
    let temp_dir = std::env::temp_dir().join("lightshotv2");
    fs::create_dir_all(&temp_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = temp_dir.join(format!("copy_{}.{}", stamp, ext));
    save_image_with_config(img, &path, format, jpeg_quality)?;
    Ok(path)
}

#[cfg(target_os = "windows")]
fn set_clipboard_file(path: &std::path::Path) -> Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };
    use windows::Win32::UI::Shell::DROPFILES;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide.push(0);

    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let bytes_len = wide.len() * std::mem::size_of::<u16>();
    let total_size = dropfiles_size + bytes_len;

    const CF_HDROP: u32 = 15;
    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size)
            .map_err(|_| anyhow!("Failed to allocate clipboard memory"))?;
        let ptr = GlobalLock(hglobal);
        if ptr.is_null() {
            return Err(anyhow!("Failed to lock clipboard memory"));
        }

        let ptr = ptr as *mut u8;
        let dropfiles = ptr as *mut DROPFILES;
        (*dropfiles).pFiles = dropfiles_size as u32;
        (*dropfiles).fWide = windows::Win32::Foundation::BOOL::from(true);

        let list_ptr = ptr.add(dropfiles_size) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), list_ptr, wide.len());
        let _ = GlobalUnlock(hglobal);

        OpenClipboard(None).map_err(|_| anyhow!("Failed to open clipboard"))?;
        let _ = EmptyClipboard();
        let hdrop = HANDLE(hglobal.0 as _);
        if SetClipboardData(CF_HDROP, hdrop).is_err() {
            let _ = CloseClipboard();
            return Err(anyhow!("Failed to set clipboard file"));
        }
        let _ = CloseClipboard();
    }
    Ok(())
}
