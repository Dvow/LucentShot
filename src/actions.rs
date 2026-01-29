use arboard::{Clipboard, ImageData};
use image::DynamicImage;
use rfd::FileDialog;
use anyhow::{Result, anyhow};
use std::borrow::Cow;
use std::thread;
use std::time::Duration;
use std::io::Cursor;
use serde_json::json;

pub fn copy_to_clipboard(img: &DynamicImage) -> Result<()> {
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

pub fn save_to_file(img: &DynamicImage) -> Result<()> {
    let mut i = 1;
    let mut file_name = format!("screenshot_{}.png", i);
    
    // We'll use the user's Pictures folder as a starting point if available
    let start_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::Path::new(&p).join("Pictures"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Find the first available screenshot_N.png name in the target directory
    while start_dir.join(&file_name).exists() {
        i += 1;
        file_name = format!("screenshot_{}.png", i);
    }

    if let Some(path) = FileDialog::new()
        .add_filter("PNG", &["png"])
        .set_directory(&start_dir)
        .set_file_name(&file_name)
        .save_file() {
        if let Err(e) = img.save(path) {
            eprintln!("Failed to save image: {}", e);
        }
    }
    Ok(())
}

fn upload_to_anonymous_host(img: &DynamicImage) -> Result<String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part("fileToUpload", reqwest::blocking::multipart::Part::bytes(buf)
            .file_name("screenshot.png")
            .mime_str("image/png")?);

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
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    println!("Extracting text from image...");
    let form = reqwest::blocking::multipart::Form::new()
        .text("apikey", "helloworld") 
        .text("language", "eng")
        .part("file", reqwest::blocking::multipart::Part::bytes(buf)
            .file_name("screenshot.png")
            .mime_str("image/png")?);

    let resp = client.post("https://api.ocr.space/parse/image")
        .multipart(form)
        .send()?;

    let json: serde_json::Value = resp.json()?;
    
    if let Some(results) = json["ParsedResults"].as_array() {
        if let Some(first) = results.first() {
            if let Some(text) = first["ParsedText"].as_str() {
                return Ok(text.to_string());
            }
        }
    }

    Err(anyhow!("Failed to extract text: {:?}", json))
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
