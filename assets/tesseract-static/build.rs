use std::env;
use std::path::Path;

fn main() {
    let out_dir_s = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir_s);
    let lept_target = out_dir.join("leptonica.dll");
    let tess_target = out_dir.join("tesseract.dll");

    // Prefer local DLLs from project dlls/ directory (no download at build time)
    let manifest_dir_s = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = Path::new(&manifest_dir_s);
    let project_root = manifest_dir.parent();
    if let Some(root) = project_root {
        let dlls_dir = root;
        #[cfg(target_os = "windows")]
        {
            let lept_local = dlls_dir.join("leptonica-1.85.0.dll");
            let tess_local = dlls_dir.join("tesseract.dll");
            if lept_local.exists() && tess_local.exists() {
                std::fs::copy(&lept_local, &lept_target).expect("Failed to copy leptonica DLL");
                std::fs::copy(&tess_local, &tess_target).expect("Failed to copy tesseract DLL");
                return;
            }
        }
        #[cfg(target_os = "linux")]
        {
            let lept_local = dlls_dir.join("libleptonica.so");
            let tess_local = dlls_dir.join("libtesseract.so");
            if lept_local.exists() && tess_local.exists() {
                std::fs::copy(&lept_local, &lept_target).expect("Failed to copy leptonica SO");
                std::fs::copy(&tess_local, &tess_target).expect("Failed to copy tesseract SO");
                return;
            }
        }
        #[cfg(target_os = "macos")]
        {
            let lept_local = dlls_dir.join("libleptonica.dylib");
            let tess_local = dlls_dir.join("libtesseract.dylib");
            if lept_local.exists() && tess_local.exists() {
                std::fs::copy(&lept_local, &lept_target).expect("Failed to copy leptonica dylib");
                std::fs::copy(&tess_local, &tess_target).expect("Failed to copy tesseract dylib");
                return;
            }
        }
    }

    eprintln!("Error: Tesseract/Leptonica DLLs not found in assets/ folder.");
    eprintln!("Required: assets/leptonica-1.85.0.dll and assets/tesseract.dll");
    eprintln!("No download — place the DLLs in assets/ before building.");
    std::process::exit(1);
}
