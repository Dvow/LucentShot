fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    install_runtime_files(&manifest_dir);

    let icon_png = manifest_dir.join("assets").join("icons").join("icon.png");
    let icon_ico = std::env::var("OUT_DIR").unwrap() + "/icon.ico";
    if icon_png.exists() {
        use image::ImageEncoder;
        if let Ok(img) = image::open(&icon_png) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let (w, h) = (w.clamp(1, 256), h.clamp(1, 256));
            let resized = if (rgba.width(), rgba.height()) != (w, h) {
                image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Lanczos3)
            } else {
                rgba
            };
            if let Ok(file) = std::fs::File::create(&icon_ico) {
                let _ = image::codecs::ico::IcoEncoder::new(file).write_image(
                    resized.as_raw(),
                    w,
                    h,
                    image::ColorType::Rgba8.into(),
                );
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
  <assemblyIdentity name="lucentshot" version="0.1.0.0" processorArchitecture="*" type="win32"/>
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
        eprintln!("winres: {e}");
        std::process::exit(1);
    }
}

fn install_runtime_files(manifest_dir: &std::path::Path) {
    let exe_dir = runtime_dir(manifest_dir);
    let assets = manifest_dir.join("assets");

    println!("cargo:rerun-if-changed=assets/icons/icon.png");
    copy_file(
        &assets.join("icons").join("icon.png"),
        &exe_dir.join("icon.png"),
    );

    if std::env::var_os("CARGO_FEATURE_OCR").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=assets/tesseract.dll");
    println!("cargo:rerun-if-changed=assets/leptonica-1.85.0.dll");
    println!("cargo:rerun-if-changed=assets/eng.traineddata");
    copy_file(
        &assets.join("tesseract.dll"),
        &exe_dir.join("tesseract.dll"),
    );
    copy_file(
        &assets.join("leptonica-1.85.0.dll"),
        &exe_dir.join("leptonica-1.85.0.dll"),
    );
    let tessdata = exe_dir.join("tessdata");
    let _ = std::fs::create_dir_all(&tessdata);
    copy_file(
        &assets.join("eng.traineddata"),
        &tessdata.join("eng.traineddata"),
    );
}

fn runtime_dir(manifest_dir: &std::path::Path) -> std::path::PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let target_dir = std::path::PathBuf::from(target_dir);
        let host = std::env::var("HOST").unwrap_or_default();
        let target = std::env::var("TARGET").unwrap_or_default();
        return if !host.is_empty() && host != target {
            target_dir.join(target).join(profile)
        } else {
            target_dir.join(profile)
        };
    }

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for ancestor in out_dir.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) == Some("build") {
            if let Some(parent) = ancestor.parent() {
                return parent.to_path_buf();
            }
        }
    }

    let mut dir = manifest_dir.join("target");
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && host != target {
        dir.push(target);
    }
    dir.push(profile);
    dir
}

fn copy_file(src: &std::path::Path, dest: &std::path::Path) {
    if !src.exists() {
        panic!("missing runtime file: {}", src.display());
    }
    if dest.exists() {
        if let (Ok(src_meta), Ok(dest_meta)) = (src.metadata(), dest.metadata()) {
            if src_meta.len() == dest_meta.len()
                && src_meta.modified().ok() <= dest_meta.modified().ok()
            {
                return;
            }
        }
    }
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::copy(src, dest)
        .unwrap_or_else(|err| panic!("copy {} -> {}: {err}", src.display(), dest.display()));
}
