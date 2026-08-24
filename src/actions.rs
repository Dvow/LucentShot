use crate::config::ImageFormat;
use anyhow::{anyhow, Result};
use arboard::{Clipboard, ImageData};
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use serde_json::json;
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::LazyLock;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

static HTTP: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .user_agent(BROWSER_UA)
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
        fit: bool,
    },
    ImageSearch,
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
        .ok_or_else(|| anyhow!("Upload API failed: {api_json:?}"))
}

pub fn image_search(img: &DynamicImage) -> Result<()> {
    open_url(&bing_visual_search(&upload_public_jpeg(img)?)?)
}

fn bing_visual_search(image_url: &str) -> Result<String> {
    let mut search = reqwest::Url::parse("https://www.bing.com/images/search")
        .map_err(|e| anyhow!("Search URL: {e}"))?;
    search.query_pairs_mut()
        .append_pair("view", "detailv2")
        .append_pair("iss", "sbi")
        .append_pair("form", "SBIVSP")
        .append_pair("sbisrc", "UrlPaste")
        .append_pair("q", &format!("imgurl:{image_url}"));
    Ok(search.into())
}

fn upload_public_jpeg(img: &DynamicImage) -> Result<String> {
    let jpeg = encode_image(img, ImageFormat::Jpeg, 90)?;
    upload_uguu(&jpeg).or_else(|_| {
        upload_catbox(jpeg, "screenshot.jpg".to_string(), "image/jpeg")
    })
}

fn upload_uguu(jpeg: &[u8]) -> Result<String> {
    let form = reqwest::blocking::multipart::Form::new().part(
        "files[]",
        reqwest::blocking::multipart::Part::bytes(jpeg.to_vec())
            .file_name("screenshot.jpg")
            .mime_str("image/jpeg")?,
    );
    let body: serde_json::Value = HTTP.post("https://uguu.se/upload").multipart(form).send()?.json()?;
    body["files"][0]["url"]
        .as_str()
        .filter(|url| url.starts_with("http"))
        .map(str::to_string)
        .ok_or_else(|| anyhow!("uguu upload failed: {body}"))
}

fn upload_catbox(bytes: Vec<u8>, filename: String, mime: &str) -> Result<String> {
    let form = reqwest::blocking::multipart::Form::new()
        .text("reqtype", "fileupload")
        .part(
            "fileToUpload",
            reqwest::blocking::multipart::Part::bytes(bytes)
                .file_name(filename)
                .mime_str(mime)?,
        );
    let body = HTTP
        .post("https://catbox.moe/user/api.php")
        .multipart(form)
        .send()?
        .text()?;
    let url = body.trim();
    if url.starts_with("https://") || url.starts_with("http://") {
        Ok(url.to_string())
    } else {
        Err(anyhow!("Host failed: {body}"))
    }
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
    upload_catbox(
        encode_image(img, config.format, config.jpeg_quality)?,
        format!("screenshot.{}", config.format.extension()),
        config.format.mime(),
    )
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
    fit: bool,
) -> Result<()> {
    use winprint::printer::{FilePrinter, ImagePrinter, PrinterDevice};
    use winprint::ticket::{
        Copies, FeatureOptionPack, FeatureOptionPackWithPredefined, PageImageableSize,
        PageOrientation, PageOutputColor, PredefinedPageOrientation, PredefinedPageOutputColor,
        PrintCapabilities, PrintTicketBuilder,
    };

    let print_path = crate::paths::cache_dir().join("print.png");
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

        let mut page_microns = None;
        if let Some(predef) = paper_media(paper_size)
            && let Some(media) = capabilities
                .page_media_sizes()
                .find(|m| m.as_predefined_name() == Some(predef))
        {
            let size = media.size();
            page_microns = Some((size.width_in_micron(), size.height_in_micron()));
            if let Ok(area) = PageImageableSize::try_fetch(&device, media.clone()) {
                page_microns = Some((
                    area.extent.width_in_micron(),
                    area.extent.height_in_micron(),
                ));
            }
            let _ = builder.merge(media);
        }

        let prepared = if fit {
            let (page_w, page_h) = page_inches(paper_size, landscape, page_microns);
            fit_image(img, page_w, page_h)
        } else {
            img.clone()
        };
        prepared.save(&print_path)?;

        let ticket = builder
            .build()
            .map_err(|e| anyhow!("Build ticket: {e:?}"))?;
        ImagePrinter::new(device)
            .print(&print_path, ticket)
            .map_err(|e| anyhow!("Print: {e:?}"))
    })();
    let _ = std::fs::remove_file(&print_path);
    result
}

fn page_inches(paper: &str, landscape: bool, microns: Option<(u32, u32)>) -> (f64, f64) {
    let (mut width, mut height) = match microns {
        Some((w, h)) if w > 0 && h > 0 => (w as f64 / 25_400.0, h as f64 / 25_400.0),
        _ => match paper.trim() {
            "A4" => (210.0 / 25.4, 297.0 / 25.4),
            "Legal" => (8.5, 14.0),
            _ => (8.5, 11.0),
        },
    };
    if landscape {
        std::mem::swap(&mut width, &mut height);
    }
    (width, height)
}

fn fit_image(img: &DynamicImage, page_w_in: f64, page_h_in: f64) -> DynamicImage {
    let (width, height) = fit_dimensions(img.width(), img.height(), page_w_in, page_h_in);
    if width == img.width() && height == img.height() {
        return img.clone();
    }
    DynamicImage::ImageRgba8(image::imageops::resize(
        &img.to_rgba8(),
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    ))
}

fn fit_dimensions(img_w: u32, img_h: u32, page_w_in: f64, page_h_in: f64) -> (u32, u32) {
    const DPI: f64 = 96.0;
    let max_w = (page_w_in * DPI).max(1.0);
    let max_h = (page_h_in * DPI).max(1.0);
    let scale = (max_w / img_w.max(1) as f64).min(max_h / img_h.max(1) as f64);
    (
        (img_w as f64 * scale).round().max(1.0) as u32,
        (img_h as f64 * scale).round().max(1.0) as u32,
    )
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

#[cfg(test)]
mod tests {
    use super::{bing_visual_search, fit_dimensions};

    #[test]
    fn fit_letter_scales_wide_image_to_page_width() {
        let (width, height) = fit_dimensions(1920, 1080, 8.5, 11.0);
        assert_eq!((width, height), (816, 459));
    }

    #[test]
    fn bing_visual_search_embeds_image_url() {
        let url = bing_visual_search("https://d.uguu.se/eqVPouCh.jpg").unwrap();
        assert!(url.starts_with("https://www.bing.com/images/search?"));
        assert!(url.contains("iss=sbi"));
        assert!(url.contains("eqVPouCh.jpg"));
    }
}
