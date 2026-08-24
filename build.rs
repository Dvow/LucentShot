use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dest = exe_dir();
    let assets = root.join("assets");

    if std::env::var_os("CARGO_FEATURE_OCR").is_some() {
        copy(&assets.join("tesseract.dll"), &dest.join("tesseract.dll"));
        copy(
            &assets.join("leptonica-1.85.0.dll"),
            &dest.join("leptonica-1.85.0.dll"),
        );
        copy(
            &assets.join("eng.traineddata"),
            &dest.join("tessdata").join("eng.traineddata"),
        );
    }

    let ico = root.join("assets").join("icons").join("icon.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    let mut res = winres::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap());
    res.set_manifest(
        r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
    );
    res.compile().unwrap();
}

fn exe_dir() -> PathBuf {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    out.ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "build"))
        .and_then(|path| path.parent())
        .expect("OUT_DIR")
        .to_path_buf()
}

fn copy(src: &Path, dest: &Path) {
    println!("cargo:rerun-if-changed={}", src.display());
    if !src.exists() {
        panic!("missing {}", src.display());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::copy(src, dest).unwrap_or_else(|err| panic!("{}: {err}", src.display()));
}
