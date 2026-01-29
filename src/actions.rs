use arboard::{Clipboard, ImageData};
use image::{DynamicImage, GrayImage, imageops::FilterType};
use image::codecs::jpeg::JpegEncoder;
use rfd::FileDialog;
use anyhow::{Result, anyhow};
use std::borrow::Cow;
use std::thread;
use std::time::Duration;
use std::io::Cursor;
use std::fs;
use std::path::PathBuf;
use imageproc::contrast::{otsu_level, threshold_mut};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
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
    if let Err(e) = webbrowser::open(&search_url) {
        return Err(anyhow!("Failed to open browser: {}", e));
    }
    Ok(())
}

pub fn image_to_text(img: &DynamicImage) -> Result<String> {
    let tessdata_dir = resolve_tessdata_dir()?;

    let temp_dir = tempfile::tempdir()?;
    let input_path = temp_dir.path().join("ocr_input.png");
    let output_base = temp_dir.path().join("ocr_output");
    let processed = preprocess_for_ocr(img);
    processed.save(&input_path)?;

    let mut cmd = tesseract_command()?;
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000);
    }
    cmd.env("TESSDATA_PREFIX", &tessdata_dir);
    cmd.arg(&input_path)
        .arg(&output_base)
        .arg("-l")
        .arg("eng")
        .arg("--oem")
        .arg("1")
        .arg("--psm")
        .arg("3")
        .arg("--dpi")
        .arg("300")
        .arg("-c")
        .arg("preserve_interword_spaces=1");

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "Tesseract failed.\nstdout: {}\nstderr: {}",
            stdout.trim(),
            stderr.trim()
        ));
    }

    let text_path = output_base.with_extension("txt");
    let text = fs::read_to_string(text_path)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err(anyhow!("No text detected"))
    } else {
        Ok(trimmed.to_string())
    }
}

fn tesseract_command() -> Result<std::process::Command> {
    if let Ok(custom) = std::env::var("TESSERACT_PATH") {
        let path = PathBuf::from(custom);
        if !path.exists() {
            return Err(anyhow!("TESSERACT_PATH not found: {}", path.display()));
        }
        return Ok(std::process::Command::new(path));
    }

    let local_release = PathBuf::from("third_party/tesseract/build/bin/Release/tesseract.exe");
    if local_release.exists() {
        return Ok(std::process::Command::new(local_release));
    }
    let local_debug = PathBuf::from("third_party/tesseract/build/bin/Debug/tesseract.exe");
    if local_debug.exists() {
        return Ok(std::process::Command::new(local_debug));
    }

    Ok(std::process::Command::new("tesseract"))
}

fn resolve_tessdata_dir() -> Result<PathBuf> {
    if let Ok(env_path) = std::env::var("TESSDATA_PREFIX") {
        let path = PathBuf::from(env_path);
        if path.join("eng.traineddata").exists() {
            return Ok(path);
        }
    }

    let local_root = PathBuf::from("third_party/tessdata");
    if local_root.join("eng.traineddata").exists() {
        return Ok(local_root);
    }

    let local_in_tesseract = PathBuf::from("third_party/tesseract/tessdata");
    if local_in_tesseract.join("eng.traineddata").exists() {
        return Ok(local_in_tesseract);
    }

    Err(anyhow!(
        "Missing eng.traineddata. Download tessdata and set TESSDATA_PREFIX to the folder. Example: https://github.com/tesseract-ocr/tessdata"
    ))
}

fn preprocess_for_ocr(img: &DynamicImage) -> DynamicImage {
    let gray = img.to_luma8();
    let scaled = upscale_gray(&gray, 2);
    let level = otsu_level(&scaled);
    let mut binary = scaled.clone();
    threshold_mut(&mut binary, level);
    DynamicImage::ImageLuma8(binary)
}

fn upscale_gray(img: &GrayImage, factor: u32) -> GrayImage {
    let width = img.width().saturating_mul(factor);
    let height = img.height().saturating_mul(factor);
    image::imageops::resize(img, width, height, FilterType::Lanczos3)
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
        let script = format!(
            "Add-Type -AssemblyName System.Speech; \
             $t = Get-Content -Raw -Path '{}'; \
             $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             $s.Rate = {}; \
             $s.Volume = {}; \
             $s.Speak($t);",
            path.display().to_string().replace("'", "''")
            ,settings.tts_rate
            ,settings.tts_volume
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000)
            .output()?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Speech Error: {}", err));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Err(anyhow!("Image to speech is only supported on Windows"));
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
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };
    use windows_sys::Win32::UI::Shell::DROPFILES;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide.push(0);

    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let bytes_len = wide.len() * std::mem::size_of::<u16>();
    let total_size = dropfiles_size + bytes_len;

    const CF_HDROP: u32 = 15;
    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size);
        if hglobal.is_null() {
            return Err(anyhow!("Failed to allocate clipboard memory"));
        }
        let ptr = GlobalLock(hglobal) as *mut u8;
        if ptr.is_null() {
            return Err(anyhow!("Failed to lock clipboard memory"));
        }

        let dropfiles = ptr as *mut DROPFILES;
        (*dropfiles).pFiles = dropfiles_size as u32;
        (*dropfiles).fWide = 1;

        let list_ptr = ptr.add(dropfiles_size) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), list_ptr, wide.len());
        GlobalUnlock(hglobal);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(anyhow!("Failed to open clipboard"));
        }
        let _ = EmptyClipboard();
        if SetClipboardData(CF_HDROP, hglobal) == std::ptr::null_mut() {
            let _ = CloseClipboard();
            return Err(anyhow!("Failed to set clipboard file"));
        }
        let _ = CloseClipboard();
    }
    Ok(())
}
