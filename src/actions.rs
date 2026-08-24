use crate::config::ImageFormat;
use anyhow::{anyhow, Result};
use arboard::{Clipboard, ImageData};
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use serde_json::json;
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::LazyLock;

static HTTP: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .expect("reqwest client")
});

pub enum Export {
    Copy,
    Save,
    Upload,
    #[cfg_attr(not(feature = "ocr"), allow(dead_code))]
    Ocr,
    #[cfg_attr(not(feature = "ocr"), allow(dead_code))]
    Speak,
    Print {
        printer: String,
        copies: i32,
        landscape: bool,
        grayscale: bool,
        paper: String,
    },
    Google,
}

pub fn copy_image(img: &DynamicImage) -> Result<()> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Clipboard::new()
        .and_then(|mut clipboard| {
            clipboard.set_image(ImageData {
                width: width as usize,
                height: height as usize,
                bytes: Cow::Owned(rgba.into_raw()),
            })
        })
        .map_err(|e| anyhow!("Clipboard: {e}"))
}

pub fn copy_text(text: &str) -> Result<()> {
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
        .map_err(|e| anyhow!("Clipboard: {e}"))
}

pub fn save_image(img: &DynamicImage) -> Result<bool> {
    let config = crate::config::get();
    let ext = config.format.extension();
    let start_dir = dirs::picture_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut index = 1;
    let mut file_name = format!("screenshot_{index}.{ext}");
    while start_dir.join(&file_name).exists() {
        index += 1;
        file_name = format!("screenshot_{index}.{ext}");
    }

    let Some(path) = rfd::FileDialog::new()
        .add_filter(config.format.label(), &[ext])
        .set_directory(&start_dir)
        .set_file_name(&file_name)
        .save_file()
    else {
        return Ok(false);
    };

    let path = if path.extension().is_some() {
        path
    } else {
        path.with_extension(ext)
    };
    write_image(img, &path, config.format, config.jpeg_quality)?;
    Ok(true)
}

pub fn pick_save_path(
    default_name: &str,
    filter_label: &str,
    ext: &str,
) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter(filter_label, &[ext])
        .set_file_name(default_name)
        .save_file()
}

pub fn open_url(url: &str) -> Result<()> {
    webbrowser::open(url).map_err(|e| anyhow!("Failed to open URL: {e}"))
}

pub fn upload_prntsc(img: &DynamicImage) -> Result<String> {
    let img_url = upload_anonymous(img)?;
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

    let api_json: serde_json::Value = HTTP
        .post("https://api.prntscr.com/v1/")
        .json(&payload)
        .send()?
        .json()?;
    api_json["result"]["url"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Lightshot API failed: {api_json:?}"))
}

pub fn google_search(img: &DynamicImage) -> Result<()> {
    let direct_url = upload_anonymous(img)?;
    open_url(&format!(
        "https://lens.google.com/uploadbyurl?url={direct_url}"
    ))
}

pub fn apply_upload_result(url: &str, auto_copy_link: bool, auto_close_upload: bool) -> Result<()> {
    if !auto_close_upload {
        open_url(url)?;
    }
    if auto_copy_link {
        copy_text(url)?;
    }
    Ok(())
}

fn upload_anonymous(img: &DynamicImage) -> Result<String> {
    let config = crate::config::get();
    let bytes = encode_image(img, config.format, config.jpeg_quality)?;
    let form = reqwest::blocking::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part(
            "fileToUpload",
            reqwest::blocking::multipart::Part::bytes(bytes)
                .file_name(format!("screenshot.{}", config.format.extension()))
                .mime_str(config.format.mime())?,
        );

    let resp = HTTP
        .post("https://catbox.moe/user/api.php")
        .multipart(form)
        .send()?;
    if !resp.status().is_success() {
        let err_body = resp
            .text()
            .unwrap_or_else(|e| format!("Failed to read error body: {e}"));
        return Err(anyhow!("Host failed: {err_body}"));
    }
    Ok(resp.text()?)
}

pub fn printers() -> Vec<String> {
    use winprint::printer::PrinterDevice;
    let names: Vec<String> = PrinterDevice::all()
        .map(|devices| devices.into_iter().map(|d| d.name().to_string()).collect())
        .unwrap_or_default();
    if names.is_empty() {
        vec!["Microsoft Print to PDF".to_string()]
    } else {
        names
    }
}

pub fn print_image(
    img: &DynamicImage,
    printer_name: &str,
    copies: i32,
    landscape: bool,
    grayscale: bool,
    paper_size: &str,
) -> Result<()> {
    use winprint::printer::{FilePrinter, ImagePrinter, PrinterDevice};
    use winprint::ticket::{
        Copies, FeatureOptionPack, FeatureOptionPackWithPredefined, PageOrientation,
        PageOutputColor, PredefinedPageOrientation, PredefinedPageOutputColor, PrintCapabilities,
        PrintTicketBuilder,
    };

    let temp_path = std::env::temp_dir().join("lightshot_print.png");
    img.save(&temp_path)?;
    let result = (|| {
        let device = PrinterDevice::all()
            .map_err(|e| anyhow!("List printers: {e:?}"))?
            .into_iter()
            .find(|d| d.name() == printer_name)
            .ok_or_else(|| anyhow!("Printer not found: {printer_name}"))?;

        let capabilities =
            PrintCapabilities::fetch(&device).map_err(|e| anyhow!("Fetch capabilities: {e:?}"))?;
        let mut builder =
            PrintTicketBuilder::new(&device).map_err(|e| anyhow!("Print ticket: {e:?}"))?;
        builder.merge(Copies(copies.clamp(1, 9999) as u16))?;

        let orient = if landscape {
            PredefinedPageOrientation::Landscape
        } else {
            PredefinedPageOrientation::Portrait
        };
        if let Some(opt) =
            PageOrientation::list(&capabilities).find(|o| o.as_predefined_name() == Some(orient))
        {
            let _ = builder.merge(opt);
        }

        let color = if grayscale {
            PredefinedPageOutputColor::Grayscale
        } else {
            PredefinedPageOutputColor::Color
        };
        if let Some(opt) =
            PageOutputColor::list(&capabilities).find(|o| o.as_predefined_name() == Some(color))
        {
            let _ = builder.merge(opt);
        }

        if let Some(predef) = paper_media(paper_size) {
            let media = capabilities
                .page_media_sizes()
                .find(|m| m.as_predefined_name() == Some(predef));
            if let Some(media) = media {
                let _ = builder.merge(media);
            }
        }

        let ticket = builder
            .build()
            .map_err(|e| anyhow!("Build ticket: {e:?}"))?;
        ImagePrinter::new(device)
            .print(&temp_path, ticket)
            .map_err(|e| anyhow!("Print: {e:?}"))
    })();
    let _ = std::fs::remove_file(&temp_path);
    result
}

fn paper_media(paper: &str) -> Option<winprint::ticket::PredefinedMediaName> {
    use winprint::ticket::PredefinedMediaName;
    match paper.trim() {
        "A4" => Some(PredefinedMediaName::ISOA4),
        "Letter" => Some(PredefinedMediaName::NorthAmericaLetter),
        "Legal" => Some(PredefinedMediaName::NorthAmericaLegal),
        other => PredefinedMediaName::try_from(other).ok(),
    }
}

fn write_image(
    img: &DynamicImage,
    path: &std::path::Path,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<()> {
    std::fs::write(path, encode_image(img, format, jpeg_quality)?)?;
    Ok(())
}

fn encode_image(img: &DynamicImage, format: ImageFormat, jpeg_quality: u8) -> Result<Vec<u8>> {
    match format {
        ImageFormat::Jpeg => {
            let mut buf = Vec::new();
            JpegEncoder::new_with_quality(&mut buf, jpeg_quality.clamp(1, 100))
                .encode_image(img)?;
            Ok(buf)
        }
        ImageFormat::Png => encode_with(img, image::ImageFormat::Png),
        ImageFormat::Bmp => encode_with(img, image::ImageFormat::Bmp),
        ImageFormat::Gif => encode_with(img, image::ImageFormat::Gif),
    }
}

fn encode_with(img: &DynamicImage, format: image::ImageFormat) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), format)?;
    Ok(buf)
}
