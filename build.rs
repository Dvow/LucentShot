fn main() {
    // Font is provided by the dejavu crate (embedded in binary — no downloads or user installs)

    // Ensure tessdata/eng.traineddata exists (required for OCR)
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let tessdata_dir = manifest_dir.join("tessdata");
    let eng_path = tessdata_dir.join("eng.traineddata");
    if !eng_path.exists() {
        std::fs::create_dir_all(&tessdata_dir).ok();
        println!("cargo:warning=Downloading eng.traineddata for Tesseract OCR...");
        let url = "https://github.com/tesseract-ocr/tessdata_best/raw/main/eng.traineddata";
        match reqwest::blocking::get(url) {
            Ok(resp) if resp.status().is_success() => {
                if let Err(e) = std::fs::write(&eng_path, resp.bytes().unwrap_or_default()) {
                    eprintln!("cargo:warning=Failed to write tessdata/eng.traineddata: {}", e);
                }
            }
            Ok(resp) => eprintln!("cargo:warning=Download failed (status {}): {}", resp.status(), url),
            Err(e) => eprintln!("cargo:warning=Download failed: {}. Run: Invoke-WebRequest -Uri \"{}\" -OutFile tessdata/eng.traineddata", e, url),
        }
    }

    // Convert icon.png to .ico for Windows exe
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon_png = manifest_dir.join("src").join("icon").join("icon.png");
    let icon_ico = std::env::var("OUT_DIR").unwrap() + "/icon.ico";
    if icon_png.exists() {
        use image::ImageEncoder;
        if let Ok(img) = image::open(&icon_png) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let (w, h) = (w.min(256).max(1), h.min(256).max(1));
            let resized = if (rgba.width(), rgba.height()) != (w, h) {
                image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Lanczos3)
            } else {
                rgba
            };
            if let Ok(file) = std::fs::File::create(&icon_ico) {
                let _ = image::codecs::ico::IcoEncoder::new(file)
                    .write_image(resized.as_raw(), w, h, image::ColorType::Rgba8);
            }
        }
    }

    let mut res = winres::WindowsResource::new();
    if std::path::Path::new(&icon_ico).exists() {
        res.set_icon(&icon_ico);
    }
    res.set_manifest(
        r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity name="lightshotv2" version="0.1.0.0" processorArchitecture="*" type="win32"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
      <supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"/>
    </application>
  </compatibility>
</assembly>
"#,
    );
    if let Err(e) = res.compile() {
        eprintln!("winres: {}", e);
        std::process::exit(1);
    }
}
